//! sqlite-vec 向量存储
//!
//! 基于 SQLite 的向量相似度搜索。
//! 使用余弦相似度进行 KNN 检索。
//!
//! 优化：
//! - 共享连接（避免每次操作开新连接）
//! - 内存向量缓存（避免重复 JSON 反序列化）
//! - WAL 模式 + 性能 pragma
//! - 预归一化向量：搜索时 cosine 退化为 dot product
//! - Top-K 堆：O(n log k) 替代 O(n log n) 全排序
//! - BLOB 存储：省 60% 空间 + 免 JSON 解析
//! - SIMD 友好循环：8 元素块处理利于自动向量化

use ordered_float::OrderedFloat;
use rusqlite::{params, Connection};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 缓存的向量条目（存储预归一化向量）
struct CachedEntry {
    vector: Vec<f32>,
    metadata: String,
}

/// 缓存最大条目数（约 15MB @ 384 维）
const MAX_CACHE_ENTRIES: usize = 10_000;

/// 向量存储
pub struct VectorStore {
    /// 共享数据库连接
    conn: Arc<Mutex<Connection>>,
    /// 向量维度
    dimension: usize,
    /// 内存向量缓存: id -> (normalized_vector, metadata)
    cache: Mutex<HashMap<String, CachedEntry>>,
    /// 缓存是否已从 DB 加载
    cache_loaded: Mutex<bool>,
}

/// 归一化向量（就地），返回原始范数
fn normalize_vector(v: &mut [f32]) -> f32 {
    let norm = dot_product(v, v).sqrt();
    if norm > 1e-8 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    norm
}

/// 将 f32 切片编码为字节 (little-endian)
fn vector_to_blob(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for &x in v {
        buf.extend_from_slice(&x.to_le_bytes());
    }
    buf
}

