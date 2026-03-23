use crate::component::NbtComponent;
use crate::encoder::{NbtEncoder, build};
use crate::error::ParseError;
use crate::{PlatformType, tag_id};
use std::io::Write;

/// Root type used to initialize a streaming NBT writer.
pub enum RootType {
    /// Root is a compound (`0x0a`) with an empty name.
    Compound,
    /// Root is a list (`0x09`) with an empty name.
    List { id: u8, length: i32 },
}

enum Scope {
    Compound,
    List { id: u8, remaining: i32 },
}

/// Streaming NBT writer.
///
/// The writer emits a valid NBT document incrementally while maintaining
/// container state internally.
pub struct Writer {
    encoder: Box<dyn NbtEncoder>,
    stack: Vec<Scope>,
}

impl Writer {
    /// Creates a writer for the given root container.
    ///
    /// Panics when initialization fails. Use [`Writer::try_new`] for a
    /// fallible constructor.
    pub fn new(write: Box<dyn Write>, platform: PlatformType, root: RootType) -> Writer {
        Writer::try_new(write, platform, root).expect("failed to initialize nbt writer")
    }

    /// Creates a writer for the given root container.
    pub fn try_new(
        write: Box<dyn Write>,
        platform: PlatformType,
        root: RootType,
    ) -> Result<Writer, ParseError> {
        let mut encoder = build(write, platform);
        let mut stack = Vec::new();

        match root {
            RootType::Compound => {
                encoder.write_id(tag_id::COMPOUND)?;
                encoder.write_tag("")?;
                stack.push(Scope::Compound);
            }
            RootType::List { id, length } => {
                if length < 0 {
                    return Err(ParseError::InvalidLength(length));
                }
                encoder.write_id(tag_id::LIST)?;
                encoder.write_tag("")?;
                encoder.write_id(id)?;
                encoder.write_int(length)?;
                if length > 0 {
                    stack.push(Scope::List {
                        id,
                        remaining: length,
                    });
                }
            }
        }

        Ok(Writer { encoder, stack })
    }

    fn writer_finished_error() -> ParseError {
        ParseError::Other("writer is already finished".to_string())
    }

    fn invalid_end_value_error() -> ParseError {
        ParseError::Other("TAG_End cannot be written as a value".to_string())
    }

    fn pop_completed_lists(&mut self) {
        while matches!(self.stack.last(), Some(Scope::List { remaining: 0, .. })) {
            self.stack.pop();
        }
    }

    fn component_id(component: &NbtComponent) -> Result<u8, ParseError> {
        let id = match component {
            NbtComponent::End => return Err(Self::invalid_end_value_error()),
            NbtComponent::Byte(_) => tag_id::BYTE,
            NbtComponent::Short(_) => tag_id::SHORT,
            NbtComponent::Int(_) => tag_id::INT,
            NbtComponent::Long(_) => tag_id::LONG,
            NbtComponent::Float(_) => tag_id::FLOAT,
            NbtComponent::Double(_) => tag_id::DOUBLE,
            NbtComponent::ByteArray(_) => tag_id::BYTE_ARRAY,
            NbtComponent::String(_) => tag_id::STRING,
            NbtComponent::List { .. } => tag_id::LIST,
            NbtComponent::Compound => tag_id::COMPOUND,
            NbtComponent::IntArray(_) => tag_id::INT_ARRAY,
            NbtComponent::LongArray(_) => tag_id::LONG_ARRAY,
        };
        Ok(id)
    }

    fn write_payload(&mut self, component: &NbtComponent) -> Result<(), ParseError> {
        match component {
            NbtComponent::Byte(value) => self.encoder.write_byte(*value),
            NbtComponent::Short(value) => self.encoder.write_short(*value),
            NbtComponent::Int(value) => self.encoder.write_int(*value),
            NbtComponent::Long(value) => self.encoder.write_long(*value),
            NbtComponent::Float(value) => self.encoder.write_float(*value),
            NbtComponent::Double(value) => self.encoder.write_double(*value),
            NbtComponent::ByteArray(values) => self.encoder.write_byte_array(values),
            NbtComponent::String(value) => self.encoder.write_string(value),
            NbtComponent::IntArray(values) => self.encoder.write_int_array(values),
            NbtComponent::LongArray(values) => self.encoder.write_long_array(values),
            NbtComponent::List { id, length } => {
                if *length < 0 {
                    return Err(ParseError::InvalidLength(*length));
                }
                self.encoder.write_id(*id)?;
                self.encoder.write_int(*length)?;
                if *length > 0 {
                    self.stack.push(Scope::List {
                        id: *id,
                        remaining: *length,
                    });
                }
                Ok(())
            }
            NbtComponent::Compound => {
                self.stack.push(Scope::Compound);
                Ok(())
            }
            NbtComponent::End => Err(Self::invalid_end_value_error()),
        }
    }

    /// Writes one value into the current container.
    ///
    /// - In a compound scope, `tag` is emitted as the field name.
    /// - In a list scope, `tag` must be empty and element type must match.
    pub fn write(&mut self, tag: &str, component: NbtComponent) -> Result<(), ParseError> {
        self.pop_completed_lists();

        let component_id = Self::component_id(&component)?;
        let scope = self
            .stack
            .last_mut()
            .ok_or_else(Self::writer_finished_error)?;

        match scope {
            Scope::Compound => {
                self.encoder.write_id(component_id)?;
                self.encoder.write_tag(tag)?;
            }
            Scope::List { id, remaining } => {
                if !tag.is_empty() {
                    return Err(ParseError::Other(
                        "list elements cannot have tags".to_string(),
                    ));
                }
                if *remaining <= 0 {
                    return Err(ParseError::Other("list is already full".to_string()));
                }
                if *id != component_id {
                    return Err(ParseError::Other(format!(
                        "list element type mismatch, expected id {id}, got {component_id}"
                    )));
                }
                *remaining -= 1;
            }
        }

        self.write_payload(&component)?;
        self.pop_completed_lists();
        Ok(())
    }

    /// Closes the current compound by writing `TAG_End`.
    pub fn end(&mut self) -> Result<(), ParseError> {
        self.pop_completed_lists();

        let scope = self.stack.pop().ok_or_else(Self::writer_finished_error)?;

        match scope {
            Scope::Compound => {
                self.encoder.write_id(tag_id::END)?;
                self.pop_completed_lists();
                Ok(())
            }
            Scope::List { .. } => Err(ParseError::Other(
                "cannot manually end a list scope".to_string(),
            )),
        }
    }

    /// Returns whether writing has completed.
    pub fn is_finished(&self) -> bool {
        !self
            .stack
            .iter()
            .any(|scope| !matches!(scope, Scope::List { remaining: 0, .. }))
    }

    /// Flushes and validates the stream is complete.
    pub fn finish(&mut self) -> Result<(), ParseError> {
        if !self.is_finished() {
            return Err(ParseError::Other(
                "unfinished nbt document: open containers remain".to_string(),
            ));
        }
        self.encoder.flush()
    }
}
