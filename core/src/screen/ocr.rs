//! 图像预处理 + 本地 OCR 引擎 + Tesseract 桥接
//!
//! 提供灰度转换、二值化、缩放、Otsu 阈值等图像预处理工具，
//! 基于连通区域分析的文本检测引擎，以及 Tesseract CLI 桥接。

use async_trait::async_trait;
use std::collections::VecDeque;

use super::analyzer::TextRegion;
use super::capture::ScreenSnapshot;
use super::ocr_traits::OcrEngine;
use super::types::ScreenRegion;

/// 图像预处理工具
pub struct ImagePreprocessor;

impl ImagePreprocessor {
    /// RGBA → 灰度 (ITU-R BT.601: 0.299R + 0.587G + 0.114B)
    pub fn to_grayscale(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let pixel_count = (width * height) as usize;
        let mut gray = Vec::with_capacity(pixel_count);
        for i in 0..pixel_count {
            let base = i * 4;
            if base + 2 < data.len() {
                let r = data[base] as f32;
                let g = data[base + 1] as f32;
                let b = data[base + 2] as f32;
                gray.push((0.299 * r + 0.587 * g + 0.114 * b) as u8);
            } else {
                gray.push(0);
            }
        }
        gray
    }

    /// 二值化阈值处理
    pub fn threshold(grayscale: &[u8], threshold_val: u8) -> Vec<u8> {
        grayscale
            .iter()
            .map(|&v| if v > threshold_val { 255 } else { 0 })
            .collect()
    }

    /// 最近邻缩放 (灰度图)
    pub fn resize(data: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
        if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
            return vec![];
        }
        let mut result = Vec::with_capacity((dst_w * dst_h) as usize);
        for y in 0..dst_h {
            for x in 0..dst_w {
                let src_x = (x * src_w / dst_w).min(src_w - 1);
                let src_y = (y * src_h / dst_h).min(src_h - 1);
                let idx = (src_y * src_w + src_x) as usize;
                result.push(if idx < data.len() { data[idx] } else { 0 });
            }
        }
        result
    }

    /// 计算灰度直方图 (256 bins)
    pub fn histogram(grayscale: &[u8]) -> [u32; 256] {
        let mut hist = [0u32; 256];
        for &v in grayscale {
            hist[v as usize] += 1;
        }
        hist
    }

    /// Otsu 自动阈值
    pub fn otsu_threshold(grayscale: &[u8]) -> u8 {
        if grayscale.is_empty() {
            return 128;
        }
        let hist = Self::histogram(grayscale);
        let total = grayscale.len() as f64;

        let mut sum_total: f64 = 0.0;
        for (i, &count) in hist.iter().enumerate() {
            sum_total += i as f64 * count as f64;
        }

        let mut sum_bg: f64 = 0.0;
        let mut weight_bg: f64 = 0.0;
        let mut max_variance: f64 = 0.0;
        let mut best_threshold: u8 = 0;

        for (t, &count) in hist.iter().enumerate() {
            weight_bg += count as f64;
            if weight_bg == 0.0 {
                continue;
            }
            let weight_fg = total - weight_bg;
            if weight_fg == 0.0 {
                break;
            }
            sum_bg += t as f64 * count as f64;
            let mean_bg = sum_bg / weight_bg;
            let mean_fg = (sum_total - sum_bg) / weight_fg;
            let variance = weight_bg * weight_fg * (mean_bg - mean_fg) * (mean_bg - mean_fg);
            if variance > max_variance {
                max_variance = variance;
                best_threshold = t as u8;
            }
        }
        best_threshold
    }
}

/// 连通分量的包围盒
#[derive(Debug, Clone)]
struct BlobBounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    pixel_count: u32,
}

impl BlobBounds {
    fn width(&self) -> u32 {
        self.max_x - self.min_x + 1
    }

    fn height(&self) -> u32 {
        self.max_y - self.min_y + 1
    }

    fn aspect_ratio(&self) -> f32 {
        self.width() as f32 / self.height().max(1) as f32
    }
}

