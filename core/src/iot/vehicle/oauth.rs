//! 通用 OAuth2 客户端
//!
//! 为 Tesla Fleet API 和 SmartCar API 提供共享的 OAuth2 认证基础设施。
//! 支持 Authorization Code + PKCE 流程、Token 交换、自动刷新。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// OAuth2 Token 存储
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStore {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: String,
    pub scope: Option<String>,
}

impl TokenStore {
    /// Token 是否已过期（提前 60 秒判定）
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => Utc::now() + Duration::seconds(60) >= expires_at,
            None => false, // 无过期时间视为永不过期
        }
    }
}

/// OAuth2 Provider 配置
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

/// Token endpoint 响应
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: String,
    scope: Option<String>,
}
/// 通用 OAuth2 客户端
pub struct OAuthClient {
    config: OAuthConfig,
    client: reqwest::Client,
    token: Arc<RwLock<Option<TokenStore>>>,
    base_url: Option<String>,
}

impl OAuthClient {
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            token: Arc::new(RwLock::new(None)),
            base_url: None,
        }
    }

    /// 测试用：覆盖 token_url 的 base
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = Some(url.trim_end_matches('/').to_string());
        self
    }

    /// 直接注入 token（用于已有 token 的场景）
    pub async fn set_token(&self, token: TokenStore) {
        let mut guard = self.token.write().await;
        *guard = Some(token);
    }

    /// 获取 token endpoint URL（支持 base_url 覆盖）
    fn token_endpoint(&self) -> String {
        match &self.base_url {
            Some(base) => format!("{}/oauth/token", base),
            None => self.config.token_url.clone(),
        }
    }

    /// 生成授权 URL（含 PKCE）
    pub fn build_authorization_url(&self) -> (String, String, PkceChallenge) {
        let state = generate_state();
        let pkce = generate_pkce();

        let scope = self.config.scopes.join(" ");
        let url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            self.config.auth_url,
            urlencoded(&self.config.client_id),
            urlencoded(&self.config.redirect_uri),
            urlencoded(&scope),
            urlencoded(&state),
            urlencoded(&pkce.challenge),
        );

        (url, state, pkce)
    }

    /// 用授权码换取 token
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> anyhow::Result<TokenStore> {
        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.config.redirect_uri),
            ("client_id", &self.config.client_id),
            ("code_verifier", code_verifier),
        ];

        let secret_str;
        if let Some(ref secret) = self.config.client_secret {
            secret_str = secret.clone();
            params.push(("client_secret", &secret_str));
        }

        let resp = self
            .client
            .post(self.token_endpoint())
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token exchange failed ({}): {}", status, body);
        }

        let token_resp: TokenResponse = resp.json().await?;
        let store = token_response_to_store(token_resp);

        let mut guard = self.token.write().await;
        *guard = Some(store.clone());
        Ok(store)
    }

    /// 刷新 token
    pub async fn refresh(&self) -> anyhow::Result<TokenStore> {
        let current = self.token.read().await;
        let refresh_token = current
            .as_ref()
            .and_then(|t| t.refresh_token.clone())
            .ok_or_else(|| anyhow::anyhow!("no refresh token available"))?;
        drop(current);

        let mut params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.clone()),
        ];

        if let Some(ref secret) = self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let resp = self
            .client
            .post(self.token_endpoint())
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token refresh failed ({}): {}", status, body);
        }

        let token_resp: TokenResponse = resp.json().await?;
        let store = token_response_to_store(token_resp);

        let mut guard = self.token.write().await;
        *guard = Some(store.clone());
        Ok(store)
    }

    /// 获取有效 token（过期时自动刷新）
    pub async fn get_valid_token(&self) -> anyhow::Result<String> {
        let current = self.token.read().await;
        match current.as_ref() {
            Some(token) if !token.is_expired() => return Ok(token.access_token.clone()),
            Some(_) => { /* expired, need refresh */ }
            None => anyhow::bail!("no token configured — call exchange_code or set_token first"),
        }
        drop(current);

        let refreshed = self.refresh().await?;
        Ok(refreshed.access_token)
    }
}

