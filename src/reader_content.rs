use crate::decoder::NbtDecoder;
use crate::{NbtComponent, ParseError};

pub trait Content {
    fn next(&mut self, decoder: &mut dyn NbtDecoder) -> Result<(String, NbtComponent), ParseError>;
    fn has_next(&self) -> bool;
    fn format(&self) -> String;
}

pub struct ListContent {
    tag: String,
    id: u8,
    index: i32,
    length: i32,
}

impl ListContent {
    pub(crate) fn new(tag: String, id: u8, length: i32) -> ListContent {
        ListContent {
            tag,
            id,
            index: 0,
            length,
        }
    }
}

impl Content for ListContent {
    fn next(&mut self, decoder: &mut dyn NbtDecoder) -> Result<(String, NbtComponent), ParseError> {
        let component = next_by_id(self.id, decoder)?;
        let tag = format!("[{}]", self.index);
        self.index += 1;
        Ok((tag, component))
    }

    fn has_next(&self) -> bool {
        self.index < self.length
    }

    fn format(&self) -> String {
        self.tag.clone()
    }
}

pub struct ComponentContent {
    tag: String,
    has_next: bool,
}

impl ComponentContent {
    pub(crate) fn new(tag: String) -> ComponentContent {
        ComponentContent {
            tag,
            has_next: true,
        }
    }
}
impl Content for ComponentContent {
    fn next(&mut self, decoder: &mut dyn NbtDecoder) -> Result<(String, NbtComponent), ParseError> {
        let id = decoder.read_id()?;
        if id == 0x00 {
            self.has_next = false;
            return Ok((String::new(), NbtComponent::End));
        }
        let tag = decoder.read_tag()?;
        let component: NbtComponent = next_by_id(id, decoder)?;
        Ok((tag, component))
    }

    fn has_next(&self) -> bool {
        self.has_next
    }

    fn format(&self) -> String {
        self.tag.clone()
    }
}

fn next_by_id(id: u8, decoder: &mut dyn NbtDecoder) -> Result<NbtComponent, ParseError> {
    match id {
        0x00 => Ok(NbtComponent::End),
        0x01 => Ok(decoder.read_byte()?.into()),
        0x02 => Ok(decoder.read_short()?.into()),
        0x03 => Ok(decoder.read_int()?.into()),
        0x04 => Ok(decoder.read_long()?.into()),
        0x05 => Ok(decoder.read_float()?.into()),
        0x06 => Ok(decoder.read_double()?.into()),
        0x07 => Ok(decoder.read_byte_array()?.into()),
        0x08 => Ok(decoder.read_string()?.into()),
        0x09 => Ok((decoder.read_id()?, decoder.read_int()?).into()),
        0x0a => Ok(NbtComponent::Compound),
        0x0b => Ok(decoder.read_int_array()?.into()),
        0x0c => Ok(decoder.read_long_array()?.into()),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}
