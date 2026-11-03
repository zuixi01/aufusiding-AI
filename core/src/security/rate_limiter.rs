//! 速率限制器
//!
//! 防止 DoS 攻击和资源滥用。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::errors::RateLimitError;

#[derive(Clone)]
struct RateLimit {
    minute_count: u32,
    hour_count: u32,
    day_count: u32,
    minute_reset: Instant,
    hour_reset: Instant,
    day_reset: Instant,
}

pub struct RateLimiter {
    limits: Arc<RwLock<HashMap<String, RateLimit>>>,
    max_requests_per_minute: u32,
    max_requests_per_hour: u32,
    max_requests_per_day: u32,
}

impl RateLimiter {
    /// 创建默认配置的速率限制器
    pub fn new() -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            max_requests_per_minute: 60,
            max_requests_per_hour: 1000,
            max_requests_per_day: 10000,
        }
    }

    /// 创建自定义配置的速率限制器
    pub fn with_limits(per_minute: u32, per_hour: u32, per_day: u32) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            max_requests_per_minute: per_minute,
            max_requests_per_hour: per_hour,
            max_requests_per_day: per_day,
        }
    }

    /// 检查并更新速率限制
    pub async fn check(&self, client_id: &str) -> Result<(), RateLimitError> {
        let mut limits = self.limits.write().await;
        let now = Instant::now();

        let limit = limits
            .entry(client_id.to_string())
            .or_insert_with(|| RateLimit {
                minute_count: 0,
                hour_count: 0,
                day_count: 0,
                minute_reset: now + Duration::from_secs(60),
                hour_reset: now + Duration::from_secs(3600),
                day_reset: now + Duration::from_secs(86400),
            });

        // 重置计数器
        if now >= limit.minute_reset {
            limit.minute_count = 0;
            limit.minute_reset = now + Duration::from_secs(60);
        }
        if now >= limit.hour_reset {
            limit.hour_count = 0;
            limit.hour_reset = now + Duration::from_secs(3600);
        }
        if now >= limit.day_reset {
            limit.day_count = 0;
            limit.day_reset = now + Duration::from_secs(86400);
        }

        // 检查限制
        if limit.minute_count >= self.max_requests_per_minute {
            tracing::warn!(client_id = client_id, "每分钟请求数超限");
            return Err(RateLimitError::MinuteExceeded);
        }
        if limit.hour_count >= self.max_requests_per_hour {
            tracing::warn!(client_id = client_id, "每小时请求数超限");
            return Err(RateLimitError::HourExceeded);
        }
        if limit.day_count >= self.max_requests_per_day {
            tracing::warn!(client_id = client_id, "每日请求数超限");
            return Err(RateLimitError::DayExceeded);
        }

        // 增加计数
        limit.minute_count += 1;
        limit.hour_count += 1;
        limit.day_count += 1;

        Ok(())
    }

    /// 获取客户端当前使用情况
    pub async fn get_usage(&self, client_id: &str) -> Option<(u32, u32, u32)> {
        let limits = self.limits.read().await;
        limits
            .get(client_id)
            .map(|l| (l.minute_count, l.hour_count, l.day_count))
    }

    /// 清理过期的限制记录
    pub async fn cleanup(&self) {
        let mut limits = self.limits.write().await;
        let now = Instant::now();
        limits.retain(|_, limit| now < limit.day_reset);
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allows_within_limit() {
        let limiter = RateLimiter::with_limits(5, 100, 1000);

        for _ in 0..5 {
            assert!(limiter.check("client1").await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_blocks_over_minute_limit() {
        let limiter = RateLimiter::with_limits(3, 100, 1000);

        for _ in 0..3 {
            assert!(limiter.check("client1").await.is_ok());
        }

        assert!(limiter.check("client1").await.is_err());
    }

    #[tokio::test]
    async fn test_separate_clients() {
        let limiter = RateLimiter::with_limits(2, 100, 1000);

        assert!(limiter.check("client1").await.is_ok());
        assert!(limiter.check("client1").await.is_ok());
        assert!(limiter.check("client1").await.is_err());

        // client2 应该有独立的限制
        assert!(limiter.check("client2").await.is_ok());
        assert!(limiter.check("client2").await.is_ok());
    }

    #[tokio::test]
    async fn test_get_usage() {
        let limiter = RateLimiter::with_limits(10, 100, 1000);

        limiter.check("client1").await.unwrap();
        limiter.check("client1").await.unwrap();
        limiter.check("client1").await.unwrap();

        let (minute, hour, day) = limiter.get_usage("client1").await.unwrap();
        assert_eq!(minute, 3);
        assert_eq!(hour, 3);
        assert_eq!(day, 3);
    }
}