/// PKCE challenge pair
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

/// 生成 PKCE code_verifier 和 code_challenge (S256)
pub fn generate_pkce() -> PkceChallenge {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf).expect("RNG failed");
    let verifier = base64_url_encode(&buf);

    let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
    let challenge = base64_url_encode(digest.as_ref());

    PkceChallenge {
        verifier,
        challenge,
    }
}

/// 生成随机 state 参数
fn generate_state() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut buf = [0u8; 16];
    rng.fill(&mut buf).expect("RNG failed");
    base64_url_encode(&buf)
}

/// Base64 URL-safe 编码（无 padding）
fn base64_url_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(data)
}

/// 简单 URL 编码
fn urlencoded(s: &str) -> String {
    s.replace(' ', "+").replace('&', "%26").replace('=', "%3D")
}

/// TokenResponse → TokenStore
fn token_response_to_store(resp: TokenResponse) -> TokenStore {
    let expires_at = resp
        .expires_in
        .map(|secs| Utc::now() + Duration::seconds(secs));
    TokenStore {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at,
        token_type: resp.token_type,
        scope: resp.scope,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client-id".into(),
            client_secret: Some("test-secret".into()),
            auth_url: "https://auth.example.com/authorize".into(),
            token_url: "https://auth.example.com/oauth/token".into(),
            redirect_uri: "https://app.example.com/callback".into(),
            scopes: vec!["read".into(), "write".into()],
        }
    }

    fn make_token(expired: bool) -> TokenStore {
        TokenStore {
            access_token: "access-123".into(),
            refresh_token: Some("refresh-456".into()),
            expires_at: Some(if expired {
                Utc::now() - Duration::seconds(100)
            } else {
                Utc::now() + Duration::hours(1)
            }),
            token_type: "Bearer".into(),
            scope: Some("read write".into()),
        }
    }

    // --- PKCE tests ---

    #[test]
    fn pkce_verifier_length() {
        let pkce = generate_pkce();
        assert!(pkce.verifier.len() >= 43);
        assert!(pkce.verifier.len() <= 128);
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let pkce = generate_pkce();
        let digest = ring::digest::digest(&ring::digest::SHA256, pkce.verifier.as_bytes());
        let expected = base64_url_encode(digest.as_ref());
        assert_eq!(pkce.challenge, expected);
    }

    #[test]
    fn pkce_matches_rfc7636_appendix_b() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
        assert_eq!(
            base64_url_encode(digest.as_ref()),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_generates_unique_pairs() {
        let a = generate_pkce();
        let b = generate_pkce();
        assert_ne!(a.verifier, b.verifier);
    }

    // --- Authorization URL tests ---

    #[test]
    fn build_authorization_url_contains_required_params() {
        let client = OAuthClient::new(test_config());
        let (url, state, _pkce) = client.build_authorization_url();

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("scope=read+write"));
        assert!(url.contains(&format!("state={}", state)));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn build_authorization_url_state_is_random() {
        let client = OAuthClient::new(test_config());
        let (_, state1, _) = client.build_authorization_url();
        let (_, state2, _) = client.build_authorization_url();
        assert_ne!(state1, state2);
    }

    // --- TokenStore tests ---

    #[test]
    fn token_store_serde_roundtrip() {
        let token = make_token(false);
        let json = serde_json::to_string(&token).unwrap();
        let restored: TokenStore = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.access_token, "access-123");
        assert_eq!(restored.refresh_token, Some("refresh-456".into()));
        assert_eq!(restored.token_type, "Bearer");
    }

    #[test]
    fn token_not_expired() {
        let token = make_token(false);
        assert!(!token.is_expired());
    }

    #[test]
    fn token_expired() {
        let token = make_token(true);
        assert!(token.is_expired());
    }

    #[test]
    fn token_no_expiry_never_expires() {
        let token = TokenStore {
            access_token: "abc".into(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".into(),
            scope: None,
        };
        assert!(!token.is_expired());
    }

    // --- exchange_code tests ---

    #[tokio::test]
    async fn exchange_code_sends_correct_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/oauth/token")
                .form_urlencoded_tuple("grant_type", "authorization_code")
                .form_urlencoded_tuple("code", "auth-code-xyz")
                .form_urlencoded_tuple("client_id", "test-client-id")
                .form_urlencoded_tuple("client_secret", "test-secret")
                .form_urlencoded_tuple("code_verifier", "test-verifier");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "access_token": "new-access",
                    "refresh_token": "new-refresh",
                    "expires_in": 3600,
                    "token_type": "Bearer",
                    "scope": "read write"
                }));
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        let token = client
            .exchange_code("auth-code-xyz", "test-verifier")
            .await
            .unwrap();

        assert_eq!(token.access_token, "new-access");
        assert_eq!(token.refresh_token, Some("new-refresh".into()));
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_at.is_some());
        mock.assert();
    }

    #[tokio::test]
    async fn exchange_code_handles_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(400)
                .json_body(serde_json::json!({"error": "invalid_grant"}));
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        let result = client.exchange_code("bad-code", "verifier").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("400"));
    }

    // --- refresh tests ---

    #[tokio::test]
    async fn refresh_sends_correct_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/oauth/token")
                .form_urlencoded_tuple("grant_type", "refresh_token")
                .form_urlencoded_tuple("refresh_token", "refresh-456")
                .form_urlencoded_tuple("client_id", "test-client-id");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "access_token": "refreshed-access",
                    "refresh_token": "refreshed-refresh",
                    "expires_in": 7200,
                    "token_type": "Bearer"
                }));
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        client.set_token(make_token(true)).await;

        let token = client.refresh().await.unwrap();
        assert_eq!(token.access_token, "refreshed-access");
        assert_eq!(token.refresh_token, Some("refreshed-refresh".into()));
        mock.assert();
    }

    #[tokio::test]
    async fn refresh_updates_internal_store() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200).json_body(serde_json::json!({
                "access_token": "updated",
                "expires_in": 3600,
                "token_type": "Bearer"
            }));
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        client.set_token(make_token(true)).await;
        client.refresh().await.unwrap();

        let guard = client.token.read().await;
        assert_eq!(guard.as_ref().unwrap().access_token, "updated");
    }

    #[tokio::test]
    async fn refresh_handles_revoked_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(401).body("token revoked");
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        client.set_token(make_token(true)).await;

        let result = client.refresh().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("401"));
    }

    #[tokio::test]
    async fn refresh_fails_without_refresh_token() {
        let client = OAuthClient::new(test_config());
        let token = TokenStore {
            access_token: "abc".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() - Duration::seconds(100)),
            token_type: "Bearer".into(),
            scope: None,
        };
        client.set_token(token).await;

        let result = client.refresh().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no refresh token"));
    }

    // --- get_valid_token tests ---

    #[tokio::test]
    async fn get_valid_token_returns_current_when_not_expired() {
        let client = OAuthClient::new(test_config());
        client.set_token(make_token(false)).await;

        let token = client.get_valid_token().await.unwrap();
        assert_eq!(token, "access-123");
    }

    #[tokio::test]
    async fn get_valid_token_refreshes_when_expired() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200).json_body(serde_json::json!({
                "access_token": "auto-refreshed",
                "refresh_token": "new-refresh",
                "expires_in": 3600,
                "token_type": "Bearer"
            }));
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        client.set_token(make_token(true)).await;

        let token = client.get_valid_token().await.unwrap();
        assert_eq!(token, "auto-refreshed");
    }

    #[tokio::test]
    async fn get_valid_token_fails_when_no_token() {
        let client = OAuthClient::new(test_config());
        let result = client.get_valid_token().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no token"));
    }

    #[tokio::test]
    async fn get_valid_token_fails_when_refresh_fails() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(401).body("invalid");
        });

        let client = OAuthClient::new(test_config()).with_base_url(&server.base_url());
        client.set_token(make_token(true)).await;

        let result = client.get_valid_token().await;
        assert!(result.is_err());
    }
}
