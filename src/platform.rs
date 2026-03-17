/// Minecraft NBT binary format variant.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PlatformType {
    /// Java Edition NBT encoding.
    JavaEdition,
    /// Bedrock Edition NBT encoding.
    BedrockEdition,
}
