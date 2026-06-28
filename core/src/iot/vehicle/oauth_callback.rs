//! OAuth2 本地回调服务器
//!
//! 启动临时 HTTP 服务器接收 OAuth2 授权码回调。
//! 使用 tokio TcpListener，无需额外 HTTP 框架依赖。

use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 回调结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// 回调服务器错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackError {
    /// 用户拒绝授权
    AccessDenied(String),
    /// 缺少 code 参数
    MissingCode,
    /// 缺少 state 参数
    MissingState,
    /// state 不匹配（CSRF 防护）
    StateMismatch { expected: String, got: String },
    /// 等待超时
    Timeout,
    /// 服务器绑定失败
    BindFailed(String),
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallbackError::AccessDenied(msg) => write!(f, "access denied: {}", msg),
            CallbackError::MissingCode => write!(f, "missing 'code' parameter in callback"),
            CallbackError::MissingState => write!(f, "missing 'state' parameter in callback"),
            CallbackError::StateMismatch { expected, got } => {
                write!(f, "state mismatch: expected {}, got {}", expected, got)
            }
            CallbackError::Timeout => {
                write!(f, "callback timeout: user did not complete authorization")
            }
            CallbackError::BindFailed(msg) => write!(f, "failed to bind callback server: {}", msg),
        }
    }
}

impl std::error::Error for CallbackError {}

/// 解析 HTTP 请求行中的 query string
///
/// 输入: "GET /callback?code=abc&state=xyz HTTP/1.1\r\n..."
/// 输出: HashMap { "code" => "abc", "state" => "xyz" }
pub fn parse_query_params(request: &str) -> HashMap<String, String> {
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    let query = match path.split_once('?') {
        Some((_, q)) => q,
        None => return HashMap::new(),
    };

    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((url_decode(k), url_decode(v)))
        })
        .collect()
}

/// 简单 URL 解码（%XX + '+' → ' '）
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'+' => result.push(' '),
            b'%' => {
                let h = chars.next().unwrap_or(b'0');
                let l = chars.next().unwrap_or(b'0');
                let hex = [h, l];
                if let Ok(s) = std::str::from_utf8(&hex) {
                    if let Ok(byte) = u8::from_str_radix(s, 16) {
                        result.push(byte as char);
                    }
                }
            }
            _ => result.push(b as char),
        }
    }
    result
}

/// 验证回调参数，返回 CallbackResult 或 CallbackError
pub fn validate_callback(
    params: &HashMap<String, String>,
    expected_state: &str,
) -> Result<CallbackResult, CallbackError> {
    // 检查是否有 error 参数（用户拒绝授权）
    if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| err.clone());
        return Err(CallbackError::AccessDenied(desc));
    }

    let code = params
        .get("code")
        .ok_or(CallbackError::MissingCode)?
        .clone();

    let state = params
        .get("state")
        .ok_or(CallbackError::MissingState)?
        .clone();

    if state != expected_state {
        return Err(CallbackError::StateMismatch {
            expected: expected_state.to_string(),
            got: state,
        });
    }

    Ok(CallbackResult { code, state })
}

/// 生成成功响应 HTML
fn success_html() -> &'static str {
    "<html><body><h2>Authorization successful!</h2><p>You can close this window and return to Prometheus.</p></body></html>"
}

/// 生成错误响应 HTML
fn error_html(msg: &str) -> String {
    format!("<html><body><h2>Authorization failed</h2><p>{}</p><p>Please close this window and try again.</p></body></html>", msg)
}

/// 构建 HTTP 响应
fn http_response(status: u16, status_text: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        body.len(),
        body
    )
}

/// 启动本地回调服务器，等待 OAuth2 授权码
///
/// - `port`: 监听端口（如 8765）
/// - `expected_state`: 预期的 state 参数（CSRF 防护）
/// - `timeout_secs`: 超时秒数
///
/// 返回授权码和 state，或错误。
pub async fn wait_for_callback(
    port: u16,
    expected_state: &str,
    timeout_secs: u64,
) -> Result<CallbackResult, CallbackError> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| CallbackError::BindFailed(e.to_string()))?;

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        accept_one(&listener, expected_state),
    )
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(CallbackError::Timeout),
    }
}

