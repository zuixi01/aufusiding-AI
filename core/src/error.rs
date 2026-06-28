//! 统一错误体系
//!
//! 为核心引擎提供可分类、可编码、面向用户可解释的错误模型。

#[derive(Debug, thiserror::Error)]
pub enum PrometheusError {
    #[error("LLM provider error: {0}")]
    Llm(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Sync error: {0}")]
    Sync(String),
    #[error("Security error: {0}")]
    Security(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("IoT device error: {0}")]
    IoT(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl PrometheusError {
    pub fn code(&self) -> &'static str {
        match self {
            PrometheusError::Llm(_) => "E1001",
            PrometheusError::Storage(_) => "E2001",
            PrometheusError::Sync(_) => "E3001",
            PrometheusError::Security(_) => "E4001",
            PrometheusError::Network(_) => "E5001",
            PrometheusError::IoT(_) => "E6001",
            PrometheusError::InvalidInput(_) => "E9001",
            PrometheusError::Internal(_) => "E9999",
        }
    }

    pub fn user_message(&self) -> &'static str {
        match self {
            PrometheusError::Llm(_) => "模型服务暂时不可用，请稍后重试。",
            PrometheusError::Storage(_) => "本地数据访问失败，请检查存储状态。",
            PrometheusError::Sync(_) => "同步暂时不可用，稍后会自动重试。",
            PrometheusError::Security(_) => "安全校验失败，请重新验证身份。",
            PrometheusError::Network(_) => "网络连接异常，请检查网络后重试。",
            PrometheusError::IoT(_) => "设备通信失败，请检查设备状态。",
            PrometheusError::InvalidInput(_) => "输入内容无效，请检查后重试。",
            PrometheusError::Internal(_) => "系统出现内部错误，请稍后重试。",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_prefixes_match_domain() {
        assert!(PrometheusError::Llm("x".into()).code().starts_with("E1"));
        assert!(PrometheusError::Storage("x".into())
            .code()
            .starts_with("E2"));
        assert!(PrometheusError::Sync("x".into()).code().starts_with("E3"));
        assert!(PrometheusError::Security("x".into())
            .code()
            .starts_with("E4"));
        assert!(PrometheusError::Network("x".into())
            .code()
            .starts_with("E5"));
        assert!(PrometheusError::IoT("x".into())
            .code()
            .starts_with("E6"));
    }

    #[test]
    fn user_message_is_non_empty() {
        let msg = PrometheusError::InvalidInput("missing model".into()).user_message();
        assert!(!msg.trim().is_empty());
    }
}
