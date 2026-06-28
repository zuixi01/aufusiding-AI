//! 食材库存追踪器
//!
//! 管理冰箱/储藏室/冷冻柜中的食材库存，支持低库存告警、
//! 过期检测、自动生成购物清单、语音摘要。

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── 核心类型 ────────────────────────────────────────────

/// 存储位置
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageLocation {
    Fridge,
    Pantry,
    Freezer,
}

/// 食材分类
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemCategory {
    Dairy,
    Meat,
    Vegetable,
    Fruit,
    Grain,
    Beverage,
    Condiment,
    Snack,
    Frozen,
    Other(String),
}

/// 库存项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: Uuid,
    pub name: String,
    pub category: ItemCategory,
    pub quantity: f64,
    pub unit: String,
    pub expiry_date: Option<NaiveDate>,
    pub location: StorageLocation,
    pub added_at: DateTime<Utc>,
    pub low_stock_threshold: f64,
}

/// 购物清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShoppingListEntry {
    pub id: Uuid,
    pub item_name: String,
    pub quantity: f64,
    pub unit: String,
    pub auto_generated: bool,
    pub checked: bool,
}

// ─── InventoryTracker ────────────────────────────────────

/// 食材库存追踪器
pub struct InventoryTracker {
    items: Mutex<HashMap<Uuid, InventoryItem>>,
    shopping_list: Mutex<Vec<ShoppingListEntry>>,
}

