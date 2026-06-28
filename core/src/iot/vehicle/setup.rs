//! 车辆绑定向导
//!
//! 统一的 VehicleSetup trait 抽象 Tesla / SmartCar / OBD-II 三种绑定流程。
//! 每种流程封装为独立的 SetupWizard 实现，对外提供一致的接口。

use super::oauth::{OAuthClient, OAuthConfig, PkceChallenge, TokenStore};
use super::oauth_callback::wait_for_callback;
use super::obd::{Elm327, MockSerialPort, SerialPort};
use std::collections::HashMap;

/// 向导步骤状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStep {
    /// 等待用户输入配置
    AwaitingConfig,
    /// 等待用户在浏览器中授权
    AwaitingAuthorization { url: String },
    /// 正在交换 Token
    ExchangingToken,
    /// 正在发现车辆
    DiscoveringVehicles,
    /// 完成
    Complete { device_ids: Vec<String> },
    /// 失败
    Failed { reason: String },
}

/// 车辆绑定配置
#[derive(Debug, Clone)]
pub enum VehicleSetupConfig {
    Tesla(TeslaSetupConfig),
    SmartCar(SmartCarSetupConfig),
    Obd(ObdSetupConfig),
}

#[derive(Debug, Clone)]
pub struct TeslaSetupConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub region: String, // "na", "eu", "cn"
}

#[derive(Debug, Clone)]
pub struct SmartCarSetupConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct ObdSetupConfig {
    pub port_name: String,
}

/// 回调服务器默认端口
const CALLBACK_PORT: u16 = 8765;
/// 回调超时（秒）
const CALLBACK_TIMEOUT_SECS: u64 = 300;

/// Tesla OAuth2 端点
const TESLA_AUTH_URL: &str = "https://auth.tesla.com/oauth2/v3/authorize";
const TESLA_TOKEN_URL: &str = "https://auth.tesla.com/oauth2/v3/token";

/// SmartCar OAuth2 端点
const SMARTCAR_AUTH_URL: &str = "https://connect.smartcar.com/oauth/authorize";
const SMARTCAR_TOKEN_URL: &str = "https://auth.smartcar.com/oauth/token";

/// 向导结果
#[derive(Debug, Clone)]
pub struct SetupResult {
    pub device_id: String,
    pub provider: String,
    pub token: Option<TokenStore>,
    pub metadata: HashMap<String, String>,
}

/// Tesla 绑定向导
pub struct TeslaSetupWizard {
    config: TeslaSetupConfig,
    callback_port: u16,
    /// 测试用：覆盖 token_url base
    base_url_override: Option<String>,
}

impl TeslaSetupWizard {
    pub fn new(config: TeslaSetupConfig) -> Self {
        Self {
            config,
            callback_port: CALLBACK_PORT,
            base_url_override: None,
        }
    }

    pub fn with_callback_port(mut self, port: u16) -> Self {
        self.callback_port = port;
        self
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.callback_port)
    }

    fn build_oauth_config(&self) -> OAuthConfig {
        OAuthConfig {
            client_id: self.config.client_id.clone(),
            client_secret: self.config.client_secret.clone(),
            auth_url: TESLA_AUTH_URL.to_string(),
            token_url: TESLA_TOKEN_URL.to_string(),
            redirect_uri: self.redirect_uri(),
            scopes: vec![
                "openid".into(),
                "vehicle_device_data".into(),
                "vehicle_cmds".into(),
                "vehicle_charging_cmds".into(),
            ],
        }
    }

    /// 生成授权 URL 和 PKCE
    pub fn authorization_url(&self) -> (String, String, PkceChallenge) {
        let oauth = self.build_oauth_client();
        oauth.build_authorization_url()
    }

    fn build_oauth_client(&self) -> OAuthClient {
        let config = self.build_oauth_config();
        let client = OAuthClient::new(config);
        match &self.base_url_override {
            Some(url) => client.with_base_url(url),
            None => client,
        }
    }

    /// 准备绑定：生成授权 URL，返回 (url, state, pkce, oauth_client)
    pub fn prepare(&self) -> (String, String, PkceChallenge, OAuthClient) {
        let oauth = self.build_oauth_client();
        let (url, state, pkce) = oauth.build_authorization_url();
        (url, state, pkce, oauth)
    }

    /// 等待回调并换取 Token（使用 prepare() 返回的参数）
    pub async fn complete(
        &self,
        state: &str,
        pkce: &PkceChallenge,
        oauth: &OAuthClient,
        auth_url: &str,
    ) -> Result<SetupResult, String> {
        let callback = wait_for_callback(self.callback_port, state, CALLBACK_TIMEOUT_SECS)
            .await
            .map_err(|e| e.to_string())?;

        let token = oauth
            .exchange_code(&callback.code, &pkce.verifier)
            .await
            .map_err(|e| format!("token exchange failed: {}", e))?;

        let mut metadata = HashMap::new();
        metadata.insert("provider".into(), "tesla".into());
        metadata.insert("region".into(), self.config.region.clone());
        metadata.insert("auth_url".into(), auth_url.to_string());

        Ok(SetupResult {
            device_id: format!(
                "tesla-{}",
                &self.config.client_id[..8.min(self.config.client_id.len())]
            ),
            provider: "tesla".into(),
            token: Some(token),
            metadata,
        })
    }

    /// 执行完整绑定流程：生成 URL → 等待回调 → 换取 Token
    pub async fn run(&self) -> Result<SetupResult, String> {
        let (url, state, pkce, oauth) = self.prepare();
        self.complete(&state, &pkce, &oauth, &url).await
    }
}

