#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Output};

const INVALID_CONFIG: &str = "max_tokens = [\n";

struct TestConfig {
    root: PathBuf,
    file: PathBuf,
}

impl TestConfig {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("prometheus-config-test-{}", uuid::Uuid::new_v4()));
        let file = root.join("prometheus-ai/config.toml");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, INVALID_CONFIG).unwrap();
        Self { root, file }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_prometheus"))
            .env("XDG_CONFIG_HOME", &self.root)
            .args(args)
            .output()
            .expect("CLI subprocess must start")
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn reset_recovers_invalid_config_and_list_reads_the_result() {
    let config = TestConfig::new();

    let reset = config.run(&["config", "reset"]);
    assert!(
        reset.status.success(),
        "config reset failed: {}",
        String::from_utf8_lossy(&reset.stderr)
    );
    assert_ne!(
        std::fs::read_to_string(&config.file).unwrap(),
        INVALID_CONFIG
    );

    let list = config.run(&["config", "list"]);
    assert!(
        list.status.success(),
        "config list failed after reset: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("default_model"));
}

#[test]
fn ordinary_command_rejects_invalid_config_without_overwriting_it() {
    let config = TestConfig::new();

    let list = config.run(&["config", "list"]);
    assert!(!list.status.success());
    assert_eq!(
        std::fs::read_to_string(&config.file).unwrap(),
        INVALID_CONFIG
    );
}
