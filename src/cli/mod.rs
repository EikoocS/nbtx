use clap::{Parser, ValueEnum};
use flate2::Compression as FlateCompression;
use nbtx::decoder::{NbtDecoder, build as build_decoder};
use nbtx::encoder::{NbtEncoder, build as build_encoder};
use nbtx::{tag_id, NbtComponent, ParseError, PlatformType, Reader};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, ErrorKind, Read, Write};
use std::path::Path;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CliPlatform {
    Java,
    Bedrock,
}

impl From<CliPlatform> for PlatformType {
    fn from(value: CliPlatform) -> Self {
        match value {
            CliPlatform::Java => PlatformType::JavaEdition,
            CliPlatform::Bedrock => PlatformType::BedrockEdition,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "nbtx", version, about = "Read or modify NBT value by path")]
struct Args {
    #[arg(help = "NBT file path")]
    file: String,
    #[arg(help = "NBT path (leaf/component) or regex path prefixed with re:")]
    path: String,
    #[arg(long = "show-type", help = "Show typed output like Int(1)")]
    show_type: bool,
    #[arg(long = "set", value_name = "VALUE", help = "Set leaf value at path")]
    set: Option<String>,
    #[arg(long = "create", value_name = "VALUE", help = "Create value at path")]
    create: Option<String>,
    #[arg(long = "delete", help = "Delete value at path")]
    delete: bool,
    #[arg(
        long = "where",
        value_name = "EXPR",
        help = "Filter list element deletion, e.g. id==123&&count<999"
    )]
    where_expr: Option<String>,
    #[arg(long = "output", help = "Output file path (default: overwrite input)")]
    output: Option<String>,
    #[arg(long = "platform", value_enum, default_value_t = CliPlatform::Java)]
    platform: CliPlatform,
}

#[derive(Clone, Debug)]
enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List { id: u8, elements: Vec<NbtValue> },
    Compound(Vec<(String, NbtValue)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Copy, Clone, Debug)]
enum CompressionType {
    None,
    Gzip,
    Zlib,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PathSegment {
    Field(String),
    Index(usize),
}

enum PathSelector {
    Exact(Vec<PathSegment>),
    Regex(Regex),
}

#[derive(Clone)]
enum DeleteTail {
    Field(String),
    Index(usize),
}

#[derive(Clone)]
enum WhereValue {
    Number(f64),
    Text(String),
}

#[derive(Copy, Clone)]
enum WhereOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Regex,
}

struct WhereClause {
    field_path: Vec<String>,
    op: WhereOp,
    value: WhereValue,
    value_regex: Option<Regex>,
}

fn is_descendant_path(path: &str, target: &str) -> bool {
    if target.is_empty() {
        return true;
    }
    if !path.starts_with(target) {
        return false;
    }
    if path.len() == target.len() {
        return false;
    }
    matches!(path.as_bytes()[target.len()], b'.' | b'[')
}

fn format_component(component: &NbtComponent, show_type: bool) -> String {
    if show_type {
        return format!("{:?}", component);
    }

    match component {
        NbtComponent::End => "END".to_string(),
        NbtComponent::Byte(value) => value.to_string(),
        NbtComponent::Short(value) => value.to_string(),
        NbtComponent::Int(value) => value.to_string(),
        NbtComponent::Long(value) => value.to_string(),
        NbtComponent::Float(value) => value.to_string(),
        NbtComponent::Double(value) => value.to_string(),
        NbtComponent::ByteArray(value) => format!("{:?}", value),
        NbtComponent::String(value) => value.clone(),
        NbtComponent::List { id, length } => format!("list(id={id}, length={length})"),
        NbtComponent::Compound => "compound".to_string(),
        NbtComponent::IntArray(value) => format!("{:?}", value),
        NbtComponent::LongArray(value) => format!("{:?}", value),
    }
}

fn read_magic_number(path: &str) -> std::io::Result<[u8; 2]> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

fn detect_compression(path: &str) -> std::io::Result<CompressionType> {
    let magic = read_magic_number(path)?;
    let compression = match magic {
        [0x1F, 0x8B] => CompressionType::Gzip,
        [0x78, 0x9C] | [0x78, 0x01] | [0x78, 0xDA] => CompressionType::Zlib,
        _ => CompressionType::None,
    };
    Ok(compression)
}

fn open_read_stream(path: &str) -> std::io::Result<Box<dyn Read>> {
    let compression = detect_compression(path)?;
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let stream: Box<dyn Read> = match compression {
        CompressionType::Gzip => Box::new(flate2::read::GzDecoder::new(reader)),
        CompressionType::Zlib => Box::new(flate2::read::ZlibDecoder::new(reader)),
        CompressionType::None => Box::new(reader),
    };
    Ok(stream)
}

