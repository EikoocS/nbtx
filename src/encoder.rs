use crate::error::ParseError;
use crate::platform::PlatformType;
use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use cesu8::to_java_cesu8;
use std::io::Write;

pub enum Encoder {
    Java { writer: Box<dyn Write> },
    Bedrock { writer: Box<dyn Write> },
}

impl Encoder {
    pub fn new(writer: Box<dyn Write>, platform: PlatformType) -> Self {
        match platform {
            PlatformType::JavaEdition => Self::Java { writer },
            PlatformType::BedrockEdition => Self::Bedrock { writer },
        }
    }

    fn writer(&mut self) -> &mut dyn Write {
        match self {
            Self::Java { writer } | Self::Bedrock { writer } => writer.as_mut(),
        }
    }

    pub fn write_id(&mut self, id: u8) -> Result<(), ParseError> {
        self.writer().write_all(&[id])?;
        Ok(())
    }

    pub fn write_tag_length(&mut self, length: u16) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_u16::<BigEndian>(length)?,
            Self::Bedrock { writer } => writer.write_u16::<LittleEndian>(length)?,
        }
        Ok(())
    }

    pub fn write_tag(&mut self, tag: &str) -> Result<(), ParseError> {
        let encoded = to_java_cesu8(tag);
        let length = encoded.len();
        if length > u16::MAX as usize {
            return Err(ParseError::Other(format!(
                "tag length exceeds u16: {length}"
            )));
        }
        self.write_tag_length(length as u16)?;
        self.writer().write_all(encoded.as_ref())?;
        Ok(())
    }

    pub fn write_byte(&mut self, value: i8) -> Result<(), ParseError> {
        self.writer().write_all(&[value as u8])?;
        Ok(())
    }

    pub fn write_short(&mut self, value: i16) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_i16::<BigEndian>(value)?,
            Self::Bedrock { writer } => writer.write_i16::<LittleEndian>(value)?,
        }
        Ok(())
    }

    pub fn write_int(&mut self, value: i32) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_i32::<BigEndian>(value)?,
            Self::Bedrock { writer } => writer.write_i32::<LittleEndian>(value)?,
        }
        Ok(())
    }

    pub fn write_long(&mut self, value: i64) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_i64::<BigEndian>(value)?,
            Self::Bedrock { writer } => writer.write_i64::<LittleEndian>(value)?,
        }
        Ok(())
    }

    pub fn write_float(&mut self, value: f32) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_f32::<BigEndian>(value)?,
            Self::Bedrock { writer } => writer.write_f32::<LittleEndian>(value)?,
        }
        Ok(())
    }

    pub fn write_double(&mut self, value: f64) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_f64::<BigEndian>(value)?,
            Self::Bedrock { writer } => writer.write_f64::<LittleEndian>(value)?,
        }
        Ok(())
    }

    pub fn write_byte_array(&mut self, values: &[u8]) -> Result<(), ParseError> {
        if values.len() > i32::MAX as usize {
            return Err(ParseError::Other(format!(
                "byte array length exceeds i32: {}",
                values.len()
            )));
        }
        self.write_int(values.len() as i32)?;
        self.writer().write_all(values)?;
        Ok(())
    }

    pub fn write_string_length(&mut self, length: i16) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => writer.write_i16::<BigEndian>(length)?,
            Self::Bedrock { writer } => writer.write_i16::<LittleEndian>(length)?,
        }
        Ok(())
    }

    pub fn write_string(&mut self, value: &str) -> Result<(), ParseError> {
        match self {
            Self::Java { writer } => {
                let encoded = to_java_cesu8(value);
                if encoded.len() > i16::MAX as usize {
                    return Err(ParseError::Other(format!(
                        "string length exceeds i16: {}",
                        encoded.len()
                    )));
                }
                writer.write_i16::<BigEndian>(encoded.len() as i16)?;
                writer.write_all(encoded.as_ref())?;
            }
            Self::Bedrock { writer } => {
                if value.len() > i16::MAX as usize {
                    return Err(ParseError::Other(format!(
                        "string length exceeds i16: {}",
                        value.len()
                    )));
                }
                writer.write_i16::<LittleEndian>(value.len() as i16)?;
                writer.write_all(value.as_bytes())?;
            }
        }
        Ok(())
    }

    pub fn write_int_array(&mut self, values: &[i32]) -> Result<(), ParseError> {
        if values.len() > i32::MAX as usize {
            return Err(ParseError::Other(format!(
                "int array length exceeds i32: {}",
                values.len()
            )));
        }
        self.write_int(values.len() as i32)?;
        for value in values {
            self.write_int(*value)?;
        }
        Ok(())
    }

    pub fn write_long_array(&mut self, values: &[i64]) -> Result<(), ParseError> {
        if values.len() > i32::MAX as usize {
            return Err(ParseError::Other(format!(
                "long array length exceeds i32: {}",
                values.len()
            )));
        }
        self.write_int(values.len() as i32)?;
        for value in values {
            self.write_long(*value)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), ParseError> {
        self.writer().flush()?;
        Ok(())
    }
}

pub fn build(writer: Box<dyn Write>, platform: PlatformType) -> Encoder {
    Encoder::new(writer, platform)
}
