use regex::Regex;

#[derive(Clone, Debug)]
pub(super) enum NbtValue {
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
pub(super) enum CompressionType {
    None,
    Gzip,
    Zlib,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PathSegment {
    Field(String),
    Index(usize),
}

pub(super) enum PathSelector {
    Exact(Vec<PathSegment>),
    Regex(Regex),
}

#[derive(Clone)]
pub(super) enum WhereValue {
    Number(f64),
    Text(String),
}

#[derive(Copy, Clone)]
pub(super) enum WhereOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Regex,
}

pub(super) struct WhereClause {
    pub(super) field_path: Vec<String>,
    pub(super) op: WhereOp,
    pub(super) value: WhereValue,
    pub(super) value_regex: Option<Regex>,
}
