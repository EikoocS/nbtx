use crate::decoder::NbtDecoder;
use crate::{NbtComponent, ParseError};
use crate::tag_id;

pub trait Content {
    fn next(&mut self, decoder: &mut dyn NbtDecoder) -> Result<(String, NbtComponent), ParseError>;
    fn has_next(&self) -> bool;
    fn format(&self) -> &str;
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

    fn format(&self) -> &str {
        &self.tag
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
        if id == tag_id::END {
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

    fn format(&self) -> &str {
        &self.tag
    }
}

fn next_by_id(id: u8, decoder: &mut dyn NbtDecoder) -> Result<NbtComponent, ParseError> {
    match id {
        tag_id::END => Ok(NbtComponent::End),
        tag_id::BYTE => Ok(decoder.read_byte()?.into()),
        tag_id::SHORT => Ok(decoder.read_short()?.into()),
        tag_id::INT => Ok(decoder.read_int()?.into()),
        tag_id::LONG => Ok(decoder.read_long()?.into()),
        tag_id::FLOAT => Ok(decoder.read_float()?.into()),
        tag_id::DOUBLE => Ok(decoder.read_double()?.into()),
        tag_id::BYTE_ARRAY => Ok(decoder.read_byte_array()?.into()),
        tag_id::STRING => Ok(decoder.read_string()?.into()),
        tag_id::LIST => Ok((decoder.read_id()?, decoder.read_int()?).into()),
        tag_id::COMPOUND => Ok(NbtComponent::Compound),
        tag_id::INT_ARRAY => Ok(decoder.read_int_array()?.into()),
        tag_id::LONG_ARRAY => Ok(decoder.read_long_array()?.into()),
        _ => Err(ParseError::UnsupportedTagId(id)),
    }
}
