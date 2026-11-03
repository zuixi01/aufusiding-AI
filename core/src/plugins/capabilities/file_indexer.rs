//! 文件索引器
//!
//! 文件名称/类型/日期/大小索引。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_timestamp: i64,
    pub is_dir: bool,
}

/// 文件索引器
pub struct FileIndexer {
    entries: Vec<FileEntry>,
}

impl FileIndexer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 索引目录
    pub fn index_directory(&mut self, path: &Path) -> anyhow::Result<usize> {
        if !path.exists() {
            anyhow::bail!("路径不存在: {}", path.display());
        }

        let mut count = 0;
        for entry in walkdir::WalkDir::new(path)
            .max_depth(5)
            .into_iter()
            .flatten()
        {
            let metadata = entry.metadata()?;
            let file_path = entry.path().to_path_buf();
            let name = entry.file_name().to_string_lossy().to_string();
            let ext = file_path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            self.entries.push(FileEntry {
                path: file_path,
                name,
                extension: ext,
                size_bytes: metadata.len(),
                modified_timestamp: metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                is_dir: metadata.is_dir(),
            });
            count += 1;
        }

        Ok(count)
    }

    /// 搜索文件
    pub fn search(&self, query: &str) -> Vec<&FileEntry> {
        let lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// 按扩展名过滤
    pub fn by_extension(&self, ext: &str) -> Vec<&FileEntry> {
        self.entries
            .iter()
            .filter(|e| e.extension.eq_ignore_ascii_case(ext))
            .collect()
    }

    /// 最近修改的文件
    pub fn recent_files(&self, count: usize) -> Vec<&FileEntry> {
        let mut sorted: Vec<&FileEntry> = self.entries.iter().filter(|e| !e.is_dir).collect();
        sorted.sort_by(|a, b| b.modified_timestamp.cmp(&a.modified_timestamp));
        sorted.into_iter().take(count).collect()
    }

    /// 索引条目数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 清空索引
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
