use clap::{Parser, ValueEnum};
use nbtx::PlatformType;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliPlatform {
    Java,
    Bedrock,
}

impl From<CliPlatform> for PlatformType {
    fn from(value: CliPlatform) -> Self {
        match value {
            CliPlatform::Java => PlatformType::JavaEdition,
            CliPlatform::Bedrock => PlatformType::BedrockEdition,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "nbtx", version, about = "Read or modify NBT value by path")]
pub(super) struct Args {
    #[arg(help = "NBT file path")]
    pub(super) file: String,
    #[arg(help = "NBT path (leaf/component) or regex path prefixed with re:")]
    pub(super) path: String,
    #[arg(long = "show-type", help = "Show typed output like Int(1)")]
    pub(super) show_type: bool,
    #[arg(long = "set", value_name = "VALUE", help = "Set leaf value at path")]
    pub(super) set: Option<String>,
    #[arg(long = "create", value_name = "VALUE", help = "Create value at path")]
    pub(super) create: Option<String>,
    #[arg(long = "delete", help = "Delete value at path")]
    pub(super) delete: bool,
    #[arg(
        long = "where",
        value_name = "EXPR",
        help = "Filter list element deletion, e.g. id==123&&count<999"
    )]
    pub(super) where_expr: Option<String>,
    #[arg(long = "output", help = "Output file path (default: overwrite input)")]
    pub(super) output: Option<String>,
    #[arg(long = "platform", value_enum, default_value_t = CliPlatform::Java)]
    pub(super) platform: CliPlatform,
}
