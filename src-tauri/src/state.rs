use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

/// 进程内共享的运行时状态。
#[derive(Default)]
pub struct AppState {
    /// 暂停记录时剪贴板监听器跳过所有新内容。
    paused: AtomicBool,
    /// 热键唤出弹窗前的前台窗口（HWND），粘贴时把焦点还给它；从托盘打开时为空。
    paste_target: Mutex<Option<isize>>,
}

impl AppState {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn set_paste_target(&self, target: Option<isize>) {
        *self.paste_target.lock().unwrap_or_else(|e| e.into_inner()) = target;
    }

    pub fn paste_target(&self) -> Option<isize> {
        *self.paste_target.lock().unwrap_or_else(|e| e.into_inner())
    }
}
