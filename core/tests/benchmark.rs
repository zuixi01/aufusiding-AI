//! 性能基准测试
//!
//! 测量核心组件的延迟和吞吐量。
//! 标记 #[ignore]，需手动运行：cargo test --test benchmark -- --ignored --nocapture

use prometheus_core::memory::{KnowledgeBase, WorkingMemory};
use prometheus_core::storage::VectorStore;
use std::path::PathBuf;
use std::time::Instant;

fn temp_path(prefix: &str) -> PathBuf {
    prometheus_core::test_support::temp_db_path(prefix)
}

#[test]
#[ignore]
fn bench_vector_insert_1000() {
    let store = VectorStore::new(temp_path("bench-vec"), 384);
    store.init().unwrap();

    let vectors: Vec<(String, Vec<f32>, String)> = (0..1000)
        .map(|i| {
            let mut v = vec![0.0f32; 384];
            v[i % 384] = 1.0;
            (format!("id-{i}"), v, format!("meta-{i}"))
        })
        .collect();

    let start = Instant::now();
    store.insert_batch(&vectors).unwrap();
    let elapsed = start.elapsed();

    println!("vector_insert_batch(1000 x 384d): {:?}", elapsed);
    println!("  per item: {:?}", elapsed / 1000);

    // Task 5: 添加断言阈值 - 1000条插入应 < 5s
    assert!(elapsed.as_secs() < 1, "1000条向量插入应在1秒内完成，实际: {:?}", elapsed);
}

#[test]
#[ignore]
fn bench_vector_search_in_1000() {
    let store = VectorStore::new(temp_path("bench-search"), 384);
    store.init().unwrap();

    let vectors: Vec<(String, Vec<f32>, String)> = (0..1000)
        .map(|i| {
            let mut v = vec![0.0f32; 384];
            v[i % 384] = 1.0;
            v[(i + 1) % 384] = 0.5;
            (format!("id-{i}"), v, format!("meta-{i}"))
        })
        .collect();
    store.insert_batch(&vectors).unwrap();

    let query = {
        let mut v = vec![0.0f32; 384];
        v[0] = 1.0;
        v[1] = 0.5;
        v
    };

    // 预热（触发缓存加载）
    let _ = store.search(&query, 10).unwrap();

    let iterations = 100u32;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = store.search(&query, 10).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "vector_search(top10 in 1000 x 384d): {:?} for {iterations}x",
        elapsed
    );
    println!("  per search: {:?}", elapsed / iterations);

    // Task 5: 添加断言阈值 - 100次搜索应 < 2s
    assert!(elapsed.as_secs() < 1, "100次向量搜索应在1秒内完成，实际: {:?}", elapsed);
}

#[test]
#[ignore]
fn bench_working_memory_ops() {
    let mut wm = WorkingMemory::new(64);

    let start = Instant::now();
    for i in 0..100 {
        wm.add(format!("memory item {i}"), 0.5 + (i as f64 % 5.0) / 10.0);
    }
    let add_elapsed = start.elapsed();
    println!(
        "working_memory_add(100, cap=64): {:?}, size={}",
        add_elapsed,
        wm.len()
    );

    let iterations = 1000u32;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = wm.get_active_context(10);
    }
    let elapsed = start.elapsed();
    println!(
        "working_memory_get_context(top10, {iterations}x): {:?}",
        elapsed
    );
    println!("  per call: {:?}", elapsed / iterations);

    // Task 5: 添加断言阈值 - 1000次查询应 < 1s
    assert!(elapsed.as_millis() < 200, "1000次工作记忆查询应在200ms内完成，实际: {:?}", elapsed);
}

#[tokio::test]
#[ignore]
async fn bench_knowledge_add_and_search() {
    let mut kb = KnowledgeBase::new(temp_path("bench-kb"), 384).unwrap();

    let start = Instant::now();
    for i in 0..10 {
        let content = format!(
            "Document {i} about topic {t}. Discusses performance, scalability, reliability.",
            i = i,
            t = i % 5
        );
        kb.add_document(&format!("doc-{i}"), &content)
            .await
            .unwrap();
    }
    println!(
        "knowledge_add(10 docs): {:?}, chunks={}",
        start.elapsed(),
        kb.count().unwrap()
    );

    let iterations = 50u32;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = kb.search("performance scalability", 5).await.unwrap();
    }
    let elapsed = start.elapsed();
    println!("knowledge_search(top5, {iterations}x): {:?}", elapsed);
    println!("  per search: {:?}", elapsed / iterations);

    // Task 5: 添加断言阈值 - 50次搜索应 < 5s
    assert!(elapsed.as_secs() < 1, "50次知识库搜索应在1秒内完成，实际: {:?}", elapsed);
}
