use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
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
    /// 每次打开弹窗加一；空闲销毁任务据此判断期间是否又打开过。
    popup_generation: AtomicU64,
    /// 每次弹窗移动加一，用于判断拖动是否已结束。
    move_generation: AtomicU64,
    /// 弹窗正在创建（WebView2 初始化期间主线程会嵌套处理消息，热键、托盘事件可能重入）。
    popup_creating: AtomicBool,
    /// 弹窗页面加载完成后需要显示。
    popup_show_pending: AtomicBool,
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

    pub fn bump_popup_generation(&self) {
        self.popup_generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn popup_generation(&self) -> u64 {
        self.popup_generation.load(Ordering::Acquire)
    }

    pub fn bump_move_generation(&self) -> u64 {
        self.move_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn move_generation(&self) -> u64 {
        self.move_generation.load(Ordering::Acquire)
    }

    /// 开始创建弹窗；已有创建在进行时返回 false。
    pub fn begin_popup_creation(&self) -> bool {
        !self.popup_creating.swap(true, Ordering::AcqRel)
    }

    pub fn end_popup_creation(&self) {
        self.popup_creating.store(false, Ordering::Release);
    }

    pub fn request_popup_show(&self) {
        self.popup_show_pending.store(true, Ordering::Release);
    }

    /// 取出并清除“加载完成后显示”的请求。
    pub fn take_popup_show(&self) -> bool {
        self.popup_show_pending.swap(false, Ordering::AcqRel)
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
    fn popup_creation_is_exclusive_and_show_request_consumed_once() {
        let s = AppState::default();
        assert!(s.begin_popup_creation());
        assert!(!s.begin_popup_creation());
        s.end_popup_creation();
        assert!(s.begin_popup_creation());

        assert!(!s.take_popup_show());
        s.request_popup_show();
        s.request_popup_show();
        assert!(s.take_popup_show());
        assert!(!s.take_popup_show());
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
