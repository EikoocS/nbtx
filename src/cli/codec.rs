use crate::cli::model::{CompressionType, NbtValue};
use flate2::Compression as FlateCompression;
use nbtx::decoder::{NbtDecoder, build as build_decoder};
use nbtx::encoder::{NbtEncoder, build as build_encoder};
use nbtx::{NbtComponent, ParseError, PlatformType, tag_id};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

pub(super) fn is_descendant_path(path: &str, target: &str) -> bool {
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

pub(super) fn format_component(component: &NbtComponent, show_type: bool) -> String {
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

pub(super) fn detect_compression(path: &str) -> std::io::Result<CompressionType> {
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

pub(super) fn parse_document(path: &str, platform: PlatformType) -> Result<NbtValue, ParseError> {
    let read = open_read_stream(path)?;
    let mut decoder = build_decoder(read, platform);

    let root_id = decoder.read_id()?;
    let _root_tag = decoder.read_tag()?;
    match root_id {
        tag_id::LIST | tag_id::COMPOUND => parse_value_by_id(root_id, &mut *decoder),
        _ => Err(ParseError::InvalidRootTag(root_id)),
    }
}

pub(super) fn component_id(value: &NbtValue) -> u8 {
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
        NbtValue::List { id, elements } => write_list_payload(encoder, *id, elements)?,
        NbtValue::Compound(fields) => {
            for (name, value) in fields {
                write_value(encoder, name, value, false)?;
            }
            encoder.write_id(tag_id::END)?;
        }
    }

    Ok(())
}

fn write_list_payload(
    encoder: &mut dyn NbtEncoder,
    id: u8,
    elements: &[NbtValue],
) -> Result<(), ParseError> {
    if elements.len() > i32::MAX as usize {
        return Err(ParseError::Other(format!(
            "list length exceeds i32: {}",
            elements.len()
        )));
    }

    encoder.write_id(id)?;
    encoder.write_int(elements.len() as i32)?;

    for element in elements {
        let element_id = component_id(element);
        if element_id != id {
            return Err(ParseError::Other(format!(
                "list element type mismatch, expected id {id}, got {element_id}"
            )));
        }
        write_value(encoder, "", element, true)?;
    }

    Ok(())
}

pub(super) fn write_document(
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
            encoder.write_id(tag_id::LIST)?;
            encoder.write_tag("")?;
            write_list_payload(&mut *encoder, *id, elements)?;
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
