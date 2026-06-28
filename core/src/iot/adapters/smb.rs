//! SMB/NAS adapter
//!
//! Controls network storage devices via an `SmbTransport` trait abstraction.
//! Capabilities: FileShare, StorageInfo
//! Implements SMB2 protocol operations with trait-based mocking for testability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// SMB share information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmbShare {
    pub name: String,
    pub share_type: String,
    pub comment: String,
}

/// SMB directory entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmbEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: u64, // Unix timestamp
}

/// SMB file information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmbFileInfo {
    pub size: u64,
    pub modified: u64,
    pub created: u64,
    pub is_directory: bool,
    pub is_readonly: bool,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// SMB connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmbConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub domain: Option<String>,
}

impl SmbConfig {
    pub fn new(host: String, username: String, password: String) -> Self {
        Self {
            host,
            port: 445,
            username,
            password,
            domain: None,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_domain(mut self, domain: String) -> Self {
        self.domain = Some(domain);
        self
    }
}

// ---------------------------------------------------------------------------
// Transport trait
// ---------------------------------------------------------------------------

/// Abstraction over SMB2 protocol operations for testability.
#[async_trait]
pub trait SmbTransport: Send + Sync {
    /// Connect to SMB server.
    async fn connect(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> anyhow::Result<()>;

    /// Disconnect from SMB server.
    async fn disconnect(&mut self) -> anyhow::Result<()>;

    /// List available shares.
    async fn list_shares(&self) -> anyhow::Result<Vec<SmbShare>>;

    /// List directory contents.
    async fn list_directory(&self, share: &str, path: &str) -> anyhow::Result<Vec<SmbEntry>>;

    /// Read file contents.
    async fn read_file(&self, share: &str, path: &str) -> anyhow::Result<Vec<u8>>;

    /// Write file contents.
    async fn write_file(&self, share: &str, path: &str, data: &[u8]) -> anyhow::Result<()>;

    /// Delete a file.
    async fn delete_file(&self, share: &str, path: &str) -> anyhow::Result<()>;

    /// Create a directory.
    async fn create_directory(&self, share: &str, path: &str) -> anyhow::Result<()>;

    /// Get file information.
    async fn file_info(&self, share: &str, path: &str) -> anyhow::Result<SmbFileInfo>;
}

// ---------------------------------------------------------------------------
// Mock transport
// ---------------------------------------------------------------------------

/// Expected call for mock transport.
#[derive(Debug, Clone)]
pub enum MockCall {
    Connect {
        host: String,
        port: u16,
        username: String,
        password: String,
    },
    Disconnect,
    ListShares,
    ListDirectory {
        share: String,
        path: String,
    },
    ReadFile {
        share: String,
        path: String,
    },
    WriteFile {
        share: String,
        path: String,
        data: Vec<u8>,
    },
    DeleteFile {
        share: String,
        path: String,
    },
    CreateDirectory {
        share: String,
        path: String,
    },
    FileInfo {
        share: String,
        path: String,
    },
}

/// Expected response for mock transport.
#[derive(Debug, Clone)]
pub enum MockResponse {
    Ok,
    Shares(Vec<SmbShare>),
    Entries(Vec<SmbEntry>),
    Data(Vec<u8>),
    Info(SmbFileInfo),
    Error(String),
}

/// Mock SMB transport that validates expected calls and returns canned responses.
pub struct MockSmbTransport {
    pub expectations: Arc<RwLock<Vec<(MockCall, MockResponse)>>>,
    pub calls: Arc<RwLock<Vec<MockCall>>>,
}

impl MockSmbTransport {
    pub fn new() -> Self {
        Self {
            expectations: Arc::new(RwLock::new(Vec::new())),
            calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn expect(&self, call: MockCall, response: MockResponse) {
        self.expectations.write().await.push((call, response));
    }

    pub async fn calls(&self) -> Vec<MockCall> {
        self.calls.read().await.clone()
    }

    async fn handle_call(&self, call: MockCall) -> anyhow::Result<MockResponse> {
        self.calls.write().await.push(call.clone());

        let mut expectations = self.expectations.write().await;
        if expectations.is_empty() {
            anyhow::bail!("unexpected call: {:?}", call);
        }

        let (expected_call, response) = expectations.remove(0);

        // Match call types
        match (&call, &expected_call) {
            (MockCall::Connect { .. }, MockCall::Connect { .. }) => {}
            (MockCall::Disconnect, MockCall::Disconnect) => {}
            (MockCall::ListShares, MockCall::ListShares) => {}
            (MockCall::ListDirectory { share: s1, path: p1 }, MockCall::ListDirectory { share: s2, path: p2 }) => {
                if s1 != s2 || p1 != p2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            (MockCall::ReadFile { share: s1, path: p1 }, MockCall::ReadFile { share: s2, path: p2 }) => {
                if s1 != s2 || p1 != p2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            (MockCall::WriteFile { share: s1, path: p1, data: d1 }, MockCall::WriteFile { share: s2, path: p2, data: d2 }) => {
                if s1 != s2 || p1 != p2 || d1 != d2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            (MockCall::DeleteFile { share: s1, path: p1 }, MockCall::DeleteFile { share: s2, path: p2 }) => {
                if s1 != s2 || p1 != p2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            (MockCall::CreateDirectory { share: s1, path: p1 }, MockCall::CreateDirectory { share: s2, path: p2 }) => {
                if s1 != s2 || p1 != p2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            (MockCall::FileInfo { share: s1, path: p1 }, MockCall::FileInfo { share: s2, path: p2 }) => {
                if s1 != s2 || p1 != p2 {
                    anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call);
                }
            }
            _ => anyhow::bail!("call mismatch: expected {:?}, got {:?}", expected_call, call),
        }

        Ok(response)
    }
}

#[async_trait]
impl SmbTransport for MockSmbTransport {
    async fn connect(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> anyhow::Result<()> {
        let response = self.handle_call(MockCall::Connect {
            host: host.to_string(),
            port,
            username: username.to_string(),
            password: password.to_string(),
        }).await?;

        match response {
            MockResponse::Ok => Ok(()),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        let response = self.handle_call(MockCall::Disconnect).await?;

        match response {
            MockResponse::Ok => Ok(()),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn list_shares(&self) -> anyhow::Result<Vec<SmbShare>> {
        let response = self.handle_call(MockCall::ListShares).await?;

        match response {
            MockResponse::Shares(shares) => Ok(shares),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn list_directory(&self, share: &str, path: &str) -> anyhow::Result<Vec<SmbEntry>> {
        let response = self.handle_call(MockCall::ListDirectory {
            share: share.to_string(),
            path: path.to_string(),
        }).await?;

        match response {
            MockResponse::Entries(entries) => Ok(entries),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn read_file(&self, share: &str, path: &str) -> anyhow::Result<Vec<u8>> {
        let response = self.handle_call(MockCall::ReadFile {
            share: share.to_string(),
            path: path.to_string(),
        }).await?;

        match response {
            MockResponse::Data(data) => Ok(data),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn write_file(&self, share: &str, path: &str, data: &[u8]) -> anyhow::Result<()> {
        let response = self.handle_call(MockCall::WriteFile {
            share: share.to_string(),
            path: path.to_string(),
            data: data.to_vec(),
        }).await?;

        match response {
            MockResponse::Ok => Ok(()),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn delete_file(&self, share: &str, path: &str) -> anyhow::Result<()> {
        let response = self.handle_call(MockCall::DeleteFile {
            share: share.to_string(),
            path: path.to_string(),
        }).await?;

        match response {
            MockResponse::Ok => Ok(()),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn create_directory(&self, share: &str, path: &str) -> anyhow::Result<()> {
        let response = self.handle_call(MockCall::CreateDirectory {
            share: share.to_string(),
            path: path.to_string(),
        }).await?;

        match response {
            MockResponse::Ok => Ok(()),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }

    async fn file_info(&self, share: &str, path: &str) -> anyhow::Result<SmbFileInfo> {
        let response = self.handle_call(MockCall::FileInfo {
            share: share.to_string(),
            path: path.to_string(),
        }).await?;

        match response {
            MockResponse::Info(info) => Ok(info),
            MockResponse::Error(e) => anyhow::bail!(e),
            _ => anyhow::bail!("unexpected response type"),
        }
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// SMB adapter using a pluggable transport.
pub struct SmbAdapter {
    transport: Arc<RwLock<Box<dyn SmbTransport>>>,
    config: SmbConfig,
    connected: Arc<RwLock<bool>>,
}

impl SmbAdapter {
    pub fn new(config: SmbConfig, transport: Box<dyn SmbTransport>) -> Self {
        Self {
            transport: Arc::new(RwLock::new(transport)),
            config,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Create adapter with mock transport for testing.
    pub fn with_transport(config: SmbConfig, transport: Box<dyn SmbTransport>) -> Self {
        Self::new(config, transport)
    }

    /// Connect to SMB server.
    pub async fn connect(&self) -> anyhow::Result<()> {
        let mut transport = self.transport.write().await;
        transport.connect(
            &self.config.host,
            self.config.port,
            &self.config.username,
            &self.config.password,
        ).await?;
        *self.connected.write().await = true;
        Ok(())
    }

    /// Disconnect from SMB server.
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        let mut transport = self.transport.write().await;
        transport.disconnect().await?;
        *self.connected.write().await = false;
        Ok(())
    }

    /// List available shares.
    pub async fn list_shares(&self) -> anyhow::Result<Vec<SmbShare>> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.list_shares().await
    }

    /// List directory contents.
    pub async fn list_directory(&self, share: &str, path: &str) -> anyhow::Result<Vec<SmbEntry>> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.list_directory(share, path).await
    }

    /// Read file contents.
    pub async fn read_file(&self, share: &str, path: &str) -> anyhow::Result<Vec<u8>> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.read_file(share, path).await
    }

    /// Write file contents.
    pub async fn write_file(&self, share: &str, path: &str, data: &[u8]) -> anyhow::Result<()> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.write_file(share, path, data).await
    }

    /// Delete a file.
    pub async fn delete_file(&self, share: &str, path: &str) -> anyhow::Result<()> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.delete_file(share, path).await
    }

    /// Create a directory.
    pub async fn create_directory(&self, share: &str, path: &str) -> anyhow::Result<()> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.create_directory(share, path).await
    }

    /// Get file information.
    pub async fn file_info(&self, share: &str, path: &str) -> anyhow::Result<SmbFileInfo> {
        if !*self.connected.read().await {
            anyhow::bail!("not connected");
        }
        let transport = self.transport.read().await;
        transport.file_info(share, path).await
    }
}

// ---------------------------------------------------------------------------
// DeviceController implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for SmbAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        self.connect().await
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        self.disconnect().await
    }

    async fn set_property(&self, _device_id: &str, property: &str, value: Value) -> anyhow::Result<()> {
        match property {
            "write_file" => {
                let share = value["share"].as_str().ok_or_else(|| anyhow::anyhow!("missing share"))?;
                let path = value["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
                let data = value["data"].as_str().ok_or_else(|| anyhow::anyhow!("missing data"))?;
                self.write_file(share, path, data.as_bytes()).await
            }
            "delete_file" => {
                let share = value["share"].as_str().ok_or_else(|| anyhow::anyhow!("missing share"))?;
                let path = value["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
                self.delete_file(share, path).await
            }
            "create_directory" => {
                let share = value["share"].as_str().ok_or_else(|| anyhow::anyhow!("missing share"))?;
                let path = value["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
                self.create_directory(share, path).await
            }
            _ => anyhow::bail!("unsupported property: {}", property),
        }
    }

    async fn get_state(&self, _device_id: &str) -> anyhow::Result<Value> {
        let connected = *self.connected.read().await;
        let shares = if connected {
            self.list_shares().await.ok()
        } else {
            None
        };

        Ok(serde_json::json!({
            "connected": connected,
            "host": self.config.host,
            "port": self.config.port,
            "shares": shares,
        }))
    }

    fn supported_type(&self) -> &str {
        "FileShare"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (SmbAdapter, Arc<RwLock<MockSmbTransport>>) {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = Arc::new(RwLock::new(MockSmbTransport::new()));
        let adapter = SmbAdapter::with_transport(config, Box::new(MockSmbTransport::new()));
        (adapter, mock)
    }

    fn setup_with_shared_mock() -> (SmbAdapter, Arc<MockSmbTransport>) {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = Arc::new(MockSmbTransport::new());
        let mock_clone = mock.clone();
        (SmbAdapter::new(config, Box::new(MockSmbTransport::new())), mock_clone)
    }

    // 1. connect success
    #[tokio::test]
    async fn connect_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        assert!(*adapter.connected.read().await);
    }

    // 2. connect failure
    #[tokio::test]
    async fn connect_failure() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "wrongpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "wrongpass".to_string(),
            },
            MockResponse::Error("authentication failed".to_string()),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        assert!(adapter.connect().await.is_err());
        assert!(!*adapter.connected.read().await);
    }

    // 3. disconnect success
    #[tokio::test]
    async fn disconnect_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(MockCall::Disconnect, MockResponse::Ok).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        adapter.disconnect().await.unwrap();
        assert!(!*adapter.connected.read().await);
    }

    // 4. list_shares success
    #[tokio::test]
    async fn list_shares_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::ListShares,
            MockResponse::Shares(vec![
                SmbShare {
                    name: "Documents".to_string(),
                    share_type: "DISK".to_string(),
                    comment: "Shared documents".to_string(),
                },
                SmbShare {
                    name: "Media".to_string(),
                    share_type: "DISK".to_string(),
                    comment: "Media files".to_string(),
                },
            ]),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        let shares = adapter.list_shares().await.unwrap();
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "Documents");
        assert_eq!(shares[1].name, "Media");
    }

    // 5. list_shares not connected
    #[tokio::test]
    async fn list_shares_not_connected() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        assert!(adapter.list_shares().await.is_err());
    }

    // 6. list_directory success
    #[tokio::test]
    async fn list_directory_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::ListDirectory {
                share: "Documents".to_string(),
                path: "/".to_string(),
            },
            MockResponse::Entries(vec![
                SmbEntry {
                    name: "file1.txt".to_string(),
                    is_directory: false,
                    size: 1024,
                    modified: 1234567890,
                },
                SmbEntry {
                    name: "folder1".to_string(),
                    is_directory: true,
                    size: 0,
                    modified: 1234567891,
                },
            ]),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        let entries = adapter.list_directory("Documents", "/").await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "file1.txt");
        assert!(!entries[0].is_directory);
        assert_eq!(entries[1].name, "folder1");
        assert!(entries[1].is_directory);
    }

    // 7. read_file success
    #[tokio::test]
    async fn read_file_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::ReadFile {
                share: "Documents".to_string(),
                path: "test.txt".to_string(),
            },
            MockResponse::Data(b"Hello, World!".to_vec()),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        let data = adapter.read_file("Documents", "test.txt").await.unwrap();
        assert_eq!(data, b"Hello, World!");
    }

    // 8. read_file not found
    #[tokio::test]
    async fn read_file_not_found() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::ReadFile {
                share: "Documents".to_string(),
                path: "missing.txt".to_string(),
            },
            MockResponse::Error("file not found".to_string()),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        assert!(adapter.read_file("Documents", "missing.txt").await.is_err());
    }

    // 9. write_file success
    #[tokio::test]
    async fn write_file_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::WriteFile {
                share: "Documents".to_string(),
                path: "new.txt".to_string(),
                data: b"New content".to_vec(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        adapter.write_file("Documents", "new.txt", b"New content").await.unwrap();
    }

    // 10. delete_file success
    #[tokio::test]
    async fn delete_file_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::DeleteFile {
                share: "Documents".to_string(),
                path: "old.txt".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        adapter.delete_file("Documents", "old.txt").await.unwrap();
    }

    // 11. create_directory success
    #[tokio::test]
    async fn create_directory_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::CreateDirectory {
                share: "Documents".to_string(),
                path: "newfolder".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        adapter.create_directory("Documents", "newfolder").await.unwrap();
    }

    // 12. file_info success
    #[tokio::test]
    async fn file_info_success() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::FileInfo {
                share: "Documents".to_string(),
                path: "test.txt".to_string(),
            },
            MockResponse::Info(SmbFileInfo {
                size: 2048,
                modified: 1234567890,
                created: 1234567800,
                is_directory: false,
                is_readonly: false,
            }),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.connect().await.unwrap();
        let info = adapter.file_info("Documents", "test.txt").await.unwrap();
        assert_eq!(info.size, 2048);
        assert!(!info.is_directory);
        assert!(!info.is_readonly);
    }

    // 13. DeviceController turn_on
    #[tokio::test]
    async fn device_controller_turn_on() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        assert!(*adapter.connected.read().await);
    }

    // 14. DeviceController turn_off
    #[tokio::test]
    async fn device_controller_turn_off() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(MockCall::Disconnect, MockResponse::Ok).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        adapter.turn_off("smb-001").await.unwrap();
        assert!(!*adapter.connected.read().await);
    }

    // 15. DeviceController set_property write_file
    #[tokio::test]
    async fn device_controller_set_property_write_file() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::WriteFile {
                share: "Documents".to_string(),
                path: "test.txt".to_string(),
                data: b"content".to_vec(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        adapter.set_property(
            "smb-001",
            "write_file",
            serde_json::json!({
                "share": "Documents",
                "path": "test.txt",
                "data": "content"
            }),
        ).await.unwrap();
    }

    // 16. DeviceController set_property delete_file
    #[tokio::test]
    async fn device_controller_set_property_delete_file() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::DeleteFile {
                share: "Documents".to_string(),
                path: "old.txt".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        adapter.set_property(
            "smb-001",
            "delete_file",
            serde_json::json!({
                "share": "Documents",
                "path": "old.txt"
            }),
        ).await.unwrap();
    }

    // 17. DeviceController set_property create_directory
    #[tokio::test]
    async fn device_controller_set_property_create_directory() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::CreateDirectory {
                share: "Documents".to_string(),
                path: "newfolder".to_string(),
            },
            MockResponse::Ok,
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        adapter.set_property(
            "smb-001",
            "create_directory",
            serde_json::json!({
                "share": "Documents",
                "path": "newfolder"
            }),
        ).await.unwrap();
    }

    // 18. DeviceController get_state connected
    #[tokio::test]
    async fn device_controller_get_state_connected() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        mock.expect(
            MockCall::Connect {
                host: "192.168.1.100".to_string(),
                port: 445,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            },
            MockResponse::Ok,
        ).await;
        mock.expect(
            MockCall::ListShares,
            MockResponse::Shares(vec![
                SmbShare {
                    name: "Documents".to_string(),
                    share_type: "DISK".to_string(),
                    comment: "".to_string(),
                },
            ]),
        ).await;

        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        adapter.turn_on("smb-001").await.unwrap();
        let state = adapter.get_state("smb-001").await.unwrap();
        assert_eq!(state["connected"], true);
        assert_eq!(state["host"], "192.168.1.100");
        assert_eq!(state["port"], 445);
        assert!(state["shares"].is_array());
    }

    // 19. DeviceController get_state not connected
    #[tokio::test]
    async fn device_controller_get_state_not_connected() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        let adapter = SmbAdapter::with_transport(config, Box::new(mock));
        let state = adapter.get_state("smb-001").await.unwrap();
        assert_eq!(state["connected"], false);
        assert!(state["shares"].is_null());
    }

    // 20. operations not connected error
    #[tokio::test]
    async fn operations_not_connected_error() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        let mock = MockSmbTransport::new();
        let adapter = SmbAdapter::with_transport(config, Box::new(mock));

        assert!(adapter.list_directory("Documents", "/").await.is_err());
        assert!(adapter.read_file("Documents", "test.txt").await.is_err());
        assert!(adapter.write_file("Documents", "test.txt", b"data").await.is_err());
        assert!(adapter.delete_file("Documents", "test.txt").await.is_err());
        assert!(adapter.create_directory("Documents", "folder").await.is_err());
        assert!(adapter.file_info("Documents", "test.txt").await.is_err());
    }

    // 21. config with custom port and domain
    #[test]
    fn config_with_custom_port_and_domain() {
        let config = SmbConfig::new(
            "192.168.1.100".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        )
        .with_port(139)
        .with_domain("WORKGROUP".to_string());

        assert_eq!(config.port, 139);
        assert_eq!(config.domain, Some("WORKGROUP".to_string()));
    }
}
