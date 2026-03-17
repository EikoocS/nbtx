use clap::{Parser, ValueEnum};
use flate2::Compression as FlateCompression;
use nbtx::decoder::{build as build_decoder, NbtDecoder};
use nbtx::encoder::{build as build_encoder, NbtEncoder};
use nbtx::{NbtComponent, ParseError, PlatformType, Reader};
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
    #[arg(help = "NBT path (leaf or component)")]
    path: String,
    #[arg(long = "show-type", help = "Show typed output like Int(1)")]
    show_type: bool,
    #[arg(long = "set", value_name = "VALUE", help = "Set leaf value at path")]
    set: Option<String>,
    #[arg(long = "create", value_name = "VALUE", help = "Create value at path")]
    create: Option<String>,
    #[arg(long = "delete", help = "Delete value at path")]
    delete: bool,
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

#[derive(Debug)]
enum PathSegment {
    Field(String),
    Index(usize),
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
        0x01 => Ok(NbtValue::Byte(decoder.read_byte()?)),
        0x02 => Ok(NbtValue::Short(decoder.read_short()?)),
        0x03 => Ok(NbtValue::Int(decoder.read_int()?)),
        0x04 => Ok(NbtValue::Long(decoder.read_long()?)),
        0x05 => Ok(NbtValue::Float(decoder.read_float()?)),
        0x06 => Ok(NbtValue::Double(decoder.read_double()?)),
        0x07 => Ok(NbtValue::ByteArray(decoder.read_byte_array()?)),
        0x08 => Ok(NbtValue::String(decoder.read_string()?)),
        0x09 => {
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
        0x0a => {
            let mut fields = Vec::new();
            loop {
                let field_id = decoder.read_id()?;
                if field_id == 0x00 {
                    break;
                }
                let tag = decoder.read_tag()?;
                let value = parse_value_by_id(field_id, decoder)?;
                fields.push((tag, value));
            }
            Ok(NbtValue::Compound(fields))
        }
        0x0b => Ok(NbtValue::IntArray(decoder.read_int_array()?)),
        0x0c => Ok(NbtValue::LongArray(decoder.read_long_array()?)),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn parse_document(path: &str, platform: PlatformType) -> Result<NbtValue, ParseError> {
    let read = open_read_stream(path)?;
    let mut decoder = build_decoder(read, platform);

    let root_id = decoder.read_id()?;
    let _root_tag = decoder.read_tag()?;
    match root_id {
        0x09 | 0x0a => parse_value_by_id(root_id, &mut *decoder),
        _ => Err(ParseError::InvalidRootTag(root_id)),
    }
}

fn component_id(value: &NbtValue) -> u8 {
    match value {
        NbtValue::Byte(_) => 0x01,
        NbtValue::Short(_) => 0x02,
        NbtValue::Int(_) => 0x03,
        NbtValue::Long(_) => 0x04,
        NbtValue::Float(_) => 0x05,
        NbtValue::Double(_) => 0x06,
        NbtValue::ByteArray(_) => 0x07,
        NbtValue::String(_) => 0x08,
        NbtValue::List { .. } => 0x09,
        NbtValue::Compound(_) => 0x0a,
        NbtValue::IntArray(_) => 0x0b,
        NbtValue::LongArray(_) => 0x0c,
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
            encoder.write_id(0x00)?;
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
            encoder.write_id(0x0a)?;
            encoder.write_tag("")?;
            for (name, value) in fields {
                write_value(&mut *encoder, name, value, false)?;
            }
            encoder.write_id(0x00)?;
        }
        NbtValue::List { id, elements } => {
            if elements.len() > i32::MAX as usize {
                return Err(ParseError::Other(format!(
                    "list length exceeds i32: {}",
                    elements.len()
                )));
            }

            encoder.write_id(0x09)?;
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
        PathSegment::Field(_) => 0x0a,
        PathSegment::Index(_) => 0x09,
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
    if *id == 0x00 && elements.is_empty() {
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

fn delete_at_path(value: &mut NbtValue, segments: &[PathSegment]) -> Result<(), String> {
    let Some((last, parent_segments)) = segments.split_last() else {
        return Err("cannot delete root value".to_string());
    };

    let parent = find_mut(value, parent_segments)?;

    match last {
        PathSegment::Field(name) => {
            let NbtValue::Compound(fields) = parent else {
                return Err(format!("path expects compound field '{name}'"));
            };

            let Some(index) = fields.iter().position(|(field_name, _)| field_name == name) else {
                return Err(format!("field not found: {name}"));
            };

            fields.remove(index);
            Ok(())
        }
        PathSegment::Index(index) => {
            let NbtValue::List { id, elements } = parent else {
                return Err(format!("path expects list index [{index}]"));
            };

            if *index >= elements.len() {
                return Err(format!("list index out of range: {index}"));
            }

            elements.remove(*index);
            if elements.is_empty() {
                *id = 0x00;
            }
            Ok(())
        }
    }
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

    let segments = parse_path(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;

    let target = find_mut(&mut document, &segments)
        .map_err(|err| Error::new(ErrorKind::NotFound, format!("path not found: {err}")))?;

    let new_value = parse_value_like(target, raw_value)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;
    *target = new_value;

    persist_document(args, &document, platform, compression)
}

fn run_create(args: &Args, raw_value: &str, platform: PlatformType) -> std::io::Result<()> {
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

    let segments = parse_path(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;

    delete_at_path(&mut document, &segments)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;

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

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let platform: PlatformType = args.platform.into();
    let action_count = args.set.is_some() as u8 + args.create.is_some() as u8 + args.delete as u8;

    if action_count > 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "only one of --set, --create, --delete can be used at a time",
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
