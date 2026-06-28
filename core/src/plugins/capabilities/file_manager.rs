//! 语音文件管理能力
//!
//! 搜索、整理、备份文件。

use std::path::{Path, PathBuf};

use super::file_indexer::{FileEntry, FileIndexer};

/// 整理规则
pub struct OrganizeRule {
    pub source_dir: PathBuf,
    pub rules: Vec<(String, PathBuf)>, // (扩展名, 目标目录)
}

/// 整理报告
#[derive(Debug)]
pub struct OrganizeReport {
    pub moved_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

/// 备份报告
#[derive(Debug)]
pub struct BackupReport {
    pub copied_count: usize,
    pub total_bytes: u64,
    pub errors: Vec<String>,
}

/// 文件管理能力
pub struct FileManagerCapability {
    indexer: FileIndexer,
    root_paths: Vec<PathBuf>,
}

impl FileManagerCapability {
    pub fn new(root_paths: Vec<PathBuf>) -> Self {
        Self {
            indexer: FileIndexer::new(),
            root_paths,
        }
    }

    /// 索引所有根路径
    pub fn build_index(&mut self) -> anyhow::Result<usize> {
        self.indexer.clear();
        let mut total = 0;
        for path in &self.root_paths.clone() {
            total += self.indexer.index_directory(path)?;
        }
        Ok(total)
    }

    /// 搜索文件
    pub fn search(&self, query: &str) -> Vec<&FileEntry> {
        self.indexer.search(query)
    }

    /// 最近文件
    pub fn recent_files(&self, count: usize) -> Vec<&FileEntry> {
        self.indexer.recent_files(count)
    }

    /// 按扩展名查找
    pub fn by_extension(&self, ext: &str) -> Vec<&FileEntry> {
        self.indexer.by_extension(ext)
    }

    /// 整理文件（按扩展名分类到目标目录）
    pub fn organize(&self, rule: &OrganizeRule) -> OrganizeReport {
        let mut report = OrganizeReport {
            moved_count: 0,
            skipped_count: 0,
            errors: Vec::new(),
        };

        let entries = match std::fs::read_dir(&rule.source_dir) {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(format!("无法读取目录: {}", e));
                return report;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                report.skipped_count += 1;
                continue;
            }

            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            if let Some((_, target_dir)) = rule.rules.iter().find(|(e, _)| e == &ext) {
                if let Err(e) = std::fs::create_dir_all(target_dir) {
                    report.errors.push(format!("创建目录失败: {}", e));
                    continue;
                }
                let target = target_dir.join(entry.file_name());
                match std::fs::rename(&path, &target) {
                    Ok(_) => report.moved_count += 1,
                    Err(e) => report.errors.push(format!("移动失败: {}", e)),
                }
            } else {
                report.skipped_count += 1;
            }
        }

        report
    }

    /// 备份目录
    pub fn backup(&self, source: &Path, target: &Path) -> BackupReport {
        let mut report = BackupReport {
            copied_count: 0,
            total_bytes: 0,
            errors: Vec::new(),
        };

        if let Err(e) = std::fs::create_dir_all(target) {
            report.errors.push(format!("创建备份目录失败: {}", e));
            return report;
        }

        let walker = walkdir::WalkDir::new(source).into_iter().flatten();
        for entry in walker {
            let src_path = entry.path();
            if src_path.is_dir() {
                continue;
            }

            let relative = src_path.strip_prefix(source).unwrap_or(src_path);
            let dst_path = target.join(relative);

            if let Some(parent) = dst_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match std::fs::copy(src_path, &dst_path) {
                Ok(bytes) => {
                    report.copied_count += 1;
                    report.total_bytes += bytes;
                }
                Err(e) => {
                    report.errors.push(format!("复制失败 {}: {}", src_path.display(), e));
                }
            }
        }

        report
    }

    /// 索引条目数
    pub fn index_count(&self) -> usize {
        self.indexer.entry_count()
    }
}