/// SmartCar 绑定向导
pub struct SmartCarSetupWizard {
    config: SmartCarSetupConfig,
    callback_port: u16,
    base_url_override: Option<String>,
}

impl SmartCarSetupWizard {
    pub fn new(config: SmartCarSetupConfig) -> Self {
        Self {
            config,
            callback_port: CALLBACK_PORT,
            base_url_override: None,
        }
    }

    pub fn with_callback_port(mut self, port: u16) -> Self {
        self.callback_port = port;
        self
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.callback_port)
    }

    fn build_oauth_config(&self) -> OAuthConfig {
        OAuthConfig {
            client_id: self.config.client_id.clone(),
            client_secret: Some(self.config.client_secret.clone()),
            auth_url: SMARTCAR_AUTH_URL.to_string(),
            token_url: SMARTCAR_TOKEN_URL.to_string(),
            redirect_uri: self.redirect_uri(),
            scopes: vec![
                "read_vehicle_info".into(),
                "read_location".into(),
                "read_odometer".into(),
                "control_security".into(),
                "read_fuel".into(),
                "read_battery".into(),
                "control_charge".into(),
            ],
        }
    }

    /// 生成授权 URL 和 PKCE
    pub fn authorization_url(&self) -> (String, String, PkceChallenge) {
        let oauth = self.build_oauth_client();
        oauth.build_authorization_url()
    }

    fn build_oauth_client(&self) -> OAuthClient {
        let config = self.build_oauth_config();
        let client = OAuthClient::new(config);
        match &self.base_url_override {
            Some(url) => client.with_base_url(url),
            None => client,
        }
    }

    /// 准备绑定：生成授权 URL，返回 (url, state, pkce, oauth_client)
    pub fn prepare(&self) -> (String, String, PkceChallenge, OAuthClient) {
        let oauth = self.build_oauth_client();
        let (url, state, pkce) = oauth.build_authorization_url();
        (url, state, pkce, oauth)
    }

    /// 等待回调并换取 Token
    pub async fn complete(
        &self,
        state: &str,
        pkce: &PkceChallenge,
        oauth: &OAuthClient,
        auth_url: &str,
    ) -> Result<SetupResult, String> {
        let callback = wait_for_callback(self.callback_port, state, CALLBACK_TIMEOUT_SECS)
            .await
            .map_err(|e| e.to_string())?;

        let token = oauth
            .exchange_code(&callback.code, &pkce.verifier)
            .await
            .map_err(|e| format!("token exchange failed: {}", e))?;

        let mut metadata = HashMap::new();
        metadata.insert("provider".into(), "smartcar".into());
        metadata.insert("auth_url".into(), auth_url.to_string());

        Ok(SetupResult {
            device_id: format!(
                "smartcar-{}",
                &self.config.client_id[..8.min(self.config.client_id.len())]
            ),
            provider: "smartcar".into(),
            token: Some(token),
            metadata,
        })
    }

    /// 执行完整绑定流程
    pub async fn run(&self) -> Result<SetupResult, String> {
        let (url, state, pkce, oauth) = self.prepare();
        self.complete(&state, &pkce, &oauth, &url).await
    }
}

/// OBD-II 绑定向导
pub struct ObdSetupWizard {
    config: ObdSetupConfig,
    /// 测试用：注入 mock 串口
    mock_port: Option<MockSerialPort>,
}

impl ObdSetupWizard {
    pub fn new(config: ObdSetupConfig) -> Self {
        Self {
            config,
            mock_port: None,
        }
    }

    /// 测试用：注入 mock 串口
    pub fn with_mock_port(mut self, port: MockSerialPort) -> Self {
        self.mock_port = Some(port);
        self
    }

