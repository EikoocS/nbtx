use crate::bedrock::BedrockNbtDecoder;
use crate::error::ParseError;
use crate::java::JavaNbtDecoder;
use crate::platform::PlatformType;
use cesu8::from_java_cesu8;
use std::io::Read;

pub trait NbtDecoder {
    fn reader(&mut self) -> &mut Box<dyn Read>;
    fn read_id(&mut self) -> Result<u8, ParseError> {
        let mut buf = [0; 1];
        self.reader().read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn read_tag_length(&mut self) -> Result<u16, ParseError>;
    fn read_tag_with_length(&mut self, length: u16) -> Result<String, ParseError> {
        let mut buf = vec![0u8; length as usize];
        self.reader().read_exact(buf.as_mut_slice())?;
        let decoded = from_java_cesu8(&buf).map_err(|err| ParseError::Decode(err.to_string()))?;
        Ok(decoded.to_string())
    }
    fn read_tag(&mut self) -> Result<String, ParseError> {
        let length = self.read_tag_length()?;
        self.read_tag_with_length(length)
    }
    fn read_byte(&mut self) -> Result<i8, ParseError> {
        let mut buf = [0; 1];
        self.reader().read_exact(&mut buf)?;
        Ok(buf[0] as i8)
    }
    fn read_short(&mut self) -> Result<i16, ParseError>;
    fn read_int(&mut self) -> Result<i32, ParseError>;
    fn read_long(&mut self) -> Result<i64, ParseError>;
    fn read_float(&mut self) -> Result<f32, ParseError>;
    fn read_double(&mut self) -> Result<f64, ParseError>;
    fn read_byte_array_with_length(&mut self, length: i32) -> Result<Vec<u8>, ParseError> {
        if length < 0 {
            return Err(ParseError::InvalidLength(length));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader().read_exact(buf.as_mut_slice())?;
        Ok(buf)
    }
    fn read_byte_array(&mut self) -> Result<Vec<u8>, ParseError> {
        let length = self.read_int()?;
        self.read_byte_array_with_length(length)
    }
    fn read_string_with_length(&mut self, length: i16) -> Result<String, ParseError>;
    fn read_string(&mut self) -> Result<String, ParseError> {
        let length = self.read_short()?;
        self.read_string_with_length(length)
    }

    fn read_int_array(&mut self) -> Result<Vec<i32>, ParseError> {
        let length = self.read_int()?;
        if length < 0 {
            return Err(ParseError::InvalidLength(length));
        }
        let mut values: Vec<i32> = Vec::new();
        for _i in 0..length {
            values.push(self.read_int()?);
        }
        Ok(values)
    }

    fn read_long_array(&mut self) -> Result<Vec<i64>, ParseError> {
        let length = self.read_int()?;
        if length < 0 {
            return Err(ParseError::InvalidLength(length));
        }
        let mut values: Vec<i64> = Vec::new();
        for _i in 0..length {
            values.push(self.read_long()?);
        }
        Ok(values)
    }
}

pub fn build(reader: Box<dyn Read>, platform: PlatformType) -> Box<dyn NbtDecoder> {
    match platform {
        PlatformType::JavaEdition => Box::new(JavaNbtDecoder::new(reader)),
        PlatformType::BedrockEdition => Box::new(BedrockNbtDecoder::new(reader)),
    }
}
