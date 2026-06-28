//! L3 知识库
//!
//! 用户导入的文档、笔记等结构化知识，支持 RAG 检索增强生成。
//! 使用混合检索：向量检索 + BM25 关键词检索 + RRF 融合。

use crate::memory::embedding::EmbeddingService;
use crate::storage::vector::VectorStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// 知识条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    /// 条目 ID
    pub id: Uuid,
    /// 来源（文件路径、URL 等）
    pub source: String,
    /// 文本块
    pub content: String,
    /// 嵌入向量
    pub embedding: Vec<f32>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 知识库
pub struct KnowledgeBase {
    /// 向量存储
    vector_store: VectorStore,
    /// BM25 索引（简化实现）
    bm25_index: BM25Index,
    /// Embedding 服务
    embedding_service: Arc<dyn EmbeddingService>,
}

/// BM25 索引（简化实现）
struct BM25Index {
    /// 文档 ID -> 词频
    documents: HashMap<String, HashMap<String, usize>>,
    /// 文档总数
    doc_count: usize,
    /// 词 -> 包含该词的文档数
    doc_freq: HashMap<String, usize>,
}

impl BM25Index {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
            doc_count: 0,
            doc_freq: HashMap::new(),
        }
    }

    fn add_document(&mut self, id: &str, text: &str) {
        let terms = Self::tokenize(text);
        let mut term_freq = HashMap::new();

        for term in &terms {
            *term_freq.entry(term.clone()).or_insert(0) += 1;
        }

        // 更新文档频率
        for term in term_freq.keys() {
            *self.doc_freq.entry(term.clone()).or_insert(0) += 1;
        }

        self.documents.insert(id.to_string(), term_freq);
        self.doc_count += 1;
    }

    fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        let query_terms = Self::tokenize(query);
        let mut scores: HashMap<String, f64> = HashMap::new();

        for (doc_id, term_freq) in &self.documents {
            let score = self.calculate_bm25_score(&query_terms, term_freq);
            if score > 0.0 {
                scores.insert(doc_id.clone(), score);
            }
        }

        let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_k);
        ranked
    }

    fn calculate_bm25_score(
        &self,
        query_terms: &[String],
        doc_term_freq: &HashMap<String, usize>,
    ) -> f64 {
        let k1 = 1.5;
        let b = 0.75;
        let avgdl = 100.0; // 假设平均文档长度

        let doc_len = doc_term_freq.values().sum::<usize>() as f64;
        let mut score = 0.0;

        for term in query_terms {
            if let Some(&tf) = doc_term_freq.get(term) {
                let df = self.doc_freq.get(term).copied().unwrap_or(1);
                let idf =
                    ((self.doc_count as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();

                let tf_component =
                    (tf as f64 * (k1 + 1.0)) / (tf as f64 + k1 * (1.0 - b + b * doc_len / avgdl));

                score += idf * tf_component;
            }
        }

        score
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && !c.is_ascii_alphabetic())
            .filter(|s| s.len() > 2)
            .map(|s| s.to_string())
            .collect()
    }
}

impl KnowledgeBase {
    /// 创建知识库
    pub fn new(db_path: PathBuf, dimension: usize) -> anyhow::Result<Self> {
        let vector_store = VectorStore::new(db_path, dimension);
        vector_store.init()?;

        let embedding_service = Arc::new(crate::memory::embedding::MockEmbeddingService::new(
            dimension,
        ));

        Ok(Self {
            vector_store,
            bm25_index: BM25Index::new(),
            embedding_service,
        })
    }

    /// 创建带自定义 EmbeddingService 的知识库
    pub fn with_embedding_service(
        db_path: PathBuf,
        embedding_service: Arc<dyn EmbeddingService>,
    ) -> anyhow::Result<Self> {
        let dimension = embedding_service.dimension();
        let vector_store = VectorStore::new(db_path, dimension);
        vector_store.init()?;

        Ok(Self {
            vector_store,
            bm25_index: BM25Index::new(),
            embedding_service,
        })
    }

    /// 添加文档
    pub async fn add_document(&mut self, source: &str, content: &str) -> anyhow::Result<usize> {
        // 文档分块
        let chunks = self.chunk_document(content);

        for chunk_text in &chunks {
            let chunk_id = Uuid::new_v4();

            // 生成向量
            let embedding = self.generate_embedding(chunk_text).await?;

            let chunk = KnowledgeChunk {
                id: chunk_id,
                source: source.to_string(),
                content: chunk_text.clone(),
                embedding: embedding.clone(),
                metadata: HashMap::new(),
            };

            // 存储到向量数据库
            let metadata = serde_json::to_string(&chunk)?;
            self.vector_store
                .insert(&chunk_id.to_string(), &embedding, &metadata)?;

            // 添加到 BM25 索引
            self.bm25_index
                .add_document(&chunk_id.to_string(), chunk_text);
        }

        Ok(chunks.len())
    }

    /// 混合检索：向量 + BM25 + RRF
    pub async fn search(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<KnowledgeChunk>> {
        // 向量检索
        let query_embedding = self.generate_embedding(query).await?;
        let vector_results = self.vector_store.search(&query_embedding, top_k * 2)?;

        // BM25 检索
        let bm25_results = self.bm25_index.search(query, top_k * 2);

        // RRF 融合
        let fused = self.reciprocal_rank_fusion(&vector_results, &bm25_results, top_k)?;

        Ok(fused)
    }

    /// Reciprocal Rank Fusion (RRF) 融合算法
    fn reciprocal_rank_fusion(
        &self,
        vector_results: &[crate::storage::vector::SearchResult],
        bm25_results: &[(String, f64)],
        top_k: usize,
    ) -> anyhow::Result<Vec<KnowledgeChunk>> {
        let k = 60.0;
        let mut scores: HashMap<String, f64> = HashMap::new();

        // 向量检索得分
        for (rank, result) in vector_results.iter().enumerate() {
            *scores.entry(result.id.clone()).or_insert(0.0) += 1.0 / (k + rank as f64);
        }

        // BM25 得分
        for (rank, (id, _)) in bm25_results.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f64);
        }