/// 本地 OCR 引擎
///
/// 基于连通区域分析的文本检测引擎。通过 BFS 洪水填充提取前景连通分量，
/// 按几何特征过滤出字符候选，再按 y 坐标聚类为文本行。
pub struct LocalOcrEngine {
    confidence_threshold: f32,
}

impl LocalOcrEngine {
    pub fn new(confidence_threshold: f32) -> Self {
        Self {
            confidence_threshold: confidence_threshold.clamp(0.0, 1.0),
        }
    }

    /// 扫描二值图像的水平投影，检测文本行候选区
    fn detect_text_rows(binary: &[u8], width: u32, height: u32) -> Vec<(u32, u32)> {
        let mut rows = Vec::new();
        let mut in_text = false;
        let mut start_y = 0u32;
        let density_threshold = width / 10;

        for y in 0..height {
            let row_start = (y * width) as usize;
            let row_end = row_start + width as usize;
            let fg_count: u32 = binary[row_start..row_end.min(binary.len())]
                .iter()
                .filter(|&&v| v == 0)
                .count() as u32;

            if fg_count > density_threshold {
                if !in_text {
                    in_text = true;
                    start_y = y;
                }
            } else if in_text {
                in_text = false;
                if y - start_y >= 4 {
                    rows.push((start_y, y));
                }
            }
        }
        if in_text && height - start_y >= 4 {
            rows.push((start_y, height));
        }
        rows
    }

    /// BFS 洪水填充提取连通分量
    fn extract_blobs(binary: &[u8], width: u32, height: u32) -> Vec<BlobBounds> {
        let w = width as usize;
        let h = height as usize;
        let mut visited = vec![false; w * h];
        let mut blobs = Vec::new();

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                if visited[idx] || binary[idx] != 0 {
                    continue;
                }
                // 前景像素 (0)，开始 BFS
                let mut queue = VecDeque::new();
                queue.push_back((x as u32, y as u32));
                visited[idx] = true;

                let mut blob = BlobBounds {
                    min_x: x as u32,
                    min_y: y as u32,
                    max_x: x as u32,
                    max_y: y as u32,
                    pixel_count: 0,
                };

                while let Some((cx, cy)) = queue.pop_front() {
                    blob.min_x = blob.min_x.min(cx);
                    blob.min_y = blob.min_y.min(cy);
                    blob.max_x = blob.max_x.max(cx);
                    blob.max_y = blob.max_y.max(cy);
                    blob.pixel_count += 1;

                    for (dx, dy) in &[(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                            continue;
                        }
                        let ni = ny as usize * w + nx as usize;
                        if !visited[ni] && binary[ni] == 0 {
                            visited[ni] = true;
                            queue.push_back((nx as u32, ny as u32));
                        }
                    }
                }

                if blob.pixel_count >= 3 {
                    blobs.push(blob);
                }
            }
        }
        blobs
    }

    /// 过滤出字符候选分量（基于几何启发式）
    fn filter_char_blobs(blobs: Vec<BlobBounds>, img_width: u32, img_height: u32) -> Vec<BlobBounds> {
        let max_blob_w = img_width / 2;
        let max_blob_h = img_height / 2;

        blobs
            .into_iter()
            .filter(|b| {
                let w = b.width();
                let h = b.height();
                // 排除过大（非字符）和过小（噪点）的分量
                w >= 2 && h >= 3 && w <= max_blob_w && h <= max_blob_h
                    && b.aspect_ratio() < 10.0
            })
            .collect()
    }

    /// 将字符分量按 y 坐标聚类为文本行
    fn group_blobs_into_lines(blobs: &[BlobBounds]) -> Vec<Vec<&BlobBounds>> {
        if blobs.is_empty() {
            return vec![];
        }

        let mut sorted: Vec<&BlobBounds> = blobs.iter().collect();
        sorted.sort_by_key(|b| b.min_y);

        let mut lines: Vec<Vec<&BlobBounds>> = Vec::new();
        let mut current_line: Vec<&BlobBounds> = vec![sorted[0]];
        let mut line_center_y = (sorted[0].min_y + sorted[0].max_y) / 2;

        for blob in sorted.iter().skip(1) {
            let blob_center_y = (blob.min_y + blob.max_y) / 2;
            let line_height = current_line
                .iter()
                .map(|b| b.height())
                .max()
                .unwrap_or(10);
            let tolerance = (line_height / 2).max(4);

            if blob_center_y.abs_diff(line_center_y) <= tolerance {
                current_line.push(blob);
            } else {
                current_line.sort_by_key(|b| b.min_x);
                lines.push(current_line);
                current_line = vec![blob];
                line_center_y = blob_center_y;
            }
        }
        if !current_line.is_empty() {
            current_line.sort_by_key(|b| b.min_x);
            lines.push(current_line);
        }
        lines
    }
}

