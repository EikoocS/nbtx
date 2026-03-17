# nbtx

`nbtx` is a Rust project for reading and writing Minecraft NBT data (Java Edition and Bedrock Edition).

`nbtx` 是一个用于读取和写入 Minecraft NBT 数据的 Rust 项目，支持 Java 版和 Bedrock 版。

## Features | 功能

- Streaming NBT reader (`Reader`) that outputs flattened paths like `Rank.Winner` and `NameList[2]`.
- Streaming NBT writer (`Writer`) with container-scope validation.
- CLI for reading and editing values by path (`--set`, `--create`, `--delete`).
- Automatic compression handling for gzip/zlib/raw NBT files.

- 流式 NBT 读取器（`Reader`），可输出 `Rank.Winner`、`NameList[2]` 这类扁平路径。
- 流式 NBT 写入器（`Writer`），内置容器作用域校验。
- 提供按路径读写的命令行工具（`--set`、`--create`、`--delete`）。
- 自动处理 gzip/zlib/原始 NBT 压缩格式。

## Project Structure | 项目结构

- `src/lib.rs`: library exports (`Reader`, `Writer`, `NbtComponent`, `PlatformType`).
- `src/main.rs`: CLI entry and read/edit logic.
- `tests/reader.rs`: reader behavior tests.
- `tests/writer.rs`: writer behavior tests.

- `src/lib.rs`：库导出入口（`Reader`、`Writer`、`NbtComponent`、`PlatformType`）。
- `src/main.rs`：命令行入口与读写逻辑。
- `tests/reader.rs`：读取器行为测试。
- `tests/writer.rs`：写入器行为测试。

## Build & Test | 构建与测试

```bash
cargo build
cargo test
```

## CLI Usage | 命令行用法

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

```bash
# 读取指定路径
cargo run -- <文件> <路径>

# 显示带类型输出
cargo run -- <文件> <路径> --show-type

# 修改已有叶子节点
cargo run -- <文件> <路径> --set "123"

# 在路径处创建值（显式类型）
cargo run -- <文件> <路径> --create "int:123"

# 删除路径对应的值
cargo run -- <文件> <路径> --delete

# 指定平台
cargo run -- <文件> <路径> --platform java
cargo run -- <文件> <路径> --platform bedrock
```

## Library Quick Start | 库使用示例

```rust
use nbtx::{PlatformType, Reader};

let mut reader = Reader::try_new_with_path("level.dat", PlatformType::JavaEdition)?;
while reader.has_next() {
    let (path, value) = reader.next()?;
    println!("{}: {:?}", path, value);
}
# Ok::<(), nbtx::ParseError>(())
```

```rust
use nbtx::{NbtComponent, PlatformType, RootType, Writer};

let out = std::fs::File::create("out.nbt")?;
let mut writer = Writer::try_new(Box::new(out), PlatformType::JavaEdition, RootType::Compound)?;

writer.write("Name", NbtComponent::String("Notch".to_string()))?;
writer.write("Score", NbtComponent::Int(42))?;
writer.end()?;
writer.finish()?;
# Ok::<(), nbtx::ParseError>(())
```

## License | 许可

No license file is currently included.

当前仓库暂未附带 License 文件。
