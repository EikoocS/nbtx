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

可直接运行的示例位于 `examples/` 目录：

```bash
# Reader 示例（输出扁平路径和值）
cargo run --example reader -- level.dat

# Writer 示例（在内存中创建一个简单 NBT 文档）
cargo run --example writer
```

## 许可

当前仓库暂未附带 License 文件。
