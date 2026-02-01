use std::sync::Arc;

use parking_lot::RwLock;

use super::dirty::DirtyFlag;

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

    pub fn is_dirty(&self) -> bool {
        self.dirty_flag.is_dirty()
    }

    pub fn mark_clean(&self) {
        self.dirty_flag.mark_clean();
    }

    pub fn binding(&self) -> PropertyBinding<T> {
        PropertyBinding {
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
pub struct PropertyBinding<T> {
    value: Arc<RwLock<T>>,
    dirty_flag: DirtyFlag,
}

impl<T: Clone + PartialEq> PropertyBinding<T> {
    pub fn get(&self) -> T {
        self.value.read().clone()
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
}

impl<T> Clone for PropertyBinding<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            dirty_flag: self.dirty_flag.clone(),
        }
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
}
