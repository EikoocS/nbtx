use crate::error::ParseError;
use std::io::Read;

const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
const ZLIB_MAGIC_1: [u8; 2] = [0x78, 0x9C];
const ZLIB_MAGIC_2: [u8; 2] = [0x78, 0x01];
const ZLIB_MAGIC_3: [u8; 2] = [0x78, 0xDA];

fn is_gzip_magic(magic: [u8; 2]) -> bool {
    magic == GZIP_MAGIC
}

fn is_zlib_magic(magic: [u8; 2]) -> bool {
    matches!(magic, ZLIB_MAGIC_1 | ZLIB_MAGIC_2 | ZLIB_MAGIC_3)
}

pub(crate) fn open_read_stream(path: &str) -> Result<Box<dyn Read>, ParseError> {
    let magic_number = read_magic_number(path)?;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    if is_gzip_magic(magic_number) {
        return Ok(Box::new(flate2::read::GzDecoder::new(reader)));
    }

    if is_zlib_magic(magic_number) {
        return Ok(Box::new(flate2::read::ZlibDecoder::new(reader)));
    }

    Ok(Box::new(reader))
}

pub(crate) fn read_magic_number(path: &str) -> Result<[u8; 2], ParseError> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}
