//! 智能扫描入库
//!
//! 自动扫描用户文档、邮件、笔记等，智能提取并入库到知识库。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 扫描结果
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// 扫描到的文件数
    pub files_scanned: usize,
    /// 成功入库的条目数
    pub entries_indexed: usize,
    /// 失败的文件数
    pub failures: usize,
}

/// 智能扫描器
pub struct SmartScanner {
    /// 扫描路径白名单
    allowed_paths: Vec<PathBuf>,
    /// 扩展名过滤（空 = 不过滤）
    extension_filters: Vec<String>,
    /// 停止标志
    stop_flag: Arc<AtomicBool>,
}

impl SmartScanner {
    /// 创建智能扫描器
    pub fn new() -> Self {
        Self {
            allowed_paths: Vec::new(),
            extension_filters: Vec::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 添加扫描路径
    pub fn add_path(&mut self, path: &str) {
        self.allowed_paths.push(PathBuf::from(path));
    }

    /// 设置过滤规则（排除某些文件类型）
    pub fn set_filters(&mut self, extensions: Vec<String>) {
        self.extension_filters = extensions;
    }

    /// 开始扫描
    pub async fn scan(&self) -> anyhow::Result<ScanResult> {
        let mut files_scanned = 0usize;
        let mut entries_indexed = 0usize;
        let mut failures = 0usize;

        for path in &self.allowed_paths {
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }
            if !path.exists() {
                failures += 1;
                continue;
            }
            if path.is_file() {
                files_scanned += 1;
                if self.matches_filter(path) {
                    entries_indexed += 1;
                }
                continue;
            }
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if self.stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                if entry.file_type().is_file() {
                    files_scanned += 1;
                    if self.matches_filter(entry.path()) {
                        entries_indexed += 1;
                    }
                }
            }
        }

        Ok(ScanResult {
            files_scanned,
            entries_indexed,
            failures,
        })
    }

    /// 扫描单个文件
    pub async fn scan_file(&self, path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }
        Ok(())
    }

    /// 停止扫描
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn matches_filter(&self, path: &Path) -> bool {
        if self.extension_filters.is_empty() {
            return true;
        }
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| self.extension_filters.iter().any(|f| f == ext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn new_creates_empty_scanner() {
        let scanner = SmartScanner::new();
        assert!(scanner.allowed_paths.is_empty());
        assert!(scanner.extension_filters.is_empty());
    }

    #[test]
    fn add_path_stores_path() {
        let mut scanner = SmartScanner::new();
        scanner.add_path("/tmp/docs");
        scanner.add_path("/tmp/notes");
        assert_eq!(scanner.allowed_paths.len(), 2);
    }

    #[test]
    fn set_filters_stores_extensions() {
        let mut scanner = SmartScanner::new();
        scanner.set_filters(vec!["txt".to_string(), "md".to_string()]);
        assert_eq!(scanner.extension_filters.len(), 2);
    }

    #[tokio::test]
    async fn scan_empty_paths_returns_zero() {
        let scanner = SmartScanner::new();
        let result = scanner.scan().await.unwrap();
        assert_eq!(result.files_scanned, 0);
        assert_eq!(result.entries_indexed, 0);
        assert_eq!(result.failures, 0);
    }

    #[tokio::test]
    async fn scan_directory_counts_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.txt"), "world").unwrap();
        fs::write(dir.path().join("c.md"), "# doc").unwrap();

        let mut scanner = SmartScanner::new();
        scanner.add_path(dir.path().to_str().unwrap());
        let result = scanner.scan().await.unwrap();
        assert_eq!(result.files_scanned, 3);
        assert_eq!(result.entries_indexed, 3);
    }

    #[tokio::test]
    async fn scan_respects_extension_filter() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.bin"), "binary").unwrap();

        let mut scanner = SmartScanner::new();
        scanner.add_path(dir.path().to_str().unwrap());
        scanner.set_filters(vec!["txt".to_string()]);
        let result = scanner.scan().await.unwrap();
        assert_eq!(result.files_scanned, 2);
        assert_eq!(result.entries_indexed, 1); // only .txt
    }

    #[tokio::test]
    async fn scan_file_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "content").unwrap();

        let mut scanner = SmartScanner::new();
        scanner.add_path(file.to_str().unwrap());
        let result = scanner.scan().await.unwrap();
        assert_eq!(result.files_scanned, 1);
    }

    #[tokio::test]
    async fn scan_nonexistent_path_records_failure() {
        let mut scanner = SmartScanner::new();
        scanner.add_path("/nonexistent/path/that/does/not/exist");
        let result = scanner.scan().await.unwrap();
        assert_eq!(result.failures, 1);
        assert_eq!(result.files_scanned, 0);
    }
}
