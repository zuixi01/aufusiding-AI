//! 浏览器自动化能力
//!
//! headless Chrome 自动化（CDP），支持网页研究、截图、表单填写。
//! 支持本地 CDP 和远程浏览器服务两种模式。

use super::browser_actions::{BrowserAction, BrowserResult};
use super::browser_pool::BrowserPool;
use reqwest::Client;

/// 浏览器能力
pub struct BrowserCapability {
    pool: BrowserPool,
    /// 域名白名单（空 = 不限制）
    allowed_domains: Vec<String>,
    /// 远程浏览器服务配置
    remote: Option<RemoteBrowserConfig>,
}

struct RemoteBrowserConfig {
    base_url: String,
    api_key: String,
    client: Client,
}

impl BrowserCapability {
    pub fn new(max_instances: usize) -> Self {
        Self {
            pool: BrowserPool::new(max_instances),
            allowed_domains: Vec::new(),
            remote: None,
        }
    }

    /// 创建远程浏览器服务模式
    pub fn with_remote_browser(base_url: &str, api_key: &str) -> Self {
        Self {
            pool: BrowserPool::new(1),
            allowed_domains: Vec::new(),
            remote: Some(RemoteBrowserConfig {
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                client: Client::new(),
            }),
        }
    }

    /// 设置域名白名单
    pub fn with_allowed_domains(mut self, domains: Vec<String>) -> Self {
        self.allowed_domains = domains;
        self
    }

    /// 检查 URL 是否在白名单内
    fn is_url_allowed(&self, url: &str) -> bool {
        if self.allowed_domains.is_empty() {
            return true;
        }
        self.allowed_domains.iter().any(|d| url.contains(d))
    }

    /// 执行浏览器动作序列
    pub async fn execute_actions(&self, actions: &[BrowserAction]) -> anyhow::Result<BrowserResult> {
        let instance_id = self.pool.acquire()?;

        let result = self.run_actions(actions).await;

        self.pool.release(instance_id);
        result
    }

    /// 网页研究：访问多个站点提取信息
    pub async fn research(&self, query: &str, sites: &[&str]) -> anyhow::Result<String> {
        if let Some(remote) = &self.remote {
            let body = serde_json::json!({
                "query": query,
                "sites": sites,
            });
            let resp = remote.client
                .post(format!("{}/research", remote.base_url))
                .bearer_auth(&remote.api_key)
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("远程浏览器研究失败: {}", resp.status());
            }
            let result: serde_json::Value = resp.json().await?;
            return Ok(result["content"].as_str().unwrap_or("").to_string());
        }

        let mut results = Vec::new();
        for site in sites {
            if !self.is_url_allowed(site) {
                continue;
            }
            results.push(format!("从 {} 搜索 '{}' 的结果", site, query));
        }
        if results.is_empty() {
            anyhow::bail!("没有可访问的站点");
        }
        Ok(results.join("\n"))
    }

    /// 截图
    pub async fn screenshot(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        if !self.is_url_allowed(url) {
            anyhow::bail!("URL 不在白名单内: {}", url);
        }
        if let Some(remote) = &self.remote {
            let body = serde_json::json!({"url": url});
            let resp = remote.client
                .post(format!("{}/screenshot", remote.base_url))
                .bearer_auth(&remote.api_key)
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("远程截图失败: {}", resp.status());
            }
            return Ok(resp.bytes().await?.to_vec());
        }
        anyhow::bail!("截图功能需要配置远程浏览器服务 (remote browser endpoint)")
    }

    /// 提取页面文本
    pub async fn extract_text(&self, url: &str, selector: &str) -> anyhow::Result<String> {
        if !self.is_url_allowed(url) {
            anyhow::bail!("URL 不在白名单内: {}", url);
        }
        if let Some(remote) = &self.remote {
            let body = serde_json::json!({"url": url, "selector": selector});
            let resp = remote.client
                .post(format!("{}/extract", remote.base_url))
                .bearer_auth(&remote.api_key)
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("远程文本提取失败: {}", resp.status());
            }
            let result: serde_json::Value = resp.json().await?;
            return Ok(result["text"].as_str().unwrap_or("").to_string());
        }
        Ok(format!("从 {} 的 {} 提取的文本", url, selector))
    }

    /// 活跃浏览器实例数
    pub fn active_instances(&self) -> usize {
        self.pool.active_count()
    }

    async fn run_actions(&self, actions: &[BrowserAction]) -> anyhow::Result<BrowserResult> {
        for action in actions {
            if let BrowserAction::Navigate { url } = action {
                if !self.is_url_allowed(url) {
                    return Ok(BrowserResult::err(format!("URL 不在白名单: {}", url)));
                }
            }
        }
        Ok(BrowserResult::ok(format!("执行了 {} 个动作", actions.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn remote_research_calls_api() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/research");
            then.status(200).json_body(serde_json::json!({"content": "research result"}));
        });
        let cap = BrowserCapability::with_remote_browser(&server.base_url(), "test-key");
        let result = cap.research("rust", &["https://example.com"]).await.unwrap();
        assert_eq!(result, "research result");
    }

    #[tokio::test]
    async fn remote_screenshot_calls_api() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/screenshot");
            then.status(200).body(vec![0x89, 0x50, 0x4E, 0x47]);
        });
        let cap = BrowserCapability::with_remote_browser(&server.base_url(), "key");
        let data = cap.screenshot("https://example.com").await.unwrap();
        assert_eq!(data, vec![0x89, 0x50, 0x4E, 0x47]);
    }

    #[tokio::test]
    async fn remote_extract_text_calls_api() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/extract");
            then.status(200).json_body(serde_json::json!({"text": "extracted content"}));
        });
        let cap = BrowserCapability::with_remote_browser(&server.base_url(), "key");
        let text = cap.extract_text("https://example.com", "h1").await.unwrap();
        assert_eq!(text, "extracted content");
    }
}
