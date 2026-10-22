use std::sync::atomic::{AtomicBool, Ordering};

/// 进程内共享的运行时状态。
#[derive(Default)]
pub struct AppState {
    /// 暂停记录时剪贴板监听器跳过所有新内容。
    paused: AtomicBool,
}

impl AppState {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }
}
