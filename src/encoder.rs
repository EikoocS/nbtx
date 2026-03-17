use crate::bedrock::BedrockNbtEncoder;
use crate::error::ParseError;
use crate::java::JavaNbtEncoder;
use crate::platform::PlatformType;
use cesu8::to_java_cesu8;
use std::io::Write;

pub trait NbtEncoder {
    fn writer(&mut self) -> &mut Box<dyn Write>;

    fn write_id(&mut self, id: u8) -> Result<(), ParseError> {
        self.writer().write_all(&[id])?;
        Ok(())
    }

    fn write_tag_length(&mut self, length: u16) -> Result<(), ParseError>;

    fn write_tag(&mut self, tag: &str) -> Result<(), ParseError> {
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

    fn write_byte(&mut self, value: i8) -> Result<(), ParseError> {
        self.writer().write_all(&[value as u8])?;
        Ok(())
    }

    fn write_short(&mut self, value: i16) -> Result<(), ParseError>;
    fn write_int(&mut self, value: i32) -> Result<(), ParseError>;
    fn write_long(&mut self, value: i64) -> Result<(), ParseError>;
    fn write_float(&mut self, value: f32) -> Result<(), ParseError>;
    fn write_double(&mut self, value: f64) -> Result<(), ParseError>;

    fn write_byte_array(&mut self, values: &[u8]) -> Result<(), ParseError> {
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

    fn write_string_length(&mut self, length: i16) -> Result<(), ParseError>;

    fn write_string(&mut self, value: &str) -> Result<(), ParseError>;

    fn write_int_array(&mut self, values: &[i32]) -> Result<(), ParseError> {
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

    fn write_long_array(&mut self, values: &[i64]) -> Result<(), ParseError> {
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

    fn flush(&mut self) -> Result<(), ParseError> {
        self.writer().flush()?;
        Ok(())
    }
}

pub fn build(writer: Box<dyn Write>, platform: PlatformType) -> Box<dyn NbtEncoder> {
    match platform {
        PlatformType::JavaEdition => Box::new(JavaNbtEncoder::new(writer)),
        PlatformType::BedrockEdition => Box::new(BedrockNbtEncoder::new(writer)),
    }
}
