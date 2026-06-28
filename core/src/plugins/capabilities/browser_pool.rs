//! 浏览器实例池管理

use std::sync::atomic::{AtomicUsize, Ordering};

/// 浏览器实例信息
#[derive(Debug)]
pub struct BrowserInstance {
    pub id: usize,
    pub in_use: bool,
}

/// 浏览器实例池
pub struct BrowserPool {
    max_instances: usize,
    instances: std::sync::Mutex<Vec<BrowserInstance>>,
    next_id: AtomicUsize,
}

impl BrowserPool {
    pub fn new(max_instances: usize) -> Self {
        Self {
            max_instances,
            instances: std::sync::Mutex::new(Vec::new()),
            next_id: AtomicUsize::new(0),
        }
    }

    /// 获取一个可用实例
    pub fn acquire(&self) -> anyhow::Result<usize> {
        let mut instances = self.instances.lock().unwrap_or_else(|e| e.into_inner());

        // 尝试复用空闲实例
        if let Some(inst) = instances.iter_mut().find(|i| !i.in_use) {
            inst.in_use = true;
            return Ok(inst.id);
        }

        // 创建新实例
        if instances.len() >= self.max_instances {
            anyhow::bail!("浏览器实例池已满 (max={})", self.max_instances);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        instances.push(BrowserInstance { id, in_use: true });
        Ok(id)
    }

    /// 释放实例
    pub fn release(&self, id: usize) {
        let mut instances = self.instances.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(inst) = instances.iter_mut().find(|i| i.id == id) {
            inst.in_use = false;
        }
    }

    /// 活跃实例数
    pub fn active_count(&self) -> usize {
        self.instances.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|i| i.in_use).count()
    }

    /// 总实例数
    pub fn total_count(&self) -> usize {
        self.instances.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 最大实例数
    pub fn max_instances(&self) -> usize {
        self.max_instances
    }
}
