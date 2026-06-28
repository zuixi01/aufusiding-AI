//! OCR 引擎 trait
//!
//! 定义文字识别引擎的抽象接口。

use async_trait::async_trait;

use super::analyzer::TextRegion;
use super::capture::ScreenSnapshot;

/// OCR 引擎 trait
#[async_trait]
pub trait OcrEngine: Send + Sync {
    /// 识别文本区域（带位置和置信度）
    async fn recognize(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>>;
    /// 提取全部文本为单一字符串
    async fn extract_all_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: &dyn OcrEngine) {}
    }
}
