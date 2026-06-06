use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use parking_lot::Mutex;

use crate::reactive::Property;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub usize);

#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct TaskMetadata {
    pub id: TaskId,
    pub name: String,
    pub started_at: Instant,
}

#[derive(Clone, Debug)]
pub struct TaskHandle {
    metadata: TaskMetadata,
    token: CancellationToken,
}

impl TaskHandle {
    pub fn id(&self) -> TaskId {
        self.metadata.id
    }

    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    pub fn started_at(&self) -> Instant {
        self.metadata.started_at
    }

    pub fn metadata(&self) -> &TaskMetadata {
        &self.metadata
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[derive(Debug)]
struct TaskRegistryInner {
    next_id: usize,
    tasks: Vec<TaskHandle>,
}

#[derive(Clone, Debug)]
pub struct TaskRegistry {
    inner: Arc<Mutex<TaskRegistryInner>>,
    running: Property<bool>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TaskRegistryInner {
                next_id: 1,
                tasks: Vec::new(),
            })),
            running: Property::new(false),
        }
    }

    pub fn register(&self, name: impl Into<String>) -> TaskHandle {
        let handle = {
            let mut inner = self.inner.lock();
            let id = TaskId(inner.next_id);
            inner.next_id += 1;
            let handle = TaskHandle {
                metadata: TaskMetadata {
                    id,
                    name: name.into(),
                    started_at: Instant::now(),
                },
                token: CancellationToken::new(),
            };
            inner.tasks.push(handle.clone());
            handle
        };
        self.running.set(true);
        handle
    }

    pub fn spawn<F>(&self, name: impl Into<String>, run: F) -> (TaskHandle, thread::JoinHandle<()>)
    where
        F: FnOnce(CancellationToken) + Send + 'static,
    {
        let handle = self.register(name);
        let token = handle.token();
        let guard = RegisteredTaskGuard {
            registry: self.clone(),
            id: handle.id(),
        };
        let join = thread::spawn(move || {
            let _guard = guard;
            run(token);
        });
        (handle, join)
    }

    pub fn unregister(&self, id: TaskId) -> bool {
        let is_running = {
            let mut inner = self.inner.lock();
            let Some(pos) = inner.tasks.iter().position(|task| task.id() == id) else {
                return false;
            };
            inner.tasks.remove(pos);
            !inner.tasks.is_empty()
        };
        self.running.set(is_running);
        true
    }

    pub fn handles(&self) -> Vec<TaskHandle> {
        self.inner.lock().tasks.clone()
    }

    pub fn current(&self) -> Option<TaskHandle> {
        self.inner.lock().tasks.last().cloned()
    }

    pub fn cancel_current(&self) -> bool {
        let Some(handle) = self.current() else {
            return false;
        };
        handle.cancel();
        true
    }

    pub fn cancel_all(&self) -> usize {
        let handles = self.handles();
        for handle in &handles {
            handle.cancel();
        }
        handles.len()
    }

    pub fn is_running(&self) -> bool {
        !self.inner.lock().tasks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn running_property(&self) -> Property<bool> {
        self.running.clone()
    }
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

struct RegisteredTaskGuard {
    registry: TaskRegistry,
    id: TaskId,
}

impl Drop for RegisteredTaskGuard {
    fn drop(&mut self) {
        self.registry.unregister(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn cancellation_token_cancel_is_shared_by_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(!token.is_cancelled());
        clone.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[test]
    fn registry_tracks_running_property_and_handles() {
        let registry = TaskRegistry::new();
        let running = registry.running_property();

        assert!(!running.get());

        let first = registry.register("first");
        let second = registry.register("second");

        assert!(running.get());
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.current().expect("current task").id(), second.id());
        assert_eq!(registry.handles()[0].name(), "first");

        assert!(registry.unregister(first.id()));
        assert!(running.get());
        assert!(registry.unregister(second.id()));
        assert!(!running.get());
    }

    #[test]
    fn cancel_current_cancels_latest_registered_task() {
        let registry = TaskRegistry::new();
        let first = registry.register("first");
        let second = registry.register("second");

        assert!(registry.cancel_current());

        assert!(!first.is_cancelled());
        assert!(second.is_cancelled());
    }

    #[test]
    fn spawn_unregisters_when_thread_finishes_after_cancellation() {
        let registry = TaskRegistry::new();
        let running = registry.running_property();
        let (started_tx, started_rx) = mpsc::channel();

        let (handle, join) = registry.spawn("worker", move |token| {
            started_tx.send(()).expect("send started");
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
        });

        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker started");
        assert!(running.get());

        handle.cancel();
        join.join().expect("worker exits cleanly");

        assert!(!running.get());
        assert!(registry.is_empty());
    }
}
