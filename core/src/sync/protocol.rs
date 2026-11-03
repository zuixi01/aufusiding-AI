//! 同步协议 (CRDT)
//!
//! 使用 CRDT (Conflict-free Replicated Data Type) 实现无冲突的多设备同步。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 同步操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    Insert { id: Uuid, data: String },
    Update { id: Uuid, data: String },
    Delete { id: Uuid },
}

/// CRDT 操作记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtOp {
    /// 操作 ID
    pub id: Uuid,
    /// 设备 ID
    pub device_id: String,
    /// 操作类型
    pub operation: Operation,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 向量时钟
    pub vector_clock: Vec<u64>,
}

/// 向量时钟
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorClock {
    pub clocks: Vec<u64>,
}

impl VectorClock {
    pub fn new(size: usize) -> Self {
        Self {
            clocks: vec![0; size],
        }
    }

    pub fn increment(&mut self, index: usize) {
        if index < self.clocks.len() {
            self.clocks[index] += 1;
        }
    }

    pub fn merge(&self, other: &VectorClock) -> VectorClock {
        let len = self.clocks.len().max(other.clocks.len());
        let mut merged = vec![0u64; len];
        for i in 0..len {
            let a = self.clocks.get(i).copied().unwrap_or(0);
            let b = other.clocks.get(i).copied().unwrap_or(0);
            merged[i] = a.max(b);
        }
        VectorClock { clocks: merged }
    }

    /// Returns true if self happens-before other
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let len = self.clocks.len().max(other.clocks.len());
        let mut at_least_one_less = false;
        for i in 0..len {
            let a = self.clocks.get(i).copied().unwrap_or(0);
            let b = other.clocks.get(i).copied().unwrap_or(0);
            if a > b {
                return false;
            }
            if a < b {
                at_least_one_less = true;
            }
        }
        at_least_one_less
    }

    /// Returns true if neither happens-before the other
    pub fn concurrent(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }
}

/// 同步协议
pub struct SyncProtocol {
    _device_id: String,
    device_index: usize,
    clock: VectorClock,
    ops: Vec<CrdtOp>,
    state: HashMap<Uuid, String>,
}

impl SyncProtocol {
    pub fn new(device_id: String) -> Self {
        Self {
            _device_id: device_id,
            device_index: 0,
            clock: VectorClock::new(2),
            ops: Vec::new(),
            state: HashMap::new(),
        }
    }

    pub fn with_index(device_id: String, index: usize, num_devices: usize) -> Self {
        Self {
            _device_id: device_id,
            device_index: index,
            clock: VectorClock::new(num_devices),
            ops: Vec::new(),
            state: HashMap::new(),
        }
    }

    /// 生成同步操作
    pub fn create_op(&mut self, operation: Operation) -> CrdtOp {
        self.clock.increment(self.device_index);
        let op = CrdtOp {
            id: Uuid::new_v4(),
            device_id: self._device_id.clone(),
            operation: operation.clone(),
            timestamp: Utc::now(),
            vector_clock: self.clock.clocks.clone(),
        };
        self.apply_operation(&operation);
        self.ops.push(op.clone());
        op
    }

    /// 应用操作到本地状态
    pub fn apply_op(&mut self, op: &CrdtOp) {
        self.apply_operation(&op.operation);
        let remote_clock = VectorClock {
            clocks: op.vector_clock.clone(),
        };
        self.clock = self.clock.merge(&remote_clock);
        if !self.ops.iter().any(|o| o.id == op.id) {
            self.ops.push(op.clone());
        }
    }

    fn apply_operation(&mut self, op: &Operation) {
        match op {
            Operation::Insert { id, data } => {
                self.state.insert(*id, data.clone());
            }
            Operation::Update { id, data } => {
                self.state.insert(*id, data.clone());
            }
            Operation::Delete { id } => {
                self.state.remove(id);
            }
        }
    }

    /// 合并远程操作 (LWW)
    pub fn merge(&self, local_ops: &[CrdtOp], remote_ops: &[CrdtOp]) -> Vec<CrdtOp> {
        let mut all: Vec<CrdtOp> = local_ops.to_vec();
        for rop in remote_ops {
            if !all.iter().any(|o| o.id == rop.id) {
                all.push(rop.clone());
            }
        }
        // Sort by timestamp (LWW)
        all.sort_by_key(|operation| operation.timestamp);
        all
    }

    /// 计算增量操作
    pub fn diff(&self, local_clock: &[u64], remote_clock: &[u64]) -> Vec<CrdtOp> {
        self.ops
            .iter()
            .filter(|op| {
                // Include ops that remote hasn't seen
                for (i, &val) in op.vector_clock.iter().enumerate() {
                    let remote_val = remote_clock.get(i).copied().unwrap_or(0);
                    let local_val = local_clock.get(i).copied().unwrap_or(0);
                    if val > remote_val && val <= local_val {
                        return true;
                    }
                }
                false
            })
            .cloned()
            .collect()
    }

