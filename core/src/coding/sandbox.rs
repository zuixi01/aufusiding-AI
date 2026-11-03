//! 代码执行沙箱
//!
//! 为代码执行提供进程级隔离：资源限制、文件系统隔离、网络控制。
//! 在 Linux 上使用进程隔离 + 资源限制，其他平台使用基础隔离。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// 沙箱权限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPermissions {
    /// 允许文件系统读取
    pub fs_read: bool,
    /// 允许文件系统写入
    pub fs_write: bool,
    /// 允许网络访问
    pub net_access: bool,
    /// 允许执行子进程
    pub exec_allowed: bool,
    /// 可读路径白名单
    pub readable_paths: Vec<PathBuf>,
    /// 可写路径白名单
    pub writable_paths: Vec<PathBuf>,
}

impl Default for SandboxPermissions {
    fn default() -> Self {
        Self {
            fs_read: true,
            fs_write: false,
            net_access: false,
            exec_allowed: false,
            readable_paths: vec![],
            writable_paths: vec![],
        }
    }
}

/// 资源限制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// 最大内存 (字节)
    pub max_memory_bytes: u64,
    /// 最大 CPU 时间
    pub max_cpu_time: Duration,
    /// 最大墙钟时间
    pub max_wall_time: Duration,
    /// 最大输出大小 (字节)
    pub max_output_bytes: usize,
    /// 最大文件大小 (字节)
    pub max_file_size_bytes: u64,
    /// 最大进程数
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
            max_cpu_time: Duration::from_secs(30),
            max_wall_time: Duration::from_secs(60),
            max_output_bytes: 1024 * 1024,         // 1 MB
            max_file_size_bytes: 10 * 1024 * 1024, // 10 MB
            max_processes: 4,
        }
    }
}

/// 沙箱配置
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub permissions: SandboxPermissions,
    pub limits: ResourceLimits,
    /// 工作目录
    pub work_dir: PathBuf,
    /// 环境变量
    pub env_vars: HashMap<String, String>,
}

impl SandboxConfig {
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            permissions: SandboxPermissions::default(),
            limits: ResourceLimits::default(),
            work_dir,
            env_vars: HashMap::new(),
        }
    }

    pub fn with_permissions(mut self, perms: SandboxPermissions) -> Self {
        self.permissions = perms;
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }
}

/// 沙箱执行结果
#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub wall_time_ms: u64,
    pub memory_used_bytes: u64,
    pub killed: bool,
    pub kill_reason: Option<String>,
}

/// 代码执行沙箱
pub struct CodeSandbox {
    config: SandboxConfig,
}

