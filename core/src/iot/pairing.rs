//! 安全配对
//!
//! 双门安全配对机制，确保设备接入的安全性。

use chrono::{DateTime, Duration, Utc};

/// 配对状态
#[derive(Debug, Clone, PartialEq)]
pub enum PairingState {
    Idle,
    Discovering,
    Pairing,
    Paired,
    Failed,
}

/// 配对会话
#[derive(Debug, Clone)]
pub struct PairingSession {
    pub device_id: String,
    pub pin: String,
    pub expires_at: DateTime<Utc>,
}

/// 已配对设备
#[derive(Debug, Clone)]
pub struct PairedDevice {
    pub device_id: String,
    pub paired_at: DateTime<Utc>,
}

/// 安全配对管理器
pub struct SecurePairing {
    _state: PairingState,
    session: Option<PairingSession>,
    paired_devices: Vec<PairedDevice>,
}

impl SecurePairing {
    /// 创建配对管理器
    pub fn new() -> Self {
        Self {
            _state: PairingState::Idle,
            session: None,
            paired_devices: Vec::new(),
        }
    }

    /// 发起配对请求，返回 6 位 PIN
    pub async fn initiate(&mut self, device_id: &str) -> anyhow::Result<String> {
        if self._state == PairingState::Pairing {
            anyhow::bail!("already pairing another device");
        }
        // 生成 6 位 PIN（确定性模拟：基于 device_id 哈希）
        let pin = format!(
            "{:06}",
            device_id
                .bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32))
                % 1_000_000
        );
        self._state = PairingState::Pairing;
        self.session = Some(PairingSession {
            device_id: device_id.to_string(),
            pin: pin.clone(),
            expires_at: Utc::now() + Duration::minutes(5),
        });
        Ok(pin)
    }

    /// 确认配对（验证 PIN）
    pub async fn confirm(&mut self, pin: &str) -> anyhow::Result<()> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no active pairing session"))?;
        if session.pin != pin {
            self._state = PairingState::Failed;
            self.session = None;
            anyhow::bail!("invalid PIN");
        }
        let device_id = session.device_id.clone();
        self.paired_devices.push(PairedDevice {
            device_id,
            paired_at: Utc::now(),
        });
        self._state = PairingState::Paired;
        self.session = None;
        Ok(())
    }

    /// 取消配对
    pub fn cancel(&mut self) {
        self._state = PairingState::Idle;
        self.session = None;
    }

    /// 获取配对状态
    pub fn state(&self) -> &PairingState {
        &self._state
    }

    /// 解除配对
    pub async fn unpair(&mut self, device_id: &str) -> anyhow::Result<()> {
        let before = self.paired_devices.len();
        self.paired_devices.retain(|d| d.device_id != device_id);
        if self.paired_devices.len() == before {
            anyhow::bail!("device not paired: {}", device_id);
        }
        if self.paired_devices.is_empty() {
            self._state = PairingState::Idle;
        }
        Ok(())
    }

    /// 获取已配对设备列表
    pub fn paired_devices(&self) -> &[PairedDevice] {
        &self.paired_devices
    }

    /// 检查设备是否已配对
    pub fn is_paired(&self, device_id: &str) -> bool {
        self.paired_devices.iter().any(|d| d.device_id == device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_idle() {
        let p = SecurePairing::new();
        assert_eq!(*p.state(), PairingState::Idle);
        assert!(p.paired_devices().is_empty());
    }

    #[tokio::test]
    async fn initiate_generates_pin() {
        let mut p = SecurePairing::new();
        let pin = p.initiate("light-001").await.unwrap();
        assert_eq!(pin.len(), 6);
        assert!(pin.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(*p.state(), PairingState::Pairing);
    }

    #[tokio::test]
    async fn correct_pin_pairs() {
        let mut p = SecurePairing::new();
        let pin = p.initiate("light-001").await.unwrap();
        p.confirm(&pin).await.unwrap();
        assert_eq!(*p.state(), PairingState::Paired);
        assert!(p.is_paired("light-001"));
    }

    #[tokio::test]
    async fn wrong_pin_fails() {
        let mut p = SecurePairing::new();
        p.initiate("light-001").await.unwrap();
        let result = p.confirm("000000").await;
        assert!(result.is_err());
        assert_eq!(*p.state(), PairingState::Failed);
    }

    #[test]
    fn cancel_resets_state() {
        let mut p = SecurePairing::new();
        p._state = PairingState::Pairing;
        p.cancel();
        assert_eq!(*p.state(), PairingState::Idle);
    }

    #[tokio::test]
    async fn unpair_removes_device() {
        let mut p = SecurePairing::new();
        let pin = p.initiate("light-001").await.unwrap();
        p.confirm(&pin).await.unwrap();
        assert!(p.is_paired("light-001"));
        p.unpair("light-001").await.unwrap();
        assert!(!p.is_paired("light-001"));
    }

    #[tokio::test]
    async fn paired_devices_list() {
        let mut p = SecurePairing::new();
        let pin = p.initiate("light-001").await.unwrap();
        p.confirm(&pin).await.unwrap();
        // Reset state to allow second pairing
        p._state = PairingState::Idle;
        let pin2 = p.initiate("lock-001").await.unwrap();
        p.confirm(&pin2).await.unwrap();
        assert_eq!(p.paired_devices().len(), 2);
    }

    #[tokio::test]
    async fn duplicate_pairing_while_active_fails() {
        let mut p = SecurePairing::new();
        p.initiate("light-001").await.unwrap();
        let result = p.initiate("lock-001").await;
        assert!(result.is_err());
    }
}