fn create_write_stream(
    path: &str,
    compression: CompressionType,
) -> std::io::Result<Box<dyn Write>> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let stream: Box<dyn Write> = match compression {
        CompressionType::Gzip => Box::new(flate2::write::GzEncoder::new(
            writer,
            FlateCompression::default(),
        )),
        CompressionType::Zlib => Box::new(flate2::write::ZlibEncoder::new(
            writer,
            FlateCompression::default(),
        )),
        CompressionType::None => Box::new(writer),
    };
    Ok(stream)
}

fn parse_value_by_id(id: u8, decoder: &mut dyn NbtDecoder) -> Result<NbtValue, ParseError> {
    match id {
        tag_id::BYTE => Ok(NbtValue::Byte(decoder.read_byte()?)),
        tag_id::SHORT => Ok(NbtValue::Short(decoder.read_short()?)),
        tag_id::INT => Ok(NbtValue::Int(decoder.read_int()?)),
        tag_id::LONG => Ok(NbtValue::Long(decoder.read_long()?)),
        tag_id::FLOAT => Ok(NbtValue::Float(decoder.read_float()?)),
        tag_id::DOUBLE => Ok(NbtValue::Double(decoder.read_double()?)),
        tag_id::BYTE_ARRAY => Ok(NbtValue::ByteArray(decoder.read_byte_array()?)),
        tag_id::STRING => Ok(NbtValue::String(decoder.read_string()?)),
        tag_id::LIST => {
            let list_id = decoder.read_id()?;
            let length = decoder.read_int()?;
            if length < 0 {
                return Err(ParseError::InvalidLength(length));
            }

            let mut elements = Vec::with_capacity(length as usize);
            for _ in 0..length {
                elements.push(parse_value_by_id(list_id, decoder)?);
            }

            Ok(NbtValue::List {
                id: list_id,
                elements,
            })
        }
        tag_id::COMPOUND => {
            let mut fields = Vec::new();
            loop {
                let field_id = decoder.read_id()?;
                if field_id == tag_id::END {
                    break;
                }
                let tag = decoder.read_tag()?;
                let value = parse_value_by_id(field_id, decoder)?;
                fields.push((tag, value));
            }
            Ok(NbtValue::Compound(fields))
        }
        tag_id::INT_ARRAY => Ok(NbtValue::IntArray(decoder.read_int_array()?)),
        tag_id::LONG_ARRAY => Ok(NbtValue::LongArray(decoder.read_long_array()?)),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn parse_document(path: &str, platform: PlatformType) -> Result<NbtValue, ParseError> {
    let read = open_read_stream(path)?;
    let mut decoder = build_decoder(read, platform);

    let root_id = decoder.read_id()?;
    let _root_tag = decoder.read_tag()?;
    match root_id {
        tag_id::LIST | tag_id::COMPOUND => parse_value_by_id(root_id, &mut *decoder),
        _ => Err(ParseError::InvalidRootTag(root_id)),
    }
}

fn component_id(value: &NbtValue) -> u8 {
    match value {
        NbtValue::Byte(_) => tag_id::BYTE,
        NbtValue::Short(_) => tag_id::SHORT,
        NbtValue::Int(_) => tag_id::INT,
        NbtValue::Long(_) => tag_id::LONG,
        NbtValue::Float(_) => tag_id::FLOAT,
        NbtValue::Double(_) => tag_id::DOUBLE,
        NbtValue::ByteArray(_) => tag_id::BYTE_ARRAY,
        NbtValue::String(_) => tag_id::STRING,
        NbtValue::List { .. } => tag_id::LIST,
        NbtValue::Compound(_) => tag_id::COMPOUND,
        NbtValue::IntArray(_) => tag_id::INT_ARRAY,
        NbtValue::LongArray(_) => tag_id::LONG_ARRAY,
    }
}

fn write_value(
    encoder: &mut dyn NbtEncoder,
    tag: &str,
    value: &NbtValue,
    in_list: bool,
) -> Result<(), ParseError> {
    if !in_list {
        encoder.write_id(component_id(value))?;
        encoder.write_tag(tag)?;
    }

    match value {
        NbtValue::Byte(value) => encoder.write_byte(*value)?,
        NbtValue::Short(value) => encoder.write_short(*value)?,
        NbtValue::Int(value) => encoder.write_int(*value)?,
        NbtValue::Long(value) => encoder.write_long(*value)?,
        NbtValue::Float(value) => encoder.write_float(*value)?,
        NbtValue::Double(value) => encoder.write_double(*value)?,
        NbtValue::ByteArray(value) => encoder.write_byte_array(value)?,
        NbtValue::String(value) => encoder.write_string(value)?,
        NbtValue::IntArray(value) => encoder.write_int_array(value)?,
        NbtValue::LongArray(value) => encoder.write_long_array(value)?,
        NbtValue::List { id, elements } => {
            if elements.len() > i32::MAX as usize {
                return Err(ParseError::Other(format!(
                    "list length exceeds i32: {}",
                    elements.len()
                )));
            }

            encoder.write_id(*id)?;
            encoder.write_int(elements.len() as i32)?;

            for element in elements {
                let element_id = component_id(element);
                if element_id != *id {
                    return Err(ParseError::Other(format!(
                        "list element type mismatch, expected id {id}, got {element_id}"
                    )));
                }
                write_value(encoder, "", element, true)?;
            }
        }
        NbtValue::Compound(fields) => {
            for (name, value) in fields {
                write_value(encoder, name, value, false)?;
            }
            encoder.write_id(tag_id::END)?;
        }
    }

    Ok(())
}

fn write_document(
    path: &str,
    value: &NbtValue,
    platform: PlatformType,
    compression: CompressionType,
) -> Result<(), ParseError> {
    let write = create_write_stream(path, compression)?;
    let mut encoder = build_encoder(write, platform);

    match value {
        NbtValue::Compound(fields) => {
            encoder.write_id(tag_id::COMPOUND)?;
            encoder.write_tag("")?;
            for (name, value) in fields {
                write_value(&mut *encoder, name, value, false)?;
            }
            encoder.write_id(tag_id::END)?;
        }
        NbtValue::List { id, elements } => {
            if elements.len() > i32::MAX as usize {
                return Err(ParseError::Other(format!(
                    "list length exceeds i32: {}",
                    elements.len()
                )));
            }

            encoder.write_id(tag_id::LIST)?;
            encoder.write_tag("")?;
            encoder.write_id(*id)?;
            encoder.write_int(elements.len() as i32)?;

            for element in elements {
                let element_id = component_id(element);
                if element_id != *id {
                    return Err(ParseError::Other(format!(
                        "list element type mismatch, expected id {id}, got {element_id}"
                    )));
                }
                write_value(&mut *encoder, "", element, true)?;
            }
        }
        _ => {
            return Err(ParseError::Other(
                "root tag must be compound or list".to_string(),
            ));
        }
    }

    encoder.flush()?;
    Ok(())
}

fn parse_path(path: &str) -> Result<Vec<PathSegment>, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }

    let chars: Vec<char> = path.chars().collect();
    let mut i = 0usize;
    let mut field = String::new();
    let mut segments = Vec::new();

    while i < chars.len() {
        match chars[i] {
            '.' => {
                if field.is_empty() {
                    if i > 0 && chars[i - 1] == ']' {
                        i += 1;
                        continue;
                    }
                    return Err(format!("invalid path near position {i}"));
                }
                segments.push(PathSegment::Field(std::mem::take(&mut field)));
                i += 1;
            }
            '[' => {
                if !field.is_empty() {
                    segments.push(PathSegment::Field(std::mem::take(&mut field)));
                }

                i += 1;
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }

                if start == i || i >= chars.len() || chars[i] != ']' {
                    return Err(format!("invalid list index near position {start}"));
                }

                let index: usize = path[start..i]
                    .parse()
                    .map_err(|_| format!("invalid list index near position {start}"))?;
                segments.push(PathSegment::Index(index));
                i += 1;
            }
            ch => {
                field.push(ch);
                i += 1;
            }
        }
    }

    if !field.is_empty() {
        segments.push(PathSegment::Field(field));
    }

    Ok(segments)
}

