//! 语音数据类型

/// 音频缓冲区
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// PCM 采样数据
    pub samples: Vec<f32>,
    /// 采样率 (Hz)
    pub sample_rate: u32,
    /// 声道数
    pub channels: u16,
}

impl AudioBuffer {
    /// 创建音频缓冲区
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> anyhow::Result<Self> {
        if sample_rate == 0 {
            anyhow::bail!("采样率不能为 0");
        }
        if channels == 0 {
            anyhow::bail!("声道数不能为 0");
        }
        if !samples.is_empty() && !samples.len().is_multiple_of(channels as usize) {
            anyhow::bail!(
                "采样数 ({}) 必须是声道数 ({}) 的整数倍",
                samples.len(),
                channels
            );
        }
        Ok(Self {
            samples,
            sample_rate,
            channels,
        })
    }

    /// 音频时长（秒）
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    /// 帧数
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// 创建静音缓冲区
    pub fn silence(duration_secs: f64, sample_rate: u32, channels: u16) -> Self {
        let num_samples = (duration_secs * sample_rate as f64 * channels as f64) as usize;
        Self {
            samples: vec![0.0; num_samples],
            sample_rate,
            channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_valid_buffer() {
        let buf = AudioBuffer::new(vec![0.0; 16000], 16000, 1).unwrap();
        assert_eq!(buf.duration_secs(), 1.0);
        assert_eq!(buf.frame_count(), 16000);
    }

    #[test]
    fn create_stereo_buffer() {
        let buf = AudioBuffer::new(vec![0.0; 32000], 16000, 2).unwrap();
        assert_eq!(buf.duration_secs(), 1.0);
        assert_eq!(buf.frame_count(), 16000);
    }

    #[test]
    fn zero_sample_rate_rejected() {
        assert!(AudioBuffer::new(vec![0.0; 100], 0, 1).is_err());
    }

    #[test]
    fn zero_channels_rejected() {
        assert!(AudioBuffer::new(vec![0.0; 100], 16000, 0).is_err());
    }

    #[test]
    fn misaligned_samples_rejected() {
        assert!(AudioBuffer::new(vec![0.0; 3], 16000, 2).is_err());
    }

    #[test]
    fn empty_buffer() {
        let buf = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(buf.is_empty());
        assert_eq!(buf.duration_secs(), 0.0);
    }

    #[test]
    fn silence_buffer() {
        let buf = AudioBuffer::silence(0.5, 16000, 1);
        assert_eq!(buf.samples.len(), 8000);
        assert!(buf.samples.iter().all(|&s| s == 0.0));
    }
}
