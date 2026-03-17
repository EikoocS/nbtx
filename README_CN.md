# nbtx

`nbtx` 是一个用于读取和写入 Minecraft NBT 数据的 Rust 项目，支持 Java 版和 Bedrock 版。

English [README](./README.md)

## 功能

- 流式 NBT 读取器（`Reader`），可输出 `Rank.Winner`、`NameList[2]` 这类扁平路径。
- 流式 NBT 写入器（`Writer`），内置容器作用域校验。
- 提供按路径读写的命令行工具（`--set`、`--create`、`--delete`）。
- 自动处理 gzip/zlib/原始 NBT 压缩格式。

## 项目结构

- `src/lib.rs`：库导出入口（`Reader`、`Writer`、`NbtComponent`、`PlatformType`）。
- `src/main.rs`：命令行入口与读写逻辑。
- `tests/reader.rs`：读取器行为测试。
- `tests/writer.rs`：写入器行为测试。

## 构建与测试

```bash
cargo build
cargo test
```

## 命令行用法

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

## 库使用示例

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

## 许可

当前仓库暂未附带 License 文件。
