//! 能耗监控报告
//!
//! 家庭能耗监控 + 语音报告 + 节能建议。

/// 设备能耗记录
#[derive(Debug, Clone)]
pub struct DeviceConsumption {
    pub device_id: String,
    pub device_name: String,
    pub watts: f64,
}

/// 能耗报告器
pub struct EnergyReporter {
    devices: Vec<DeviceConsumption>,
    /// 月度历史（日期标签 -> 总瓦时）
    daily_history: Vec<(String, f64)>,
}

impl EnergyReporter {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            daily_history: Vec::new(),
        }
    }

    /// 更新设备能耗
    pub fn update_device(&mut self, device: DeviceConsumption) {
        if let Some(existing) = self
            .devices
            .iter_mut()
            .find(|d| d.device_id == device.device_id)
        {
            existing.watts = device.watts;
            existing.device_name = device.device_name;
        } else {
            self.devices.push(device);
        }
    }

    /// 记录每日总能耗
    pub fn record_daily(&mut self, date: &str, watt_hours: f64) {
        self.daily_history.push((date.to_string(), watt_hours));
    }

    /// 当前总功耗（瓦特）
    pub fn current_consumption(&self) -> f64 {
        self.devices.iter().map(|d| d.watts).sum()
    }

    /// 生成每日报告
    pub fn daily_report(&self) -> String {
        let total = self.current_consumption();
        let device_count = self.devices.len();

        if device_count == 0 {
            return "暂无能耗数据。".into();
        }

        let top_consumer = self.devices.iter().max_by(|a, b| {
            a.watts
                .partial_cmp(&b.watts)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut report = format!("当前 {} 个设备总功耗 {:.0} 瓦", device_count, total);

        if let Some(top) = top_consumer {
            report.push_str(&format!(
                "，最耗电的是{}（{:.0}瓦）",
                top.device_name, top.watts
            ));
        }

        report.push('。');
        report
    }

    /// 月度趋势
    pub fn monthly_trend(&self) -> &[(String, f64)] {
        &self.daily_history
    }

    /// 节能建议
    pub fn suggest_savings(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let total = self.current_consumption();

        if total > 3000.0 {
            suggestions.push("当前总功耗较高，建议关闭不必要的设备。".into());
        }

        for device in &self.devices {
            if device.watts > 1000.0 {
                suggestions.push(format!(
                    "{}功耗 {:.0} 瓦，建议检查是否需要持续运行。",
                    device.device_name, device.watts
                ));
            }
        }

        if suggestions.is_empty() {
            suggestions.push("当前能耗正常，继续保持。".into());
        }

        suggestions
    }

    /// 监控设备数
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}
