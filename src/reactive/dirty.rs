use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe dirty flag for change tracking.
#[derive(Clone, Debug)]
pub struct DirtyFlag {
    version: Arc<AtomicU64>,
    cleaned_version: Arc<AtomicU64>,
}

/// Per-consumer observer for [`DirtyFlag`].
///
/// Unlike [`DirtyFlag::mark_clean`], observers do not clear shared state. This allows multiple
/// views to independently detect changes without interfering with each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirtyObserver {
    last_seen_version: u64,
}

impl DirtyFlag {
    pub fn new() -> Self {
        Self {
            // Start dirty: version != cleaned_version.
            version: Arc::new(AtomicU64::new(1)),
            cleaned_version: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn mark_dirty(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn is_dirty(&self) -> bool {
        self.version.load(Ordering::Acquire) != self.cleaned_version.load(Ordering::Acquire)
    }

    pub fn mark_clean(&self) {
        let version = self.version.load(Ordering::Acquire);
        self.cleaned_version.store(version, Ordering::Release);
    }

    /// Mark clean and return previous dirty state.
    pub fn check_and_clear(&self) -> bool {
        let version = self.version.load(Ordering::Acquire);
        let prev_cleaned = self.cleaned_version.swap(version, Ordering::AcqRel);
        prev_cleaned != version
    }

    /// Create a new [`DirtyObserver`] initialized to the current version.
    pub fn observer(&self) -> DirtyObserver {
        DirtyObserver {
            last_seen_version: self.version.load(Ordering::Acquire),
        }
    }

    /// Returns `true` if the flag changed since the observer last checked, updating the observer.
    pub fn check(&self, observer: &mut DirtyObserver) -> bool {
        let version = self.version.load(Ordering::Acquire);
        if version == observer.last_seen_version {
            return false;
        }
        observer.last_seen_version = version;
        true
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

    #[test]
    fn dirty_observer_does_not_clear_global_dirty_state() {
        let flag = DirtyFlag::new();
        flag.mark_clean();
        let mut observer = flag.observer();

        assert!(
            !flag.check(&mut observer),
            "observer should start at current version"
        );
        flag.mark_dirty();
        assert!(flag.check(&mut observer), "observer should see new version");
        assert!(
            flag.is_dirty(),
            "observer checks should not clear global dirty state"
        );
    }
}
