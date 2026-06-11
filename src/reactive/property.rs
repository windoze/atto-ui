use std::sync::Arc;

use parking_lot::RwLock;

use super::dirty::{DirtyFlag, DirtyObserver};

/// A reactive property that tracks changes via a shared [`DirtyFlag`].
#[derive(Debug)]
pub struct Property<T> {
    value: Arc<RwLock<T>>,
    dirty_flag: DirtyFlag,
}

impl<T: Clone + PartialEq> Property<T> {
    pub fn new(initial: T) -> Self {
        Self {
            value: Arc::new(RwLock::new(initial)),
            dirty_flag: DirtyFlag::new(),
        }
    }

    pub fn get(&self) -> T {
        self.value.read().clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let guard = self.value.read();
        f(&*guard)
    }

    pub fn set(&self, new_value: T) {
        let mut guard = self.value.write();
        if *guard != new_value {
            *guard = new_value;
            drop(guard);
            self.dirty_flag.mark_dirty();
        }
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        {
            let mut guard = self.value.write();
            f(&mut *guard);
        }
        self.dirty_flag.mark_dirty();
    }

    pub fn update_if<F>(&self, f: F) -> bool
    where
        F: FnOnce(&mut T) -> bool,
    {
        let changed = {
            let mut guard = self.value.write();
            f(&mut *guard)
        };
        if changed {
            self.dirty_flag.mark_dirty();
        }
        changed
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_flag.is_dirty()
    }

    pub fn mark_clean(&self) {
        self.dirty_flag.mark_clean();
    }

    /// Creates a per-consumer dirty observer initialized to the current version.
    pub fn dirty_observer(&self) -> DirtyObserver {
        self.dirty_flag.observer()
    }

    /// Returns `true` if the value changed since the observer last checked.
    pub fn check_dirty(&self, observer: &mut DirtyObserver) -> bool {
        self.dirty_flag.check(observer)
    }

    pub fn binding(&self) -> Binding<T> {
        Binding {
            value: self.value.clone(),
            dirty_flag: self.dirty_flag.clone(),
        }
    }
}

impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            dirty_flag: self.dirty_flag.clone(),
        }
    }
}

/// A two-way binding to a [`Property`].
#[derive(Debug)]
pub struct Binding<T> {
    value: Arc<RwLock<T>>,
    dirty_flag: DirtyFlag,
}

impl<T: Clone + PartialEq> Binding<T> {
    pub fn new(initial: T) -> Self {
        Property::new(initial).binding()
    }

    pub fn get(&self) -> T {
        self.value.read().clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let guard = self.value.read();
        f(&*guard)
    }

    pub fn set(&self, new_value: T) {
        let mut guard = self.value.write();
        if *guard != new_value {
            *guard = new_value;
            drop(guard);
            self.dirty_flag.mark_dirty();
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_flag.is_dirty()
    }

    pub fn mark_clean(&self) {
        self.dirty_flag.mark_clean();
    }

    /// Creates a per-consumer dirty observer initialized to the current version.
    pub fn dirty_observer(&self) -> DirtyObserver {
        self.dirty_flag.observer()
    }

    /// Returns `true` if the value changed since the observer last checked.
    pub fn check_dirty(&self, observer: &mut DirtyObserver) -> bool {
        self.dirty_flag.check(observer)
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        {
            let mut guard = self.value.write();
            f(&mut *guard);
        }
        self.dirty_flag.mark_dirty();
    }

    pub fn update_if<F>(&self, f: F) -> bool
    where
        F: FnOnce(&mut T) -> bool,
    {
        let changed = {
            let mut guard = self.value.write();
            f(&mut *guard)
        };
        if changed {
            self.dirty_flag.mark_dirty();
        }
        changed
    }
}

impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            dirty_flag: self.dirty_flag.clone(),
        }
    }
}

impl<T: Clone + PartialEq> From<T> for Binding<T> {
    fn from(value: T) -> Self {
        Binding::new(value)
    }
}

impl From<&str> for Binding<String> {
    fn from(value: &str) -> Self {
        Binding::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_get_set() {
        let prop = Property::new(42);
        assert_eq!(prop.get(), 42);

        prop.set(100);
        assert_eq!(prop.get(), 100);
    }

    #[test]
    fn property_dirty_on_change() {
        let prop = Property::new(42);
        prop.mark_clean();

        prop.set(100);
        assert!(prop.is_dirty(), "should be dirty after set");
    }

    #[test]
    fn property_not_dirty_if_same_value() {
        let prop = Property::new(42);
        prop.mark_clean();

        prop.set(42);
        assert!(!prop.is_dirty(), "should not be dirty if value unchanged");
    }

    #[test]
    fn property_update_marks_dirty() {
        let prop = Property::new(vec![1, 2, 3]);
        prop.mark_clean();

        prop.update(|v| v.push(4));

        assert_eq!(prop.get(), vec![1, 2, 3, 4]);
        assert!(prop.is_dirty(), "should be dirty after update");
    }

    #[test]
    fn property_update_if_marks_dirty_only_when_changed() {
        let prop = Property::new(vec![1, 2]);
        prop.mark_clean();

        let changed = prop.update_if(|_| false);

        assert!(!changed);
        assert!(!prop.is_dirty(), "unchanged update should stay clean");

        let changed = prop.update_if(|v| {
            v.push(3);
            true
        });

        assert!(changed);
        assert_eq!(prop.get(), vec![1, 2, 3]);
        assert!(prop.is_dirty(), "changed update should mark dirty");
    }

    #[test]
    fn property_binding_shares_state() {
        let prop = Property::new("hello".to_string());
        let binding = prop.binding();

        binding.set("world".to_string());

        assert_eq!(prop.get(), "world");
        assert!(prop.is_dirty(), "original property should be dirty");
    }

    #[test]
    fn property_clone_shares_value_and_dirty_flag() {
        let prop1 = Property::new(42);
        let prop2 = prop1.clone();

        prop1.set(100);

        assert_eq!(prop2.get(), 100, "clone should see changes");
        assert!(prop2.is_dirty(), "clone should share dirty state");
    }

    #[test]
    fn binding_update_mutates_value() {
        let prop = Property::new(vec![1, 2]);
        let binding = prop.binding();

        binding.update(|v| v.push(3));

        assert_eq!(prop.get(), vec![1, 2, 3]);
    }

    #[test]
    fn binding_update_if_marks_dirty_only_when_changed() {
        let binding = Binding::new(vec![1, 2]);
        binding.mark_clean();

        let changed = binding.update_if(|_| false);

        assert!(!changed);
        assert!(!binding.is_dirty(), "unchanged update should stay clean");

        let changed = binding.update_if(|v| {
            v.push(3);
            true
        });

        assert!(changed);
        assert_eq!(binding.get(), vec![1, 2, 3]);
        assert!(binding.is_dirty(), "changed update should mark dirty");
    }

    #[test]
    fn property_dirty_observer_sees_binding_updates() {
        let prop = Property::new(0usize);
        let binding = prop.binding();
        let mut observer = prop.dirty_observer();

        assert!(!prop.check_dirty(&mut observer));
        binding.set(1);
        assert!(prop.check_dirty(&mut observer));
        assert!(!prop.check_dirty(&mut observer));
    }

    #[test]
    fn binding_dirty_observer_sees_property_updates() {
        let prop = Property::new("before".to_string());
        let binding = prop.binding();
        let mut observer = binding.dirty_observer();

        assert!(!binding.check_dirty(&mut observer));
        prop.set("after".to_string());
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(binding.get(), "after");
    }
}
