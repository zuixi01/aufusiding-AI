//! Prompt 缓存
//!
//! LRU + TTL 的 prompt 级缓存，用于避免重复推理，降低成本。
//! 使用 SHA-256 哈希作为缓存键。

use std::time::Duration;

use chrono::{DateTime, Utc};
use linked_hash_map::LinkedHashMap;
use ring::digest;

/// 缓存条目
struct CacheEntry {
    response: String,
    tokens_used: usize,
    created_at: DateTime<Utc>,
    ttl: Duration,
    hit_count: u64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::ZERO);
        elapsed > self.ttl
    }
}

/// 缓存统计
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_tokens_saved: u64,
}

impl CacheStats {
    /// 命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// 估算节省的成本
    pub fn estimated_cost_saved(&self, cost_per_1k_tokens: f64) -> f64 {
        self.total_tokens_saved as f64 / 1000.0 * cost_per_1k_tokens
    }
}

/// Prompt 缓存 (LRU + TTL)
///
/// 使用 LinkedHashMap 实现 O(1) 的 get/put/evict 操作。
pub struct PromptCache {
    entries: LinkedHashMap<String, CacheEntry>,
    max_entries: usize,
    default_ttl: Duration,
    stats: CacheStats,
}

impl PromptCache {
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self {
        Self {
            entries: LinkedHashMap::new(),
            max_entries: max_entries.max(1),
            default_ttl,
            stats: CacheStats::default(),
        }
    }

    /// 计算缓存键: SHA-256(prompt + temperature_bits + max_tokens)
    fn cache_key(prompt: &str, temperature: f32, max_tokens: usize) -> String {
        let mut input = prompt.as_bytes().to_vec();
        input.extend_from_slice(&temperature.to_bits().to_le_bytes());
        input.extend_from_slice(&max_tokens.to_le_bytes());
        let hash = digest::digest(&digest::SHA256, &input);
        hex_encode(hash.as_ref())
    }

