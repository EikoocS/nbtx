use crate::error::ParseError;
use std::io::Read;

pub(crate) fn open_read_stream(path: &str) -> Result<Box<dyn Read>, ParseError> {
    let magic_number = read_magic_number(path)?;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    match magic_number {
        [0x1F, 0x8B] => {
            // GZIP format
            Ok(Box::new(flate2::read::GzDecoder::new(reader)))
        }
        [0x78, 0x9C] | [0x78, 0x01] | [0x78, 0xDA] => {
            // ZLIB format
            Ok(Box::new(flate2::read::ZlibDecoder::new(reader)))
        }
        _ => Ok(Box::new(reader)),
    }
}

pub(crate) fn read_magic_number(path: &str) -> Result<[u8; 2], ParseError> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}
