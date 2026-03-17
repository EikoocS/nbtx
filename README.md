# nbtx

`nbtx` is a Rust project for reading and writing Minecraft NBT data (Java Edition and Bedrock Edition).

中文版 [README](./README_CN.md)

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

Runnable examples are provided in `examples/`:

```bash
# Reader example (prints flattened paths and values)
cargo run --example reader -- level.dat

# Writer example (creates a simple in-memory NBT document)
cargo run --example writer
```

## License

No license file is currently included.