    /// 查询缓存 — O(1)
    pub fn get(
        &mut self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Option<(String, usize)> {
        if temperature > 0.0 {
            self.stats.misses += 1;
            return None;
        }

        let key = Self::cache_key(prompt, temperature, max_tokens);

        if let Some(entry) = self.entries.get(&key) {
            if entry.is_expired() {
                self.entries.remove(&key);
                self.stats.misses += 1;
                self.stats.evictions += 1;
                return None;
            }
            // get_refresh moves to back (most-recently-used) — O(1)
            if let Some(entry) = self.entries.get_refresh(&key) {
                entry.hit_count += 1;
                self.stats.hits += 1;
                self.stats.total_tokens_saved += entry.tokens_used as u64;
                Some((entry.response.clone(), entry.tokens_used))
            } else {
                self.stats.misses += 1;
                None
            }
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// 写入缓存 — O(1)
    pub fn put(
        &mut self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
        response: &str,
        tokens_used: usize,
    ) {
        if temperature > 0.0 {
            return;
        }

        let key = Self::cache_key(prompt, temperature, max_tokens);

        // 如果 key 已存在，先移除旧条目
        self.entries.remove(&key);

        // 容量满时 pop_front 淘汰最久未使用 — O(1)
        while self.entries.len() >= self.max_entries {
            if self.entries.pop_front().is_some() {
                self.stats.evictions += 1;
            } else {
                break;
            }
        }

        self.entries.insert(
            key,
            CacheEntry {
                response: response.to_string(),
                tokens_used,
                created_at: Utc::now(),
                ttl: self.default_ttl,
                hit_count: 0,
            },
        );
    }

    /// 清理过期条目 — O(k) where k = expired count
    pub fn evict_expired(&mut self) -> usize {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        let count = expired_keys.len();
        for key in &expired_keys {
            self.entries.remove(key);
        }
        self.stats.evictions += count as u64;
        count
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// 简易 hex 编码
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache() -> PromptCache {
        PromptCache::new(10, Duration::from_secs(60))
    }

    #[test]
    fn cache_hit() {
        let mut cache = make_cache();
        cache.put("hello", 0.0, 100, "world", 50);
        let result = cache.get("hello", 0.0, 100);
        assert_eq!(result, Some(("world".to_string(), 50)));
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn cache_miss() {
        let mut cache = make_cache();
        assert!(cache.get("nonexistent", 0.0, 100).is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn temperature_nonzero_skips_cache() {
        let mut cache = make_cache();
        cache.put("hello", 0.7, 100, "world", 50);
        assert!(cache.is_empty()); // 不应写入
        assert!(cache.get("hello", 0.7, 100).is_none());
    }

    #[test]
    fn ttl_expiration() {
        let mut cache = PromptCache::new(10, Duration::from_millis(1));
        cache.put("hello", 0.0, 100, "world", 50);
        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("hello", 0.0, 100).is_none());
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn lru_eviction() {
        let mut cache = PromptCache::new(2, Duration::from_secs(60));
        cache.put("a", 0.0, 100, "1", 10);
        cache.put("b", 0.0, 100, "2", 20);
        cache.put("c", 0.0, 100, "3", 30); // 应淘汰 "a"
        assert!(cache.get("a", 0.0, 100).is_none());
        assert!(cache.get("b", 0.0, 100).is_some());
        assert!(cache.get("c", 0.0, 100).is_some());
    }

    #[test]
    fn stats_accurate() {
        let mut cache = make_cache();
        cache.put("q", 0.0, 100, "a", 42);
        cache.get("q", 0.0, 100); // hit
        cache.get("q", 0.0, 100); // hit
        cache.get("x", 0.0, 100); // miss
        assert_eq!(cache.stats().hits, 2);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().total_tokens_saved, 84);
    }

    #[test]
    fn cache_key_consistency() {
        let k1 = PromptCache::cache_key("test", 0.0, 100);
        let k2 = PromptCache::cache_key("test", 0.0, 100);
        let k3 = PromptCache::cache_key("test", 0.0, 200);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn cost_saved_calculation() {
        let mut stats = CacheStats::default();
        stats.total_tokens_saved = 10000;
        let cost = stats.estimated_cost_saved(0.002); // $0.002/1k tokens
        assert!((cost - 0.02).abs() < 0.001);
    }

    #[test]
    fn hit_rate_calculation() {
        let stats = CacheStats {
            hits: 3,
            misses: 7,
            ..Default::default()
        };
        assert!((stats.hit_rate() - 0.3).abs() < 0.001);
    }

    #[test]
    fn clear_empties_cache() {
        let mut cache = make_cache();
        cache.put("a", 0.0, 100, "1", 10);
        cache.put("b", 0.0, 100, "2", 20);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn evict_expired_batch() {
        let mut cache = PromptCache::new(10, Duration::from_millis(1));
        cache.put("a", 0.0, 100, "1", 10);
        cache.put("b", 0.0, 100, "2", 20);
        std::thread::sleep(Duration::from_millis(10));
        let evicted = cache.evict_expired();
        assert_eq!(evicted, 2);
        assert!(cache.is_empty());
    }

    #[test]
    fn mass_insert_preserves_lru_order() {
        let mut cache = PromptCache::new(100, Duration::from_secs(60));
        for i in 0..100 {
            cache.put(
                &format!("prompt-{}", i),
                0.0,
                100,
                &format!("resp-{}", i),
                10,
            );
        }
        assert_eq!(cache.len(), 100);
        // Insert one more — should evict prompt-0 (oldest)
        cache.put("prompt-100", 0.0, 100, "resp-100", 10);
        assert_eq!(cache.len(), 100);
        assert!(cache.get("prompt-0", 0.0, 100).is_none());
        assert!(cache.get("prompt-100", 0.0, 100).is_some());
    }

    #[test]
    fn access_reorders_eviction() {
        let mut cache = PromptCache::new(3, Duration::from_secs(60));
        cache.put("a", 0.0, 100, "1", 10);
        cache.put("b", 0.0, 100, "2", 20);
        cache.put("c", 0.0, 100, "3", 30);
        // Access "a" to move it to most-recently-used
        cache.get("a", 0.0, 100);
        // Insert "d" — should evict "b" (now the LRU), not "a"
        cache.put("d", 0.0, 100, "4", 40);
        assert!(cache.get("a", 0.0, 100).is_some());
        assert!(cache.get("b", 0.0, 100).is_none());
        assert!(cache.get("c", 0.0, 100).is_some());
        assert!(cache.get("d", 0.0, 100).is_some());
    }
}
