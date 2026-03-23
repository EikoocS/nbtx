use crate::component::NbtComponent;
use crate::decoder::{build, NbtDecoder};
use crate::error::ParseError;
use crate::reader_content::*;
use crate::util::open_read_stream;
use crate::{tag_id, PlatformType};
use std::io::Read;

/// Streaming NBT reader that yields flattened leaf values.
///
/// Each call to [`Reader::next`] returns a `(path, value)` pair, where `path`
/// is a dotted path with list indices (for example `foo.bar[0].name`).
pub struct Reader {
    decoder: Box<dyn NbtDecoder>,
    stack: Vec<Box<dyn Content>>,
    next: Option<(String, NbtComponent)>,
}

impl Reader {
    /// Creates a reader from an input stream.
    ///
    /// Panics if initialization fails. Use [`Reader::try_new`] for fallible
    /// initialization.
    pub fn new(read: Box<dyn Read>, platform: PlatformType) -> Reader {
        Reader::try_new(read, platform).expect("failed to initialize nbt reader")
    }

    /// Creates a reader from an input stream.
    pub fn try_new(read: Box<dyn Read>, platform: PlatformType) -> Result<Reader, ParseError> {
        let decoder = build(read, platform);
        let stack = Vec::new();
        let mut reader = Reader {
            decoder,
            stack,
            next: None,
        };
        reader.init()?;
        Ok(reader)
    }

    /// Opens a file path and creates a reader.
    ///
    /// Panics if opening or initialization fails. Use
    /// [`Reader::try_new_with_path`] for fallible initialization.
    pub fn new_with_path(path: &str, platform: PlatformType) -> Reader {
        Reader::try_new_with_path(path, platform)
            .expect("failed to initialize nbt reader from path")
    }

    /// Opens a file path and creates a reader.
    pub fn try_new_with_path(path: &str, platform: PlatformType) -> Result<Reader, ParseError> {
        let reader = open_read_stream(path)?;
        Reader::try_new(reader, platform)
    }

    fn append_path_part(path: &mut String, part: &str) {
        if path.is_empty() || part.starts_with('[') {
            path.push_str(part);
            return;
        }

        path.push('.');
        path.push_str(part);
    }

    fn path(&self, tag: &str) -> String {
        let mut path = String::new();

        for content in self.stack.iter().skip(1) {
            Self::append_path_part(&mut path, content.format());
        }
        Self::append_path_part(&mut path, tag);

        path
    }

    fn push_nested_content(&mut self, tag: String, component: &NbtComponent) {
        match component {
            NbtComponent::Compound => {
                self.stack.push(Box::new(ComponentContent::new(tag)));
            }
            NbtComponent::List { id, length } if *length > 0 => {
                self.stack.push(Box::new(ListContent::new(tag, *id, *length)));
            }
            _ => {}
        }
    }

    /// Returns whether another leaf value is available.
    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }

    fn init(&mut self) -> Result<(), ParseError> {
        let id = self.decoder.read_id()?;
        let tag = self.decoder.read_tag()?;
        match id {
            tag_id::LIST => {
                let id = self.decoder.read_id()?;
                let length = self.decoder.read_int()?;
                if length > 0 {
                    self.stack.push(Box::new(ListContent::new(tag, id, length)));
                }
            }
            tag_id::COMPOUND => {
                self.stack.push(Box::new(ComponentContent::new(tag)));
            }
            _ => {
                return Err(ParseError::InvalidRootTag(id));
            }
        }

        self.advance_to_next_leaf()
    }

    /// Returns the next `(path, value)` pair in traversal order.
    ///
    /// Returns [`ParseError::UnexpectedEOF`] when the stream has been exhausted.
    pub fn next(&mut self) -> Result<(String, NbtComponent), ParseError> {
        let current = self.next.take().ok_or(ParseError::UnexpectedEOF)?;
        self.advance_to_next_leaf()?;
        Ok(current)
    }

    fn advance_to_next_leaf(&mut self) -> Result<(), ParseError> {
        self.next = None;

        loop {
            self.pop_until_has_next();

            let Some(content) = self.stack.last_mut() else {
                return Ok(());
            };

            let (tag, component) = content.next(&mut *self.decoder)?;

            match &component {
                NbtComponent::End => {
                    self.pop_until_has_next();
                }
                NbtComponent::Compound | NbtComponent::List { .. } => self.push_nested_content(tag, &component),
                _ => {
                    let path = self.path(&tag);
                    self.pop_until_has_next();
                    self.next = Some((path, component));
                    return Ok(());
                }
            }
        }
    }

    fn pop_until_has_next(&mut self) {
        while let Some(x) = self.stack.last() {
            if !x.has_next() {
                self.stack.pop();
            } else {
                break;
            }
        }
    }
}
