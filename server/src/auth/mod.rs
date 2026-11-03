//! 认证与授权模块
//!
//! 支持两种认证方式：
//! - API Key: 通过 `X-API-Key` 请求头
//! - JWT Bearer Token: 通过 `Authorization: Bearer <token>` 请求头
//!
//! 权限级别: Read < Write < Admin

#![allow(dead_code)]

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// 权限级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Write,
    Admin,
}

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 主题（用户标识）
    pub sub: String,
    /// 权限级别
    pub permission: Permission,
    /// 过期时间 (Unix timestamp)
    pub exp: usize,
    /// 签发时间
    pub iat: usize,
}

/// 认证配置
#[derive(Clone)]
pub struct AuthConfig {
    /// JWT 签名密钥
    pub jwt_secret: String,
    /// 有效的 API Key 列表（key -> permission）
    pub api_keys: Vec<ApiKeyEntry>,
    /// 是否启用认证（开发模式可关闭）
    pub enabled: bool,
}

/// API Key 条目
#[derive(Clone)]
pub struct ApiKeyEntry {
    pub key: String,
    pub permission: Permission,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: std::env::var("PROMETHEUS_JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-in-production".into()),
            api_keys: Vec::new(),
            enabled: std::env::var("PROMETHEUS_AUTH_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        }
    }
}

impl AuthConfig {
    /// 从环境变量加载 API Keys
    /// 格式: PROMETHEUS_API_KEYS="key1:admin,key2:write,key3:read"
    pub fn load_api_keys_from_env(&mut self) {
        if let Ok(keys_str) = std::env::var("PROMETHEUS_API_KEYS") {
            for entry in keys_str.split(',') {
                let parts: Vec<&str> = entry.trim().split(':').collect();
                if parts.len() == 2 {
                    let permission = match parts[1] {
                        "admin" => Permission::Admin,
                        "write" => Permission::Write,
                        _ => Permission::Read,
                    };
                    self.api_keys.push(ApiKeyEntry {
                        key: parts[0].to_string(),
                        permission,
                    });
                }
            }
        }
    }

    /// 验证 API Key，返回权限
    pub fn validate_api_key(&self, key: &str) -> Option<Permission> {
        self.api_keys
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.permission)
    }
}

/// 生成 JWT Token
pub fn generate_token(
    config: &AuthConfig,
    sub: &str,
    permission: Permission,
    duration_secs: u64,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: sub.to_string(),
        permission,
        iat: now,
        exp: now + duration_secs as usize,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;
    Ok(token)
}

/// 验证 JWT Token
pub fn validate_token(config: &AuthConfig, token: &str) -> Result<Claims, AuthError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
    .map_err(|e| AuthError::InvalidToken(e.to_string()))
}

/// 认证错误
#[derive(Debug)]
pub enum AuthError {
    MissingCredentials,
    InvalidToken(String),
    InvalidApiKey,
    InsufficientPermission,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingCredentials => (
                StatusCode::UNAUTHORIZED,
                "Missing authentication credentials",
            ),
            AuthError::InvalidToken(ref _e) => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::InvalidApiKey => (StatusCode::UNAUTHORIZED, "Invalid API key"),
            AuthError::InsufficientPermission => (StatusCode::FORBIDDEN, "Insufficient permission"),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// 从请求中提取认证信息
fn extract_auth(config: &AuthConfig, req: &Request) -> Result<Claims, AuthError> {
    // 尝试 API Key
    if let Some(key) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        if let Some(permission) = config.validate_api_key(key) {
            // 用 key 前 8 字符作为标识，避免所有 API Key 用户共享同一限流桶
            let key_prefix = if key.len() >= 8 { &key[..8] } else { key };
            return Ok(Claims {
                sub: format!("apikey-{}", key_prefix),
                permission,
                iat: 0,
                exp: 0,
            });
        }
        return Err(AuthError::InvalidApiKey);
    }

    // 尝试 Bearer Token
    if let Some(auth_header) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            return validate_token(config, token);
        }
    }

    Err(AuthError::MissingCredentials)
}