fn parse_path_selector(path: &str) -> Result<PathSelector, String> {
    if let Some(pattern) = path.strip_prefix("re:") {
        let regex = Regex::new(pattern).map_err(|err| format!("invalid path regex: {err}"))?;
        return Ok(PathSelector::Regex(regex));
    }

    parse_path(path).map(PathSelector::Exact)
}

fn append_field_path(base: &str, field: &str) -> String {
    if base.is_empty() {
        field.to_string()
    } else {
        format!("{base}.{field}")
    }
}

fn append_index_path(base: &str, index: usize) -> String {
    format!("{base}[{index}]")
}

fn collect_paths(
    value: &NbtValue,
    current_segments: &mut Vec<PathSegment>,
    current_path: &str,
    out: &mut Vec<(String, Vec<PathSegment>)>,
) {
    out.push((current_path.to_string(), current_segments.clone()));

    match value {
        NbtValue::Compound(fields) => {
            for (name, next) in fields {
                current_segments.push(PathSegment::Field(name.clone()));
                let next_path = append_field_path(current_path, name);
                collect_paths(next, current_segments, &next_path, out);
                current_segments.pop();
            }
        }
        NbtValue::List { elements, .. } => {
            for (index, next) in elements.iter().enumerate() {
                current_segments.push(PathSegment::Index(index));
                let next_path = append_index_path(current_path, index);
                collect_paths(next, current_segments, &next_path, out);
                current_segments.pop();
            }
        }
        _ => {}
    }
}

fn resolve_selector_paths(value: &NbtValue, selector: &PathSelector) -> Vec<Vec<PathSegment>> {
    match selector {
        PathSelector::Exact(segments) => vec![segments.clone()],
        PathSelector::Regex(regex) => {
            let mut all_paths = Vec::new();
            let mut segments = Vec::new();
            collect_paths(value, &mut segments, "", &mut all_paths);
            all_paths
                .into_iter()
                .filter_map(|(path, segs)| regex.is_match(&path).then_some(segs))
                .collect()
        }
    }
}