#[async_trait]
impl OcrEngine for LocalOcrEngine {
    async fn recognize(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        if snapshot.data.is_empty() || snapshot.width == 0 || snapshot.height == 0 {
            return Ok(vec![]);
        }

        let gray = ImagePreprocessor::to_grayscale(&snapshot.data, snapshot.width, snapshot.height);
        let thresh = ImagePreprocessor::otsu_threshold(&gray);
        let binary = ImagePreprocessor::threshold(&gray, thresh);

        let blobs = Self::extract_blobs(&binary, snapshot.width, snapshot.height);
        let char_blobs = Self::filter_char_blobs(blobs, snapshot.width, snapshot.height);

        if char_blobs.is_empty() {
            // 回退到行密度检测
            let rows = Self::detect_text_rows(&binary, snapshot.width, snapshot.height);
            return Ok(rows
                .into_iter()
                .enumerate()
                .map(|(i, (y_start, y_end))| TextRegion {
                    text: format!("[text_row_{}]", i),
                    bounds: ScreenRegion {
                        x: 0,
                        y: y_start,
                        width: snapshot.width,
                        height: y_end - y_start,
                    },
                    confidence: self.confidence_threshold * 0.5,
                })
                .collect());
        }

        let lines = Self::group_blobs_into_lines(&char_blobs);
        let regions: Vec<TextRegion> = lines
            .iter()
            .enumerate()
            .map(|(i, line_blobs)| {
                let min_x = line_blobs.iter().map(|b| b.min_x).min().unwrap_or(0);
                let min_y = line_blobs.iter().map(|b| b.min_y).min().unwrap_or(0);
                let max_x = line_blobs.iter().map(|b| b.max_x).max().unwrap_or(0);
                let max_y = line_blobs.iter().map(|b| b.max_y).max().unwrap_or(0);
                let char_count = line_blobs.len();

                TextRegion {
                    text: format!("[line_{}_chars_{}]", i, char_count),
                    bounds: ScreenRegion {
                        x: min_x,
                        y: min_y,
                        width: max_x - min_x + 1,
                        height: max_y - min_y + 1,
                    },
                    confidence: self.confidence_threshold,
                }
            })
            .collect();

        Ok(regions)
    }

    async fn extract_all_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<String> {
        let regions = self.recognize(snapshot).await?;
        let text = regions
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }
}

/// Tesseract CLI 桥接 OCR 引擎
///
/// 通过调用系统 `tesseract` 命令实现 OCR。若 tesseract 未安装，
/// 自动回退到 `LocalOcrEngine` 并输出警告日志。
pub struct TesseractOcrEngine {
    /// tesseract --psm 模式 (默认 6: 单一文本块)
    psm: u8,
    /// 语言参数 (默认 "chi_sim+eng")
    lang: String,
    /// 回退引擎
    fallback: LocalOcrEngine,
}

impl TesseractOcrEngine {
    pub fn new() -> Self {
        Self {
            psm: 6,
            lang: "chi_sim+eng".to_string(),
            fallback: LocalOcrEngine::new(0.5),
        }
    }

    pub fn with_lang(mut self, lang: &str) -> Self {
        self.lang = lang.to_string();
        self
    }

    pub fn with_psm(mut self, psm: u8) -> Self {
        self.psm = psm;
        self
    }

