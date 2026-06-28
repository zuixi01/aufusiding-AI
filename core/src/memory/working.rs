//! L1 工作记忆
//!
//! 短期记忆，维护当前对话的活跃上下文，类似人类的工作记忆容量限制。
//! 使用 Ebbinghaus 遗忘曲线模型实现记忆衰减。

use std::time::Instant;

/// 工作记忆条目
#[derive(Debug, Clone)]
pub struct MemoryItem {
    /// 内容
    pub content: String,
    /// 创建时间
    pub timestamp: Instant,
    /// 基础重要性 (0.0 - 1.0)
    pub importance: f64,
    /// 访问次数
    pub access_count: usize,
    /// 用户是否显式标记
    pub user_marked: bool,
}

/// 工作记忆
pub struct WorkingMemory {
    /// 记忆条目
    items: Vec<MemoryItem>,
    /// 最大容量
    capacity: usize,
    /// 衰减时间常数 τ (秒)
    decay_constant: f64,
}

impl WorkingMemory {
    /// 创建工作记忆，指定容量
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Vec::new(),
            capacity,
            decay_constant: 3600.0, // 默认 1 小时
        }
    }

    /// 设置衰减时间常数
    pub fn with_decay_constant(mut self, tau: f64) -> Self {
        self.decay_constant = tau;
        self
    }

    /// 添加新记忆条目
    pub fn add(&mut self, content: String, importance: f64) {
        let item = MemoryItem {
            content,
            timestamp: Instant::now(),
            importance: importance.clamp(0.0, 1.0),
            access_count: 0,
            user_marked: false,
        };

        self.items.push(item);
        self.evict_if_needed();
    }

    /// 标记记忆为重要
    pub fn mark_important(&mut self, index: usize) {
        if let Some(item) = self.items.get_mut(index) {
            item.user_marked = true;
        }
    }

    /// 访问记忆（增加访问计数）
    pub fn access(&mut self, index: usize) {
        if let Some(item) = self.items.get_mut(index) {
            item.access_count += 1;
        }
    }

    /// 计算记忆权重：重要性 × e^(-t/τ)
    pub fn calculate_weight(&self, item: &MemoryItem) -> f64 {
        let elapsed = item.timestamp.elapsed().as_secs_f64();
        let retention = (-elapsed / self.decay_constant).exp();

        // 综合重要性计算
        let base_importance = item.importance; // 基础重要性
        let user_weight = if item.user_marked { 1.0 } else { 0.0 };
        let access_weight = (item.access_count as f64 + 1.0).ln(); // +1 避免 ln(0)

        // 权重组合：基础重要性 + 用户标记 + 访问频率
        let importance = 0.5 * base_importance + 0.3 * user_weight + 0.2 * access_weight;

        importance * retention
    }

    /// 获取活跃上下文（高权重记忆）
    pub fn get_active_context(&self, max_items: usize) -> Vec<String> {
        let mut scored: Vec<(usize, f64)> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let weight = self.calculate_weight(item);
                (idx, weight)
            })
            .collect();

        // 按权重排序
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .iter()
            .take(max_items)
            .map(|(idx, _)| self.items[*idx].content.clone())
            .collect()
    }

    /// 基于权重淘汰低价值记忆
    fn evict_if_needed(&mut self) {
        if self.items.len() <= self.capacity {
            return;
        }

        // 计算所有记忆的权重
        let mut scored: Vec<(usize, f64)> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx, self.calculate_weight(item)))
            .collect();

        // 按权重排序（从低到高）
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 移除权重最低的项
        let to_remove = self.items.len() - self.capacity;
        let mut indices_to_remove: Vec<usize> =
            scored.iter().take(to_remove).map(|(idx, _)| *idx).collect();

        // 从后往前删除，避免索引变化
        indices_to_remove.sort_by(|a, b| b.cmp(a));
        for idx in indices_to_remove {
            self.items.remove(idx);
        }
    }

    /// 获取当前记忆数量
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 获取所有记忆条目
    pub fn all_items(&self) -> &[MemoryItem] {
        &self.items
    }

    /// 取出重要性 >= min_importance 的条目（从工作记忆中移除）
    pub fn drain_important(&mut self, min_importance: f64) -> Vec<MemoryItem> {
        // 先计算所有权重
        let weights: Vec<f64> = self
            .items
            .iter()
            .map(|item| self.calculate_weight(item))
            .collect();

        let mut important = Vec::new();
        let mut remaining = Vec::new();

        for (item, weight) in self.items.drain(..).zip(weights.into_iter()) {
            if weight >= min_importance {
                important.push(item);
            } else {
                remaining.push(item);
            }
        }

        self.items = remaining;
        important
    }

    /// 清空所有记忆
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// 兼容旧测试
    #[cfg(test)]
    pub fn items(&self) -> &[MemoryItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_add_memory() {
        let mut memory = WorkingMemory::new(5);
        memory.add("Hello".to_string(), 0.8);

        assert_eq!(memory.len(), 1);
        assert_eq!(memory.items()[0].content, "Hello");
        assert_eq!(memory.items()[0].importance, 0.8);
    }

    #[test]
    fn test_capacity_limit() {
        let mut memory = WorkingMemory::new(3);

        memory.add("Item 1".to_string(), 0.5);
        memory.add("Item 2".to_string(), 0.8);
        memory.add("Item 3".to_string(), 0.6);
        memory.add("Item 4".to_string(), 0.9);

        // 应该只保留 3 个最重要的
        assert_eq!(memory.len(), 3);
    }

    #[test]
    fn test_weight_calculation() {
        let memory = WorkingMemory::new(5);
        let item = MemoryItem {
            content: "Test".to_string(),
            timestamp: Instant::now(),
            importance: 0.8,
            access_count: 0,
            user_marked: false,
        };

        let weight = memory.calculate_weight(&item);

        // 新创建的记忆权重应该较高
        assert!(weight > 0.1);
        assert!(weight <= 1.0);
    }

    #[test]
    fn test_memory_decay() {
        let mut memory = WorkingMemory::new(5).with_decay_constant(1.0); // 1秒衰减

        memory.add("Fresh".to_string(), 0.8);

        let weight_before = memory.calculate_weight(&memory.items()[0]);

        // 等待一段时间
        thread::sleep(Duration::from_millis(500));

        let weight_after = memory.calculate_weight(&memory.items()[0]);

        // 权重应该下降
        assert!(weight_after < weight_before);
    }

    #[test]
    fn test_user_marked_importance() {
        let mut memory = WorkingMemory::new(5);

        memory.add("Normal".to_string(), 0.5);
        memory.add("Important".to_string(), 0.5);
        memory.mark_important(1);

        let weight_normal = memory.calculate_weight(&memory.items()[0]);
        let weight_marked = memory.calculate_weight(&memory.items()[1]);

        // 用户标记的记忆权重应该更高
        assert!(weight_marked > weight_normal);
    }

    #[test]
    fn test_access_count_affects_weight() {
        let mut memory = WorkingMemory::new(5);

        memory.add("Rarely accessed".to_string(), 0.5);
        memory.add("Frequently accessed".to_string(), 0.5);

        // 多次访问第二个记忆
        memory.access(1);
        memory.access(1);
        memory.access(1);

        let weight_rare = memory.calculate_weight(&memory.items()[0]);
        let weight_frequent = memory.calculate_weight(&memory.items()[1]);

        // 频繁访问的记忆权重应该更高
        assert!(weight_frequent > weight_rare);
    }

    #[test]
    fn test_get_active_context() {
        let mut memory = WorkingMemory::new(10);

        memory.add("Low importance".to_string(), 0.2);
        memory.add("High importance".to_string(), 0.9);
        memory.add("Medium importance".to_string(), 0.5);

        let context = memory.get_active_context(2);

        // 应该返回权重最高的 2 个
        assert_eq!(context.len(), 2);
        assert!(context.contains(&"High importance".to_string()));
    }

    #[test]
    fn test_eviction_keeps_high_weight_items() {
        let mut memory = WorkingMemory::new(2);

        memory.add("Low".to_string(), 0.3);
        memory.add("High".to_string(), 0.9);
        memory.add("Medium".to_string(), 0.6);

        // 应该保留 High 和 Medium
        assert_eq!(memory.len(), 2);
        let contents: Vec<String> = memory
            .items()
            .iter()
            .map(|item| item.content.clone())
            .collect();

        assert!(contents.contains(&"High".to_string()));
        assert!(contents.contains(&"Medium".to_string()));
        assert!(!contents.contains(&"Low".to_string()));
    }

    #[test]
    fn test_importance_clamping() {
        let mut memory = WorkingMemory::new(5);

        memory.add("Over limit".to_string(), 1.5);
        memory.add("Under limit".to_string(), -0.5);

        assert_eq!(memory.items()[0].importance, 1.0);
        assert_eq!(memory.items()[1].importance, 0.0);
    }

    #[test]
    fn drain_important_splits_by_weight() {
        let mut memory = WorkingMemory::new(10);
        memory.add("Low".to_string(), 0.1);
        memory.add("High".to_string(), 0.9);
        memory.add("Medium".to_string(), 0.5);

        // Weight = (0.5*importance + 0.3*0 + 0.2*ln(1)) * retention ≈ 0.5*importance
        // Low: ~0.05, Medium: ~0.25, High: ~0.45
        let drained = memory.drain_important(0.2);
        assert!(drained.iter().any(|i| i.content == "High"));
        assert!(drained.iter().any(|i| i.content == "Medium"));
        assert!(!drained.iter().any(|i| i.content == "Low"));
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn drain_important_empty_memory() {
        let mut memory = WorkingMemory::new(5);
        let drained = memory.drain_important(0.5);
        assert!(drained.is_empty());
        assert_eq!(memory.len(), 0);
    }

    #[test]
    fn all_items_returns_all() {
        let mut memory = WorkingMemory::new(10);
        memory.add("A".to_string(), 0.5);
        memory.add("B".to_string(), 0.7);
        memory.add("C".to_string(), 0.3);

        let items = memory.all_items();
        assert_eq!(items.len(), 3);
        let contents: Vec<&str> = items.iter().map(|i| i.content.as_str()).collect();
        assert!(contents.contains(&"A"));
        assert!(contents.contains(&"B"));
        assert!(contents.contains(&"C"));
    }

    #[test]
    fn clear_removes_all() {
        let mut memory = WorkingMemory::new(10);
        memory.add("A".to_string(), 0.5);
        memory.add("B".to_string(), 0.7);
        memory.clear();
        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
    }
}