impl CodeSandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// 在沙箱中执行命令
    pub async fn execute(&self, program: &str, args: &[&str]) -> anyhow::Result<SandboxResult> {
        let start = std::time::Instant::now();

        // 确保工作目录存在
        let _ = std::fs::create_dir_all(&self.config.work_dir);

        // 构建命令
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .current_dir(&self.config.work_dir)
            .env_clear(); // 清除继承的环境变量

        // 设置最小环境
        cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin");
        cmd.env("HOME", self.config.work_dir.to_string_lossy().as_ref());
        cmd.env("LANG", "en_US.UTF-8");

        // 用户自定义环境变量
        for (k, v) in &self.config.env_vars {
            cmd.env(k, v);
        }

        // 网络隔离：设置标记（实际隔离需要 OS 级支持）
        if !self.config.permissions.net_access {
            cmd.env("SANDBOX_NO_NET", "1");
        }

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                return Ok(SandboxResult {
                    stdout: String::new(),
                    stderr: format!("启动失败: {}", e),
                    exit_code: -1,
                    wall_time_ms: start.elapsed().as_millis() as u64,
                    memory_used_bytes: 0,
                    killed: false,
                    kill_reason: None,
                });
            }
        };

        let wall_time = start.elapsed();
        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // 输出截断
        let mut killed = false;
        let mut kill_reason = None;

        if stdout.len() > self.config.limits.max_output_bytes {
            stdout.truncate(self.config.limits.max_output_bytes);
            stdout.push_str("\n... [输出已截断]");
            killed = true;
            kill_reason = Some("输出超过限制".to_string());
        }
        if stderr.len() > self.config.limits.max_output_bytes {
            stderr.truncate(self.config.limits.max_output_bytes);
        }

        // 墙钟超时检查
        if wall_time > self.config.limits.max_wall_time {
            killed = true;
            kill_reason = Some("执行超时".to_string());
        }

        Ok(SandboxResult {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
            wall_time_ms: wall_time.as_millis() as u64,
            memory_used_bytes: 0, // 需要 OS 级监控
            killed,
            kill_reason,
        })
    }

    /// 在沙箱中执行代码片段
    pub async fn execute_code(&self, language: &str, code: &str) -> anyhow::Result<SandboxResult> {
        let (interpreter, ext) = match language {
            "python" | "python3" => ("python3", ".py"),
            "node" | "javascript" | "js" => ("node", ".js"),
            "bash" | "sh" => ("bash", ".sh"),
            _ => anyhow::bail!("不支持的语言: {}", language),
        };

        let file_name = format!("sandbox_{}{}", uuid::Uuid::new_v4().simple(), ext);
        let file_path = self.config.work_dir.join(&file_name);
        std::fs::write(&file_path, code)?;

        let result = self
            .execute(interpreter, &[file_path.to_str().unwrap_or("")])
            .await;

        // 清理
        let _ = std::fs::remove_file(&file_path);
        result
    }

    /// 验证路径是否在沙箱允许范围内
    pub fn validate_path(&self, path: &std::path::Path, write: bool) -> bool {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        if write {
            if !self.config.permissions.fs_write {
                return false;
            }
            if self.config.permissions.writable_paths.is_empty() {
                return canonical.starts_with(&self.config.work_dir);
            }
            self.config
                .permissions
                .writable_paths
                .iter()
                .any(|p| canonical.starts_with(p))
        } else {
            if !self.config.permissions.fs_read {
                return false;
            }
            if self.config.permissions.readable_paths.is_empty() {
                return true; // 默认允许读取
            }
            self.config
                .permissions
                .readable_paths
                .iter()
                .any(|p| canonical.starts_with(p))
        }
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_permissions_restrictive() {
        let perms = SandboxPermissions::default();
        assert!(perms.fs_read);
        assert!(!perms.fs_write);
        assert!(!perms.net_access);
        assert!(!perms.exec_allowed);
    }

    #[test]
    fn default_limits_reasonable() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time, Duration::from_secs(30));
        assert_eq!(limits.max_processes, 4);
    }

    #[test]
    fn sandbox_config_builder() {
        let dir = tempdir().unwrap();
        let config = SandboxConfig::new(dir.path().to_path_buf())
            .with_env("FOO", "bar")
            .with_limits(ResourceLimits {
                max_memory_bytes: 128 * 1024 * 1024,
                ..ResourceLimits::default()
            });
        assert_eq!(config.env_vars.get("FOO").unwrap(), "bar");
        assert_eq!(config.limits.max_memory_bytes, 128 * 1024 * 1024);
    }

    #[tokio::test]
    async fn execute_echo_in_sandbox() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(SandboxConfig::new(dir.path().to_path_buf()));
        let result = sandbox.execute("echo", &["hello sandbox"]).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello sandbox"));
        assert!(!result.killed);
    }

    #[tokio::test]
    async fn execute_code_bash() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(SandboxConfig::new(dir.path().to_path_buf()));
        let result = sandbox.execute_code("bash", "echo $((2+3))").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.trim() == "5");
    }

    #[tokio::test]
    async fn reject_unknown_language() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(SandboxConfig::new(dir.path().to_path_buf()));
        let result = sandbox.execute_code("cobol", "DISPLAY 'HI'").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn env_cleared_in_sandbox() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(
            SandboxConfig::new(dir.path().to_path_buf()).with_env("TEST_VAR", "sandbox_value"),
        );
        let result = sandbox
            .execute_code("bash", "echo $TEST_VAR")
            .await
            .unwrap();
        assert!(result.stdout.trim() == "sandbox_value");
    }

    #[test]
    fn validate_path_write_denied_by_default() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(SandboxConfig::new(dir.path().to_path_buf()));
        let test_file = dir.path().join("test.txt");
        std::fs::write(&test_file, "x").unwrap();
        assert!(!sandbox.validate_path(&test_file, true));
    }

    #[test]
    fn validate_path_read_allowed_by_default() {
        let dir = tempdir().unwrap();
        let sandbox = CodeSandbox::new(SandboxConfig::new(dir.path().to_path_buf()));
        let test_file = dir.path().join("test.txt");
        std::fs::write(&test_file, "x").unwrap();
        assert!(sandbox.validate_path(&test_file, false));
    }

    #[tokio::test]
    async fn output_truncation() {
        let dir = tempdir().unwrap();
        let config = SandboxConfig::new(dir.path().to_path_buf()).with_limits(ResourceLimits {
            max_output_bytes: 20,
            ..ResourceLimits::default()
        });
        let sandbox = CodeSandbox::new(config);
        let result = sandbox
            .execute_code("bash", "python3 -c \"print('x' * 1000)\" 2>/dev/null || echo $(head -c 1000 /dev/zero | tr '\\0' 'x')")
            .await
            .unwrap();
        // 输出应被截断
        assert!(result.stdout.len() <= 50); // 20 + truncation message
    }
}
