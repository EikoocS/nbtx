//! Streaming NBT parser for Java and Bedrock editions.

mod bedrock;
mod component;
pub mod decoder;
pub mod encoder;
mod error;
mod java;
mod platform;
mod reader;
mod reader_content;
mod util;
mod writer;

/// NBT value variants returned by [`Reader`].
pub use component::NbtComponent;
/// Error types returned when parsing fails.
pub use error::*;
/// Target platform used to choose binary decoding rules.
pub use platform::PlatformType;
/// Streaming reader that yields flattened NBT leaf values.
pub use reader::Reader;
/// Streaming writer that emits NBT incrementally.
pub use writer::{RootType, Writer};
