use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::wm::WindowId;

/// Tracks which windows are dirty and schedules renders.
pub struct RenderScheduler {
    window_dirty: HashMap<WindowId, Arc<AtomicBool>>,
    global_dirty: Arc<AtomicBool>,
    last_render: Instant,
    min_frame_interval: Duration,
    force_render: bool,
}

impl RenderScheduler {
    pub fn new() -> Self {
        let min_frame_interval = Duration::from_millis(16);
        let last_render = Instant::now()
            .checked_sub(min_frame_interval)
            .unwrap_or_else(Instant::now);

        Self {
            window_dirty: HashMap::new(),
            global_dirty: Arc::new(AtomicBool::new(true)),
            last_render,
            min_frame_interval,
            force_render: false,
        }
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.min_frame_interval = Duration::from_millis(1000 / fps.max(1) as u64);
    }

    pub fn mark_dirty(&mut self, window_id: WindowId) {
        self.window_dirty
            .entry(window_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Release);
        self.global_dirty.store(true, Ordering::Release);
    }

    pub fn mark_all_dirty(&mut self) {
        for flag in self.window_dirty.values() {
            flag.store(true, Ordering::Release);
        }
        self.global_dirty.store(true, Ordering::Release);
    }

    pub fn force_render(&mut self) {
        self.force_render = true;
        self.global_dirty.store(true, Ordering::Release);
    }

    pub fn is_any_dirty(&self) -> bool {
        self.global_dirty.load(Ordering::Acquire)
    }

    pub fn is_window_dirty(&self, window_id: WindowId) -> bool {
        self.window_dirty
            .get(&window_id)
            .map(|f| f.load(Ordering::Acquire))
            .unwrap_or(false)
    }

    pub fn should_render(&self) -> bool {
        if self.force_render {
            return true;
        }
        if !self.is_any_dirty() {
            return false;
        }
        self.last_render.elapsed() >= self.min_frame_interval
    }

    pub fn mark_rendered(&mut self) {
        self.last_render = Instant::now();
        self.force_render = false;

        for flag in self.window_dirty.values() {
            flag.store(false, Ordering::Release);
        }
        self.global_dirty.store(false, Ordering::Release);
    }

    pub fn time_until_next_frame(&self) -> Duration {
        let elapsed = self.last_render.elapsed();
        if elapsed >= self.min_frame_interval {
            Duration::ZERO
        } else {
            self.min_frame_interval - elapsed
        }
    }

    pub fn poll_timeout(&self) -> Duration {
        if self.is_any_dirty() {
            self.time_until_next_frame()
        } else {
            Duration::from_millis(100)
        }
    }

    pub fn register_window(&mut self, window_id: WindowId) {
        self.window_dirty
            .insert(window_id, Arc::new(AtomicBool::new(true)));
        self.global_dirty.store(true, Ordering::Release);
    }

    pub fn unregister_window(&mut self, window_id: WindowId) {
        self.window_dirty.remove(&window_id);
    }

    pub fn global_dirty_flag(&self) -> Arc<AtomicBool> {
        self.global_dirty.clone()
    }
}

impl Default for RenderScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn scheduler_initial_dirty() {
        let scheduler = RenderScheduler::new();
        assert!(scheduler.is_any_dirty(), "should start dirty");
        assert!(scheduler.should_render(), "should allow initial render");
    }

    #[test]
    fn scheduler_mark_dirty() {
        let mut scheduler = RenderScheduler::new();
        scheduler.mark_rendered();
        assert!(!scheduler.is_any_dirty(), "should be clean after render");

        let wid = WindowId(1);
        scheduler.register_window(wid);
        scheduler.mark_dirty(wid);

        assert!(scheduler.is_any_dirty(), "should be dirty after mark");
        assert!(scheduler.is_window_dirty(wid), "window should be dirty");
    }

    #[test]
    fn scheduler_fps_cap() {
        let mut scheduler = RenderScheduler::new();
        scheduler.set_target_fps(60);
        scheduler.mark_rendered();
        scheduler.mark_all_dirty();

        assert!(!scheduler.should_render(), "should respect FPS cap");
        sleep(Duration::from_millis(17));
        assert!(scheduler.should_render(), "should render after interval");
    }

    #[test]
    fn scheduler_force_render() {
        let mut scheduler = RenderScheduler::new();
        scheduler.mark_rendered();

        scheduler.force_render();
        assert!(scheduler.should_render(), "force render should work");
    }

    #[test]
    fn scheduler_poll_timeout() {
        let mut scheduler = RenderScheduler::new();
        scheduler.mark_rendered();

        let timeout = scheduler.poll_timeout();
        assert_eq!(timeout, Duration::from_millis(100));

        scheduler.mark_all_dirty();
        let timeout = scheduler.poll_timeout();
        assert!(
            timeout < Duration::from_millis(100),
            "active timeout should be shorter"
        );
    }
}