fn is_ancestor_path(ancestor: &[PathSegment], descendant: &[PathSegment]) -> bool {
    if ancestor.len() > descendant.len() {
        return false;
    }

    ancestor
        .iter()
        .zip(descendant.iter())
        .all(|(a, b)| match (a, b) {
            (PathSegment::Field(left), PathSegment::Field(right)) => left == right,
            (PathSegment::Index(left), PathSegment::Index(right)) => left == right,
            _ => false,
        })
}

fn normalize_delete_paths(paths: Vec<Vec<PathSegment>>) -> Vec<Vec<PathSegment>> {
    let mut sorted = paths;
    sorted.sort_by(|left, right| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| path_segments_to_string(left).cmp(&path_segments_to_string(right)))
    });

    let mut deduped: Vec<Vec<PathSegment>> = Vec::new();
    for candidate in sorted {
        let skip = deduped
            .iter()
            .any(|existing| is_ancestor_path(&candidate, existing));
        if !skip {
            deduped.push(candidate);
        }
    }

    deduped
}

fn path_segments_to_string(segments: &[PathSegment]) -> String {
    let mut text = String::new();
    for segment in segments {
        match segment {
            PathSegment::Field(name) => {
                text = append_field_path(&text, name);
            }
            PathSegment::Index(index) => {
                text = append_index_path(&text, *index);
            }
        }
    }
    text
}

