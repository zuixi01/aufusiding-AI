//! 屏幕代理模块
//!
//! 提供屏幕捕获、分析、无障碍桥接、智能代理、OCR 和视觉分析能力。

pub mod accessibility;
pub mod agent;
pub mod analyzer;
pub mod capture;
pub mod http_ocr;
pub mod ocr;
pub mod ocr_mock;
pub mod ocr_traits;
pub mod system_accessibility;
pub mod system_analyzer;
pub mod system_capture;
pub mod types;
pub mod vision;

pub use accessibility::{AccessAction, AccessibilityBridge, MockAccessibilityBridge};
pub use agent::ScreenAgent;
pub use analyzer::{MockScreenAnalyzer, ScreenAnalyzer};
pub use capture::{MockScreenCapture, ScreenCapture};
pub use http_ocr::{HttpOcrConfig, HttpOcrEngine};
pub use ocr::{ImagePreprocessor, TesseractOcrEngine};
pub use ocr_traits::OcrEngine;
pub use system_accessibility::SystemAccessibilityBridge;
pub use system_analyzer::{HttpAnalyzerConfig, HttpScreenAnalyzer};
pub use system_capture::SystemScreenCapture;
pub use types::{
    ElementType, ScreenAction, ScreenActionEdge, ScreenActionGraph, ScreenActionNode,
    ScreenContext, ScreenRegion, ScreenTarget, UIElement,
};
pub use vision::VisionAnalyzer;
