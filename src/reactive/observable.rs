use std::sync::Arc;

use parking_lot::RwLock;

use super::dirty::DirtyFlag;

type ChangeCallback<T> = Arc<dyn Fn(&T) + Send + Sync + 'static>;

/// Observable value that notifies subscribers on change.
pub struct Observable<T> {
    value: Arc<RwLock<T>>,
    callbacks: Arc<RwLock<Vec<ChangeCallback<T>>>>,
    dirty_flag: DirtyFlag,
}

impl<T: Clone> Observable<T> {
    pub fn new(initial: T) -> Self {
        Self {
            value: Arc::new(RwLock::new(initial)),
            callbacks: Arc::new(RwLock::new(Vec::new())),
            dirty_flag: DirtyFlag::new(),
        }
    }

    pub fn get(&self) -> T {
        self.value.read().clone()
    }

    pub fn set(&self, new_value: T) {
        *self.value.write() = new_value.clone();
        self.dirty_flag.mark_dirty();

        // Avoid holding the callbacks lock while calling user code.
        let callbacks = self.callbacks.read().clone();
        for cb in callbacks {
            cb(&new_value);
        }
    }

    pub fn subscribe<F>(&self, callback: F)
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.callbacks.write().push(Arc::new(callback));
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_flag.is_dirty()
    }

    pub fn mark_clean(&self) {
        self.dirty_flag.mark_clean();
    }
}

impl<T> Clone for Observable<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            callbacks: self.callbacks.clone(),
            dirty_flag: self.dirty_flag.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn observable_notifies_subscribers() {
        let obs = Observable::new(0);
        let called = Arc::new(AtomicUsize::new(0));

        let called_clone = called.clone();
        obs.subscribe(move |value| {
            assert_eq!(*value, 42);
            called_clone.fetch_add(1, Ordering::Relaxed);
        });

        obs.set(42);
        assert_eq!(
            called.load(Ordering::Relaxed),
            1,
            "callback should be called"
        );
    }

    #[test]
    fn observable_multiple_subscribers() {
        let obs = Observable::new(0);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let c = counter.clone();
            obs.subscribe(move |_| {
                c.fetch_add(1, Ordering::Relaxed);
            });
        }

        obs.set(42);
        assert_eq!(
            counter.load(Ordering::Relaxed),
            3,
            "all subscribers should be notified"
        );
    }
}
