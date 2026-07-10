//! Tokio runtime and task registration helpers.

use std::future::Future;
use std::io;
use std::sync::mpsc;

use atto_ui::task::{CancellationToken, TaskHandle, TaskId, TaskRegistry};
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;

/// Builds a current-thread tokio runtime with I/O and timers enabled.
pub fn build_current_thread_runtime() -> io::Result<Runtime> {
    Builder::new_current_thread().enable_all().build()
}

/// Builds a multi-thread tokio runtime with I/O and timers enabled.
pub fn build_multi_thread_runtime(worker_threads: usize) -> io::Result<Runtime> {
    let mut builder = Builder::new_multi_thread();
    builder.enable_all();
    if worker_threads > 0 {
        builder.worker_threads(worker_threads);
    }
    builder.build()
}

/// Spawns an async task registered with `TaskRegistry` and connected to the core action channel.
pub fn spawn_async<A, N, F, Fut>(
    registry: &TaskRegistry,
    name: N,
    action_sender: mpsc::Sender<A>,
    run: F,
) -> (TaskHandle, JoinHandle<()>)
where
    A: Send + 'static,
    N: Into<String>,
    F: FnOnce(CancellationToken, mpsc::Sender<A>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let handle = registry.register(name);
    let token = handle.token();
    let guard = TaskRegistrationGuard {
        registry: registry.clone(),
        id: handle.id(),
    };
    let join = tokio::spawn(async move {
        let _guard = guard;
        run(token, action_sender).await;
    });

    (handle, join)
}

/// Runs blocking work on tokio's blocking pool while preserving task cancellation metadata.
pub fn spawn_blocking<A, N, F>(
    registry: &TaskRegistry,
    name: N,
    action_sender: mpsc::Sender<A>,
    run: F,
) -> (TaskHandle, JoinHandle<()>)
where
    A: Send + 'static,
    N: Into<String>,
    F: FnOnce(CancellationToken, mpsc::Sender<A>) + Send + 'static,
{
    let handle = registry.register(name);
    let token = handle.token();
    let guard = TaskRegistrationGuard {
        registry: registry.clone(),
        id: handle.id(),
    };
    let join = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        run(token, action_sender);
    });

    (handle, join)
}

struct TaskRegistrationGuard {
    registry: TaskRegistry,
    id: TaskId,
}

impl Drop for TaskRegistrationGuard {
    fn drop(&mut self) {
        self.registry.unregister(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spawn_async_sends_action_and_unregisters() {
        let runtime = build_current_thread_runtime().expect("runtime");
        runtime.block_on(async {
            let registry = TaskRegistry::new();
            let (tx, rx) = mpsc::channel();

            let (_handle, join) =
                spawn_async(&registry, "async", tx, |_token, actions| async move {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    actions.send("done").ok();
                });

            join.await.expect("task joined");
            assert_eq!(rx.try_recv().expect("action"), "done");
            assert!(registry.is_empty());
        });
    }

    #[test]
    fn spawn_blocking_observes_cancellation_and_unregisters() {
        let runtime = build_current_thread_runtime().expect("runtime");
        runtime.block_on(async {
            let registry = TaskRegistry::new();
            let (tx, rx) = mpsc::channel();

            let (handle, join) = spawn_blocking(&registry, "blocking", tx, |token, actions| {
                token.cancel();
                actions.send(token.is_cancelled()).ok();
            });

            join.await.expect("task joined");
            assert!(rx.try_recv().expect("action"));
            assert!(handle.is_cancelled());
            assert!(registry.is_empty());
        });
    }
}
