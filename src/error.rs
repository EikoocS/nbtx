use std::fmt::{Display, Formatter};

/// Errors that can happen while decoding an NBT stream.
#[derive(Debug)]
pub enum ParseError {
    /// The stream ended before a complete value could be decoded.
    UnexpectedEOF,
    /// A low-level I/O error from the underlying reader.
    Io(std::io::Error),
    /// Invalid string/number payload during decoding.
    Decode(String),
    /// Encountered an NBT tag ID not supported by this crate.
    UnsupportedTagId(u8),
    /// Encountered an invalid length in array/list data.
    InvalidLength(i32),
    /// Root tag is not list (`0x09`) or compound (`0x0a`).
    InvalidRootTag(u8),
    /// Other parsing error.
    Other(String),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEOF => write!(f, "unexpected end of nbt stream"),
            ParseError::Io(err) => write!(f, "io error: {err}"),
            ParseError::Decode(err) => write!(f, "decode error: {err}"),
            ParseError::UnsupportedTagId(id) => write!(f, "unsupported tag id: {id}"),
            ParseError::InvalidLength(length) => write!(f, "invalid nbt length: {length}"),
            ParseError::InvalidRootTag(id) => {
                write!(
                    f,
                    "root tag must be list(0x09) or compound(0x0a), got: {id}"
                )
            }
            ParseError::Other(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        ParseError::Io(value)
    }
}