        // 排序
        let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 获取 top-k 结果
        let mut results = Vec::new();
        for (id, _score) in ranked.iter().take(top_k) {
            if let Some((_, metadata)) = self.vector_store.get(id)? {
                let chunk: KnowledgeChunk = serde_json::from_str(&metadata)?;
                results.push(chunk);
            }
        }

        Ok(results)
    }

    /// 文档分块策略
    fn chunk_document(&self, content: &str) -> Vec<String> {
        let chunk_size = 512; // 字符数
        let overlap = 50; // 重叠字符数

        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let end = (start + chunk_size).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            chunks.push(chunk);

            if end >= chars.len() {
                break;
            }
            start += chunk_size - overlap;
        }

        chunks
    }

    /// 生成嵌入向量（委托给 EmbeddingService）
    async fn generate_embedding(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        self.embedding_service.embed(text).await
    }

    /// 获取知识块数量
    pub fn count(&self) -> anyhow::Result<usize> {
        self.vector_store.count()
    }

    /// 删除知识块
    pub fn delete(&self, id: &Uuid) -> anyhow::Result<()> {
        self.vector_store.delete(&id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kb() -> KnowledgeBase {
        let db = crate::test_support::temp_db_path("kb-test");
        KnowledgeBase::new(db, 128).unwrap()
    }

    #[tokio::test]
    async fn test_add_document() {
        let mut kb = create_test_kb();

        let content = "这是一个测试文档。它包含一些测试内容。";
        let count = kb.add_document("test.txt", content).await.unwrap();

        assert!(count > 0);
        assert_eq!(kb.count().unwrap(), count);
    }

    #[tokio::test]
    async fn test_chunk_document() {
        let kb = create_test_kb();

        let content = "a".repeat(1000);
        let chunks = kb.chunk_document(&content);

        // 应该分成多个块
        assert!(chunks.len() > 1);

        // 每个块应该有重叠
        assert!(chunks[0].len() <= 512);
    }

    #[tokio::test]
    async fn test_search() {
        let mut kb = create_test_kb();

        // 添加文档
        kb.add_document("doc1.txt", "Rust 是一门系统编程语言")
            .await
            .unwrap();
        kb.add_document("doc2.txt", "Python 是一门脚本语言")
            .await
            .unwrap();
        kb.add_document("doc3.txt", "编程语言有很多种")
            .await
            .unwrap();

        // 搜索
        let results = kb.search("编程语言", 2).await.unwrap();

        assert!(results.len() <= 2);
        assert!(results.len() > 0);
    }

    #[tokio::test]
    async fn test_bm25_search() {
        let mut index = BM25Index::new();

        index.add_document("doc1", "rust programming language");
        index.add_document("doc2", "python programming language");
        index.add_document("doc3", "java programming");

        let results = index.search("programming language", 2);

        assert_eq!(results.len(), 2);
        assert!(results[0].1 > 0.0);
    }

    #[tokio::test]
    async fn test_bm25_tokenize() {
        let tokens = BM25Index::tokenize("Hello, World! This is a test.");

        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        assert!(!tokens.contains(&"is".to_string())); // 太短被过滤
    }

    #[tokio::test]
    async fn test_rrf_fusion() {
        let mut kb = create_test_kb();

        // 添加多个文档
        kb.add_document("doc1", "人工智能和机器学习").await.unwrap();
        kb.add_document("doc2", "深度学习是机器学习的一个分支")
            .await
            .unwrap();
        kb.add_document("doc3", "自然语言处理").await.unwrap();

        // 搜索应该融合向量和 BM25 结果
        let results = kb.search("机器学习", 2).await.unwrap();

        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_chunk_overlap() {
        let kb = create_test_kb();

        let content = "0123456789".repeat(100); // 1000 字符
        let chunks = kb.chunk_document(&content);

        // 检查重叠
        if chunks.len() > 1 {
            let end_of_first = &chunks[0][chunks[0].len() - 50..];
            let start_of_second = &chunks[1][..50];
            assert_eq!(end_of_first, start_of_second);
        }
    }

    #[tokio::test]
    async fn test_embedding_consistency() {
        let kb = create_test_kb();

        let emb1 = kb.generate_embedding("测试文本").await.unwrap();
        let emb2 = kb.generate_embedding("测试文本").await.unwrap();

        assert_eq!(emb1, emb2);
        assert_eq!(emb1.len(), 128);
    }

    #[tokio::test]
    async fn test_multiple_documents() {
        let mut kb = create_test_kb();

        for i in 0..5 {
            let content = format!("文档 {} 的内容", i);
            kb.add_document(&format!("doc{}.txt", i), &content)
                .await
                .unwrap();
        }

        assert!(kb.count().unwrap() >= 5);
    }

    #[tokio::test]
    async fn test_long_document_chunking() {
        let mut kb = create_test_kb();

        let long_content = "这是一个很长的文档。".repeat(200);
        let count = kb.add_document("long.txt", &long_content).await.unwrap();

        // 长文档应该被分成多个块
        assert!(count > 1);
    }
}
