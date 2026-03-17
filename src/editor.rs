use crate::component::NbtComponent;
use crate::decoder::{build as build_decoder, NbtDecoder};
use crate::encoder::{build as build_encoder, NbtEncoder};
use crate::error::ParseError;
use crate::platform::PlatformType;
use crate::util::{open_read_stream, read_magic_number};
use flate2::Compression;
use std::io::{BufWriter, Write};

#[derive(Debug)]
enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List { id: u8, items: Vec<NbtValue> },
    Compound(Vec<(String, NbtValue)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Debug)]
enum PathToken {
    Field(String),
    Index(usize),
}

pub fn update_path_value(
    file_path: &str,
    target_path: &str,
    component: NbtComponent,
    platform: PlatformType,
) -> Result<(), ParseError> {
    let source = open_read_stream(file_path)?;
    let mut decoder = build_decoder(source, platform);

    let root_id = decoder.read_id()?;
    if root_id != 0x0a && root_id != 0x09 {
        return Err(ParseError::InvalidRootTag(root_id));
    }
    let root_tag = decoder.read_tag()?;
    let mut root = read_payload(root_id, &mut *decoder)?;

    let tokens = parse_path(target_path)?;
    if tokens.is_empty() {
        return Err(ParseError::Other("path cannot be empty".to_string()));
    }

    let replacement = from_component(component)?;
    let mut replaced = false;
    set_value(&mut root, &tokens, replacement, &mut replaced)?;
    if !replaced {
        return Err(ParseError::Other(format!("path not found: {target_path}")));
    }

    let mut encoder = build_encoder(
        open_write_stream_with_same_compression(file_path)?,
        platform,
    );
    encoder.write_id(root_id)?;
    encoder.write_tag(&root_tag)?;
    write_payload(&mut *encoder, &root)?;
    encoder.flush()?;

    commit_temporary_file(file_path)
}

fn parse_path(path: &str) -> Result<Vec<PathToken>, ParseError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut tokens: Vec<PathToken> = Vec::new();
    for segment in trimmed.split('.') {
        if segment.is_empty() {
            return Err(ParseError::Other(format!(
                "invalid path segment in '{path}'"
            )));
        }

        let mut rest = segment;
        if let Some(index_start) = rest.find('[') {
            if index_start > 0 {
                tokens.push(PathToken::Field(rest[..index_start].to_string()));
            }
            rest = &rest[index_start..];
        } else {
            tokens.push(PathToken::Field(rest.to_string()));
            continue;
        }

        while !rest.is_empty() {
            if !rest.starts_with('[') {
                return Err(ParseError::Other(format!(
                    "invalid path segment in '{path}'"
                )));
            }
            let close = rest
                .find(']')
                .ok_or_else(|| ParseError::Other(format!("invalid path segment in '{path}'")))?;
            let index_str = &rest[1..close];
            let index = index_str
                .parse::<usize>()
                .map_err(|_| ParseError::Other(format!("invalid list index '{index_str}'")))?;
            tokens.push(PathToken::Index(index));
            rest = &rest[close + 1..];
        }
    }
    Ok(tokens)
}

fn set_value(
    root: &mut NbtValue,
    tokens: &[PathToken],
    replacement: NbtValue,
    replaced: &mut bool,
) -> Result<(), ParseError> {
    if tokens.is_empty() {
        return Ok(());
    }

    let mut current = root;
    for token in &tokens[..tokens.len() - 1] {
        match token {
            PathToken::Field(name) => {
                let NbtValue::Compound(entries) = current else {
                    return Err(ParseError::Other(format!(
                        "cannot access field '{name}' on non-compound node"
                    )));
                };
                let (_, value) = entries
                    .iter_mut()
                    .find(|(field_name, _)| field_name == name)
                    .ok_or_else(|| {
                        ParseError::Other(format!("path not found: missing field '{name}'"))
                    })?;
                current = value;
            }
            PathToken::Index(index) => {
                let NbtValue::List { items, .. } = current else {
                    return Err(ParseError::Other(format!(
                        "cannot access index [{index}] on non-list node"
                    )));
                };
                current = items.get_mut(*index).ok_or_else(|| {
                    ParseError::Other(format!("path not found: index out of range [{index}]"))
                })?;
            }
        }
    }

    match &tokens[tokens.len() - 1] {
        PathToken::Field(name) => {
            let NbtValue::Compound(entries) = current else {
                return Err(ParseError::Other(format!(
                    "cannot write field '{name}' on non-compound node"
                )));
            };
            let (_, value) = entries
                .iter_mut()
                .find(|(field_name, _)| field_name == name)
                .ok_or_else(|| {
                    ParseError::Other(format!("path not found: missing field '{name}'"))
                })?;
            *value = replacement;
            *replaced = true;
        }
        PathToken::Index(index) => {
            let NbtValue::List { items, .. } = current else {
                return Err(ParseError::Other(format!(
                    "cannot write index [{index}] on non-list node"
                )));
            };
            let value = items.get_mut(*index).ok_or_else(|| {
                ParseError::Other(format!("path not found: index out of range [{index}]"))
            })?;
            *value = replacement;
            *replaced = true;
        }
    }

    Ok(())
}

