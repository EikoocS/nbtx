use crate::cli::args::Args;
use crate::cli::codec::{component_id, detect_compression};
use crate::cli::path::{
    normalize_delete_paths, parse_path, parse_path_selector, path_segments_to_string,
};
use crate::cli::types::{CompressionType, NbtValue, PathSegment, PathSelector, WhereClause};
use crate::cli::values::{parse_value_for_create, parse_value_like};
use crate::cli::where_expr::{parse_where_expr, where_matches_all};
use flate2::Compression as FlateCompression;
use nbtx::decoder::{Decoder};
use nbtx::encoder::{Encoder};
use nbtx::{ParseError, PlatformType, tag_id};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::System;

const FULL_TREE_MEMORY_RATIO: f64 = 0.20;
const FULL_TREE_MIN_AVAILABLE_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn prefer_full_tree_edit(input_path: &str) -> std::io::Result<bool> {
    let compression = detect_compression(input_path)?;
    let file_size = std::fs::metadata(input_path)?.len();
    let estimate = estimate_full_tree_working_set(file_size, compression);

    let mut system = System::new();
    system.refresh_memory();
    let available = system.available_memory();

    if available < FULL_TREE_MIN_AVAILABLE_BYTES {
        return Ok(false);
    }

    Ok((estimate as f64) <= (available as f64) * FULL_TREE_MEMORY_RATIO)
}

fn estimate_full_tree_working_set(file_size: u64, compression: CompressionType) -> u64 {
    let multiplier: u64 = match compression {
        CompressionType::None => 4,
        CompressionType::Gzip | CompressionType::Zlib => 10,
    };
    let base = file_size.saturating_mul(multiplier);
    base.saturating_add(64 * 1024 * 1024)
}

#[derive(Debug)]
enum EditError {
    Io(std::io::Error),
    Parse(ParseError),
    InvalidInput(String),
}

impl From<std::io::Error> for EditError {
    fn from(value: std::io::Error) -> Self {
        EditError::Io(value)
    }
}

impl From<ParseError> for EditError {
    fn from(value: ParseError) -> Self {
        EditError::Parse(value)
    }
}

impl EditError {
    fn into_io(self) -> std::io::Error {
        match self {
            EditError::Io(err) => err,
            EditError::Parse(err) => std::io::Error::other(err.to_string()),
            EditError::InvalidInput(err) => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, err)
            }
        }
    }
}

struct SetPlan {
    selector: PathSelector,
    raw_value: String,
    hits: usize,
}

struct CreatePlan {
    segments: Vec<PathSegment>,
    value: NbtValue,
    done: bool,
}

struct DeletePlan {
    targets: HashSet<String>,
    hits: usize,
}

struct WherePlan {
    clauses: Vec<WhereClause>,
    list_targets: HashSet<String>,
    deleted: usize,
    where_expr: String,
}

enum Plan {
    Set(SetPlan),
    Create(CreatePlan),
    Delete {
        base: DeletePlan,
        where_plan: Option<WherePlan>,
    },
}

fn remove_file_if_exists(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn path_not_found_error(path: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("path not found: {path}"),
    )
}

pub(super) fn run_set_stream(
    args: &Args,
    raw_value: &str,
    platform: PlatformType,
) -> std::io::Result<()> {
    let selector = parse_path_selector(&args.path)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
    let mut plan = Plan::Set(SetPlan {
        selector,
        raw_value: raw_value.to_string(),
        hits: 0,
    });
    run_with_plan(args, platform, &mut plan)
}

pub(super) fn run_create_stream(
    args: &Args,
    raw_value: &str,
    platform: PlatformType,
) -> std::io::Result<()> {
    if args.path.starts_with("re:") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "--create does not support regex path; provide an exact path",
        ));
    }

    let segments = parse_path(&args.path)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
    if segments.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path cannot be empty",
        ));
    }

    let value = parse_value_for_create(raw_value)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
    let mut plan = Plan::Create(CreatePlan {
        segments,
        value,
        done: false,
    });
    run_with_plan(args, platform, &mut plan)
}

