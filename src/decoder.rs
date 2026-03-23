use crate::error::ParseError;
use crate::platform::PlatformType;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use cesu8::from_java_cesu8;
use std::io::Read;

pub enum Decoder {
    Java { reader: Box<dyn Read> },
    Bedrock { reader: Box<dyn Read> },
}

impl Decoder {
    pub fn new(reader: Box<dyn Read>, platform: PlatformType) -> Self {
        match platform {
            PlatformType::JavaEdition => Self::Java { reader },
            PlatformType::BedrockEdition => Self::Bedrock { reader },
        }
    }

    fn reader(&mut self) -> &mut dyn Read {
        match self {
            Self::Java { reader } | Self::Bedrock { reader } => reader.as_mut(),
        }
    }

    pub fn read_id(&mut self) -> Result<u8, ParseError> {
        let mut buf = [0; 1];
        self.reader().read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_tag_length(&mut self) -> Result<u16, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_u16::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_u16::<LittleEndian>()?),
        }
    }

    pub fn read_tag_with_length(&mut self, length: u16) -> Result<String, ParseError> {
        let mut buf = vec![0u8; length as usize];
        self.reader().read_exact(buf.as_mut_slice())?;
        let decoded = from_java_cesu8(&buf).map_err(|err| ParseError::Decode(err.to_string()))?;
        Ok(decoded.to_string())
    }

    pub fn read_tag(&mut self) -> Result<String, ParseError> {
        let length = self.read_tag_length()?;
        self.read_tag_with_length(length)
    }

    pub fn read_byte(&mut self) -> Result<i8, ParseError> {
        let mut buf = [0; 1];
        self.reader().read_exact(&mut buf)?;
        Ok(buf[0] as i8)
    }

    pub fn read_short(&mut self) -> Result<i16, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_i16::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_i16::<LittleEndian>()?),
        }
    }

    pub fn read_int(&mut self) -> Result<i32, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_i32::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_i32::<LittleEndian>()?),
        }
    }

    pub fn read_long(&mut self) -> Result<i64, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_i64::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_i64::<LittleEndian>()?),
        }
    }

    pub fn read_float(&mut self) -> Result<f32, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_f32::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_f32::<LittleEndian>()?),
        }
    }

    pub fn read_double(&mut self) -> Result<f64, ParseError> {
        match self {
            Self::Java { reader } => Ok(reader.read_f64::<BigEndian>()?),
            Self::Bedrock { reader } => Ok(reader.read_f64::<LittleEndian>()?),
        }
    }

    pub fn read_byte_array_with_length(&mut self, length: i32) -> Result<Vec<u8>, ParseError> {
        if length < 0 {
            return Err(ParseError::InvalidLength(length));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader().read_exact(buf.as_mut_slice())?;
        Ok(buf)
    }

    pub fn read_byte_array(&mut self) -> Result<Vec<u8>, ParseError> {
        let length = self.read_int()?;
        self.read_byte_array_with_length(length)
    }

    pub fn read_string_with_length(&mut self, length: i16) -> Result<String, ParseError> {
        if length < 0 {
            return Err(ParseError::InvalidLength(length as i32));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader().read_exact(buf.as_mut_slice())?;
        match self {
            Self::Java { .. } => {
                let decoded =
                    from_java_cesu8(&buf).map_err(|err| ParseError::Decode(err.to_string()))?;
                Ok(decoded.to_string())
            }
            Self::Bedrock { .. } => {
                let decoded =
                    String::from_utf8(buf).map_err(|err| ParseError::Decode(err.to_string()))?;
                Ok(decoded)
            }
        }
    }

    pub fn read_string(&mut self) -> Result<String, ParseError> {
        let length = self.read_short()?;
        self.read_string_with_length(length)
    }

    pub fn read_int_array(&mut self) -> Result<Vec<i32>, ParseError> {
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

    pub fn read_long_array(&mut self) -> Result<Vec<i64>, ParseError> {
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

pub fn build(reader: Box<dyn Read>, platform: PlatformType) -> Decoder {
    Decoder::new(reader, platform)
}