fn from_component(component: NbtComponent) -> Result<NbtValue, ParseError> {
    let value = match component {
        NbtComponent::Byte(value) => NbtValue::Byte(value),
        NbtComponent::Short(value) => NbtValue::Short(value),
        NbtComponent::Int(value) => NbtValue::Int(value),
        NbtComponent::Long(value) => NbtValue::Long(value),
        NbtComponent::Float(value) => NbtValue::Float(value),
        NbtComponent::Double(value) => NbtValue::Double(value),
        NbtComponent::ByteArray(values) => NbtValue::ByteArray(values),
        NbtComponent::String(value) => NbtValue::String(value),
        NbtComponent::IntArray(values) => NbtValue::IntArray(values),
        NbtComponent::LongArray(values) => NbtValue::LongArray(values),
        NbtComponent::List { .. } | NbtComponent::Compound | NbtComponent::End => {
            return Err(ParseError::Other(
                "updating path only supports non-container values".to_string(),
            ));
        }
    };
    Ok(value)
}

fn read_payload(id: u8, decoder: &mut dyn NbtDecoder) -> Result<NbtValue, ParseError> {
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

            let mut items = Vec::with_capacity(length as usize);
            for _ in 0..length {
                items.push(read_payload(list_id, decoder)?);
            }

            Ok(NbtValue::List { id: list_id, items })
        }
        0x0a => {
            let mut entries: Vec<(String, NbtValue)> = Vec::new();
            loop {
                let child_id = decoder.read_id()?;
                if child_id == 0x00 {
                    break;
                }

                let name = decoder.read_tag()?;
                let value = read_payload(child_id, decoder)?;
                entries.push((name, value));
            }
            Ok(NbtValue::Compound(entries))
        }
        0x0b => Ok(NbtValue::IntArray(decoder.read_int_array()?)),
        0x0c => Ok(NbtValue::LongArray(decoder.read_long_array()?)),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}

fn value_id(value: &NbtValue) -> u8 {
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

fn write_payload(encoder: &mut dyn NbtEncoder, value: &NbtValue) -> Result<(), ParseError> {
    match value {
        NbtValue::Byte(value) => encoder.write_byte(*value),
        NbtValue::Short(value) => encoder.write_short(*value),
        NbtValue::Int(value) => encoder.write_int(*value),
        NbtValue::Long(value) => encoder.write_long(*value),
        NbtValue::Float(value) => encoder.write_float(*value),
        NbtValue::Double(value) => encoder.write_double(*value),
        NbtValue::ByteArray(values) => encoder.write_byte_array(values),
        NbtValue::String(value) => encoder.write_string(value),
        NbtValue::List { id, items } => {
            encoder.write_id(*id)?;
            if items.len() > i32::MAX as usize {
                return Err(ParseError::Other(format!(
                    "list length exceeds i32: {}",
                    items.len()
                )));
            }
            encoder.write_int(items.len() as i32)?;

            for item in items {
                let item_id = value_id(item);
                if item_id != *id {
                    return Err(ParseError::Other(format!(
                        "list element type mismatch, expected id {id}, got {item_id}"
                    )));
                }
                write_payload(encoder, item)?;
            }

            Ok(())
        }
        NbtValue::Compound(entries) => {
            for (name, child) in entries {
                encoder.write_id(value_id(child))?;
                encoder.write_tag(name)?;
                write_payload(encoder, child)?;
            }
            encoder.write_id(0x00)
        }
        NbtValue::IntArray(values) => encoder.write_int_array(values),
        NbtValue::LongArray(values) => encoder.write_long_array(values),
    }
}

fn temp_output_path(file_path: &str) -> String {
    format!("{file_path}.nbtx.tmp")
}

fn open_write_stream_with_same_compression(file_path: &str) -> Result<Box<dyn Write>, ParseError> {
    let compression_header = read_magic_number(file_path)?;
    let output = std::fs::File::create(temp_output_path(file_path))?;
    let writer = BufWriter::new(output);

    let boxed: Box<dyn Write> = match compression_header {
        [0x1F, 0x8B] => Box::new(flate2::write::GzEncoder::new(
            writer,
            Compression::default(),
        )),
        [0x78, 0x9C] | [0x78, 0x01] | [0x78, 0xDA] => Box::new(flate2::write::ZlibEncoder::new(
            writer,
            Compression::default(),
        )),
        _ => Box::new(writer),
    };

    Ok(boxed)
}

fn commit_temporary_file(file_path: &str) -> Result<(), ParseError> {
    let temp_path = temp_output_path(file_path);
    std::fs::remove_file(file_path)?;
    std::fs::rename(temp_path, file_path)?;
    Ok(())
}