fn delete_paths(value: &mut NbtValue, paths: Vec<Vec<PathSegment>>) -> Result<usize, String> {
    if paths.is_empty() {
        return Ok(0);
    }

    let mut groups: BTreeMap<String, (Vec<PathSegment>, Vec<DeleteTail>)> = BTreeMap::new();
    for path in paths {
        if path.is_empty() {
            return Err("cannot delete root value".to_string());
        }

        let mut parent = path.clone();
        let last = parent.pop().expect("checked non-empty path");
        let key = path_segments_to_string(&parent);
        let tail = match last {
            PathSegment::Field(name) => DeleteTail::Field(name),
            PathSegment::Index(index) => DeleteTail::Index(index),
        };

        groups
            .entry(key)
            .and_modify(|(_, tails)| tails.push(tail.clone()))
            .or_insert((parent, vec![tail]));
    }

    let mut deleted = 0usize;
    for (_, (parent_segments, tails)) in groups {
        let parent = find_mut(value, &parent_segments)?;
        match parent {
            NbtValue::Compound(fields) => {
                let mut names: Vec<String> = tails
                    .iter()
                    .map(|tail| match tail {
                        DeleteTail::Field(name) => Ok(name.clone()),
                        DeleteTail::Index(_) => {
                            Err("cannot delete list index from compound parent".to_string())
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                names.sort();
                names.dedup();

                let before = fields.len();
                fields.retain(|(name, _)| !names.iter().any(|target| target == name));
                deleted += before.saturating_sub(fields.len());
            }
            NbtValue::List { id, elements } => {
                let mut indexes: Vec<usize> = tails
                    .iter()
                    .map(|tail| match tail {
                        DeleteTail::Index(index) => Ok(*index),
                        DeleteTail::Field(_) => {
                            Err("cannot delete compound field from list parent".to_string())
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                indexes.sort_unstable();
                indexes.dedup();
                indexes.reverse();

                for index in indexes {
                    if index >= elements.len() {
                        return Err(format!("list index out of range: {index}"));
                    }
                    elements.remove(index);
                    deleted += 1;
                }

                if elements.is_empty() {
                    *id = tag_id::END;
                }
            }
            _ => {
                return Err("delete target parent is not compound or list".to_string());
            }
        }
    }

    Ok(deleted)
}

fn parse_where_value(raw: &str) -> WhereValue {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return WhereValue::Text(trimmed[1..trimmed.len() - 1].to_string());
    }

    if let Ok(number) = trimmed.parse::<f64>() {
        return WhereValue::Number(number);
    }

    WhereValue::Text(trimmed.to_string())
}

fn parse_where_expr(raw: &str) -> Result<Vec<WhereClause>, String> {
    let mut clauses = Vec::new();
    for token in raw.split("&&") {
        let clause = token.trim();
        if clause.is_empty() {
            return Err("empty where clause".to_string());
        }

        let operators = [
            ("==", WhereOp::Eq),
            ("!=", WhereOp::Ne),
            ("<=", WhereOp::Le),
            (">=", WhereOp::Ge),
            ("~=", WhereOp::Regex),
            ("<", WhereOp::Lt),
            (">", WhereOp::Gt),
        ];

        let mut parsed = None;
        for (symbol, op) in operators {
            if let Some((lhs, rhs)) = clause.split_once(symbol) {
                parsed = Some((lhs.trim(), op, rhs.trim()));
                break;
            }
        }

        let Some((lhs, op, rhs)) = parsed else {
            return Err(format!("invalid where clause: {clause}"));
        };

        if lhs.is_empty() || rhs.is_empty() {
            return Err(format!("invalid where clause: {clause}"));
        }

        let field_path = lhs
            .split('.')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if field_path.is_empty() {
            return Err(format!("invalid where field path: {lhs}"));
        }

        let value = parse_where_value(rhs);
        let value_regex = match (&op, &value) {
            (WhereOp::Regex, WhereValue::Text(pattern)) => {
                Some(Regex::new(pattern).map_err(|err| format!("invalid where regex: {err}"))?)
            }
            (WhereOp::Regex, WhereValue::Number(_)) => {
                return Err("where regex operator expects text value".to_string());
            }
            _ => None,
        };

        clauses.push(WhereClause {
            field_path,
            op,
            value,
            value_regex,
        });
    }

    if clauses.is_empty() {
        return Err("where expression is empty".to_string());
    }

    Ok(clauses)
}

fn lookup_compound_field<'a>(value: &'a NbtValue, field_path: &[String]) -> Option<&'a NbtValue> {
    let mut current = value;
    for field in field_path {
        let NbtValue::Compound(fields) = current else {
            return None;
        };
        let (_, next) = fields.iter().find(|(name, _)| name == field)?;
        current = next;
    }
    Some(current)
}

fn as_number(value: &NbtValue) -> Option<f64> {
    match value {
        NbtValue::Byte(number) => Some(*number as f64),
        NbtValue::Short(number) => Some(*number as f64),
        NbtValue::Int(number) => Some(*number as f64),
        NbtValue::Long(number) => Some(*number as f64),
        NbtValue::Float(number) => Some(*number as f64),
        NbtValue::Double(number) => Some(*number),
        _ => None,
    }
}

fn as_text(value: &NbtValue) -> Option<&str> {
    match value {
        NbtValue::String(text) => Some(text.as_str()),
        _ => None,
    }
}

fn where_clause_matches(value: &NbtValue, clause: &WhereClause) -> bool {
    let Some(target) = lookup_compound_field(value, &clause.field_path) else {
        return false;
    };

    match clause.op {
        WhereOp::Eq => match (&clause.value, as_number(target), as_text(target)) {
            (WhereValue::Number(expected), Some(actual), _) => actual == *expected,
            (WhereValue::Text(expected), _, Some(actual)) => actual == expected,
            _ => false,
        },
        WhereOp::Ne => match (&clause.value, as_number(target), as_text(target)) {
            (WhereValue::Number(expected), Some(actual), _) => actual != *expected,
            (WhereValue::Text(expected), _, Some(actual)) => actual != expected,
            _ => false,
        },
        WhereOp::Lt => match (&clause.value, as_number(target)) {
            (WhereValue::Number(expected), Some(actual)) => actual < *expected,
            _ => false,
        },
        WhereOp::Le => match (&clause.value, as_number(target)) {
            (WhereValue::Number(expected), Some(actual)) => actual <= *expected,
            _ => false,
        },
        WhereOp::Gt => match (&clause.value, as_number(target)) {
            (WhereValue::Number(expected), Some(actual)) => actual > *expected,
            _ => false,
        },
        WhereOp::Ge => match (&clause.value, as_number(target)) {
            (WhereValue::Number(expected), Some(actual)) => actual >= *expected,
            _ => false,
        },
        WhereOp::Regex => match (clause.value_regex.as_ref(), as_text(target)) {
            (Some(regex), Some(actual)) => regex.is_match(actual),
            _ => false,
        },
    }
}

fn where_matches_all(value: &NbtValue, clauses: &[WhereClause]) -> bool {
    clauses
        .iter()
        .all(|clause| where_clause_matches(value, clause))
}

fn resolve_list_targets_for_where(
    document: &NbtValue,
    matched_paths: Vec<Vec<PathSegment>>,
) -> Vec<Vec<PathSegment>> {
    let mut targets: Vec<Vec<PathSegment>> = Vec::new();

    for path in matched_paths {
        for depth in (0..=path.len()).rev() {
            let candidate = path[..depth].to_vec();
            let Ok(value) = find_ref(document, &candidate) else {
                continue;
            };
            if matches!(value, NbtValue::List { .. }) {
                if !targets.contains(&candidate) {
                    targets.push(candidate);
                }
                break;
            }
        }
    }

    targets
}

fn find_mut<'a>(
    value: &'a mut NbtValue,
    segments: &[PathSegment],
) -> Result<&'a mut NbtValue, String> {
    let mut current = value;

    for segment in segments {
        match segment {
            PathSegment::Field(name) => {
                let NbtValue::Compound(fields) = current else {
                    return Err(format!("path expects compound field '{name}'"));
                };

                let Some((_, next)) = fields.iter_mut().find(|(field_name, _)| field_name == name)
                else {
                    return Err(format!("field not found: {name}"));
                };
                current = next;
            }
            PathSegment::Index(index) => {
                let NbtValue::List { elements, .. } = current else {
                    return Err(format!("path expects list index [{index}]"));
                };

                let Some(next) = elements.get_mut(*index) else {
                    return Err(format!("list index out of range: {index}"));
                };
                current = next;
            }
        }
    }

    Ok(current)
}

fn find_ref<'a>(value: &'a NbtValue, segments: &[PathSegment]) -> Result<&'a NbtValue, String> {
    let mut current = value;

    for segment in segments {
        match segment {
            PathSegment::Field(name) => {
                let NbtValue::Compound(fields) = current else {
                    return Err(format!("path expects compound field '{name}'"));
                };

                let Some((_, next)) = fields.iter().find(|(field_name, _)| field_name == name)
                else {
                    return Err(format!("field not found: {name}"));
                };
                current = next;
            }
            PathSegment::Index(index) => {
                let NbtValue::List { elements, .. } = current else {
                    return Err(format!("path expects list index [{index}]"));
                };

                let Some(next) = elements.get(*index) else {
                    return Err(format!("list index out of range: {index}"));
                };
                current = next;
            }
        }
    }

    Ok(current)
}

