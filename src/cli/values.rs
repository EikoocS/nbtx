use crate::cli::codec::component_id;
use crate::cli::model::{NbtValue, PathSegment};
use nbtx::tag_id;

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

pub(super) fn parse_value_like(target: &NbtValue, raw: &str) -> Result<NbtValue, String> {
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

pub(super) fn parse_value_for_create(raw: &str) -> Result<NbtValue, String> {
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

pub(super) fn create_at_path(
    value: &mut NbtValue,
    segments: &[PathSegment],
    new_value: NbtValue,
) -> Result<(), String> {
    create_path_recursive(value, segments, &new_value)
}
