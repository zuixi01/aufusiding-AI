//! 语义任务缓存 — 基于向量相似度的任务结果缓存
//!
//! 对相似任务描述进行语义匹配，命中时跳过 LLM 执行，直接返回缓存结果。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 缓存条目
#[derive(Debug, Clone)]
pub struct CachedTaskResult {
    pub task_description: String,
    pub result: String,
    pub embedding: Vec<f32>,
    created_at: Instant,
    ttl: Duration,
}

impl CachedTaskResult {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// 缓存统计
#[derive(Debug, Clone, Default)]
pub struct SemanticCacheStats {
    pub hits: u64,
    pub misses: u64,
}

impl SemanticCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// 语义任务缓存
pub struct SemanticTaskCache {
    entries: HashMap<String, CachedTaskResult>,
    default_ttl: Duration,
    stats: SemanticCacheStats,
}

impl SemanticTaskCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            default_ttl,
            stats: SemanticCacheStats::default(),
        }
    }

    /// 查找语义相似的缓存条目
    ///
    /// threshold: cosine 相似度阈值（默认 0.92）
    pub fn lookup(&mut self, task_description: &str, threshold: f32) -> Option<CachedTaskResult> {
        let query_embedding = Self::simple_embedding(task_description);
        self.evict_expired();

        let mut best_match: Option<(&str, f32)> = None;

        for (key, entry) in &self.entries {
            let similarity = cosine_similarity(&query_embedding, &entry.embedding);
            if similarity >= threshold
                && best_match.is_none_or(|(_, best_sim)| similarity > best_sim)
            {
                best_match = Some((key.as_str(), similarity));
            }
        }

        if let Some((key, _)) = best_match {
            let entry = self.entries.get(key).cloned();
            self.stats.hits += 1;
            entry
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// 存储任务结果到缓存
    pub fn store(&mut self, task_description: &str, result: &str) {
        let embedding = Self::simple_embedding(task_description);
        let key = task_description.to_string();
        self.entries.insert(
            key,
            CachedTaskResult {
                task_description: task_description.to_string(),
                result: result.to_string(),
                embedding,
                created_at: Instant::now(),
                ttl: self.default_ttl,
            },
        );
    }

    /// 获取缓存统计
    pub fn stats(&self) -> &SemanticCacheStats {
        &self.stats
    }

    /// 清理过期条目
    fn evict_expired(&mut self) {
        self.entries.retain(|_, entry| !entry.is_expired());
    }

    /// 简单 embedding 生成（基于字符 n-gram 哈希，128 维）
    ///
    /// 生产环境应替换为真实 embedding provider
    fn simple_embedding(text: &str) -> Vec<f32> {
        let dim = 128;
        let mut vec = vec![0.0f32; dim];
        let chars: Vec<char> = text.chars().collect();

        for window in chars.windows(3) {
            let hash = window.iter().fold(0u64, |acc, &c| {
                acc.wrapping_mul(31).wrapping_add(c as u64)
            });
            let idx = (hash as usize) % dim;
            vec[idx] += 1.0;
        }

        // L2 归一化
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

/// 余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_skips_execution() {
        let mut cache = SemanticTaskCache::new(Duration::from_secs(3600));
        cache.store("设计一个用户认证系统", "JWT + OAuth2 方案");

        let result = cache.lookup("设计一个用户认证系统", 0.92);
        assert!(result.is_some());
        assert_eq!(result.unwrap().result, "JWT + OAuth2 方案");
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn similar_task_hits_cache() {
        let mut cache = SemanticTaskCache::new(Duration::from_secs(3600));
        cache.store("设计用户认证系统的架构", "JWT + OAuth2 方案");

        // 非常相似的描述应该命中
        let result = cache.lookup("设计用户认证系统的架构", 0.92);
        assert!(result.is_some());
    }

    #[test]
    fn dissimilar_task_misses_cache() {
        let mut cache = SemanticTaskCache::new(Duration::from_secs(3600));
        cache.store("设计用户认证系统", "JWT + OAuth2 方案");

        // 完全不同的任务不应命中
        let result = cache.lookup("implement a binary search tree in rust", 0.92);
        assert!(result.is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn ttl_expiration_invalidates_entry() {
        let mut cache = SemanticTaskCache::new(Duration::from_millis(1));
        cache.store("设计缓存系统", "Redis + LRU");

        // 等待过期
        std::thread::sleep(Duration::from_millis(10));

        let result = cache.lookup("设计缓存系统", 0.92);
        assert!(result.is_none());
    }
}