fn parse_number_list<T>(raw: &str) -> Result<Vec<T>, String>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let mut text = raw.trim();
    if text.starts_with('[') {
        if !text.ends_with(']') {
            return Err("array literal is missing closing ']'".to_string());
        }
        text = &text[1..text.len() - 1];
    }

    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    text.split(',')
        .map(|part| {
            let token = part.trim();
            token
                .parse::<T>()
                .map_err(|err| format!("failed to parse '{token}': {err}"))
        })
        .collect()
}

fn parse_value_like(target: &NbtValue, raw: &str) -> Result<NbtValue, String> {
    match target {
        NbtValue::Byte(_) => raw
            .trim()
            .parse::<i8>()
            .map(NbtValue::Byte)
            .map_err(|err| format!("invalid byte: {err}")),
        NbtValue::Short(_) => raw
            .trim()
            .parse::<i16>()
            .map(NbtValue::Short)
            .map_err(|err| format!("invalid short: {err}")),
        NbtValue::Int(_) => raw
            .trim()
            .parse::<i32>()
            .map(NbtValue::Int)
            .map_err(|err| format!("invalid int: {err}")),
        NbtValue::Long(_) => raw
            .trim()
            .parse::<i64>()
            .map(NbtValue::Long)
            .map_err(|err| format!("invalid long: {err}")),
        NbtValue::Float(_) => raw
            .trim()
            .parse::<f32>()
            .map(NbtValue::Float)
            .map_err(|err| format!("invalid float: {err}")),
        NbtValue::Double(_) => raw
            .trim()
            .parse::<f64>()
            .map(NbtValue::Double)
            .map_err(|err| format!("invalid double: {err}")),
        NbtValue::String(_) => Ok(NbtValue::String(raw.to_string())),
        NbtValue::ByteArray(_) => parse_number_list::<u8>(raw).map(NbtValue::ByteArray),
        NbtValue::IntArray(_) => parse_number_list::<i32>(raw).map(NbtValue::IntArray),
        NbtValue::LongArray(_) => parse_number_list::<i64>(raw).map(NbtValue::LongArray),
        NbtValue::List { .. } | NbtValue::Compound(_) => {
            Err("only leaf values can be modified by --set".to_string())
        }
    }
}

fn parse_value_for_create(raw: &str) -> Result<NbtValue, String> {
    let text = raw.trim();
    if let Some((prefix, value)) = text.split_once(':') {
        let kind = prefix.trim().to_ascii_lowercase();
        let payload = value.trim();
        return match kind.as_str() {
            "byte" => payload
                .parse::<i8>()
                .map(NbtValue::Byte)
                .map_err(|err| format!("invalid byte: {err}")),
            "short" => payload
                .parse::<i16>()
                .map(NbtValue::Short)
                .map_err(|err| format!("invalid short: {err}")),
            "int" => payload
                .parse::<i32>()
                .map(NbtValue::Int)
                .map_err(|err| format!("invalid int: {err}")),
            "long" => payload
                .parse::<i64>()
                .map(NbtValue::Long)
                .map_err(|err| format!("invalid long: {err}")),
            "float" => payload
                .parse::<f32>()
                .map(NbtValue::Float)
                .map_err(|err| format!("invalid float: {err}")),
            "double" => payload
                .parse::<f64>()
                .map(NbtValue::Double)
                .map_err(|err| format!("invalid double: {err}")),
            "string" => Ok(NbtValue::String(payload.to_string())),
            "bytes" => parse_number_list::<u8>(payload).map(NbtValue::ByteArray),
            "ints" => parse_number_list::<i32>(payload).map(NbtValue::IntArray),
            "longs" => parse_number_list::<i64>(payload).map(NbtValue::LongArray),
            _ => Err(format!(
                "unsupported create type '{kind}', expected one of: byte, short, int, long, float, double, string, bytes, ints, longs"
            )),
        };
    }

    if let Ok(value) = text.parse::<i32>() {
        return Ok(NbtValue::Int(value));
    }
    if let Ok(value) = text.parse::<i64>() {
        return Ok(NbtValue::Long(value));
    }
    if let Ok(value) = text.parse::<f64>() {
        return Ok(NbtValue::Double(value));
    }

    Ok(NbtValue::String(text.to_string()))
}

