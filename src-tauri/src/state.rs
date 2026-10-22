use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

/// 进程内共享的运行时状态。
#[derive(Default)]
pub struct AppState {
    /// 暂停记录时剪贴板监听器跳过所有新内容。
    paused: AtomicBool,
    /// 热键唤出弹窗前的前台窗口（HWND），粘贴时把焦点还给它；从托盘打开时为空。
    paste_target: Mutex<Option<isize>>,
    /// 永久复制进行中：截止时间之前采集到的下一条内容记为永久。
    permanent_until: Mutex<Option<Instant>>,
    /// 永久复制流程正在执行，忽略重复按下的热键。
    permanent_busy: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn set_paste_target(&self, target: Option<isize>) {
        *lock(&self.paste_target) = target;
    }

    pub fn paste_target(&self) -> Option<isize> {
        *lock(&self.paste_target)
    }

    pub fn arm_permanent(&self, window: Duration) {
        *lock(&self.permanent_until) = Some(Instant::now() + window);
    }

    pub fn disarm_permanent(&self) {
        *lock(&self.permanent_until) = None;
    }

    pub fn permanent_armed(&self) -> bool {
        lock(&self.permanent_until).is_some_and(|t| Instant::now() < t)
    }

    /// 取出并清除永久标记；已超时的标记视为不存在。
    pub fn take_permanent_armed(&self) -> bool {
        lock(&self.permanent_until)
            .take()
            .is_some_and(|t| Instant::now() < t)
    }

    /// 开始永久复制流程；已有流程在执行时返回 false。
    pub fn begin_permanent_copy(&self) -> bool {
        !self.permanent_busy.swap(true, Ordering::AcqRel)
    }

    pub fn end_permanent_copy(&self) {
        self.permanent_busy.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_mark_is_consumed_once() {
        let s = AppState::default();
        assert!(!s.take_permanent_armed());
        s.arm_permanent(Duration::from_secs(5));
        assert!(s.permanent_armed());
        assert!(s.take_permanent_armed());
        assert!(!s.permanent_armed());
        assert!(!s.take_permanent_armed());
    }

    #[test]
    fn expired_mark_is_ignored() {
        let s = AppState::default();
        s.arm_permanent(Duration::ZERO);
        assert!(!s.permanent_armed());
        assert!(!s.take_permanent_armed());
    }

    #[test]
    fn busy_flag_blocks_reentry() {
        let s = AppState::default();
        assert!(s.begin_permanent_copy());
        assert!(!s.begin_permanent_copy());
        s.end_permanent_copy();
        assert!(s.begin_permanent_copy());
    }
}
