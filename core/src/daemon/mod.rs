//! 后台守护模块
//!
//! 管理后台守护进程和定时任务调度。

pub mod process;
pub mod scheduler;

pub use process::DaemonProcess;
pub use scheduler::TaskScheduler;