    /// 检查 tesseract 是否可用
    async fn is_tesseract_available() -> bool {
        tokio::process::Command::new("tesseract")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// 将 RGBA 数据写为 PGM (灰度) 临时文件供 tesseract 读取
    fn write_pgm_temp(gray: &[u8], width: u32, height: u32) -> anyhow::Result<tempfile::NamedTempFile> {
        use std::io::Write;
        let mut tmp = tempfile::Builder::new()
            .suffix(".pgm")
            .tempfile()?;
        // PGM P5 header
        write!(tmp, "P5\n{} {}\n255\n", width, height)?;
        tmp.write_all(gray)?;
        tmp.flush()?;
        Ok(tmp)
    }

    /// 解析 tesseract stdout 为 TextRegion 列表
    fn parse_output(output: &str, img_width: u32, img_height: u32) -> Vec<TextRegion> {
        let lines: Vec<&str> = output
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.is_empty() {
            return vec![];
        }

        let line_count = lines.len();
        let line_height = (img_height / line_count.max(1) as u32).max(1);

        lines
            .iter()
            .enumerate()
            .map(|(i, &text)| TextRegion {
                text: text.to_string(),
                bounds: ScreenRegion {
                    x: 0,
                    y: i as u32 * line_height,
                    width: img_width,
                    height: line_height.min(img_height - i as u32 * line_height),
                },
                confidence: 0.8,
            })
            .collect()
    }
}

impl Default for TesseractOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrEngine for TesseractOcrEngine {
    async fn recognize(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        if snapshot.data.is_empty() || snapshot.width == 0 || snapshot.height == 0 {
            return Ok(vec![]);
        }

        if !Self::is_tesseract_available().await {
            tracing::warn!(
                "tesseract 未安装，回退到 LocalOcrEngine 像素密度分析"
            );
            return self.fallback.recognize(snapshot).await;
        }

        let gray = ImagePreprocessor::to_grayscale(
            &snapshot.data,
            snapshot.width,
            snapshot.height,
        );

        let tmp = Self::write_pgm_temp(&gray, snapshot.width, snapshot.height)?;
        let tmp_path = tmp.path().to_string_lossy().to_string();

        let output = tokio::process::Command::new("tesseract")
            .arg(&tmp_path)
            .arg("stdout")
            .arg("-l")
            .arg(&self.lang)
            .arg("--psm")
            .arg(self.psm.to_string())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("tesseract 执行失败: {}，回退到 LocalOcrEngine", stderr);
            return self.fallback.recognize(snapshot).await;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let regions = Self::parse_output(&stdout, snapshot.width, snapshot.height);

        if regions.is_empty() {
            // tesseract 无输出时回退
            return self.fallback.recognize(snapshot).await;
        }

        Ok(regions)
    }

