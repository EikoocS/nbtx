use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use cesu8::{from_java_cesu8, to_java_cesu8};
use std::io::{Read, Write};

use crate::decoder::NbtDecoder;
use crate::encoder::NbtEncoder;
use crate::error::ParseError;

pub struct JavaNbtDecoder {
    reader: Box<dyn Read>,
}

pub struct JavaNbtEncoder {
    writer: Box<dyn Write>,
}

impl JavaNbtDecoder {
    pub fn new(reader: Box<dyn Read>) -> Self {
        JavaNbtDecoder { reader }
    }
}

impl JavaNbtEncoder {
    pub fn new(writer: Box<dyn Write>) -> Self {
        JavaNbtEncoder { writer }
    }
}

impl NbtDecoder for JavaNbtDecoder {
    fn reader(&mut self) -> &mut Box<dyn Read> {
        &mut self.reader
    }

    fn read_tag_length(&mut self) -> Result<u16, ParseError> {
        Ok(self.reader.read_u16::<BigEndian>()?)
    }

    fn read_short(&mut self) -> Result<i16, ParseError> {
        Ok(self.reader.read_i16::<BigEndian>()?)
    }

    fn read_int(&mut self) -> Result<i32, ParseError> {
        Ok(self.reader.read_i32::<BigEndian>()?)
    }

    fn read_long(&mut self) -> Result<i64, ParseError> {
        Ok(self.reader.read_i64::<BigEndian>()?)
    }

    fn read_float(&mut self) -> Result<f32, ParseError> {
        Ok(self.reader.read_f32::<BigEndian>()?)
    }

    fn read_double(&mut self) -> Result<f64, ParseError> {
        Ok(self.reader.read_f64::<BigEndian>()?)
    }

    fn read_string_with_length(&mut self, length: i16) -> Result<String, ParseError> {
        if length < 0 {
            return Err(ParseError::InvalidLength(length as i32));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader.read_exact(buf.as_mut_slice())?;
        let decoded = from_java_cesu8(&buf).map_err(|err| ParseError::Decode(err.to_string()))?;
        Ok(decoded.to_string())
    }
}

impl NbtEncoder for JavaNbtEncoder {
    fn writer(&mut self) -> &mut Box<dyn Write> {
        &mut self.writer
    }

    fn write_tag_length(&mut self, length: u16) -> Result<(), ParseError> {
        self.writer.write_u16::<BigEndian>(length)?;
        Ok(())
    }

    fn write_short(&mut self, value: i16) -> Result<(), ParseError> {
        self.writer.write_i16::<BigEndian>(value)?;
        Ok(())
    }

    fn write_int(&mut self, value: i32) -> Result<(), ParseError> {
        self.writer.write_i32::<BigEndian>(value)?;
        Ok(())
    }

    fn write_long(&mut self, value: i64) -> Result<(), ParseError> {
        self.writer.write_i64::<BigEndian>(value)?;
        Ok(())
    }

    fn write_float(&mut self, value: f32) -> Result<(), ParseError> {
        self.writer.write_f32::<BigEndian>(value)?;
        Ok(())
    }

    fn write_double(&mut self, value: f64) -> Result<(), ParseError> {
        self.writer.write_f64::<BigEndian>(value)?;
        Ok(())
    }

    fn write_string_length(&mut self, length: i16) -> Result<(), ParseError> {
        self.writer.write_i16::<BigEndian>(length)?;
        Ok(())
    }

    fn write_string(&mut self, value: &str) -> Result<(), ParseError> {
        let encoded = to_java_cesu8(value);
        if encoded.len() > i16::MAX as usize {
            return Err(ParseError::Other(format!(
                "string length exceeds i16: {}",
                encoded.len()
            )));
        }
        self.write_string_length(encoded.len() as i16)?;
        self.writer.write_all(encoded.as_ref())?;
        Ok(())
    }
}