impl InventoryTracker {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(HashMap::new()),
            shopping_list: Mutex::new(Vec::new()),
        }
    }

    pub fn add_item(&self, item: InventoryItem) -> Uuid {
        let id = item.id;
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.insert(id, item);
        id
    }

    pub fn remove_item(&self, id: &Uuid) -> Option<InventoryItem> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.remove(id)
    }

    pub fn get_item(&self, id: &Uuid) -> Option<InventoryItem> {
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.get(id).cloned()
    }

    /// 消耗库存，返回剩余量
    pub fn consume(&self, id: &Uuid, amount: f64) -> anyhow::Result<f64> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        let item = items.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("item not found: {}", id))?;
        if amount > item.quantity {
            anyhow::bail!("insufficient stock: have {}, need {}", item.quantity, amount);
        }
        item.quantity -= amount;
        Ok(item.quantity)
    }

    /// 低库存告警
    pub fn low_stock_alerts(&self) -> Vec<InventoryItem> {
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.values()
            .filter(|i| i.quantity <= i.low_stock_threshold)
            .cloned()
            .collect()
    }

    /// 即将过期的食材
    pub fn expiring_soon(&self, within_days: i64) -> Vec<InventoryItem> {
        let today = Utc::now().date_naive();
        let deadline = today + chrono::Duration::days(within_days);
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.values()
            .filter(|i| {
                i.expiry_date
                    .map(|d| d >= today && d <= deadline)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// 从低库存自动生成购物清单
    pub fn generate_shopping_list(&self) -> Vec<ShoppingListEntry> {
        let alerts = self.low_stock_alerts();
        let mut list = self.shopping_list.lock().unwrap_or_else(|e| e.into_inner());
        let mut generated = Vec::new();
        for item in alerts {
            let already_exists = list.iter().any(|e| e.item_name == item.name && !e.checked);
            if !already_exists {
                let entry = ShoppingListEntry {
                    id: Uuid::new_v4(),
                    item_name: item.name.clone(),
                    quantity: item.low_stock_threshold * 2.0 - item.quantity,
                    unit: item.unit.clone(),
                    auto_generated: true,
                    checked: false,
                };
                generated.push(entry.clone());
                list.push(entry);
            }
        }
        generated
    }

    pub fn add_to_shopping_list(&self, item_name: &str, quantity: f64, unit: &str) -> Uuid {
        let entry = ShoppingListEntry {
            id: Uuid::new_v4(),
            item_name: item_name.to_string(),
            quantity,
            unit: unit.to_string(),
            auto_generated: false,
            checked: false,
        };
        let id = entry.id;
        let mut list = self.shopping_list.lock().unwrap_or_else(|e| e.into_inner());
        list.push(entry);
        id
    }

    pub fn check_shopping_item(&self, id: &Uuid) -> anyhow::Result<()> {
        let mut list = self.shopping_list.lock().unwrap_or_else(|e| e.into_inner());
        let entry = list.iter_mut().find(|e| e.id == *id)
            .ok_or_else(|| anyhow::anyhow!("shopping item not found: {}", id))?;
        entry.checked = true;
        Ok(())
    }

    pub fn shopping_list(&self) -> Vec<ShoppingListEntry> {
        self.shopping_list.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn item_count(&self) -> usize {
        self.items.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 语音摘要
    pub fn voice_summary(&self) -> String {
        let low = self.low_stock_alerts();
        let expiring = self.expiring_soon(3);
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());

        if items.is_empty() {
            return "库存为空，没有追踪任何食材。".into();
        }

        let mut parts = Vec::new();
        if !low.is_empty() {
            let names: Vec<&str> = low.iter().map(|i| i.name.as_str()).collect();
            parts.push(format!("{}库存不足", names.join("、")));
        }
        if !expiring.is_empty() {
            let names: Vec<&str> = expiring.iter().map(|i| i.name.as_str()).collect();
            parts.push(format!("{}即将过期", names.join("、")));
        }
        if parts.is_empty() {
            format!("库存正常，共追踪 {} 种食材。", items.len())
        } else {
            parts.join("，") + "。"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn milk() -> InventoryItem {
        InventoryItem {
            id: Uuid::new_v4(),
            name: "牛奶".into(),
            category: ItemCategory::Dairy,
            quantity: 2.0,
            unit: "盒".into(),
            expiry_date: Some(Utc::now().date_naive() + chrono::Duration::days(2)),
            location: StorageLocation::Fridge,
            added_at: Utc::now(),
            low_stock_threshold: 1.0,
        }
    }

    fn eggs() -> InventoryItem {
        InventoryItem {
            id: Uuid::new_v4(),
            name: "鸡蛋".into(),
            category: ItemCategory::Other("蛋类".into()),
            quantity: 3.0,
            unit: "个".into(),
            expiry_date: Some(Utc::now().date_naive() + chrono::Duration::days(10)),
            location: StorageLocation::Fridge,
            added_at: Utc::now(),
            low_stock_threshold: 5.0,
        }
    }

    fn rice() -> InventoryItem {
        InventoryItem {
            id: Uuid::new_v4(),
            name: "大米".into(),
            category: ItemCategory::Grain,
            quantity: 5.0,
            unit: "kg".into(),
            expiry_date: None,
            location: StorageLocation::Pantry,
            added_at: Utc::now(),
            low_stock_threshold: 2.0,
        }
    }

    #[test]
    fn add_and_get_item() {
        let tracker = InventoryTracker::new();
        let item = milk();
        let id = tracker.add_item(item.clone());
        let got = tracker.get_item(&id).unwrap();
        assert_eq!(got.name, "牛奶");
        assert_eq!(tracker.item_count(), 1);
    }

    #[test]
    fn remove_item() {
        let tracker = InventoryTracker::new();
        let id = tracker.add_item(milk());
        let removed = tracker.remove_item(&id);
        assert!(removed.is_some());
        assert_eq!(tracker.item_count(), 0);
    }

    #[test]
    fn consume_reduces_quantity() {
        let tracker = InventoryTracker::new();
        let id = tracker.add_item(milk());
        let remaining = tracker.consume(&id, 1.0).unwrap();
        assert!((remaining - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn consume_insufficient_stock() {
        let tracker = InventoryTracker::new();
        let id = tracker.add_item(milk());
        assert!(tracker.consume(&id, 10.0).is_err());
    }

    #[test]
    fn consume_unknown_item() {
        let tracker = InventoryTracker::new();
        assert!(tracker.consume(&Uuid::new_v4(), 1.0).is_err());
    }

    #[test]
    fn low_stock_alerts_detects() {
        let tracker = InventoryTracker::new();
        tracker.add_item(eggs()); // quantity 3 < threshold 5
        tracker.add_item(rice()); // quantity 5 > threshold 2
        let alerts = tracker.low_stock_alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].name, "鸡蛋");
    }

    #[test]
    fn expiring_soon_detects() {
        let tracker = InventoryTracker::new();
        tracker.add_item(milk()); // expires in 2 days
        tracker.add_item(rice()); // no expiry
        let expiring = tracker.expiring_soon(3);
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].name, "牛奶");
    }

    #[test]
    fn expiring_soon_ignores_far_future() {
        let tracker = InventoryTracker::new();
        tracker.add_item(eggs()); // expires in 10 days
        let expiring = tracker.expiring_soon(3);
        assert!(expiring.is_empty());
    }

    #[test]
    fn generate_shopping_list_from_low_stock() {
        let tracker = InventoryTracker::new();
        tracker.add_item(eggs()); // low stock
        let generated = tracker.generate_shopping_list();
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].item_name, "鸡蛋");
        assert!(generated[0].auto_generated);
    }

    #[test]
    fn generate_shopping_list_no_duplicates() {
        let tracker = InventoryTracker::new();
        tracker.add_item(eggs());
        tracker.generate_shopping_list();
        let second = tracker.generate_shopping_list();
        assert!(second.is_empty());
    }

    #[test]
    fn manual_shopping_list_entry() {
        let tracker = InventoryTracker::new();
        let id = tracker.add_to_shopping_list("酱油", 1.0, "瓶");
        let list = tracker.shopping_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].item_name, "酱油");
        assert!(!list[0].auto_generated);
        tracker.check_shopping_item(&id).unwrap();
        let list = tracker.shopping_list();
        assert!(list[0].checked);
    }

    #[test]
    fn check_unknown_shopping_item() {
        let tracker = InventoryTracker::new();
        assert!(tracker.check_shopping_item(&Uuid::new_v4()).is_err());
    }

    #[test]
    fn voice_summary_empty() {
        let tracker = InventoryTracker::new();
        assert!(tracker.voice_summary().contains("库存为空"));
    }

    #[test]
    fn voice_summary_normal() {
        let tracker = InventoryTracker::new();
        tracker.add_item(rice());
        assert!(tracker.voice_summary().contains("库存正常"));
    }

    #[test]
    fn voice_summary_low_stock() {
        let tracker = InventoryTracker::new();
        tracker.add_item(eggs());
        let summary = tracker.voice_summary();
        assert!(summary.contains("鸡蛋"));
        assert!(summary.contains("库存不足"));
    }

    #[test]
    fn storage_location_serde_roundtrip() {
        let loc = StorageLocation::Fridge;
        let json = serde_json::to_string(&loc).unwrap();
        let back: StorageLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, back);
    }

    #[test]
    fn item_category_serde_roundtrip() {
        let cat = ItemCategory::Other("蛋类".into());
        let json = serde_json::to_string(&cat).unwrap();
        let back: ItemCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(cat, back);
    }
}