fn infer_list_element_id(tail: &[PathSegment], new_value: &NbtValue) -> u8 {
    if tail.is_empty() {
        return component_id(new_value);
    }

    match tail[0] {
        PathSegment::Field(_) => tag_id::COMPOUND,
        PathSegment::Index(_) => tag_id::LIST,
    }
}

fn make_intermediate_value(path_tail: &[PathSegment], new_value: &NbtValue) -> NbtValue {
    match path_tail[0] {
        PathSegment::Field(_) => NbtValue::Compound(Vec::new()),
        PathSegment::Index(_) => NbtValue::List {
            id: infer_list_element_id(&path_tail[1..], new_value),
            elements: Vec::new(),
        },
    }
}

fn ensure_list_id(id: &mut u8, elements: &[NbtValue], expected: u8) -> Result<(), String> {
    if *id == tag_id::END && elements.is_empty() {
        *id = expected;
        return Ok(());
    }

    if *id != expected {
        return Err(format!(
            "list element type mismatch, expected id {id}, got {expected}"
        ));
    }

    Ok(())
}

fn create_path_recursive(
    current: &mut NbtValue,
    segments: &[PathSegment],
    new_value: &NbtValue,
) -> Result<(), String> {
    let Some((first, rest)) = segments.split_first() else {
        return Err("path cannot be empty".to_string());
    };

    if rest.is_empty() {
        return match first {
            PathSegment::Field(name) => {
                let NbtValue::Compound(fields) = current else {
                    return Err(format!("path expects compound field '{name}'"));
                };
                if fields.iter().any(|(field_name, _)| field_name == name) {
                    return Err(format!("path already exists: {name}"));
                }
                fields.push((name.clone(), new_value.clone()));
                Ok(())
            }
            PathSegment::Index(index) => {
                let NbtValue::List { id, elements } = current else {
                    return Err(format!("path expects list index [{index}]"));
                };
                if *index < elements.len() {
                    return Err(format!("path already exists at list index: {index}"));
                }
                if *index > elements.len() {
                    return Err(format!("cannot create sparse list index: {index}"));
                }
                let expected_id = component_id(new_value);
                ensure_list_id(id, elements, expected_id)?;
                elements.push(new_value.clone());
                Ok(())
            }
        };
    }

    match first {
        PathSegment::Field(name) => {
            let NbtValue::Compound(fields) = current else {
                return Err(format!("path expects compound field '{name}'"));
            };

            if let Some(position) = fields.iter().position(|(field_name, _)| field_name == name) {
                return create_path_recursive(&mut fields[position].1, rest, new_value);
            }

            fields.push((name.clone(), make_intermediate_value(rest, new_value)));
            let last = fields.len() - 1;
            create_path_recursive(&mut fields[last].1, rest, new_value)
        }
        PathSegment::Index(index) => {
            let NbtValue::List { id, elements } = current else {
                return Err(format!("path expects list index [{index}]"));
            };

            if *index > elements.len() {
                return Err(format!("cannot create sparse list index: {index}"));
            }

            if *index == elements.len() {
                let intermediate = make_intermediate_value(rest, new_value);
                let intermediate_id = component_id(&intermediate);
                ensure_list_id(id, elements, intermediate_id)?;
                elements.push(intermediate);
            }

            create_path_recursive(&mut elements[*index], rest, new_value)
        }
    }
}

fn create_at_path(
    value: &mut NbtValue,
    segments: &[PathSegment],
    new_value: NbtValue,
) -> Result<(), String> {
    create_path_recursive(value, segments, &new_value)
}

fn run_read(args: &Args, platform: PlatformType) -> std::io::Result<()> {
    let mut reader = Reader::try_new_with_path(&args.file, platform)
        .map_err(|err| std::io::Error::other(format!("failed to create reader: {err}")))?;

    let mut descendants: Vec<(String, String)> = Vec::new();

    while reader.has_next() {
        let (path, component) = reader
            .next()
            .map_err(|err| std::io::Error::other(format!("failed to read entry: {err}")))?;

        if path == args.path {
            println!("{}", format_component(&component, args.show_type));
            return Ok(());
        }

        if is_descendant_path(&path, &args.path) {
            descendants.push((path, format_component(&component, args.show_type)));
        }
    }

    if !descendants.is_empty() {
        for (path, value) in descendants {
            println!("{}: {}", path, value);
        }
        return Ok(());
    }

    Err(Error::new(
        ErrorKind::NotFound,
        format!("path not found: {}", args.path),
    ))
}

