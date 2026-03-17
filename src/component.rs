#[derive(Debug, PartialEq)]
/// NBT tag payload represented as Rust values.
pub enum NbtComponent {
    /// TAG_End (0x00), used as a sentinel inside compounds.
    End,
    /// TAG_Byte (0x01).
    Byte(i8),
    /// TAG_Short (0x02).
    Short(i16),
    /// TAG_Int (0x03).
    Int(i32),
    /// TAG_Long (0x04).
    Long(i64),
    /// TAG_Float (0x05).
    Float(f32),
    /// TAG_Double (0x06).
    Double(f64),
    /// TAG_Byte_Array (0x07).
    ByteArray(Vec<u8>),
    /// TAG_String (0x08).
    String(String),
    /// TAG_List (0x09), with element tag id and element count.
    List { id: u8, length: i32 },
    /// TAG_Compound (0x0A), signals a nested compound scope.
    Compound,
    /// TAG_Int_Array (0x0B).
    IntArray(Vec<i32>),
    /// TAG_Long_Array (0x0C).
    LongArray(Vec<i64>),
}

impl From<i8> for NbtComponent {
    fn from(value: i8) -> Self {
        NbtComponent::Byte(value)
    }
}

impl From<i16> for NbtComponent {
    fn from(value: i16) -> Self {
        NbtComponent::Short(value)
    }
}

impl From<i32> for NbtComponent {
    fn from(value: i32) -> Self {
        NbtComponent::Int(value)
    }
}

impl From<i64> for NbtComponent {
    fn from(value: i64) -> Self {
        NbtComponent::Long(value)
    }
}

impl From<f32> for NbtComponent {
    fn from(value: f32) -> Self {
        NbtComponent::Float(value)
    }
}

impl From<f64> for NbtComponent {
    fn from(value: f64) -> Self {
        NbtComponent::Double(value)
    }
}

impl From<Vec<u8>> for NbtComponent {
    fn from(value: Vec<u8>) -> Self {
        NbtComponent::ByteArray(value)
    }
}

impl From<String> for NbtComponent {
    fn from(value: String) -> Self {
        NbtComponent::String(value)
    }
}

impl From<&str> for NbtComponent {
    fn from(value: &str) -> Self {
        NbtComponent::String(value.to_string())
    }
}

impl From<(u8, i32)> for NbtComponent {
    fn from(value: (u8, i32)) -> Self {
        NbtComponent::List {
            id: value.0,
            length: value.1,
        }
    }
}

impl From<Vec<i32>> for NbtComponent {
    fn from(value: Vec<i32>) -> Self {
        NbtComponent::IntArray(value)
    }
}

impl From<&Vec<i32>> for NbtComponent {
    fn from(value: &Vec<i32>) -> Self {
        NbtComponent::IntArray(value.to_vec())
    }
}

impl From<Vec<i64>> for NbtComponent {
    fn from(value: Vec<i64>) -> Self {
        NbtComponent::LongArray(value)
    }
}

impl From<&Vec<i64>> for NbtComponent {
    fn from(value: &Vec<i64>) -> Self {
        NbtComponent::LongArray(value.to_vec())
    }
}