pub(super) fn run_delete_stream(args: &Args, platform: PlatformType) -> std::io::Result<()> {
    let selector = parse_path_selector(&args.path)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
    let compression =
        detect_compression(&args.file).map_err(|err| std::io::Error::other(err.to_string()))?;
    let input_raw = make_temp_path_for(&args.file, "rawin");
    remove_file_if_exists(&input_raw);
    stream_decode_to_raw(Path::new(&args.file), &input_raw, compression)
        .map_err(EditError::into_io)?;

    let scan = scan_paths(&input_raw, platform, &selector).map_err(EditError::into_io)?;
    if scan.matched_paths.is_empty() {
        remove_file_if_exists(&input_raw);
        return Err(path_not_found_error(&args.path));
    }

    let where_plan = if let Some(where_expr) = &args.where_expr {
        let clauses = parse_where_expr(where_expr)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
        let list_targets = resolve_list_targets_from_scan(&scan);
        if list_targets.is_empty() {
            remove_file_if_exists(&input_raw);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "path does not resolve to any list for --where: {}",
                    args.path
                ),
            ));
        }
        Some(WherePlan {
            clauses,
            list_targets,
            deleted: 0,
            where_expr: where_expr.clone(),
        })
    } else {
        None
    };

    let mut targets = HashSet::new();
    if where_plan.is_none() {
        let delete_targets = normalize_delete_paths(scan.matched_paths);
        if delete_targets.iter().any(|path| path.is_empty()) {
            remove_file_if_exists(&input_raw);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot delete root value",
            ));
        }
        for target in delete_targets {
            targets.insert(path_segments_to_string(&target));
        }
    }

    let mut plan = Plan::Delete {
        base: DeletePlan { targets, hits: 0 },
        where_plan,
    };

    let output_target = args.output.as_deref().unwrap_or(&args.file);
    let output_raw = make_temp_path_for(output_target, "rawout");
    remove_file_if_exists(&output_raw);

    let transform_result = transform_raw(&input_raw, &output_raw, platform, &mut plan);
    remove_file_if_exists(&input_raw);
    if let Err(err) = transform_result {
        remove_file_if_exists(&output_raw);
        return Err(err.into_io());
    }

    if let Plan::Delete { base, where_plan } = &plan {
        if let Some(where_plan) = where_plan {
            if where_plan.deleted == 0 {
                remove_file_if_exists(&output_raw);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "no list elements matched where expression: {}",
                        where_plan.where_expr
                    ),
                ));
            }
        } else if base.hits == 0 {
            remove_file_if_exists(&output_raw);
            return Err(path_not_found_error(&args.path));
        }
    }

    if let Err(err) = emit_output(&output_raw, output_target, compression) {
        remove_file_if_exists(&output_raw);
        return Err(err.into_io());
    }

    Ok(())
}

fn run_with_plan(args: &Args, platform: PlatformType, plan: &mut Plan) -> std::io::Result<()> {
    let compression =
        detect_compression(&args.file).map_err(|err| std::io::Error::other(err.to_string()))?;
    let input_raw = make_temp_path_for(&args.file, "rawin");
    let output_target = args.output.as_deref().unwrap_or(&args.file);
    let output_raw = make_temp_path_for(output_target, "rawout");

    remove_file_if_exists(&input_raw);
    remove_file_if_exists(&output_raw);

    stream_decode_to_raw(Path::new(&args.file), &input_raw, compression)
        .map_err(EditError::into_io)?;
    let transform_result = transform_raw(&input_raw, &output_raw, platform, plan);
    remove_file_if_exists(&input_raw);

    if let Err(err) = transform_result {
        remove_file_if_exists(&output_raw);
        return Err(err.into_io());
    }

    match plan {
        Plan::Set(set) => {
            if set.hits == 0 {
                remove_file_if_exists(&output_raw);
                return Err(path_not_found_error(&args.path));
            }
        }
        Plan::Create(create) => {
            if !create.done {
                remove_file_if_exists(&output_raw);
                return Err(path_not_found_error(&args.path));
            }
        }
        Plan::Delete { .. } => {}
    }

    emit_output(&output_raw, output_target, compression).map_err(EditError::into_io)
}

fn make_temp_path_for(base: &str, suffix: &str) -> PathBuf {
    let base_path = Path::new(base);
    let parent = base_path
        .parent()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = base_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("nbtx");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    parent.join(format!("{stem}.nbtx.{pid}.{nanos}.{suffix}.tmp"))
}

