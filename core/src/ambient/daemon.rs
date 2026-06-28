//! 环境守护进程
//!
//! `AmbientDaemon` — 包装 `DaemonProcess`，拥有主循环，24h 常驻后台。

use std::sync::Arc;

use crate::daemon::{DaemonProcess, TaskScheduler};
use crate::iot::EventBus;

use super::config::AmbientConfig;
use super::loop_runner::AmbientLoopRunner;

/// 环境守护进程
pub struct AmbientDaemon {
    process: DaemonProcess,
    loop_runner: AmbientLoopRunner,
    config: AmbientConfig,
    event_bus: Arc<EventBus>,
    scheduler: TaskScheduler,
    /// 主循环最大迭代次数（0 = 无限）
    max_iterations: u64,
}

impl AmbientDaemon {
    pub fn new(
        process: DaemonProcess,
        loop_runner: AmbientLoopRunner,
        config: AmbientConfig,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            process,
            loop_runner,
            config,
            event_bus,
            scheduler: TaskScheduler::new(),
            max_iterations: 0,
        }
    }

    /// 设置最大迭代次数（用于测试）
    pub fn with_max_iterations(mut self, max: u64) -> Self {
        self.max_iterations = max;
        self
    }

    /// 获取守护进程状态
    pub fn is_running(&self) -> bool {
        self.process.is_running()
    }

    /// 获取配置
    pub fn config(&self) -> &AmbientConfig {
        &self.config
    }

    /// 获取事件总线
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// 获取调度器的可变引用
    pub fn scheduler_mut(&mut self) -> &mut TaskScheduler {
        &mut self.scheduler
    }

    /// 获取主循环运行器的可变引用
    pub fn loop_runner_mut(&mut self) -> &mut AmbientLoopRunner {
        &mut self.loop_runner
    }

    /// 启动守护进程和主循环
    pub async fn start(&mut self) -> anyhow::Result<()> {
        self.process.start().await?;
        self.scheduler.start().await?;

        tracing::info!(
            device_id = %self.config.device_id,
            room = %self.config.default_room,
            wakeword = %self.config.wakeword,
            "环境守护进程已启动"
        );

        self.run_loop().await
    }

    /// 停止守护进程
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.loop_runner.reset();
        self.scheduler.stop();
        self.process.stop().await?;

        tracing::info!("环境守护进程已停止");
        Ok(())
    }

    /// 主循环
    async fn run_loop(&mut self) -> anyhow::Result<()> {
        let mut iteration: u64 = 0;

        loop {
            if !self.process.is_running() {
                break;
            }

            if self.max_iterations > 0 && iteration >= self.max_iterations {
                break;
            }

            match self.loop_runner.tick().await {
                Ok(Some(result)) => {
                    tracing::debug!(
                        target = ?result.target,
                        response_len = result.response.len(),
                        "分发完成"
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "主循环迭代出错");
                }
            }

            iteration += 1;
        }

        Ok(())
    }
}
