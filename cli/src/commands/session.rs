//! 会话管理子命令

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// 列出所有会话
    List,
    /// 新建会话
    New {
        /// 会话标题
        #[arg(short, long)]
        title: Option<String>,
    },
    /// 删除会话
    Delete {
        /// 会话 ID 前缀
        id: String,
    },
    /// 导出会话
    Export {
        /// 会话 ID 前缀
        id: String,
        /// 输出路径
        #[arg(short, long)]
        output: Option<String>,
    },
    /// 查看会话详情
    Show {
        /// 会话 ID 前缀
        id: String,
    },
}