    pub fn state(&self) -> &HashMap<Uuid, String> {
        &self.state
    }

    pub fn clock(&self) -> &VectorClock {
        &self.clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_op_increments_clock() {
        let mut p = SyncProtocol::new("dev-1".into());
        let id = Uuid::new_v4();
        let op = p.create_op(Operation::Insert {
            id,
            data: "hello".into(),
        });
        assert_eq!(op.vector_clock[0], 1);
        assert_eq!(op.device_id, "dev-1");
    }

    #[test]
    fn vector_clock_increment() {
        let mut vc = VectorClock::new(3);
        vc.increment(1);
        assert_eq!(vc.clocks, vec![0, 1, 0]);
    }

    #[test]
    fn merge_no_conflict() {
        let p = SyncProtocol::new("dev-1".into());
        let op1 = CrdtOp {
            id: Uuid::new_v4(),
            device_id: "dev-1".into(),
            operation: Operation::Insert {
                id: Uuid::new_v4(),
                data: "a".into(),
            },
            timestamp: Utc::now(),
            vector_clock: vec![1, 0],
        };
        let op2 = CrdtOp {
            id: Uuid::new_v4(),
            device_id: "dev-2".into(),
            operation: Operation::Insert {
                id: Uuid::new_v4(),
                data: "b".into(),
            },
            timestamp: Utc::now(),
            vector_clock: vec![0, 1],
        };
        let merged = p.merge(&[op1], &[op2]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_conflict_lww() {
        let p = SyncProtocol::new("dev-1".into());
        let target_id = Uuid::new_v4();
        let t1 = Utc::now();
        let t2 = t1 + chrono::Duration::seconds(1);
        let op1 = CrdtOp {
            id: Uuid::new_v4(),
            device_id: "dev-1".into(),
            operation: Operation::Update {
                id: target_id,
                data: "old".into(),
            },
            timestamp: t1,
            vector_clock: vec![1, 0],
        };
        let op2 = CrdtOp {
            id: Uuid::new_v4(),
            device_id: "dev-2".into(),
            operation: Operation::Update {
                id: target_id,
                data: "new".into(),
            },
            timestamp: t2,
            vector_clock: vec![0, 1],
        };
        let merged = p.merge(&[op1], &[op2]);
        // LWW: last op wins
        assert_eq!(merged.len(), 2);
        if let Operation::Update { data, .. } = &merged[1].operation {
            assert_eq!(data, "new");
        }
    }

    #[test]
    fn diff_computes_delta() {
        let mut p = SyncProtocol::with_index("dev-1".into(), 0, 2);
        let id = Uuid::new_v4();
        p.create_op(Operation::Insert {
            id,
            data: "a".into(),
        });
        p.create_op(Operation::Update {
            id,
            data: "b".into(),
        });
        // Remote has seen clock [1,0], local is at [2,0]
        let delta = p.diff(&[2, 0], &[1, 0]);
        assert_eq!(delta.len(), 1);
    }

    #[test]
    fn diff_empty_when_synced() {
        let mut p = SyncProtocol::with_index("dev-1".into(), 0, 2);
        let id = Uuid::new_v4();
        p.create_op(Operation::Insert {
            id,
            data: "a".into(),
        });
        let delta = p.diff(&[1, 0], &[1, 0]);
        assert!(delta.is_empty());
    }

    #[test]
    fn concurrent_detection() {
        let vc1 = VectorClock { clocks: vec![1, 0] };
        let vc2 = VectorClock { clocks: vec![0, 1] };
        assert!(vc1.concurrent(&vc2));
    }

    #[test]
    fn happens_before() {
        let vc1 = VectorClock { clocks: vec![1, 0] };
        let vc2 = VectorClock { clocks: vec![2, 1] };
        assert!(vc1.happens_before(&vc2));
        assert!(!vc2.happens_before(&vc1));
    }

    #[test]
    fn idempotent_merge() {
        let p = SyncProtocol::new("dev-1".into());
        let op = CrdtOp {
            id: Uuid::new_v4(),
            device_id: "dev-1".into(),
            operation: Operation::Insert {
                id: Uuid::new_v4(),
                data: "a".into(),
            },
            timestamp: Utc::now(),
            vector_clock: vec![1, 0],
        };
        let merged = p.merge(std::slice::from_ref(&op), std::slice::from_ref(&op));
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn state_consistency_after_ops() {
        let mut p = SyncProtocol::with_index("dev-1".into(), 0, 2);
        let id = Uuid::new_v4();
        p.create_op(Operation::Insert {
            id,
            data: "v1".into(),
        });
        p.create_op(Operation::Update {
            id,
            data: "v2".into(),
        });
        assert_eq!(p.state().get(&id).unwrap(), "v2");
        p.create_op(Operation::Delete { id });
        assert!(p.state().get(&id).is_none());
    }
}