/// 认证中间件（要求最低 Read 权限）
pub async fn require_auth(req: Request, next: Next) -> Result<Response, AuthError> {
    require_permission(req, next, Permission::Read).await
}

/// 写权限中间件
pub async fn require_write(req: Request, next: Next) -> Result<Response, AuthError> {
    require_permission(req, next, Permission::Write).await
}

/// 管理员权限中间件
pub async fn require_admin(req: Request, next: Next) -> Result<Response, AuthError> {
    require_permission(req, next, Permission::Admin).await
}

async fn require_permission(
    mut req: Request,
    next: Next,
    min_permission: Permission,
) -> Result<Response, AuthError> {
    let config = req
        .extensions()
        .get::<AuthConfig>()
        .cloned()
        .unwrap_or_default();

    if !config.enabled {
        return Ok(next.run(req).await);
    }

    let claims = extract_auth(&config, &req)?;
    if claims.permission < min_permission {
        return Err(AuthError::InsufficientPermission);
    }

    // 将 Claims 传播到后续处理函数
    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AuthConfig {
        AuthConfig {
            jwt_secret: "test-secret".into(),
            api_keys: vec![
                ApiKeyEntry {
                    key: "read-key".into(),
                    permission: Permission::Read,
                },
                ApiKeyEntry {
                    key: "write-key".into(),
                    permission: Permission::Write,
                },
                ApiKeyEntry {
                    key: "admin-key".into(),
                    permission: Permission::Admin,
                },
            ],
            enabled: true,
        }
    }

    #[test]
    fn validate_api_key_found() {
        let config = test_config();
        assert_eq!(
            config.validate_api_key("admin-key"),
            Some(Permission::Admin)
        );
        assert_eq!(
            config.validate_api_key("write-key"),
            Some(Permission::Write)
        );
        assert_eq!(config.validate_api_key("read-key"), Some(Permission::Read));
    }

    #[test]
    fn validate_api_key_not_found() {
        let config = test_config();
        assert_eq!(config.validate_api_key("bad-key"), None);
    }

    #[test]
    fn jwt_roundtrip() {
        let config = test_config();
        let token = generate_token(&config, "user1", Permission::Write, 3600).unwrap();
        assert_eq!(
            jsonwebtoken::decode_header(&token).unwrap().alg,
            Algorithm::HS256
        );
        let claims = validate_token(&config, &token).unwrap();
        assert_eq!(claims.sub, "user1");
        assert_eq!(claims.permission, Permission::Write);
    }

    #[test]
    fn jwt_invalid_token() {
        let config = test_config();
        let result = validate_token(&config, "invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn jwt_wrong_secret() {
        let config = test_config();
        let token = generate_token(&config, "user1", Permission::Read, 3600).unwrap();

        let other_config = AuthConfig {
            jwt_secret: "other-secret".into(),
            ..config
        };
        let result = validate_token(&other_config, &token);
        assert!(result.is_err());
    }

    #[test]
    fn jwt_expired_token_is_rejected() {
        let config = test_config();
        let claims = Claims {
            sub: "user1".into(),
            permission: Permission::Read,
            exp: 1,
            iat: 0,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
        )
        .unwrap();

        assert!(matches!(
            validate_token(&config, &token),
            Err(AuthError::InvalidToken(_))
        ));
    }

    #[test]
    fn jwt_other_hmac_algorithm_is_rejected() {
        let config = test_config();
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = Claims {
            sub: "user1".into(),
            permission: Permission::Read,
            exp: now + 3600,
            iat: now,
        };
        let token = encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
        )
        .unwrap();

        assert!(matches!(
            validate_token(&config, &token),
            Err(AuthError::InvalidToken(_))
        ));
    }

    #[test]
    fn permission_ordering() {
        assert!(Permission::Read < Permission::Write);
        assert!(Permission::Write < Permission::Admin);
        assert!(Permission::Read < Permission::Admin);
    }

    #[test]
    fn default_config_auth_disabled() {
        let config = AuthConfig::default();
        assert!(!config.enabled);
    }
}
