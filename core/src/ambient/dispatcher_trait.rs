//! 分发器 trait
//!
//! 统一分发接口，让 AmbientLoopRunner 可以接受不同的分发器实现。

use async_trait::async_trait;

use super::intent::DispatchResult;

/// 统一分发接口
#[async_trait]
pub trait Dispatcher: Send + Sync {
    /// 分发用户输入并返回结果
    async fn dispatch(&self, text: &str) -> DispatchResult;
}
