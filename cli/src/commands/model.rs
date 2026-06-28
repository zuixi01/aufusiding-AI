//! 模型管理子命令

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ModelArgs {
    #[command(subcommand)]
    pub command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// 列出可用模型
    List,
    /// 查看模型详情
    Info {
        /// 模型名称
        name: String,
    },
    /// 设置默认模型
    Default {
        /// 模型名称
        name: String,
    },
    /// 测试模型连通性
    Test {
        /// 模型名称
        name: String,
    },
}
