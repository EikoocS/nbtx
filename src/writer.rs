use crate::component::NbtComponent;
use crate::encoder::{build, NbtEncoder};
use crate::error::ParseError;
use crate::PlatformType;
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
                encoder.write_id(0x0a)?;
                encoder.write_tag("")?;
                stack.push(Scope::Compound);
            }
            RootType::List { id, length } => {
                if length < 0 {
                    return Err(ParseError::InvalidLength(length));
                }
                encoder.write_id(0x09)?;
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

    fn pop_completed_lists(&mut self) {
        while matches!(self.stack.last(), Some(Scope::List { remaining: 0, .. })) {
            self.stack.pop();
        }
    }

    fn component_id(component: &NbtComponent) -> Result<u8, ParseError> {
        let id = match component {
            NbtComponent::End => {
                return Err(ParseError::Other(
                    "TAG_End cannot be written as a value".to_string(),
                ));
            }
            NbtComponent::Byte(_) => 0x01,
            NbtComponent::Short(_) => 0x02,
            NbtComponent::Int(_) => 0x03,
            NbtComponent::Long(_) => 0x04,
            NbtComponent::Float(_) => 0x05,
            NbtComponent::Double(_) => 0x06,
            NbtComponent::ByteArray(_) => 0x07,
            NbtComponent::String(_) => 0x08,
            NbtComponent::List { .. } => 0x09,
            NbtComponent::Compound => 0x0a,
            NbtComponent::IntArray(_) => 0x0b,
            NbtComponent::LongArray(_) => 0x0c,
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
            NbtComponent::End => Err(ParseError::Other(
                "TAG_End cannot be written as a value".to_string(),
            )),
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
            .ok_or_else(|| ParseError::Other("writer is already finished".to_string()))?;

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

        let scope = self
            .stack
            .pop()
            .ok_or_else(|| ParseError::Other("writer is already finished".to_string()))?;

        match scope {
            Scope::Compound => {
                self.encoder.write_id(0x00)?;
                self.pop_completed_lists();
                Ok(())
            }
            Scope::List { .. } => Err(ParseError::Other(
                "cannot manually end a list scope".to_string(),
            )),
        }
    }

    /// Returns whether writing has completed.
    pub fn is_finished(&mut self) -> bool {
        self.pop_completed_lists();
        self.stack.is_empty()
    }

    /// Flushes and validates the stream is complete.
    pub fn finish(&mut self) -> Result<(), ParseError> {
        self.pop_completed_lists();
        if !self.stack.is_empty() {
            return Err(ParseError::Other(
                "unfinished nbt document: open containers remain".to_string(),
            ));
        }
        self.encoder.flush()
    }
}