fn run_set(args: &Args, raw_value: &str, platform: PlatformType) -> std::io::Result<()> {
    let compression = detect_compression(&args.file)?;
    let mut document = parse_document(&args.file, platform)
        .map_err(|err| std::io::Error::other(err.to_string()))?;

    let selector = parse_path_selector(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;
    let targets = resolve_selector_paths(&document, &selector);
    if targets.is_empty() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("path not found: {}", args.path),
        ));
    }

    for segments in targets {
        let rendered_path = path_segments_to_string(&segments);
        let target = find_mut(&mut document, &segments)
            .map_err(|err| Error::new(ErrorKind::NotFound, format!("path not found: {err}")))?;
        let new_value = parse_value_like(target, raw_value).map_err(|err| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid value at path '{rendered_path}': {err}"),
            )
        })?;
        *target = new_value;
    }

    persist_document(args, &document, platform, compression)
}

fn run_create(args: &Args, raw_value: &str, platform: PlatformType) -> std::io::Result<()> {
    if args.path.starts_with("re:") {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "--create does not support regex path; provide an exact path",
        ));
    }

    let compression = detect_compression(&args.file)?;
    let mut document = parse_document(&args.file, platform)
        .map_err(|err| std::io::Error::other(err.to_string()))?;

    let segments = parse_path(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;
    let new_value = parse_value_for_create(raw_value)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;

    create_at_path(&mut document, &segments, new_value)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;

    persist_document(args, &document, platform, compression)
}

fn run_delete(args: &Args, platform: PlatformType) -> std::io::Result<()> {
    let compression = detect_compression(&args.file)?;
    let mut document = parse_document(&args.file, platform)
        .map_err(|err| std::io::Error::other(err.to_string()))?;

    let selector = parse_path_selector(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;
    let matched_paths = resolve_selector_paths(&document, &selector);

    if matched_paths.is_empty() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("path not found: {}", args.path),
        ));
    }

    if let Some(where_expr) = &args.where_expr {
        let clauses = parse_where_expr(where_expr)
            .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid where: {err}")))?;
        let list_targets = resolve_list_targets_for_where(&document, matched_paths);
        if list_targets.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "path does not resolve to any list for --where: {}",
                    args.path
                ),
            ));
        }
        let mut deleted = 0usize;

        for path in list_targets {
            let rendered_path = path_segments_to_string(&path);
            let target = find_mut(&mut document, &path).map_err(|err| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("path not found during delete: {err}"),
                )
            })?;

            let NbtValue::List { id, elements } = target else {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("path is not a list: {rendered_path}"),
                ));
            };

            let before = elements.len();
            elements.retain(|element| {
                if !matches!(element, NbtValue::Compound(_)) {
                    return true;
                }
                !where_matches_all(element, &clauses)
            });
            deleted += before.saturating_sub(elements.len());
            if elements.is_empty() {
                    *id = tag_id::END;
            }
        }

        if deleted == 0 {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("no list elements matched where expression: {where_expr}"),
            ));
        }
    } else {
        let delete_targets = normalize_delete_paths(matched_paths);
        let deleted = delete_paths(&mut document, delete_targets)
            .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;
        if deleted == 0 {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("path not found: {}", args.path),
            ));
        }
    }

    persist_document(args, &document, platform, compression)
}

fn persist_document(
    args: &Args,
    document: &NbtValue,
    platform: PlatformType,
    compression: CompressionType,
) -> std::io::Result<()> {
    if let Some(output) = &args.output {
        write_document(output, document, platform, compression)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        return Ok(());
    }

    let temp_path = format!("{}.nbtx.tmp", args.file);
    write_document(&temp_path, document, platform, compression)
        .map_err(|err| std::io::Error::other(err.to_string()))?;

    if Path::new(&args.file).exists() {
        std::fs::remove_file(&args.file)?;
    }
    std::fs::rename(&temp_path, &args.file)?;
    Ok(())
}

pub fn run() -> std::io::Result<()> {
    let args = Args::parse();
    let platform: PlatformType = args.platform.into();
    let action_count = args.set.is_some() as u8 + args.create.is_some() as u8 + args.delete as u8;

    if action_count > 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "only one of --set, --create, --delete can be used at a time",
        ));
    }

    if args.where_expr.is_some() && !args.delete {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "--where can only be used with --delete",
        ));
    }

    if let Some(raw_value) = &args.set {
        return run_set(&args, raw_value, platform);
    }

    if let Some(raw_value) = &args.create {
        return run_create(&args, raw_value, platform);
    }

    if args.delete {
        return run_delete(&args, platform);
    }

    run_read(&args, platform)
}

#[cfg(test)]
mod tests;