    async fn extract_all_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<String> {
        let regions = self.recognize(snapshot).await?;
        let text = regions
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::capture::ImageFormat;
    use chrono::Utc;

    #[test]
    fn grayscale_conversion() {
        // 纯红像素 RGBA
        let data = vec![255, 0, 0, 255];
        let gray = ImagePreprocessor::to_grayscale(&data, 1, 1);
        assert_eq!(gray.len(), 1);
        assert_eq!(gray[0], 76); // 0.299 * 255 ≈ 76
    }

    #[test]
    fn threshold_binary() {
        let gray = vec![50, 100, 150, 200];
        let bin = ImagePreprocessor::threshold(&gray, 120);
        assert_eq!(bin, vec![0, 0, 255, 255]);
    }

    #[test]
    fn resize_downscale() {
        let data = vec![10, 20, 30, 40]; // 2x2
        let resized = ImagePreprocessor::resize(&data, 2, 2, 1, 1);
        assert_eq!(resized.len(), 1);
        assert_eq!(resized[0], 10); // 最近邻取 (0,0)
    }

    #[test]
    fn histogram_counts() {
        let gray = vec![0, 0, 128, 255];
        let hist = ImagePreprocessor::histogram(&gray);
        assert_eq!(hist[0], 2);
        assert_eq!(hist[128], 1);
        assert_eq!(hist[255], 1);
    }

    #[test]
    fn otsu_bimodal() {
        // 双峰分布: 50个暗像素(30) + 50个亮像素(220)
        let mut data = vec![30u8; 50];
        data.extend(vec![220u8; 50]);
        let thresh = ImagePreprocessor::otsu_threshold(&data);
        // Otsu 应选择 30（最后一个暗像素值），因为此处类间方差最大
        assert!(thresh >= 30 && thresh <= 220, "Otsu 阈值应合理: {}", thresh);
    }

    #[test]
    fn otsu_empty() {
        assert_eq!(ImagePreprocessor::otsu_threshold(&[]), 128);
    }

    #[tokio::test]
    async fn ocr_empty_snapshot() {
        let engine = LocalOcrEngine::new(0.8);
        let snap = ScreenSnapshot {
            width: 0, height: 0, data: vec![],
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let regions = engine.recognize(&snap).await.unwrap();
        assert!(regions.is_empty());
    }

    #[tokio::test]
    async fn ocr_detects_text_rows() {
        // 创建 10x10 RGBA 图像，中间行有暗像素（模拟文本）
        let mut data = vec![255u8; 10 * 10 * 4]; // 全白
        // 第 3-6 行设为黑色
        for y in 3..7 {
            for x in 0..10 {
                let base = (y * 10 + x) * 4;
                data[base] = 0;
                data[base + 1] = 0;
                data[base + 2] = 0;
            }
        }
        let snap = ScreenSnapshot {
            width: 10, height: 10, data,
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let engine = LocalOcrEngine::new(0.7);
        let regions = engine.recognize(&snap).await.unwrap();
        assert!(!regions.is_empty());
    }

    // --- 连通分量分析测试 ---

    #[test]
    fn extract_blobs_finds_components() {
        // 8x8 二值图，两个独立的 2x2 黑色块
        let mut binary = vec![255u8; 64];
        // 块1: (1,1)-(2,2)
        binary[1 * 8 + 1] = 0;
        binary[1 * 8 + 2] = 0;
        binary[2 * 8 + 1] = 0;
        binary[2 * 8 + 2] = 0;
        // 块2: (5,5)-(6,6)
        binary[5 * 8 + 5] = 0;
        binary[5 * 8 + 6] = 0;
        binary[6 * 8 + 5] = 0;
        binary[6 * 8 + 6] = 0;

        let blobs = LocalOcrEngine::extract_blobs(&binary, 8, 8);
        assert_eq!(blobs.len(), 2);
        assert_eq!(blobs[0].pixel_count, 4);
        assert_eq!(blobs[1].pixel_count, 4);
    }

    #[test]
    fn filter_char_blobs_removes_noise() {
        let tiny = BlobBounds { min_x: 0, min_y: 0, max_x: 0, max_y: 0, pixel_count: 1 };
        let good = BlobBounds { min_x: 0, min_y: 0, max_x: 5, max_y: 8, pixel_count: 30 };
        let huge = BlobBounds { min_x: 0, min_y: 0, max_x: 99, max_y: 99, pixel_count: 5000 };

        let filtered = LocalOcrEngine::filter_char_blobs(vec![tiny, good, huge], 100, 100);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pixel_count, 30);
    }

    #[test]
    fn group_blobs_into_lines_clusters_by_y() {
        let b1 = BlobBounds { min_x: 0, min_y: 10, max_x: 5, max_y: 20, pixel_count: 20 };
        let b2 = BlobBounds { min_x: 10, min_y: 11, max_x: 15, max_y: 21, pixel_count: 20 };
        let b3 = BlobBounds { min_x: 0, min_y: 50, max_x: 5, max_y: 60, pixel_count: 20 };

        let blobs = vec![b1, b2, b3];
        let lines = LocalOcrEngine::group_blobs_into_lines(&blobs);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 2); // b1, b2 同一行
        assert_eq!(lines[1].len(), 1); // b3 单独一行
    }

    #[tokio::test]
    async fn local_ocr_connected_component_detection() {
        // 20x20 RGBA 图像，两行字符状黑色块
        let mut data = vec![255u8; 20 * 20 * 4];
        // 行1: y=2..5, 两个 3px 宽的块
        for y in 2..6 {
            for x in 2..5 {
                let base = (y * 20 + x) * 4;
                data[base] = 0; data[base + 1] = 0; data[base + 2] = 0;
            }
            for x in 8..11 {
                let base = (y * 20 + x) * 4;
                data[base] = 0; data[base + 1] = 0; data[base + 2] = 0;
            }
        }
        // 行2: y=12..16, 一个块
        for y in 12..17 {
            for x in 3..7 {
                let base = (y * 20 + x) * 4;
                data[base] = 0; data[base + 1] = 0; data[base + 2] = 0;
            }
        }

        let snap = ScreenSnapshot {
            width: 20, height: 20, data,
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let engine = LocalOcrEngine::new(0.7);
        let regions = engine.recognize(&snap).await.unwrap();
        // 应检测到 2 行
        assert!(regions.len() >= 2, "应检测到至少 2 行，实际: {}", regions.len());
        // 第一行应包含 2 个字符
        assert!(regions[0].text.contains("chars_2"), "第一行应有 2 个字符: {}", regions[0].text);
        // 包围盒应有合理的 x 偏移（不再是 x=0 全宽）
        assert!(regions[0].bounds.x > 0 || regions[0].bounds.width < 20);
    }

    // --- TesseractOcrEngine 测试 ---

    #[tokio::test]
    async fn tesseract_engine_empty_snapshot() {
        let engine = TesseractOcrEngine::new();
        let snap = ScreenSnapshot {
            width: 0, height: 0, data: vec![],
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let regions = engine.recognize(&snap).await.unwrap();
        assert!(regions.is_empty());
    }

    #[tokio::test]
    async fn tesseract_engine_falls_back_gracefully() {
        // 无论 tesseract 是否安装，都应返回结果（不 panic）
        let mut data = vec![255u8; 20 * 20 * 4];
        for y in 3..7 {
            for x in 0..20 {
                let base = (y * 20 + x) * 4;
                data[base] = 0; data[base + 1] = 0; data[base + 2] = 0;
            }
        }
        let snap = ScreenSnapshot {
            width: 20, height: 20, data,
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let engine = TesseractOcrEngine::new();
        let result = engine.recognize(&snap).await;
        assert!(result.is_ok(), "应优雅处理（回退或成功）: {:?}", result.err());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tesseract_engine_extract_all_text() {
        let mut data = vec![255u8; 20 * 20 * 4];
        for y in 3..7 {
            for x in 0..20 {
                let base = (y * 20 + x) * 4;
                data[base] = 0; data[base + 1] = 0; data[base + 2] = 0;
            }
        }
        let snap = ScreenSnapshot {
            width: 20, height: 20, data,
            format: ImageFormat::Raw, captured_at: Utc::now(),
        };
        let engine = TesseractOcrEngine::new();
        let text = engine.extract_all_text(&snap).await;
        assert!(text.is_ok());
    }

    #[test]
    fn tesseract_parse_output_splits_lines() {
        let output = "Hello World\nSecond Line\n";
        let regions = TesseractOcrEngine::parse_output(output, 200, 100);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].text, "Hello World");
        assert_eq!(regions[1].text, "Second Line");
        assert_eq!(regions[0].bounds.y, 0);
        assert!(regions[1].bounds.y > 0);
    }

    #[test]
    fn tesseract_parse_output_empty() {
        let regions = TesseractOcrEngine::parse_output("", 200, 100);
        assert!(regions.is_empty());
    }

    #[test]
    fn tesseract_default_config() {
        let engine = TesseractOcrEngine::default();
        assert_eq!(engine.psm, 6);
        assert_eq!(engine.lang, "chi_sim+eng");
    }

    #[test]
    fn tesseract_builder_methods() {
        let engine = TesseractOcrEngine::new()
            .with_lang("eng")
            .with_psm(3);
        assert_eq!(engine.lang, "eng");
        assert_eq!(engine.psm, 3);
    }

    #[test]
    fn write_pgm_temp_creates_valid_file() {
        let gray = vec![128u8; 4]; // 2x2
        let tmp = TesseractOcrEngine::write_pgm_temp(&gray, 2, 2).unwrap();
        let contents = std::fs::read(tmp.path()).unwrap();
        // PGM P5 header check
        assert!(contents.starts_with(b"P5\n2 2\n255\n"));
        // header + 4 bytes of pixel data
        let header_len = "P5\n2 2\n255\n".len();
        assert_eq!(contents.len(), header_len + 4);
    }
}
