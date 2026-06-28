//! 轻量级 Prometheus 指标系统
//!
//! 无外部依赖，使用原子计数器实现。
//! GET /metrics 导出 Prometheus 文本格式。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 应用指标
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    /// 启动时间
    start_time: Instant,
    /// 总请求数
    requests_total: AtomicU64,
    /// 成功请求数 (2xx)
    requests_success: AtomicU64,
    /// 客户端错误 (4xx)
    requests_client_error: AtomicU64,
    /// 服务端错误 (5xx)
    requests_server_error: AtomicU64,
    /// 聊天请求数
    chat_requests: AtomicU64,
    /// 记忆搜索请求数
    memory_searches: AtomicU64,
    /// 知识库写入数
    knowledge_writes: AtomicU64,
    /// 累计响应时间 (微秒)
    response_time_us_total: AtomicU64,
    /// 活跃请求数
    active_requests: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                start_time: Instant::now(),
                requests_total: AtomicU64::new(0),
                requests_success: AtomicU64::new(0),
                requests_client_error: AtomicU64::new(0),
                requests_server_error: AtomicU64::new(0),
                chat_requests: AtomicU64::new(0),
                memory_searches: AtomicU64::new(0),
                knowledge_writes: AtomicU64::new(0),
                response_time_us_total: AtomicU64::new(0),
                active_requests: AtomicU64::new(0),
            }),
        }
    }

    pub fn inc_requests_total(&self) {
        self.inner.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_success(&self) {
        self.inner.requests_success.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_client_error(&self) {
        self.inner
            .requests_client_error
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_server_error(&self) {
        self.inner
            .requests_server_error
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_chat(&self) {
        self.inner.chat_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_search(&self) {
        self.inner.memory_searches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_knowledge_write(&self) {
        self.inner.knowledge_writes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_response_time_us(&self, us: u64) {
        self.inner
            .response_time_us_total
            .fetch_add(us, Ordering::Relaxed);
    }

    pub fn inc_active(&self) {
        self.inner.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_active(&self) {
        self.inner.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// 导出 Prometheus 文本格式
    pub fn export(&self) -> String {
        let i = &self.inner;
        let uptime = i.start_time.elapsed().as_secs();
        let total = i.requests_total.load(Ordering::Relaxed);
        let success = i.requests_success.load(Ordering::Relaxed);
        let client_err = i.requests_client_error.load(Ordering::Relaxed);
        let server_err = i.requests_server_error.load(Ordering::Relaxed);
        let chat = i.chat_requests.load(Ordering::Relaxed);
        let mem_search = i.memory_searches.load(Ordering::Relaxed);
        let kb_write = i.knowledge_writes.load(Ordering::Relaxed);
        let resp_us = i.response_time_us_total.load(Ordering::Relaxed);
        let active = i.active_requests.load(Ordering::Relaxed);
        let avg_ms = if total > 0 {
            resp_us as f64 / total as f64 / 1000.0
        } else {
            0.0
        };

        format!(
            "# HELP prometheus_uptime_seconds Server uptime in seconds\n\
             # TYPE prometheus_uptime_seconds gauge\n\
             prometheus_uptime_seconds {uptime}\n\
             # HELP prometheus_http_requests_total Total HTTP requests\n\
             # TYPE prometheus_http_requests_total counter\n\
             prometheus_http_requests_total {total}\n\
             # HELP prometheus_http_requests_success Successful requests (2xx)\n\
             # TYPE prometheus_http_requests_success counter\n\
             prometheus_http_requests_success {success}\n\
             # HELP prometheus_http_requests_client_error Client errors (4xx)\n\
             # TYPE prometheus_http_requests_client_error counter\n\
             prometheus_http_requests_client_error {client_err}\n\
             # HELP prometheus_http_requests_server_error Server errors (5xx)\n\
             # TYPE prometheus_http_requests_server_error counter\n\
             prometheus_http_requests_server_error {server_err}\n\
             # HELP prometheus_chat_requests_total Chat API requests\n\
             # TYPE prometheus_chat_requests_total counter\n\
             prometheus_chat_requests_total {chat}\n\
             # HELP prometheus_memory_searches_total Memory search requests\n\
             # TYPE prometheus_memory_searches_total counter\n\
             prometheus_memory_searches_total {mem_search}\n\
             # HELP prometheus_knowledge_writes_total Knowledge write requests\n\
             # TYPE prometheus_knowledge_writes_total counter\n\
             prometheus_knowledge_writes_total {kb_write}\n\
             # HELP prometheus_http_response_avg_ms Average response time in ms\n\
             # TYPE prometheus_http_response_avg_ms gauge\n\
             prometheus_http_response_avg_ms {avg_ms:.3}\n\
             # HELP prometheus_http_active_requests Currently active requests\n\
             # TYPE prometheus_http_active_requests gauge\n\
             prometheus_http_active_requests {active}\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_counters() {
        let m = Metrics::new();
        m.inc_requests_total();
        m.inc_requests_total();
        m.inc_success();
        m.inc_client_error();
        m.inc_chat();
        m.record_response_time_us(5000);
        m.record_response_time_us(3000);

        let output = m.export();
        assert!(output.contains("prometheus_http_requests_total 2"));
        assert!(output.contains("prometheus_http_requests_success 1"));
        assert!(output.contains("prometheus_http_requests_client_error 1"));
        assert!(output.contains("prometheus_chat_requests_total 1"));
        assert!(output.contains("prometheus_http_response_avg_ms 4.000"));
    }

    #[test]
    fn metrics_active_requests() {
        let m = Metrics::new();
        m.inc_active();
        m.inc_active();
        assert!(m.export().contains("prometheus_http_active_requests 2"));
        m.dec_active();
        assert!(m.export().contains("prometheus_http_active_requests 1"));
    }

    #[test]
    fn metrics_export_format() {
        let m = Metrics::new();
        let output = m.export();
        // 验证 Prometheus 文本格式
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
        assert!(output.contains("counter"));
        assert!(output.contains("gauge"));
    }
}