/// 接受一个连接，解析请求，返回结果
async fn accept_one(
    listener: &TcpListener,
    expected_state: &str,
) -> Result<CallbackResult, CallbackError> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| CallbackError::BindFailed(e.to_string()))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| CallbackError::BindFailed(e.to_string()))?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    let params = parse_query_params(&request);
    let result = validate_callback(&params, expected_state);

    // 发送 HTTP 响应
    let response = match &result {
        Ok(_) => http_response(200, "OK", success_html()),
        Err(e) => http_response(400, "Bad Request", &error_html(&e.to_string())),
    };

    if let Err(e) = stream.write_all(response.as_bytes()).await {
        tracing::warn!("oauth callback response write failed: {}", e);
    }
    if let Err(e) = stream.flush().await {
        tracing::warn!("oauth callback response flush failed: {}", e);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    // ── 纯函数测试 ──────────────────────────────────────────────────

    #[test]
    fn parse_query_basic() {
        let req = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\nHost: localhost\r\n";
        let params = parse_query_params(req);
        assert_eq!(params.get("code").unwrap(), "abc123");
        assert_eq!(params.get("state").unwrap(), "xyz789");
    }

    #[test]
    fn parse_query_url_encoded() {
        let req = "GET /callback?code=a%20b&state=x%26y HTTP/1.1\r\n";
        let params = parse_query_params(req);
        assert_eq!(params.get("code").unwrap(), "a b");
        assert_eq!(params.get("state").unwrap(), "x&y");
    }

    #[test]
    fn parse_query_plus_as_space() {
        let req = "GET /callback?code=hello+world&state=s HTTP/1.1\r\n";
        let params = parse_query_params(req);
        assert_eq!(params.get("code").unwrap(), "hello world");
    }

    #[test]
    fn parse_query_no_query_string() {
        let req = "GET /callback HTTP/1.1\r\n";
        let params = parse_query_params(req);
        assert!(params.is_empty());
    }

    #[test]
    fn parse_query_empty_request() {
        let params = parse_query_params("");
        assert!(params.is_empty());
    }

    // ── validate_callback 测试 ──────────────────────────────────────

    #[test]
    fn validate_success() {
        let mut params = HashMap::new();
        params.insert("code".into(), "auth-code".into());
        params.insert("state".into(), "my-state".into());
        let result = validate_callback(&params, "my-state").unwrap();
        assert_eq!(result.code, "auth-code");
        assert_eq!(result.state, "my-state");
    }

    #[test]
    fn validate_missing_code() {
        let mut params = HashMap::new();
        params.insert("state".into(), "s".into());
        let err = validate_callback(&params, "s").unwrap_err();
        assert_eq!(err, CallbackError::MissingCode);
    }

    #[test]
    fn validate_missing_state() {
        let mut params = HashMap::new();
        params.insert("code".into(), "c".into());
        let err = validate_callback(&params, "s").unwrap_err();
        assert_eq!(err, CallbackError::MissingState);
    }

    #[test]
    fn validate_state_mismatch() {
        let mut params = HashMap::new();
        params.insert("code".into(), "c".into());
        params.insert("state".into(), "wrong".into());
        let err = validate_callback(&params, "expected").unwrap_err();
        assert_eq!(
            err,
            CallbackError::StateMismatch {
                expected: "expected".into(),
                got: "wrong".into()
            }
        );
    }

    #[test]
    fn validate_access_denied() {
        let mut params = HashMap::new();
        params.insert("error".into(), "access_denied".into());
        params.insert("error_description".into(), "user denied access".into());
        let err = validate_callback(&params, "s").unwrap_err();
        assert_eq!(
            err,
            CallbackError::AccessDenied("user denied access".into())
        );
    }

    #[test]
    fn validate_access_denied_no_description() {
        let mut params = HashMap::new();
        params.insert("error".into(), "access_denied".into());
        let err = validate_callback(&params, "s").unwrap_err();
        assert_eq!(err, CallbackError::AccessDenied("access_denied".into()));
    }

    // ── HTTP 响应测试 ───────────────────────────────────────────────

    #[test]
    fn http_response_format() {
        let resp = http_response(200, "OK", "hello");
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp.contains("Content-Length: 5"));
        assert!(resp.ends_with("hello"));
    }

    #[test]
    fn success_html_contains_message() {
        let html = success_html();
        assert!(html.contains("Authorization successful"));
        assert!(html.contains("close this window"));
    }

    #[test]
    fn error_html_contains_message() {
        let html = error_html("something went wrong");
        assert!(html.contains("something went wrong"));
        assert!(html.contains("Authorization failed"));
    }

    // ── 集成测试：真实 TCP 连接 ─────────────────────────────────────

    async fn find_free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn server_receives_valid_callback() {
        let port = find_free_port().await;
        let state = "test-state-123";

        let server = tokio::spawn(async move { wait_for_callback(port, state, 5).await });

        // 等服务器启动
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 模拟浏览器回调
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let request = format!(
            "GET /callback?code=my-auth-code&state={} HTTP/1.1\r\nHost: localhost\r\n\r\n",
            state
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        // 读取响应
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.contains("200 OK"));
        assert!(response.contains("Authorization successful"));

        // 验证服务器返回的结果
        let result = server.await.unwrap().unwrap();
        assert_eq!(result.code, "my-auth-code");
        assert_eq!(result.state, state);
    }

    #[tokio::test]
    async fn server_rejects_wrong_state() {
        let port = find_free_port().await;

        let server = tokio::spawn(async move { wait_for_callback(port, "correct-state", 5).await });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let request = "GET /callback?code=c&state=wrong-state HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.contains("400 Bad Request"));

        let err = server.await.unwrap().unwrap_err();
        assert_eq!(
            err,
            CallbackError::StateMismatch {
                expected: "correct-state".into(),
                got: "wrong-state".into()
            }
        );
    }

    #[tokio::test]
    async fn server_handles_access_denied() {
        let port = find_free_port().await;

        let server = tokio::spawn(async move { wait_for_callback(port, "s", 5).await });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let request =
            "GET /callback?error=access_denied&error_description=user+refused HTTP/1.1\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();

        let err = server.await.unwrap().unwrap_err();
        assert_eq!(err, CallbackError::AccessDenied("user refused".into()));
    }

    #[tokio::test]
    async fn server_timeout() {
        let port = find_free_port().await;
        let result = wait_for_callback(port, "s", 1).await;
        assert_eq!(result.unwrap_err(), CallbackError::Timeout);
    }
}
