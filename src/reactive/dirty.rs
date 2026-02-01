use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe dirty flag for change tracking.
#[derive(Clone, Debug)]
pub struct DirtyFlag {
    dirty: Arc<AtomicBool>,
}

impl DirtyFlag {
    pub fn new() -> Self {
        Self {
            dirty: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    pub fn mark_clean(&self) {
        self.dirty.store(false, Ordering::Release);
    }

    /// Mark clean and return previous dirty state.
    pub fn check_and_clear(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }
}

impl Default for DirtyFlag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_flag_initial_state_is_dirty() {
        let flag = DirtyFlag::new();
        assert!(flag.is_dirty(), "dirty flag should start dirty");
    }

    #[test]
    fn dirty_flag_mark_clean() {
        let flag = DirtyFlag::new();
        flag.mark_clean();
        assert!(
            !flag.is_dirty(),
            "dirty flag should be clean after mark_clean"
        );
    }

    #[test]
    fn dirty_flag_mark_dirty() {
        let flag = DirtyFlag::new();
        flag.mark_clean();
        flag.mark_dirty();
        assert!(
            flag.is_dirty(),
            "dirty flag should be dirty after mark_dirty"
        );
    }

    #[test]
    fn dirty_flag_check_and_clear() {
        let flag = DirtyFlag::new();
        assert!(
            flag.check_and_clear(),
            "check_and_clear should return true and clear"
        );
        assert!(
            !flag.is_dirty(),
            "dirty flag should be clean after check_and_clear"
        );
        assert!(
            !flag.check_and_clear(),
            "check_and_clear should return false when already clean"
        );
    }

    #[test]
    fn dirty_flag_clone_shares_state() {
        let flag1 = DirtyFlag::new();
        let flag2 = flag1.clone();

        flag1.mark_clean();
        assert!(!flag2.is_dirty(), "cloned flag should share state");

        flag2.mark_dirty();
        assert!(flag1.is_dirty(), "original flag should see changes");
    }
}
