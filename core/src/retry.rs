//! 共享重试工具
//!
//! 为网络操作提供指数退避重试逻辑。

use std::future::Future;
use std::time::Duration;

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数（不含首次尝试）
    pub max_retries: u32,
    /// 初始退避时间
    pub initial_backoff: Duration,
    /// 退避倍数（指数退避）
    pub backoff_multiplier: f64,
    /// 最大退避时间
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(10),
        }
    }
}

impl RetryConfig {
    /// 快速重试（适合本地服务如 Ollama）
    pub fn fast() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(200),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(2),
        }
    }

    /// 标准重试（适合远程 API）
    pub fn standard() -> Self {
        Self::default()
    }
}

/// 带指数退避的重试执行器。
///
/// `op` 返回 `Ok(T)` 时立即返回；返回 `Err` 时按配置重试。
/// `is_retryable` 判断错误是否值得重试（如网络超时），返回 false 则立即失败。
pub async fn retry_with_backoff<T, E, Fut, F, R>(
    config: &RetryConfig,
    mut op: F,
    is_retryable: R,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    R: Fn(&E) -> bool,
    E: std::fmt::Display,
{
    let mut backoff = config.initial_backoff;
    let mut last_err: Option<E> = None;

    for attempt in 0..=config.max_retries {
        if attempt > 0 {
            tracing::warn!(
                attempt = attempt,
                max = config.max_retries,
                backoff_ms = backoff.as_millis() as u64,
                "retrying after error: {}",
                last_err.as_ref().map(|e| e.to_string()).unwrap_or_default()
            );
            tokio::time::sleep(backoff).await;
            backoff = Duration::from_secs_f64(
                (backoff.as_secs_f64() * config.backoff_multiplier)
                    .min(config.max_backoff.as_secs_f64()),
            );
        }

        match op().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if !is_retryable(&e) || attempt == config.max_retries {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
    }

    // unreachable: the loop always returns via Ok or Err
    unreachable!("retry loop exited without returning")
}

/// 判断 reqwest 错误是否为可重试的瞬态错误
pub fn is_retryable_reqwest(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("timeout")
        || msg.contains("connection refused")
        || msg.contains("connection reset")
        || msg.contains("broken pipe")
        || msg.contains("dns")
        || msg.contains("temporarily unavailable")
}

/// 判断 HTTP 状态码是否为可重试
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let config = RetryConfig::fast();
        let result: Result<&str, anyhow::Error> =
            retry_with_backoff(&config, || async { Ok("ok") }, is_retryable_reqwest).await;
        assert_eq!(result.unwrap(), "ok");
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failure() {
        let config = RetryConfig::fast();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<&str, anyhow::Error> = retry_with_backoff(
            &config,
            || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(anyhow::anyhow!("connection refused"))
                    } else {
                        Ok("recovered")
                    }
                }
            },
            is_retryable_reqwest,
        )
        .await;
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_fails_on_non_retryable_error() {
        let config = RetryConfig::fast();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<&str, anyhow::Error> = retry_with_backoff(
            &config,
            || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(anyhow::anyhow!("authentication failed"))
                }
            },
            is_retryable_reqwest,
        )
        .await;
        assert!(result.is_err());
        // Should not retry non-retryable errors
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_exhausts_all_attempts() {
        let config = RetryConfig {
            max_retries: 2,
            initial_backoff: Duration::from_millis(10),
            backoff_multiplier: 1.0,
            max_backoff: Duration::from_millis(50),
        };
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<&str, anyhow::Error> = retry_with_backoff(
            &config,
            || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(anyhow::anyhow!("connection refused"))
                }
            },
            is_retryable_reqwest,
        )
        .await;
        assert!(result.is_err());
        // 1 initial + 2 retries = 3 total
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retryable_status_codes() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn config_defaults_are_reasonable() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!(config.initial_backoff >= Duration::from_millis(100));
        assert!(config.max_backoff <= Duration::from_secs(30));
    }
}
