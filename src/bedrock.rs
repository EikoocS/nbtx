use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

use crate::decoder::NbtDecoder;
use crate::encoder::NbtEncoder;
use crate::error::ParseError;

pub struct BedrockNbtDecoder {
    reader: Box<dyn Read>,
}

pub struct BedrockNbtEncoder {
    writer: Box<dyn Write>,
}

impl BedrockNbtDecoder {
    pub fn new(reader: Box<dyn Read>) -> Self {
        BedrockNbtDecoder { reader }
    }
}

impl BedrockNbtEncoder {
    pub fn new(writer: Box<dyn Write>) -> Self {
        BedrockNbtEncoder { writer }
    }
}

impl NbtDecoder for BedrockNbtDecoder {
    fn reader(&mut self) -> &mut Box<dyn Read> {
        &mut self.reader
    }

    fn read_tag_length(&mut self) -> Result<u16, ParseError> {
        Ok(self.reader.read_u16::<LittleEndian>()?)
    }

    fn read_short(&mut self) -> Result<i16, ParseError> {
        Ok(self.reader.read_i16::<LittleEndian>()?)
    }

    fn read_int(&mut self) -> Result<i32, ParseError> {
        Ok(self.reader.read_i32::<LittleEndian>()?)
    }

    fn read_long(&mut self) -> Result<i64, ParseError> {
        Ok(self.reader.read_i64::<LittleEndian>()?)
    }

    fn read_float(&mut self) -> Result<f32, ParseError> {
        Ok(self.reader.read_f32::<LittleEndian>()?)
    }

    fn read_double(&mut self) -> Result<f64, ParseError> {
        Ok(self.reader.read_f64::<LittleEndian>()?)
    }

    fn read_string_with_length(&mut self, length: i16) -> Result<String, ParseError> {
        if length < 0 {
            return Err(ParseError::InvalidLength(length as i32));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader.read_exact(buf.as_mut_slice())?;
        let decoded = String::from_utf8(buf).map_err(|err| ParseError::Decode(err.to_string()))?;
        Ok(decoded)
    }
}

impl NbtEncoder for BedrockNbtEncoder {
    fn writer(&mut self) -> &mut Box<dyn Write> {
        &mut self.writer
    }

    fn write_tag_length(&mut self, length: u16) -> Result<(), ParseError> {
        self.writer.write_u16::<LittleEndian>(length)?;
        Ok(())
    }

    fn write_short(&mut self, value: i16) -> Result<(), ParseError> {
        self.writer.write_i16::<LittleEndian>(value)?;
        Ok(())
    }

    fn write_int(&mut self, value: i32) -> Result<(), ParseError> {
        self.writer.write_i32::<LittleEndian>(value)?;
        Ok(())
    }

    fn write_long(&mut self, value: i64) -> Result<(), ParseError> {
        self.writer.write_i64::<LittleEndian>(value)?;
        Ok(())
    }

    fn write_float(&mut self, value: f32) -> Result<(), ParseError> {
        self.writer.write_f32::<LittleEndian>(value)?;
        Ok(())
    }

    fn write_double(&mut self, value: f64) -> Result<(), ParseError> {
        self.writer.write_f64::<LittleEndian>(value)?;
        Ok(())
    }

    fn write_string_length(&mut self, length: i16) -> Result<(), ParseError> {
        self.writer.write_i16::<LittleEndian>(length)?;
        Ok(())
    }

    fn write_string(&mut self, value: &str) -> Result<(), ParseError> {
        if value.len() > i16::MAX as usize {
            return Err(ParseError::Other(format!(
                "string length exceeds i16: {}",
                value.len()
            )));
        }
        self.write_string_length(value.len() as i16)?;
        self.writer.write_all(value.as_bytes())?;
        Ok(())
    }
}
