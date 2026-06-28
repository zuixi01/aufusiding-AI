//! 安全错误类型定义

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("输入过长：最大允许 {max} 字符，实际 {actual} 字符")]
    InputTooLong { max: usize, actual: usize },

    #[error("检测到 Prompt 注入攻击")]
    PromptInjectionDetected,

    #[error("检测到 SQL 注入攻击")]
    SqlInjectionDetected,

    #[error("检测到命令注入攻击")]
    CommandInjectionDetected,

    #[error("无效的设备 ID")]
    InvalidDeviceId,

    #[error("设备 ID 过长")]
    DeviceIdTooLong,

    #[error("未知命令：{0}")]
    UnknownCommand(String),

    #[error("速率限制超出：{0}")]
    RateLimitExceeded(String),

    #[error("可疑活动：{0}")]
    SuspiciousActivity(String),
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("输入过长")]
    TooLong,

    #[error("包含非法字符：{0}")]
    InvalidCharacter(char),

    #[error("包含控制字符")]
    ControlCharacter,

    #[error("无效的设备 ID")]
    InvalidDeviceId,

    #[error("无效的命令")]
    InvalidCommand,

    #[error("空输入")]
    EmptyInput,

    #[error("格式错误：{0}")]
    FormatError(String),
}

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("每分钟请求数超限")]
    MinuteExceeded,

    #[error("每小时请求数超限")]
    HourExceeded,

    #[error("每日请求数超限")]
    DayExceeded,
}
