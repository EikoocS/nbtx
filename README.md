# nbtx

`nbtx` is a Rust project for reading and writing Minecraft NBT data (Java Edition and Bedrock Edition).

Chinese README: `README_CN.md`

## Features

- Streaming NBT reader (`Reader`) that outputs flattened paths like `Rank.Winner` and `NameList[2]`.
- Streaming NBT writer (`Writer`) with container-scope validation.
- CLI for reading and editing values by path (`--set`, `--create`, `--delete`).
- Automatic compression handling for gzip/zlib/raw NBT files.

## Project Structure

- `src/lib.rs`: library exports (`Reader`, `Writer`, `NbtComponent`, `PlatformType`).
- `src/main.rs`: CLI entry and read/edit logic.
- `tests/reader.rs`: reader behavior tests.
- `tests/writer.rs`: writer behavior tests.

## Build & Test

```bash
cargo build
cargo test
```

## CLI Usage

```bash
# Read a path
cargo run -- <file> <path>

# Show typed output
cargo run -- <file> <path> --show-type

# Update existing leaf value
cargo run -- <file> <path> --set "123"

# Create value at path (typed create)
cargo run -- <file> <path> --create "int:123"

# Delete value at path
cargo run -- <file> <path> --delete

# Select platform
cargo run -- <file> <path> --platform java
cargo run -- <file> <path> --platform bedrock
```

## Library Quick Start

```rust
use nbtx::{PlatformType, Reader};

fn main() {
    let mut reader = Reader::try_new_with_path("level.dat", PlatformType::JavaEdition)
        .expect("failed to open NBT file");

    while reader.has_next() {
        let (path, value) = reader.next().expect("failed to read NBT entry");
        println!("{}: {:?}", path, value);
    }
}
```

```rust
use nbtx::{NbtComponent, PlatformType, RootType, Writer};

fn main() {
    let sink: Vec<u8> = Vec::new();
    let mut writer = Writer::try_new(
        Box::new(std::io::Cursor::new(sink)),
        PlatformType::JavaEdition,
        RootType::Compound,
    )
    .expect("failed to initialize writer");

    writer
        .write("Name", NbtComponent::String("Notch".to_string()))
        .expect("failed to write Name");
    writer
        .write("Score", NbtComponent::Int(42))
        .expect("failed to write Score");
    writer.end().expect("failed to end root compound");
    writer.finish().expect("failed to finalize document");
}
```

## License

No license file is currently included.
