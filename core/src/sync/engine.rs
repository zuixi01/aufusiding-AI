//! 同步引擎
//!
//! 编排 CRDT + 加密 + 传输，实现完整的多设备同步。

use super::crypto::E2EEncryption;
use super::protocol::{CrdtOp, Operation, SyncProtocol};
use super::transport::Transport;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// 同步状态
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub last_sync: Option<DateTime<Utc>>,
    pub pending_ops: usize,
    pub connected: bool,
}

/// 同步引擎
pub struct SyncEngine {
    device_id: String,
    protocol: SyncProtocol,
    encryption: E2EEncryption,
    pending: Vec<CrdtOp>,
    last_sync: Option<DateTime<Utc>>,
    connected: bool,
}

impl SyncEngine {
    pub fn new(device_id: String, encryption: E2EEncryption) -> Self {
        let protocol = SyncProtocol::with_index(device_id.clone(), 0, 2);
        Self {
            device_id,
            protocol,
            encryption,
            pending: Vec::new(),
            last_sync: None,
            connected: false,
        }
    }

    /// 记录本地变更
    pub fn push_change(&mut self, operation: Operation) {
        let op = self.protocol.create_op(operation);
        self.pending.push(op);
    }

    /// 获取同步状态
    pub fn get_status(&self) -> SyncStatus {
        SyncStatus {
            last_sync: self.last_sync,
            pending_ops: self.pending.len(),
            connected: self.connected,
        }
    }

    /// 执行完整同步流程
    pub async fn sync(&mut self, transport: &dyn Transport) -> anyhow::Result<()> {
        if !transport.is_connected() {
            anyhow::bail!("transport not connected");
        }
        self.connected = true;

        // 发送 pending ops（加密后）
        for op in &self.pending {
            let serialized = serde_json::to_vec(op)?;
            let encrypted = self
                .encryption
                .encrypt(&serialized, self.encryption.public_key())?;
            transport.send(&encrypted).await?;
        }

        // 尝试接收远程 ops
        let mut remote_ops = Vec::new();
        while let Ok(data) = transport.receive().await {
            if data.is_empty() {
                break;
            }
            if let Ok(decrypted) = self.encryption.decrypt(&data) {
                if let Ok(op) = serde_json::from_slice::<CrdtOp>(&decrypted) {
                    remote_ops.push(op);
                }
            }
        }

        // 合并远程操作
        for op in &remote_ops {
            self.protocol.apply_op(op);
        }

        self.pending.clear();
        self.last_sync = Some(Utc::now());
        Ok(())
    }

    /// 获取当前 CRDT 状态
    pub fn state(&self) -> &std::collections::HashMap<Uuid, String> {
        self.protocol.state()
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::transport::create_transport_pair;

    fn make_engine(id: &str) -> SyncEngine {
        let enc = E2EEncryption::generate_keypair().unwrap();
        SyncEngine::new(id.to_string(), enc)
    }

    #[test]
    fn push_change_adds_pending() {
        let mut engine = make_engine("dev-1");
        let id = Uuid::new_v4();
        engine.push_change(Operation::Insert {
            id,
            data: "test".into(),
        });
        assert_eq!(engine.get_status().pending_ops, 1);
    }

    #[test]
    fn status_initial() {
        let engine = make_engine("dev-1");
        let status = engine.get_status();
        assert!(status.last_sync.is_none());
        assert_eq!(status.pending_ops, 0);
        assert!(!status.connected);
    }

    #[tokio::test]
    async fn sync_clears_pending() {
        let mut engine = make_engine("dev-1");
        let (mut a, _b) = create_transport_pair();
        a.connect("mock://").await.unwrap();

        engine.push_change(Operation::Insert {
            id: Uuid::new_v4(),
            data: "hello".into(),
        });
        engine.sync(&a).await.unwrap();
        assert_eq!(engine.get_status().pending_ops, 0);
        assert!(engine.get_status().last_sync.is_some());
    }

    #[tokio::test]
    async fn sync_not_connected_fails() {
        let mut engine = make_engine("dev-1");
        let (a, _b) = create_transport_pair();
        let result = engine.sync(&a).await;
        assert!(result.is_err());
    }

    #[test]
    fn state_reflects_changes() {
        let mut engine = make_engine("dev-1");
        let id = Uuid::new_v4();
        engine.push_change(Operation::Insert {
            id,
            data: "v1".into(),
        });
        assert_eq!(engine.state().get(&id).unwrap(), "v1");
    }

    #[tokio::test]
    async fn offline_cache_then_sync() {
        let mut engine = make_engine("dev-1");
        // Push changes while offline
        engine.push_change(Operation::Insert {
            id: Uuid::new_v4(),
            data: "offline-1".into(),
        });
        engine.push_change(Operation::Insert {
            id: Uuid::new_v4(),
            data: "offline-2".into(),
        });
        assert_eq!(engine.get_status().pending_ops, 2);

        // Connect and sync
        let (mut a, _b) = create_transport_pair();
        a.connect("mock://").await.unwrap();
        engine.sync(&a).await.unwrap();
        assert_eq!(engine.get_status().pending_ops, 0);
    }
}