/// 从字节解码 f32 向量
fn blob_to_vector(blob: &[u8], dimension: usize) -> Option<Vec<f32>> {
    if blob.len() != dimension * 4 {
        return None;
    }
    let mut v = Vec::with_capacity(dimension);
    for chunk in blob.chunks_exact(4) {
        v.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(v)
}

impl VectorStore {
    /// 创建向量存储
    pub fn new(db_path: PathBuf, dimension: usize) -> Self {
        let conn = Connection::open(&db_path).expect("failed to open vector db");
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -8000;
             PRAGMA mmap_size = 67108864;",
        )
        .ok();

        Self {
            conn: Arc::new(Mutex::new(conn)),
            dimension,
            cache: Mutex::new(HashMap::new()),
            cache_loaded: Mutex::new(false),
        }
    }

    /// 初始化向量表（含 BLOB 列用于高效存储）
    pub fn init(&self) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vectors (
                id TEXT PRIMARY KEY,
                dimension INTEGER NOT NULL,
                vector_json TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                vector_blob BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_vectors_dimension ON vectors(dimension);
            CREATE INDEX IF NOT EXISTS idx_vectors_created_at ON vectors(created_at);",
        )?;
        // 兼容旧表：尝试添加 vector_blob 列（已存在则忽略）
        let _ = conn.execute_batch("ALTER TABLE vectors ADD COLUMN vector_blob BLOB;");
        Ok(())
    }

    /// 确保缓存已从 DB 加载（受容量限制），缓存中存储预归一化向量
    fn ensure_cache_loaded(&self) -> anyhow::Result<bool> {
        let mut loaded = self
            .cache_loaded
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        if *loaded {
            return Ok(true);
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vectors WHERE dimension = ?1",
            params![self.dimension as i64],
            |row| row.get(0),
        )?;
        if total as usize > MAX_CACHE_ENTRIES {
            *loaded = true;
            return Ok(false);
        }
        let mut stmt = conn.prepare(
            "SELECT id, vector_json, metadata, vector_blob FROM vectors WHERE dimension = ?1",
        )?;
        let rows = stmt.query_map(params![self.dimension as i64], |row| {
            let id: String = row.get(0)?;
            let json: String = row.get(1)?;
            let meta: String = row.get(2)?;
            let blob: Option<Vec<u8>> = row.get(3)?;
            Ok((id, json, meta, blob))
        })?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        for item in rows {
            let (id, json, meta, blob) = item?;
            // 优先从 BLOB 加载，回退到 JSON
            let vec_opt = blob
                .and_then(|b| blob_to_vector(&b, self.dimension))
                .or_else(|| serde_json::from_str::<Vec<f32>>(&json).ok());
            if let Some(mut vec) = vec_opt {
                if vec.len() == self.dimension {
                    normalize_vector(&mut vec);
                    cache.insert(
                        id,
                        CachedEntry {
                            vector: vec,
                            metadata: meta,
                        },
                    );
                }
            }
        }
        *loaded = true;
        Ok(true)
    }

    /// 插入向量（同时写入 JSON 和 BLOB，缓存预归一化向量）
    pub fn insert(&self, id: &str, vector: &[f32], metadata: &str) -> anyhow::Result<()> {
        if vector.len() != self.dimension {
            anyhow::bail!(
                "vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            );
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let payload = serde_json::to_string(vector)?;
        let blob = vector_to_blob(vector);
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO vectors (id, dimension, vector_json, metadata, created_at, vector_blob) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, self.dimension as i64, payload, metadata, now, blob],
        )?;
        drop(conn);
        // 缓存预归一化向量
        if let Ok(mut cache) = self.cache.lock() {
            let mut normalized = vector.to_vec();
            normalize_vector(&mut normalized);
            cache.insert(
                id.to_string(),
                CachedEntry {
                    vector: normalized,
                    metadata: metadata.to_string(),
                },
            );
        }
        Ok(())
    }

    /// 批量插入向量（含 BLOB 存储）
    pub fn insert_batch(&self, items: &[(String, Vec<f32>, String)]) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let tx = conn.unchecked_transaction()?;
        let now = chrono::Utc::now().timestamp();
        for (id, vector, metadata) in items {
            if vector.len() != self.dimension {
                anyhow::bail!(
                    "vector dimension mismatch: expected {}, got {}",
                    self.dimension,
                    vector.len()
                );
            }
            let payload = serde_json::to_string(vector)?;
            let blob = vector_to_blob(vector);
            tx.execute(
                "INSERT OR REPLACE INTO vectors (id, dimension, vector_json, metadata, created_at, vector_blob) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, self.dimension as i64, payload, metadata, now, blob],
            )?;
        }
        tx.commit()?;
        drop(conn);
        if let Ok(mut cache) = self.cache.lock() {
            for (id, vector, metadata) in items {
                let mut normalized = vector.clone();
                normalize_vector(&mut normalized);
                cache.insert(
                    id.clone(),
                    CachedEntry {
                        vector: normalized,
                        metadata: metadata.clone(),
                    },
                );
            }
        }
        Ok(())
    }

    /// KNN 搜索 — 预归一化缓存用 dot product + top-k 堆 O(n log k)
    pub fn search(&self, query: &[f32], top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        if query.len() != self.dimension {
            anyhow::bail!(
                "query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            );
        }
        let cached = self.ensure_cache_loaded()?;
        if cached {
            // 归一化查询向量，搜索退化为 dot product
            let mut norm_query = query.to_vec();
            normalize_vector(&mut norm_query);

            let cache = self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("lock poisoned"))?;

            // Top-K 最小堆：保留最大的 k 个相似度
            let mut heap: BinaryHeap<Reverse<(OrderedFloat<f32>, String, String)>> =
                BinaryHeap::with_capacity(top_k + 1);

            for (id, entry) in cache.iter() {
                let similarity = dot_product(&norm_query, &entry.vector);
                if heap.len() < top_k {
                    heap.push(Reverse((
                        OrderedFloat(similarity),
                        id.clone(),
                        entry.metadata.clone(),
                    )));
                } else if let Some(&Reverse((min_sim, _, _))) = heap.peek() {
                    if OrderedFloat(similarity) > min_sim {
                        heap.pop();
                        heap.push(Reverse((
                            OrderedFloat(similarity),
                            id.clone(),
                            entry.metadata.clone(),
                        )));
                    }
                }
            }

            let mut results: Vec<SearchResult> = heap
                .into_vec()
                .into_iter()
                .map(|Reverse((sim, id, meta))| SearchResult {
                    id,
                    similarity: sim.into_inner(),
                    metadata: meta,
                })
                .collect();
            results.sort_unstable_by(|a, b| {
                OrderedFloat(b.similarity).cmp(&OrderedFloat(a.similarity))
            });
            Ok(results)
        } else {
            self.search_from_db(query, top_k)
        }
    }

    /// 从 DB 直接扫描搜索（大数据集回退路径），使用 BLOB 优先 + top-k 堆
    fn search_from_db(&self, query: &[f32], top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, vector_json, metadata, vector_blob FROM vectors WHERE dimension = ?1",
        )?;
        let rows = stmt.query_map(params![self.dimension as i64], |row| {
            let id: String = row.get(0)?;
            let json: String = row.get(1)?;
            let meta: String = row.get(2)?;
            let blob: Option<Vec<u8>> = row.get(3)?;
            Ok((id, json, meta, blob))
        })?;

        let mut heap: BinaryHeap<Reverse<(OrderedFloat<f32>, String, String)>> =
            BinaryHeap::with_capacity(top_k + 1);

        for item in rows {
            let (id, json, meta, blob) = item?;
            let vec_opt = blob
                .and_then(|b| blob_to_vector(&b, self.dimension))
                .or_else(|| serde_json::from_str::<Vec<f32>>(&json).ok());
            if let Some(vec) = vec_opt {
                if vec.len() == self.dimension {
                    let similarity = cosine_similarity(query, &vec);
                    if heap.len() < top_k {
                        heap.push(Reverse((OrderedFloat(similarity), id, meta)));
                    } else if let Some(&Reverse((min_sim, _, _))) = heap.peek() {
                        if OrderedFloat(similarity) > min_sim {
                            heap.pop();
                            heap.push(Reverse((OrderedFloat(similarity), id, meta)));
                        }
                    }
                }
            }
        }

        let mut results: Vec<SearchResult> = heap
            .into_vec()
            .into_iter()
            .map(|Reverse((sim, id, meta))| SearchResult {
                id,
                similarity: sim.into_inner(),
                metadata: meta,
            })
            .collect();
        results
            .sort_unstable_by(|a, b| OrderedFloat(b.similarity).cmp(&OrderedFloat(a.similarity)));
        Ok(results)
    }

    /// 删除向量
    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        conn.execute("DELETE FROM vectors WHERE id = ?1", params![id])?;
        drop(conn);
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(id);
        }
        Ok(())
    }

    /// 获取向量数量
    pub fn count(&self) -> anyhow::Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vectors WHERE dimension = ?1",
            params![self.dimension as i64],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// 清空所有向量
    pub fn clear(&self) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        conn.execute(
            "DELETE FROM vectors WHERE dimension = ?1",
            params![self.dimension as i64],
        )?;
        drop(conn);
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        Ok(())
    }

    /// 获取向量（返回原始未归一化向量）
    pub fn get(&self, id: &str) -> anyhow::Result<Option<(Vec<f32>, String)>> {
        // 从 DB 获取原始向量（缓存中是归一化的，不适合返回）
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let result = conn.query_row(
            "SELECT vector_json, metadata, vector_blob FROM vectors WHERE id = ?1",
            params![id],
            |row| {
                let json: String = row.get(0)?;
                let meta: String = row.get(1)?;
                let blob: Option<Vec<u8>> = row.get(2)?;
                Ok((json, meta, blob))
            },
        );
        match result {
            Ok((json, meta, blob)) => {
                let vector = blob
                    .and_then(|b| blob_to_vector(&b, self.dimension))
                    .or_else(|| serde_json::from_str::<Vec<f32>>(&json).ok())
                    .ok_or_else(|| anyhow::anyhow!("failed to decode vector"))?;
                Ok(Some((vector, meta)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// SIMD 友好的点积（8 元素块处理，利于 LLVM 自动向量化）
#[inline]
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    let chunks = a.len() / 8;
    let mut sum0 = 0.0f32;
    let mut sum1 = 0.0f32;
    let mut sum2 = 0.0f32;
    let mut sum3 = 0.0f32;
    let mut sum4 = 0.0f32;
    let mut sum5 = 0.0f32;
    let mut sum6 = 0.0f32;
    let mut sum7 = 0.0f32;

    for i in 0..chunks {
        let base = i * 8;
        sum0 += a[base] * b[base];
        sum1 += a[base + 1] * b[base + 1];
        sum2 += a[base + 2] * b[base + 2];
        sum3 += a[base + 3] * b[base + 3];
        sum4 += a[base + 4] * b[base + 4];
        sum5 += a[base + 5] * b[base + 5];
        sum6 += a[base + 6] * b[base + 6];
        sum7 += a[base + 7] * b[base + 7];
    }

    let mut sum = (sum0 + sum1) + (sum2 + sum3) + (sum4 + sum5) + (sum6 + sum7);
    // 处理剩余元素
    for i in (chunks * 8)..a.len() {
        sum += a[i] * b[i];
    }
    sum
}

/// 计算余弦相似度（返回 -1 到 1，用于 DB 回退路径）
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product(a, b);
    let na = dot_product(a, a).sqrt();
    let nb = dot_product(b, b).sqrt();
    let denom = (na * nb).max(1e-8);
    dot / denom
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub similarity: f32,
    pub metadata: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_store() -> VectorStore {
        let db = crate::test_support::temp_db_path("vector-test");
        let store = VectorStore::new(db, 3);
        store.init().unwrap();
        store
    }

    #[test]
    fn test_insert_and_search() {
        let store = create_test_store();
        store.insert("a", &[1.0, 0.0, 0.0], "A").unwrap();
        store.insert("b", &[0.0, 1.0, 0.0], "B").unwrap();
        store.insert("c", &[0.9, 0.1, 0.0], "C").unwrap();
        let result = store.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "a");
        assert!(result[0].similarity > 0.99);
        assert_eq!(result[1].id, "c");
    }

    #[test]
    fn test_batch_insert() {
        let store = create_test_store();
        let items = vec![
            ("x".into(), vec![1.0, 0.0, 0.0], "X".into()),
            ("y".into(), vec![0.0, 1.0, 0.0], "Y".into()),
        ];
        store.insert_batch(&items).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        store.insert("a", &[1.0, 0.0, 0.0], "A").unwrap();
        assert_eq!(store.count().unwrap(), 1);
        store.delete("a").unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_get() {
        let store = create_test_store();
        store.insert("a", &[1.0, 2.0, 3.0], "meta").unwrap();
        let (vec, meta) = store.get("a").unwrap().unwrap();
        assert_eq!(vec, vec![1.0, 2.0, 3.0]);
        assert_eq!(meta, "meta");
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn test_clear() {
        let store = create_test_store();
        store.insert("a", &[1.0, 0.0, 0.0], "A").unwrap();
        store.insert("b", &[0.0, 1.0, 0.0], "B").unwrap();
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_dimension_mismatch() {
        let store = create_test_store();
        assert!(store.insert("a", &[1.0, 0.0], "A").is_err());
        assert!(store.search(&[1.0, 0.0], 1).is_err());
    }

    #[test]
    fn test_cosine_similarity_values() {
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 0.01);
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[-1.0, 0.0, 0.0]);
        assert!((sim + 1.0).abs() < 0.01);
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]);
        assert!(sim.abs() < 0.01);
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0, 1.0]);
        assert!((sim - 0.707).abs() < 0.01);
    }

    #[test]
    fn test_search_ordering() {
        let store = create_test_store();
        store
            .insert("exact", &[1.0, 0.0, 0.0], "exact match")
            .unwrap();
        store
            .insert("close", &[0.9, 0.1, 0.0], "close match")
            .unwrap();
        store.insert("far", &[0.0, 1.0, 0.0], "far match").unwrap();
        let results = store.search(&[1.0, 0.0, 0.0], 3).unwrap();
        assert_eq!(results[0].id, "exact");
        assert_eq!(results[1].id, "close");
        assert_eq!(results[2].id, "far");
        assert!(results[0].similarity > results[1].similarity);
        assert!(results[1].similarity > results[2].similarity);
    }

    #[test]
    fn test_replace_existing() {
        let store = create_test_store();
        store.insert("a", &[1.0, 0.0, 0.0], "original").unwrap();
        store.insert("a", &[0.0, 1.0, 0.0], "updated").unwrap();
        let (vector, metadata) = store.get("a").unwrap().unwrap();
        assert_eq!(vector, vec![0.0, 1.0, 0.0]);
        assert_eq!(metadata, "updated");
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_prenormalization_equivalence() {
        // 搜索结果应与原始 cosine_similarity 排序一致
        let store = create_test_store();
        store.insert("a", &[3.0, 0.0, 0.0], "A").unwrap();
        store.insert("b", &[0.0, 5.0, 0.0], "B").unwrap();
        store.insert("c", &[2.7, 0.3, 0.0], "C").unwrap();
        let results = store.search(&[10.0, 0.0, 0.0], 3).unwrap();
        assert_eq!(results[0].id, "a");
        assert_eq!(results[1].id, "c");
        assert_eq!(results[2].id, "b");
        // 相似度应接近 cosine 值
        assert!((results[0].similarity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_blob_roundtrip() {
        let original = vec![1.5f32, -2.3, 0.0, 42.0, std::f32::consts::PI];
        let blob = vector_to_blob(&original);
        let decoded = blob_to_vector(&blob, 5).unwrap();
        assert_eq!(original, decoded);
        // 错误维度应返回 None
        assert!(blob_to_vector(&blob, 3).is_none());
    }

    #[test]
    fn test_topk_correctness() {
        let store = create_test_store();
        // 插入 10 个向量，搜索 top-3
        for i in 0..10 {
            let angle = i as f32 * 0.1;
            store
                .insert(
                    &format!("v{}", i),
                    &[angle.cos(), angle.sin(), 0.0],
                    &format!("meta-{}", i),
                )
                .unwrap();
        }
        let results = store.search(&[1.0, 0.0, 0.0], 3).unwrap();
        assert_eq!(results.len(), 3);
        // 应按相似度降序
        assert!(results[0].similarity >= results[1].similarity);
        assert!(results[1].similarity >= results[2].similarity);
        // v0 = [cos(0), sin(0), 0] = [1, 0, 0] 应最相似
        assert_eq!(results[0].id, "v0");
    }

    #[test]
    fn test_large_dataset_search() {
        let store = create_test_store();
        // 插入 200 个向量
        let items: Vec<(String, Vec<f32>, String)> = (0..200)
            .map(|i| {
                let angle = i as f32 * 0.03;
                (
                    format!("vec-{}", i),
                    vec![angle.cos(), angle.sin(), 0.01 * i as f32],
                    format!("m-{}", i),
                )
            })
            .collect();
        store.insert_batch(&items).unwrap();
        assert_eq!(store.count().unwrap(), 200);

        let results = store.search(&[1.0, 0.0, 0.0], 5).unwrap();
        assert_eq!(results.len(), 5);
        assert!(results[0].similarity > results[4].similarity);
    }
}
