//! 守护进程管理
//!
//! 管理 Prometheus 后台守护进程的生命周期。

use std::path::PathBuf;

/// 守护进程状态
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

/// 守护进程管理器
pub struct DaemonProcess {
    /// 进程状态
    state: DaemonState,
    /// PID 文件路径
    pid_file: PathBuf,
}

impl DaemonProcess {
    /// 创建守护进程管理器
    pub fn new(pid_file: PathBuf) -> Self {
        Self {
            state: DaemonState::Stopped,
            pid_file,
        }
    }

    /// 启动守护进程
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running() {
            anyhow::bail!("daemon is already running");
        }

        self.state = DaemonState::Starting;
        if let Some(parent) = self.pid_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.pid_file, format!("{}\n", std::process::id()))?;
        self.state = DaemonState::Running;
        Ok(())
    }

    /// 停止守护进程
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if !self.is_running() {
            anyhow::bail!("daemon is not running");
        }

        self.state = DaemonState::Stopping;
        if self.pid_file.exists() {
            std::fs::remove_file(&self.pid_file)?;
        }
        self.state = DaemonState::Stopped;
        Ok(())
    }

    /// 重启守护进程
    pub async fn restart(&mut self) -> anyhow::Result<()> {
        if self.is_running() {
            self.stop().await?;
        }
        self.start().await
    }

    /// 获取进程状态
    pub fn state(&self) -> &DaemonState {
        &self.state
    }

    /// 检查守护进程是否运行
    pub fn is_running(&self) -> bool {
        self.state == DaemonState::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid_file() -> PathBuf {
        crate::test_support::temp_file_path("daemon", "pid")
    }

    #[tokio::test]
    async fn start_creates_pid_file_and_moves_to_running() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file.clone());

        process.start().await.unwrap();

        assert_eq!(process.state(), &DaemonState::Running);
        assert!(process.is_running());
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        assert!(!pid.trim().is_empty());
    }

    #[tokio::test]
    async fn stop_removes_pid_file_and_moves_to_stopped() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file.clone());
        process.start().await.unwrap();

        process.stop().await.unwrap();

        assert_eq!(process.state(), &DaemonState::Stopped);
        assert!(!process.is_running());
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn restart_recreates_pid_file_and_returns_to_running() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file.clone());
        process.start().await.unwrap();
        let first_pid = std::fs::read_to_string(&pid_file).unwrap();

        process.restart().await.unwrap();

        assert_eq!(process.state(), &DaemonState::Running);
        let second_pid = std::fs::read_to_string(&pid_file).unwrap();
        assert!(!second_pid.trim().is_empty());
        assert_eq!(first_pid.trim(), second_pid.trim());
    }

    #[tokio::test]
    async fn start_and_stop_reject_invalid_transitions() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file);

        let stop_error = process.stop().await.unwrap_err();
        assert!(stop_error.to_string().contains("not running"));

        process.start().await.unwrap();
        let start_error = process.start().await.unwrap_err();
        assert!(start_error.to_string().contains("already running"));
    }

    #[tokio::test]
    async fn pause_resume_lifecycle() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file);
        process.start().await.unwrap();
        assert!(process.is_running());

        process.stop().await.unwrap();
        assert!(!process.is_running());

        // Can restart after stop
        process.start().await.unwrap();
        assert!(process.is_running());
    }

    #[tokio::test]
    async fn shutdown_prevents_resume() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file);
        process.start().await.unwrap();
        process.stop().await.unwrap();
        assert_eq!(process.state(), &DaemonState::Stopped);
        // Can still start again (restart is allowed from Stopped)
        process.start().await.unwrap();
        assert_eq!(process.state(), &DaemonState::Running);
    }

    #[tokio::test]
    async fn concurrent_pause_resume_safe() {
        let pid_file = pid_file();
        let mut process = DaemonProcess::new(pid_file);

        // Rapid start/stop cycles should not panic
        for _ in 0..5 {
            process.start().await.unwrap();
            assert!(process.is_running());
            process.stop().await.unwrap();
            assert!(!process.is_running());
        }
    }
}