    /// 执行 OBD 连接验证：初始化 ELM327 → 读取诊断数据
    pub fn run(&mut self) -> Result<SetupResult, String> {
        let port: Box<dyn SerialPort> = match self.mock_port.take() {
            Some(mock) => Box::new(mock),
            None => {
                return Err(format!(
                    "real serial port '{}' not available (serialport crate not linked)",
                    self.config.port_name
                ));
            }
        };

        let mut elm = Elm327::new(port);
        elm.init()
            .map_err(|e| format!("ELM327 init failed: {}", e))?;

        let diag = elm
            .read_all_diagnostics()
            .map_err(|e| format!("diagnostics read failed: {}", e))?;

        let mut metadata = HashMap::new();
        metadata.insert("provider".into(), "obd".into());
        metadata.insert("port".into(), self.config.port_name.clone());
        if let Some(rpm) = diag.rpm {
            metadata.insert("rpm".into(), format!("{:.0}", rpm));
        }
        if let Some(speed) = diag.speed_kmh {
            metadata.insert("speed_kmh".into(), format!("{:.0}", speed));
        }

        Ok(SetupResult {
            device_id: format!("obd-{}", self.config.port_name.replace('/', "-")),
            provider: "obd".into(),
            token: None,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use tokio::io::AsyncWriteExt;
    use tokio::net::{TcpListener, TcpStream};

    // ── Tesla 向导测试 ──────────────────────────────────────────────

    #[test]
    fn tesla_authorization_url_contains_required_params() {
        let wizard = TeslaSetupWizard::new(TeslaSetupConfig {
            client_id: "tesla-client-123".into(),
            client_secret: None,
            region: "na".into(),
        });
        let (url, state, pkce) = wizard.authorization_url();
        assert!(url.contains("auth.tesla.com"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("tesla-client-123"));
        assert!(url.contains(&state));
        assert!(url.contains("code_challenge="));
        assert!(!pkce.verifier.is_empty());
    }

    #[test]
    fn tesla_redirect_uri_uses_callback_port() {
        let wizard = TeslaSetupWizard::new(TeslaSetupConfig {
            client_id: "c".into(),
            client_secret: None,
            region: "na".into(),
        })
        .with_callback_port(9999);
        assert_eq!(wizard.redirect_uri(), "http://127.0.0.1:9999/callback");
    }

    #[test]
    fn tesla_scopes_include_vehicle_cmds() {
        let wizard = TeslaSetupWizard::new(TeslaSetupConfig {
            client_id: "c".into(),
            client_secret: None,
            region: "na".into(),
        });
        let config = wizard.build_oauth_config();
        assert!(config.scopes.contains(&"vehicle_cmds".to_string()));
        assert!(config.scopes.contains(&"vehicle_device_data".to_string()));
    }

    #[tokio::test]
    async fn tesla_full_setup_flow() {
        let mock_server = MockServer::start();
        mock_server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200).json_body(json!({
                "access_token": "tesla-access-tok",
                "refresh_token": "tesla-refresh-tok",
                "expires_in": 3600,
                "token_type": "Bearer",
                "scope": "openid vehicle_device_data"
            }));
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let wizard = TeslaSetupWizard::new(TeslaSetupConfig {
            client_id: "tesla-test-client".into(),
            client_secret: Some("secret".into()),
            region: "na".into(),
        })
        .with_callback_port(port)
        .with_base_url(&mock_server.base_url());

        // prepare() 返回一致的 state
        let (url, state, pkce, oauth) = wizard.prepare();
        let state_clone = state.clone();

        let handle =
            tokio::spawn(async move { wizard.complete(&state, &pkce, &oauth, &url).await });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let req = format!(
            "GET /callback?code=tesla-auth-code&state={} HTTP/1.1\r\nHost: localhost\r\n\r\n",
            state_clone
        );
        stream.write_all(req.as_bytes()).await.unwrap();

        let result = handle.await.unwrap().unwrap();
        assert_eq!(result.provider, "tesla");
        assert_eq!(result.token.unwrap().access_token, "tesla-access-tok");
        assert_eq!(result.metadata["region"], "na");
    }

    // ── SmartCar 向导测试 ───────────────────────────────────────────

    #[test]
    fn smartcar_authorization_url_contains_required_params() {
        let wizard = SmartCarSetupWizard::new(SmartCarSetupConfig {
            client_id: "sc-client-456".into(),
            client_secret: "sc-secret".into(),
        });
        let (url, state, pkce) = wizard.authorization_url();
        assert!(url.contains("connect.smartcar.com"));
        assert!(url.contains("sc-client-456"));
        assert!(url.contains(&state));
        assert!(!pkce.verifier.is_empty());
    }

    #[test]
    fn smartcar_scopes_include_vehicle_control() {
        let wizard = SmartCarSetupWizard::new(SmartCarSetupConfig {
            client_id: "c".into(),
            client_secret: "s".into(),
        });
        let config = wizard.build_oauth_config();
        assert!(config.scopes.contains(&"read_location".to_string()));
        assert!(config.scopes.contains(&"control_security".to_string()));
        assert!(config.scopes.contains(&"control_charge".to_string()));
    }

    #[tokio::test]
    async fn smartcar_full_setup_flow() {
        let mock_server = MockServer::start();
        mock_server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200).json_body(json!({
                "access_token": "sc-access-tok",
                "refresh_token": "sc-refresh-tok",
                "expires_in": 7200,
                "token_type": "Bearer"
            }));
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let wizard = SmartCarSetupWizard::new(SmartCarSetupConfig {
            client_id: "smartcar-test-id".into(),
            client_secret: "smartcar-secret".into(),
        })
        .with_callback_port(port)
        .with_base_url(&mock_server.base_url());

        let (url, state, pkce, oauth) = wizard.prepare();
        let state_clone = state.clone();

        let handle =
            tokio::spawn(async move { wizard.complete(&state, &pkce, &oauth, &url).await });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let req = format!(
            "GET /callback?code=sc-auth-code&state={} HTTP/1.1\r\nHost: localhost\r\n\r\n",
            state_clone
        );
        stream.write_all(req.as_bytes()).await.unwrap();

        let result = handle.await.unwrap().unwrap();
        assert_eq!(result.provider, "smartcar");
        assert_eq!(result.token.unwrap().access_token, "sc-access-tok");
    }

    // ── OBD 向导测试 ───────────────────────────────────────────────

    #[test]
    fn obd_setup_with_mock_port() {
        let mock = MockSerialPort::new()
            .expect("ATZ", "ELM327 v1.5")
            .expect("ATE0", "OK")
            .expect("ATL0", "OK")
            .expect("ATS0", "OK")
            .expect("ATSP0", "OK")
            .expect("010C", "41 0C 0B B8") // RPM = 750
            .expect("010D", "41 0D 00") // Speed = 0
            .expect("0105", "41 05 50") // Temp = 40°C
            .expect("012F", "41 2F 80") // Fuel = 50.2%
            .expect("0142", "41 42 2E E0"); // Voltage = 12.0V

        let mut wizard = ObdSetupWizard::new(ObdSetupConfig {
            port_name: "/dev/ttyUSB0".into(),
        })
        .with_mock_port(mock);

        let result = wizard.run().unwrap();
        assert_eq!(result.provider, "obd");
        assert_eq!(result.device_id, "obd--dev-ttyUSB0");
        assert!(result.token.is_none());
        assert_eq!(result.metadata["port"], "/dev/ttyUSB0");
        assert_eq!(result.metadata["rpm"], "750");
        assert_eq!(result.metadata["speed_kmh"], "0");
    }

    #[test]
    fn obd_setup_init_failure() {
        let mock = MockSerialPort::new().expect("ATZ", "ERROR");

        let mut wizard = ObdSetupWizard::new(ObdSetupConfig {
            port_name: "/dev/ttyUSB0".into(),
        })
        .with_mock_port(mock);

        let err = wizard.run().unwrap_err();
        assert!(err.contains("ELM327 init failed"));
    }

    #[test]
    fn obd_setup_no_serial_port() {
        let mut wizard = ObdSetupWizard::new(ObdSetupConfig {
            port_name: "/dev/nonexistent".into(),
        });

        let err = wizard.run().unwrap_err();
        assert!(err.contains("not available"));
    }

    // ── SetupStep / SetupResult 测试 ────────────────────────────────

    #[test]
    fn setup_step_equality() {
        assert_eq!(SetupStep::AwaitingConfig, SetupStep::AwaitingConfig);
        assert_ne!(SetupStep::AwaitingConfig, SetupStep::ExchangingToken);
    }

    #[test]
    fn setup_result_metadata() {
        let result = SetupResult {
            device_id: "test".into(),
            provider: "tesla".into(),
            token: None,
            metadata: HashMap::new(),
        };
        assert_eq!(result.device_id, "test");
        assert!(result.token.is_none());
    }

    #[test]
    fn tesla_device_id_format() {
        let wizard = TeslaSetupWizard::new(TeslaSetupConfig {
            client_id: "abcdefghijklmnop".into(),
            client_secret: None,
            region: "eu".into(),
        });
        // device_id 使用 client_id 前 8 字符
        let config = wizard.build_oauth_config();
        assert_eq!(config.client_id, "abcdefghijklmnop");
    }

    #[test]
    fn smartcar_device_id_short_client() {
        let wizard = SmartCarSetupWizard::new(SmartCarSetupConfig {
            client_id: "abc".into(),
            client_secret: "s".into(),
        });
        let config = wizard.build_oauth_config();
        assert_eq!(config.client_id, "abc");
    }
}
