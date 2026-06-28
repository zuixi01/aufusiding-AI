//! 聊天子命令

use clap::Args;

/// 单次聊天（非交互模式）
#[derive(Debug, Args)]
pub struct ChatArgs {
    /// 聊天消息
    pub message: String,

    /// 指定模型
    #[arg(short, long)]
    pub model: Option<String>,

    /// 会话 ID（继续已有会话）
    #[arg(short, long)]
    pub session: Option<String>,

    /// 输出格式: text | json | markdown
    #[arg(short, long, default_value = "text")]
    pub format: String,

    /// 流式输出
    #[arg(long, default_value_t = true)]
    pub stream: bool,
}
