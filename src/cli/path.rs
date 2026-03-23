use crate::cli::model::{NbtValue, PathSegment, PathSelector};
use nbtx::tag_id;
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Clone)]
enum DeleteTail {
    Field(String),
    Index(usize),
}

pub(super) fn parse_path(path: &str) -> Result<Vec<PathSegment>, String> {
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

pub(super) fn parse_path_selector(path: &str) -> Result<PathSelector, String> {
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

pub(super) fn resolve_selector_paths(
    value: &NbtValue,
    selector: &PathSelector,
) -> Vec<Vec<PathSegment>> {
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

pub(super) fn normalize_delete_paths(paths: Vec<Vec<PathSegment>>) -> Vec<Vec<PathSegment>> {
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

pub(super) fn path_segments_to_string(segments: &[PathSegment]) -> String {
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

pub(super) fn delete_paths(
    value: &mut NbtValue,
    paths: Vec<Vec<PathSegment>>,
) -> Result<usize, String> {
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

pub(super) fn find_mut<'a>(
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

pub(super) fn find_ref<'a>(
    value: &'a NbtValue,
    segments: &[PathSegment],
) -> Result<&'a NbtValue, String> {
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
