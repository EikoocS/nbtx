use crate::cli::model::{NbtValue, PathSegment, WhereClause, WhereOp, WhereValue};
use crate::cli::path::find_ref;
use regex::Regex;

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

pub(super) fn parse_where_expr(raw: &str) -> Result<Vec<WhereClause>, String> {
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

pub(super) fn where_matches_all(value: &NbtValue, clauses: &[WhereClause]) -> bool {
    clauses
        .iter()
        .all(|clause| where_clause_matches(value, clause))
}

pub(super) fn resolve_list_targets_for_where(
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
