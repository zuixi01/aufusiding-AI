//! 内容类型分析器
//!
//! 分析内容类型（文本/图表/视频）决定最佳输出方式。

use serde::{Deserialize, Serialize};

/// 内容类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// 纯文本（短消息）
    Text,
    /// 报告/文档
    Report,
    /// 图表/数据可视化
    Chart,
    /// 图片
    Image,
    /// 视频
    Video,
    /// 音频
    Audio,
    /// 短消息（适合语音播报）
    ShortMessage,
}

/// 内容分析器
pub struct ContentAnalyzer;

impl ContentAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// 分析内容类型
    pub fn analyze(&self, content: &str, mime_hint: Option<&str>) -> ContentType {
        // MIME 类型优先
        if let Some(mime) = mime_hint {
            if mime.starts_with("video/") {
                return ContentType::Video;
            }
            if mime.starts_with("image/") {
                return ContentType::Image;
            }
            if mime.starts_with("audio/") {
                return ContentType::Audio;
            }
            if mime == "application/pdf" || mime.contains("document") {
                return ContentType::Report;
            }
        }

        // 基于内容长度和特征判断
        let char_count = content.chars().count();

        if char_count <= 100 {
            return ContentType::ShortMessage;
        }

        // 检测图表特征
        let chart_indicators = ["图表", "chart", "graph", "数据", "统计", "趋势"];
        if chart_indicators
            .iter()
            .any(|kw| content.to_lowercase().contains(kw))
        {
            return ContentType::Chart;
        }

        // 长文本视为报告
        if char_count > 500 {
            return ContentType::Report;
        }

        ContentType::Text
    }

    /// 判断内容是否适合语音输出
    pub fn is_voice_friendly(&self, content_type: ContentType) -> bool {
        matches!(
            content_type,
            ContentType::ShortMessage | ContentType::Text | ContentType::Audio
        )
    }

    /// 判断内容是否需要屏幕展示
    pub fn needs_screen(&self, content_type: ContentType) -> bool {
        matches!(
            content_type,
            ContentType::Report | ContentType::Chart | ContentType::Image | ContentType::Video
        )
    }
}