fn open_read_stream(path: &Path, compression: CompressionType) -> Result<Box<dyn Read>, EditError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let stream: Box<dyn Read> = match compression {
        CompressionType::Gzip => Box::new(flate2::read::GzDecoder::new(reader)),
        CompressionType::Zlib => Box::new(flate2::read::ZlibDecoder::new(reader)),
        CompressionType::None => Box::new(reader),
    };
    Ok(stream)
}

fn open_write_stream(
    path: &Path,
    compression: CompressionType,
) -> Result<Box<dyn Write>, EditError> {
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

fn stream_decode_to_raw(
    input: &Path,
    raw_output: &Path,
    compression: CompressionType,
) -> Result<(), EditError> {
    let mut read = open_read_stream(input, compression)?;
    let file = File::create(raw_output)?;
    let mut write = BufWriter::new(file);
    std::io::copy(&mut read, &mut write)?;
    write.flush()?;
    Ok(())
}

fn emit_output(
    raw_output: &Path,
    output: &str,
    compression: CompressionType,
) -> Result<(), EditError> {
    let output_path = Path::new(output);
    match compression {
        CompressionType::None => {
            if output_path.exists() {
                std::fs::remove_file(output_path)?;
            }
            std::fs::rename(raw_output, output_path)?;
            Ok(())
        }
        CompressionType::Gzip | CompressionType::Zlib => {
            let compressed_temp = make_temp_path_for(output, "compressed");
            let mut read = BufReader::new(File::open(raw_output)?);
            let mut write = open_write_stream(&compressed_temp, compression)?;
            std::io::copy(&mut read, &mut write)?;
            write.flush()?;

            if output_path.exists() {
                std::fs::remove_file(output_path)?;
            }
            std::fs::rename(&compressed_temp, output_path)?;
            std::fs::remove_file(raw_output)?;
            Ok(())
        }
    }
}

struct ScanResult {
    matched_paths: Vec<Vec<PathSegment>>,
    list_paths: HashSet<String>,
}

fn scan_paths(
    raw_input: &Path,
    platform: PlatformType,
    selector: &PathSelector,
) -> Result<ScanResult, EditError> {
    let file = File::open(raw_input)?;
    let read = Box::new(BufReader::new(file)) as Box<dyn Read>;
    let mut decoder = Decoder::new(read, platform);
    let mut matched_paths = Vec::new();
    let mut list_paths = HashSet::new();
    let mut path = Vec::new();

    let root_id = decoder.read_id()?;
    let _root_tag = decoder.read_tag()?;
    if root_id != tag_id::LIST && root_id != tag_id::COMPOUND {
        return Err(EditError::Parse(ParseError::InvalidRootTag(root_id)));
    }

    scan_node(
        &mut decoder,
        root_id,
        &mut path,
        selector,
        &mut matched_paths,
        &mut list_paths,
    )?;

    Ok(ScanResult {
        matched_paths,
        list_paths,
    })
}

fn selector_matches(selector: &PathSelector, path: &[PathSegment]) -> bool {
    match selector {
        PathSelector::Exact(segments) => segments == path,
        PathSelector::Regex(regex) => regex.is_match(&path_segments_to_string(path)),
    }
}

fn scan_node(
    decoder: &mut Decoder,
    id: u8,
    path: &mut Vec<PathSegment>,
    selector: &PathSelector,
    matched_paths: &mut Vec<Vec<PathSegment>>,
    list_paths: &mut HashSet<String>,
) -> Result<(), EditError> {
    if selector_matches(selector, path) {
        matched_paths.push(path.clone());
    }

    match id {
        tag_id::LIST => {
            list_paths.insert(path_segments_to_string(path));
            let element_id = decoder.read_id()?;
            let length = decoder.read_int()?;
            if length < 0 {
                return Err(EditError::Parse(ParseError::InvalidLength(length)));
            }
            for index in 0..length {
                path.push(PathSegment::Index(index as usize));
                scan_node(
                    decoder,
                    element_id,
                    path,
                    selector,
                    matched_paths,
                    list_paths,
                )?;
                path.pop();
            }
            Ok(())
        }
        tag_id::COMPOUND => {
            loop {
                let field_id = decoder.read_id()?;
                if field_id == tag_id::END {
                    break;
                }
                let name = decoder.read_tag()?;
                path.push(PathSegment::Field(name));
                scan_node(decoder, field_id, path, selector, matched_paths, list_paths)?;
                path.pop();
            }
            Ok(())
        }
        _ => skip_value_by_id(decoder, id).map_err(EditError::from),
    }
}

fn resolve_list_targets_from_scan(scan: &ScanResult) -> HashSet<String> {
    let mut targets = HashSet::new();

    for path in &scan.matched_paths {
        for depth in (0..=path.len()).rev() {
            let candidate = path_segments_to_string(&path[..depth]);
            if scan.list_paths.contains(&candidate) {
                targets.insert(candidate);
                break;
            }
        }
    }

    targets
}

fn transform_raw(
    raw_input: &Path,
    raw_output: &Path,
    platform: PlatformType,
    plan: &mut Plan,
) -> Result<(), EditError> {
    let read_file = File::open(raw_input)?;
    let write_file = File::create(raw_output)?;
    let read = Box::new(BufReader::new(read_file)) as Box<dyn Read>;
    let write = Box::new(BufWriter::new(write_file)) as Box<dyn Write>;
    let mut decoder = Decoder::new(read, platform);
    let mut encoder = Encoder::new(write, platform);

    let root_id = decoder.read_id()?;
    let root_tag = decoder.read_tag()?;
    if root_id != tag_id::LIST && root_id != tag_id::COMPOUND {
        return Err(EditError::Parse(ParseError::InvalidRootTag(root_id)));
    }

    encoder.write_id(root_id)?;
    encoder.write_tag(&root_tag)?;

    let mut path = Vec::new();
    process_payload(
        &mut decoder,
        &mut encoder,
        root_id,
        &mut path,
        plan,
        platform,
    )?;
    encoder.flush()?;
    Ok(())
}

fn process_payload(
    decoder: &mut Decoder,
    encoder: &mut Encoder,
    id: u8,
    path: &mut Vec<PathSegment>,
    plan: &mut Plan,
    platform: PlatformType,
) -> Result<(), EditError> {
    if let Plan::Set(set) = plan
        && selector_matches(&set.selector, path)
    {
        if id == tag_id::LIST || id == tag_id::COMPOUND {
            let rendered = path_segments_to_string(path);
            return Err(EditError::InvalidInput(format!(
                "invalid value at path '{rendered}': only leaf values can be modified by --set"
            )));
        }

        let replacement = parse_set_value_for_id(id, &set.raw_value).map_err(|err| {
            let rendered = path_segments_to_string(path);
            EditError::InvalidInput(format!("invalid value at path '{rendered}': {err}"))
        })?;

        skip_value_by_id(decoder, id)?;
        write_value_payload(encoder, &replacement)?;
        set.hits += 1;
        return Ok(());
    }

    if let Plan::Create(create) = plan {
        if path == &create.segments {
            return Err(EditError::InvalidInput(format!(
                "path already exists: {}",
                path_segments_to_string(path)
            )));
        }

        if has_prefix(path, &create.segments) && path.len() < create.segments.len() {
            let next = &create.segments[path.len()];
            if id != tag_id::LIST && id != tag_id::COMPOUND {
                return Err(EditError::InvalidInput(create_mismatch_message(next)));
            }
        }
    }

    match id {
        tag_id::COMPOUND => process_compound(decoder, encoder, path, plan, platform),
        tag_id::LIST => process_list(decoder, encoder, path, plan, platform),
        _ => copy_leaf_payload(decoder, encoder, id).map_err(EditError::from),
    }
}

fn process_compound(
    decoder: &mut Decoder,
    encoder: &mut Encoder,
    path: &mut Vec<PathSegment>,
    plan: &mut Plan,
    platform: PlatformType,
) -> Result<(), EditError> {
    let create_expected_field = match plan {
        Plan::Create(create)
            if has_prefix(path, &create.segments) && path.len() < create.segments.len() =>
        {
            match &create.segments[path.len()] {
                PathSegment::Field(name) => Some(name.clone()),
                PathSegment::Index(index) => {
                    return Err(EditError::InvalidInput(format!(
                        "path expects list index [{index}]"
                    )));
                }
            }
        }
        _ => None,
    };
    let mut found_field = false;

    loop {
        let field_id = decoder.read_id()?;
        if field_id == tag_id::END {
            break;
        }
        let field_tag = decoder.read_tag()?;

        if let Some(expected) = &create_expected_field
            && expected == &field_tag
        {
            found_field = true;
        }

        path.push(PathSegment::Field(field_tag.clone()));
        let should_delete = match plan {
            Plan::Delete {
                base,
                where_plan: None,
            } => base.targets.contains(&path_segments_to_string(path)),
            _ => false,
        };

        if should_delete {
            if let Plan::Delete { base, .. } = plan {
                base.hits += 1;
            }
            skip_value_by_id(decoder, field_id)?;
            path.pop();
            continue;
        }

        encoder.write_id(field_id)?;
        encoder.write_tag(&field_tag)?;
        process_payload(decoder, encoder, field_id, path, plan, platform)?;
        path.pop();
    }

    if let Some(expected) = create_expected_field
        && !found_field
        && let Plan::Create(create) = plan
    {
        let tail = &create.segments[path.len() + 1..];
        let created =
            build_value_for_create_tail(tail, &create.value).map_err(EditError::InvalidInput)?;
        encoder.write_id(component_id(&created))?;
        encoder.write_tag(&expected)?;
        write_value_payload(encoder, &created)?;
        create.done = true;
    }

    encoder.write_id(tag_id::END)?;
    Ok(())
}

fn process_list(
    decoder: &mut Decoder,
    encoder: &mut Encoder,
    path: &mut Vec<PathSegment>,
    plan: &mut Plan,
    platform: PlatformType,
) -> Result<(), EditError> {
    let mut element_id = decoder.read_id()?;
    let length = decoder.read_int()?;
    if length < 0 {
        return Err(EditError::Parse(ParseError::InvalidLength(length)));
    }

    let list_key = path_segments_to_string(path);
    let where_targeted = matches!(
        plan,
        Plan::Delete {
            where_plan: Some(where_plan),
            ..
        } if where_plan.list_targets.contains(&list_key)
    );

    let create_index = match plan {
        Plan::Create(create)
            if has_prefix(path, &create.segments) && path.len() < create.segments.len() =>
        {
            match create.segments[path.len()] {
                PathSegment::Index(index) => Some(index),
                PathSegment::Field(ref name) => {
                    return Err(EditError::InvalidInput(format!(
                        "path expects compound field '{name}'"
                    )));
                }
            }
        }
        _ => None,
    };

    let payload_temp = make_temp_path_for("list", "payload");
    remove_file_if_exists(&payload_temp);
    let temp_write = Box::new(BufWriter::new(File::create(&payload_temp)?)) as Box<dyn Write>;
    let mut temp_encoder = Encoder::new(temp_write, platform);

    let mut output_len = 0i32;
    let mut deleted_any = false;
    for index in 0..length {
        path.push(PathSegment::Index(index as usize));

        let should_delete_by_path = match plan {
            Plan::Delete {
                base,
                where_plan: None,
            } => base.targets.contains(&path_segments_to_string(path)),
            _ => false,
        };

        if should_delete_by_path {
            if let Plan::Delete { base, .. } = plan {
                base.hits += 1;
            }
            skip_value_by_id(decoder, element_id)?;
            deleted_any = true;
            path.pop();
            continue;
        }

        if where_targeted && element_id == tag_id::COMPOUND {
            let value = read_value_by_id(decoder, element_id)?;
            let should_delete = match plan {
                Plan::Delete {
                    where_plan: Some(where_plan),
                    ..
                } => where_matches_all(&value, &where_plan.clauses),
                _ => false,
            };

            if should_delete {
                if let Plan::Delete {
                    where_plan: Some(where_plan),
                    ..
                } = plan
                {
                    where_plan.deleted += 1;
                }
                deleted_any = true;
                path.pop();
                continue;
            }

            write_value_payload(&mut temp_encoder, &value)?;
            output_len += 1;
            path.pop();
            continue;
        }

        process_payload(decoder, &mut temp_encoder, element_id, path, plan, platform)?;
        output_len += 1;
        path.pop();
    }

    if let Some(index) = create_index
        && let Plan::Create(create) = plan
    {
        if index > length as usize {
            return Err(EditError::InvalidInput(format!(
                "cannot create sparse list index: {index}"
            )));
        }
        if index == length as usize {
            let tail = &create.segments[path.len() + 1..];
            let created = build_value_for_create_tail(tail, &create.value)
                .map_err(EditError::InvalidInput)?;
            let expected_id = component_id(&created);

            if length == 0 {
                if element_id == tag_id::END {
                    element_id = expected_id;
                } else if element_id != expected_id {
                    return Err(EditError::InvalidInput(format!(
                        "list element type mismatch, expected id {element_id}, got {expected_id}"
                    )));
                }
            } else if element_id != expected_id {
                return Err(EditError::InvalidInput(format!(
                    "list element type mismatch, expected id {element_id}, got {expected_id}"
                )));
            }

            write_value_payload(&mut temp_encoder, &created)?;
            output_len += 1;
            create.done = true;
        }
    }

    temp_encoder.flush()?;

    let output_id = if deleted_any && output_len == 0 {
        tag_id::END
    } else {
        element_id
    };

    encoder.write_id(output_id)?;
    encoder.write_int(output_len)?;

    let mut payload_reader = BufReader::new(File::open(&payload_temp)?);
    let mut chunk = [0u8; 8192];
    loop {
        let n = payload_reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        encoder.write_raw(&chunk[..n])?;
    }

    remove_file_if_exists(&payload_temp);
    Ok(())
}

fn has_prefix(prefix: &[PathSegment], full: &[PathSegment]) -> bool {
    if prefix.len() > full.len() {
        return false;
    }
    prefix
        .iter()
        .zip(full.iter())
        .all(|(left, right)| left == right)
}

fn create_mismatch_message(segment: &PathSegment) -> String {
    match segment {
        PathSegment::Field(name) => format!("path expects compound field '{name}'"),
        PathSegment::Index(index) => format!("path expects list index [{index}]"),
    }
}

fn parse_set_value_for_id(id: u8, raw: &str) -> Result<NbtValue, String> {
    let placeholder = match id {
        tag_id::BYTE => NbtValue::Byte(0),
        tag_id::SHORT => NbtValue::Short(0),
        tag_id::INT => NbtValue::Int(0),
        tag_id::LONG => NbtValue::Long(0),
        tag_id::FLOAT => NbtValue::Float(0.0),
        tag_id::DOUBLE => NbtValue::Double(0.0),
        tag_id::BYTE_ARRAY => NbtValue::ByteArray(Vec::new()),
        tag_id::STRING => NbtValue::String(String::new()),
        tag_id::INT_ARRAY => NbtValue::IntArray(Vec::new()),
        tag_id::LONG_ARRAY => NbtValue::LongArray(Vec::new()),
        tag_id::LIST | tag_id::COMPOUND => {
            return Err("only leaf values can be modified by --set".to_string());
        }
        _ => return Err(format!("unsupported tag id: {id}")),
    };
    parse_value_like(&placeholder, raw)
}

fn infer_list_element_id(path_tail: &[PathSegment], value: &NbtValue) -> u8 {
    if path_tail.is_empty() {
        return component_id(value);
    }
    match path_tail[0] {
        PathSegment::Field(_) => tag_id::COMPOUND,
        PathSegment::Index(_) => tag_id::LIST,
    }
}

fn build_value_for_create_tail(
    path_tail: &[PathSegment],
    value: &NbtValue,
) -> Result<NbtValue, String> {
    if path_tail.is_empty() {
        return Ok(value.clone());
    }

    match &path_tail[0] {
        PathSegment::Field(name) => {
            let child = build_value_for_create_tail(&path_tail[1..], value)?;
            Ok(NbtValue::Compound(vec![(name.clone(), child)]))
        }
        PathSegment::Index(index) => {
            if *index != 0 {
                return Err(format!("cannot create sparse list index: {index}"));
            }
            let child = build_value_for_create_tail(&path_tail[1..], value)?;
            let id = infer_list_element_id(&path_tail[1..], &child);
            Ok(NbtValue::List {
                id,
                elements: vec![child],
            })
        }
    }
}

fn copy_leaf_payload(
    decoder: &mut Decoder,
    encoder: &mut Encoder,
    id: u8,
) -> Result<(), ParseError> {
    match id {
        tag_id::BYTE => encoder.write_byte(decoder.read_byte()?),
        tag_id::SHORT => encoder.write_short(decoder.read_short()?),
        tag_id::INT => encoder.write_int(decoder.read_int()?),
        tag_id::LONG => encoder.write_long(decoder.read_long()?),
        tag_id::FLOAT => encoder.write_float(decoder.read_float()?),
        tag_id::DOUBLE => encoder.write_double(decoder.read_double()?),
        tag_id::BYTE_ARRAY => encoder.write_byte_array(&decoder.read_byte_array()?),
        tag_id::STRING => encoder.write_string(&decoder.read_string()?),
        tag_id::INT_ARRAY => encoder.write_int_array(&decoder.read_int_array()?),
        tag_id::LONG_ARRAY => encoder.write_long_array(&decoder.read_long_array()?),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn skip_value_by_id(decoder: &mut Decoder, id: u8) -> Result<(), ParseError> {
    match id {
        tag_id::BYTE => {
            decoder.read_byte()?;
            Ok(())
        }
        tag_id::SHORT => {
            decoder.read_short()?;
            Ok(())
        }
        tag_id::INT => {
            decoder.read_int()?;
            Ok(())
        }
        tag_id::LONG => {
            decoder.read_long()?;
            Ok(())
        }
        tag_id::FLOAT => {
            decoder.read_float()?;
            Ok(())
        }
        tag_id::DOUBLE => {
            decoder.read_double()?;
            Ok(())
        }
        tag_id::BYTE_ARRAY => {
            decoder.read_byte_array()?;
            Ok(())
        }
        tag_id::STRING => {
            decoder.read_string()?;
            Ok(())
        }
        tag_id::LIST => {
            let element_id = decoder.read_id()?;
            let length = decoder.read_int()?;
            if length < 0 {
                return Err(ParseError::InvalidLength(length));
            }
            for _ in 0..length {
                skip_value_by_id(decoder, element_id)?;
            }
            Ok(())
        }
        tag_id::COMPOUND => {
            loop {
                let field_id = decoder.read_id()?;
                if field_id == tag_id::END {
                    break;
                }
                let _field_tag = decoder.read_tag()?;
                skip_value_by_id(decoder, field_id)?;
            }
            Ok(())
        }
        tag_id::INT_ARRAY => {
            decoder.read_int_array()?;
            Ok(())
        }
        tag_id::LONG_ARRAY => {
            decoder.read_long_array()?;
            Ok(())
        }
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn read_value_by_id(decoder: &mut Decoder, id: u8) -> Result<NbtValue, ParseError> {
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
                elements.push(read_value_by_id(decoder, list_id)?);
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
                let field_tag = decoder.read_tag()?;
                let value = read_value_by_id(decoder, field_id)?;
                fields.push((field_tag, value));
            }
            Ok(NbtValue::Compound(fields))
        }
        tag_id::INT_ARRAY => Ok(NbtValue::IntArray(decoder.read_int_array()?)),
        tag_id::LONG_ARRAY => Ok(NbtValue::LongArray(decoder.read_long_array()?)),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn write_value_payload(encoder: &mut Encoder, value: &NbtValue) -> Result<(), ParseError> {
    match value {
        NbtValue::Byte(v) => encoder.write_byte(*v),
        NbtValue::Short(v) => encoder.write_short(*v),
        NbtValue::Int(v) => encoder.write_int(*v),
        NbtValue::Long(v) => encoder.write_long(*v),
        NbtValue::Float(v) => encoder.write_float(*v),
        NbtValue::Double(v) => encoder.write_double(*v),
        NbtValue::ByteArray(v) => encoder.write_byte_array(v),
        NbtValue::String(v) => encoder.write_string(v),
        NbtValue::IntArray(v) => encoder.write_int_array(v),
        NbtValue::LongArray(v) => encoder.write_long_array(v),
        NbtValue::List { id, elements } => {
            encoder.write_id(*id)?;
            if elements.len() > i32::MAX as usize {
                return Err(ParseError::Other(format!(
                    "list length exceeds i32: {}",
                    elements.len()
                )));
            }
            encoder.write_int(elements.len() as i32)?;
            for element in elements {
                if component_id(element) != *id {
                    return Err(ParseError::Other(format!(
                        "list element type mismatch, expected id {id}, got {}",
                        component_id(element)
                    )));
                }
                write_value_payload(encoder, element)?;
            }
            Ok(())
        }
        NbtValue::Compound(fields) => {
            for (name, element) in fields {
                encoder.write_id(component_id(element))?;
                encoder.write_tag(name)?;
                write_value_payload(encoder, element)?;
            }
            encoder.write_id(tag_id::END)
        }
    }
}
