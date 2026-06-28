//! 配置管理子命令

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// 查看所有配置
    List,
    /// 获取配置项
    Get {
        /// 配置键名
        key: String,
    },
    /// 设置配置项
    Set {
        /// 配置键名
        key: String,
        /// 配置值
        value: String,
    },
    /// 重置为默认配置
    Reset,
}
