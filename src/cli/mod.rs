mod args;
mod codec;
mod model;
mod path;
#[cfg(test)]
mod tests;
mod values;
mod where_expr;

use args::Args;
use clap::Parser;
use codec::{
    detect_compression, format_component, is_descendant_path, parse_document, write_document,
};
use model::{CompressionType, NbtValue};
use nbtx::{PlatformType, Reader, tag_id};
use path::{
    delete_paths, find_mut, normalize_delete_paths, parse_path, parse_path_selector,
    path_segments_to_string, resolve_selector_paths,
};
use std::io::{Error, ErrorKind};
use std::path::Path;
use values::{create_at_path, parse_value_for_create, parse_value_like};
use where_expr::{parse_where_expr, resolve_list_targets_for_where, where_matches_all};

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

fn load_document(
    args: &Args,
    platform: PlatformType,
) -> std::io::Result<(CompressionType, NbtValue)> {
    let compression = detect_compression(&args.file)?;
    let document = parse_document(&args.file, platform)
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    Ok((compression, document))
}

fn run_set(args: &Args, raw_value: &str, platform: PlatformType) -> std::io::Result<()> {
    let (compression, mut document) = load_document(args, platform)?;

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

    let (compression, mut document) = load_document(args, platform)?;
    let segments = parse_path(&args.path)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("invalid path: {err}")))?;
    let new_value = parse_value_for_create(raw_value)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;

    create_at_path(&mut document, &segments, new_value)
        .map_err(|err| Error::new(ErrorKind::InvalidInput, err))?;

    persist_document(args, &document, platform, compression)
}

fn run_delete(args: &Args, platform: PlatformType) -> std::io::Result<()> {
    let (compression, mut document) = load_document(args, platform)?;

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
