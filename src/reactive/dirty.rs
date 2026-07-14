use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe dirty flag for change tracking.
#[derive(Clone, Debug)]
pub struct DirtyFlag {
    version: Arc<AtomicU64>,
    cleaned_version: Arc<AtomicU64>,
}

/// A per-consumer dirty signal bound to one [`DirtyFlag`].
#[derive(Clone, Debug)]
pub struct DirtySignal {
    flag: DirtyFlag,
    observer: DirtyObserver,
}

/// A pull-based collection of dirty signals.
#[derive(Clone, Debug, Default)]
pub struct DirtySignalSet {
    signals: Vec<DirtySignal>,
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

    /// Create a per-consumer signal initialized to the current version.
    pub fn signal(&self) -> DirtySignal {
        DirtySignal::new(self.clone())
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

    fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.version, &other.version)
    }
}

impl Default for DirtyFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtySignal {
    pub fn new(flag: DirtyFlag) -> Self {
        let observer = flag.observer();
        Self { flag, observer }
    }

    pub fn changed_since_last_poll(&mut self) -> bool {
        self.flag.check(&mut self.observer)
    }

    fn observes(&self, flag: &DirtyFlag) -> bool {
        self.flag.ptr_eq(flag)
    }
}

impl DirtySignalSet {
    pub fn new(signals: Vec<DirtySignal>) -> Self {
        let mut set = Self::default();
        set.refresh(signals);
        set
    }

    pub fn from_flags(flags: impl IntoIterator<Item = DirtyFlag>) -> Self {
        Self::new(flags.into_iter().map(DirtySignal::new).collect())
    }

    pub fn len(&self) -> usize {
        self.signals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    pub fn changed_since_last_poll(&mut self) -> bool {
        let mut changed = false;
        for signal in &mut self.signals {
            changed |= signal.changed_since_last_poll();
        }
        changed
    }

    /// Refresh the tracked flags while preserving observers for existing sources.
    pub fn refresh_from_flags(&mut self, flags: impl IntoIterator<Item = DirtyFlag>) {
        self.refresh(flags.into_iter().map(DirtySignal::new));
    }

    /// Refresh the tracked signals while preserving observers for existing sources.
    pub fn refresh(&mut self, signals: impl IntoIterator<Item = DirtySignal>) {
        let mut refreshed = Vec::new();
        for signal in signals {
            if let Some(existing) = self
                .signals
                .iter()
                .find(|existing| existing.observes(&signal.flag))
            {
                refreshed.push(existing.clone());
            } else if !refreshed
                .iter()
                .any(|existing: &DirtySignal| existing.observes(&signal.flag))
            {
                refreshed.push(signal);
            } else {
                continue;
            }
        }
        self.signals = refreshed;
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

    #[test]
    fn dirty_flag_multiple_observers_advance_independently() {
        let flag = DirtyFlag::new();
        let mut first = flag.observer();
        let mut second = flag.observer();

        flag.mark_dirty();
        assert!(flag.check(&mut first));
        assert!(!flag.check(&mut first));
        assert!(flag.check(&mut second));
        assert!(!flag.check(&mut second));

        flag.mark_dirty();
        assert!(flag.check(&mut second));
        assert!(flag.check(&mut first));
    }
}
