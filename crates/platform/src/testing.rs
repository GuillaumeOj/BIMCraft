//! Deterministic, in-memory implementations of the platform traits for tests.
//!
//! Enabled by the `testing` feature. Core crates add `platform` with `features = ["testing"]` to
//! their `[dev-dependencies]`, so their tests need no files, threads, wall clock or solver
//! process. Everything here is single-threaded and runs only when the test drives it.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt;
use std::mem;
use std::pin::pin;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::{
    BoxFuture, Clock, ComputeBackend, ComputeError, Storage, StorageError, TaskExecutor, Timestamp,
};

/// Polls `future` once and returns its output if it is already complete.
///
/// The doubles in this module complete their futures immediately, so synchronous tests can call
/// async trait methods without an async runtime: `now_or_never(storage.read("a"))`.
pub fn now_or_never<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}

/// Locks `mutex`, carrying on if a panicking test poisoned it: the doubles hold no invariant a
/// panic can break.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A [`Clock`] that only moves when the test moves it.
#[derive(Debug)]
pub struct ManualClock {
    now: Cell<Timestamp>,
}

impl ManualClock {
    /// Creates a clock stopped at `start`.
    #[must_use]
    pub const fn new(start: Timestamp) -> Self {
        Self {
            now: Cell::new(start),
        }
    }

    /// Sets the current time, forwards or backwards.
    pub fn set(&self, now: Timestamp) {
        self.now.set(now);
    }

    /// Moves the clock forwards by `millis`, stopping at the largest representable time.
    pub fn advance(&self, millis: u64) {
        self.now.set(self.now.get().saturating_add_millis(millis));
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        self.now.get()
    }
}

/// A [`Storage`] that keeps values in memory.
///
/// Like a real host, an operation takes effect only when its future is polled, so a forgotten
/// `.await` fails the test; once polled, it completes immediately.
#[derive(Debug, Default)]
pub struct MemoryStorage {
    values: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryStorage {
    /// Creates an empty storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the keys that hold a value, in ascending order.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        lock(&self.values).keys().cloned().collect()
    }
}

impl Storage for MemoryStorage {
    fn read<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Vec<u8>, StorageError>> {
        Box::pin(async move {
            let value = lock(&self.values).get(key).cloned();
            value.ok_or(StorageError::NotFound)
        })
    }

    fn write<'a>(
        &'a self,
        key: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), StorageError>> {
        Box::pin(async move {
            lock(&self.values).insert(key.to_owned(), bytes.to_vec());
            Ok(())
        })
    }

    fn remove<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StorageError>> {
        Box::pin(async move {
            let removed = lock(&self.values).remove(key);
            removed.map(drop).ok_or(StorageError::NotFound)
        })
    }
}

/// A [`TaskExecutor`] that runs tasks on the calling thread.
///
/// [`spawn`](TaskExecutor::spawn) runs a task until it completes or first waits. Tasks still
/// waiting are queued and only run again when the test calls
/// [`run_until_stalled`](Self::run_until_stalled), so the order of events is fully deterministic.
#[derive(Default)]
pub struct InlineExecutor {
    waiting: Mutex<Vec<BoxFuture<'static, ()>>>,
}

impl InlineExecutor {
    /// Creates an executor with no waiting tasks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how many spawned tasks have not finished yet.
    #[must_use]
    pub fn pending(&self) -> usize {
        lock(&self.waiting).len()
    }

    /// Polls waiting tasks again and again until a whole pass finishes none of them.
    pub fn run_until_stalled(&self) {
        loop {
            // Taken out of the mutex so tasks can call `spawn` while they are polled.
            let mut tasks = mem::take(&mut *lock(&self.waiting));
            let polled = tasks.len();
            tasks.retain_mut(|task| now_or_never(task.as_mut()).is_none());
            let finished = tasks.len() < polled;
            // Older tasks stay ahead of the ones spawned during this pass.
            lock(&self.waiting).splice(0..0, tasks);
            // Stopping on "nothing finished" rather than "nothing spawned" also ends the loop
            // when a waiting task spawns another waiting task every time it is polled.
            if !finished {
                return;
            }
        }
    }
}

impl fmt::Debug for InlineExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InlineExecutor")
            .field("pending", &self.pending())
            .finish()
    }
}

impl TaskExecutor for InlineExecutor {
    fn spawn(&self, mut task: BoxFuture<'static, ()>) {
        if now_or_never(task.as_mut()).is_none() {
            lock(&self.waiting).push(task);
        }
    }
}

/// A solver registered on a [`FakeComputeBackend`].
type Solver = Box<dyn Fn(Vec<u8>) -> Result<Vec<u8>, ComputeError> + Send + Sync>;

/// A [`ComputeBackend`] whose solvers are closures registered by the test.
#[derive(Default)]
pub struct FakeComputeBackend {
    solvers: BTreeMap<String, Solver>,
}

impl FakeComputeBackend {
    /// Creates a backend with no solvers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `solver` under `name`, replacing any solver already registered under it.
    #[must_use]
    pub fn with_solver(
        mut self,
        name: impl Into<String>,
        solver: impl Fn(Vec<u8>) -> Result<Vec<u8>, ComputeError> + Send + Sync + 'static,
    ) -> Self {
        self.solvers.insert(name.into(), Box::new(solver));
        self
    }
}

impl fmt::Debug for FakeComputeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeComputeBackend")
            .field("solvers", &self.solvers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ComputeBackend for FakeComputeBackend {
    fn run<'a>(
        &'a self,
        solver: &'a str,
        input: Vec<u8>,
    ) -> BoxFuture<'a, Result<Vec<u8>, ComputeError>> {
        Box::pin(async move {
            match self.solvers.get(solver) {
                Some(run) => run(input),
                None => Err(ComputeError::UnknownSolver {
                    name: solver.to_owned(),
                }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::future::{pending, poll_fn, ready};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn now_or_never_returns_only_finished_output() {
        assert_eq!(now_or_never(ready(7)), Some(7));
        assert_eq!(now_or_never(pending::<i32>()), None);
    }

    #[test]
    fn manual_clock_moves_only_when_told() {
        let clock = ManualClock::new(Timestamp::from_unix_millis(1_000));
        assert_eq!(clock.now(), Timestamp::from_unix_millis(1_000));
        clock.advance(250);
        assert_eq!(clock.now(), Timestamp::from_unix_millis(1_250));
        clock.set(Timestamp::from_unix_millis(10));
        assert_eq!(clock.now(), Timestamp::from_unix_millis(10));
    }

    #[test]
    fn manual_clock_stops_at_the_largest_time() {
        let clock = ManualClock::new(Timestamp::from_unix_millis(u64::MAX - 1));
        clock.advance(5);
        assert_eq!(clock.now(), Timestamp::from_unix_millis(u64::MAX));
    }

    #[test]
    fn memory_storage_reads_back_the_latest_write() {
        let storage = MemoryStorage::new();
        let dyn_storage: &dyn Storage = &storage;
        assert_eq!(now_or_never(dyn_storage.write("a", b"one")), Some(Ok(())));
        assert_eq!(now_or_never(dyn_storage.write("a", b"two")), Some(Ok(())));
        assert_eq!(
            now_or_never(dyn_storage.read("a")),
            Some(Ok(b"two".to_vec()))
        );
    }

    #[test]
    fn memory_storage_ignores_operations_that_are_never_awaited() {
        let storage = MemoryStorage::new();
        drop(storage.write("a", b"one"));
        assert!(storage.keys().is_empty());
        assert_eq!(now_or_never(storage.write("a", b"one")), Some(Ok(())));
        drop(storage.remove("a"));
        assert_eq!(storage.keys(), ["a"]);
    }

    #[test]
    fn memory_storage_reports_missing_keys() {
        let storage = MemoryStorage::new();
        assert_eq!(
            now_or_never(storage.read("missing")),
            Some(Err(StorageError::NotFound))
        );
        assert_eq!(
            now_or_never(storage.remove("missing")),
            Some(Err(StorageError::NotFound))
        );
    }

    #[test]
    fn memory_storage_removes_and_lists_keys_in_order() {
        let storage = MemoryStorage::new();
        for key in ["b", "c", "a"] {
            assert_eq!(now_or_never(storage.write(key, &[])), Some(Ok(())));
        }
        assert_eq!(storage.keys(), ["a", "b", "c"]);
        assert_eq!(now_or_never(storage.remove("b")), Some(Ok(())));
        assert_eq!(storage.keys(), ["a", "c"]);
        assert_eq!(
            now_or_never(storage.read("b")),
            Some(Err(StorageError::NotFound))
        );
    }

    /// A future that waits until `gate` is opened.
    fn wait_for(gate: &Arc<AtomicBool>) -> impl Future<Output = ()> + 'static {
        let gate = Arc::clone(gate);
        poll_fn(move |_| {
            if gate.load(Ordering::SeqCst) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
    }

    #[test]
    fn inline_executor_runs_ready_tasks_during_spawn() {
        let executor = InlineExecutor::new();
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        executor.spawn(Box::pin(async move { flag.store(true, Ordering::SeqCst) }));
        assert!(ran.load(Ordering::SeqCst));
        assert_eq!(executor.pending(), 0);
    }

    #[test]
    fn inline_executor_resumes_waiting_tasks_only_when_run() {
        let executor = InlineExecutor::new();
        let gate = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (wait, flag) = (wait_for(&gate), Arc::clone(&done));
        executor.spawn(Box::pin(async move {
            wait.await;
            flag.store(true, Ordering::SeqCst);
        }));
        assert_eq!(executor.pending(), 1);

        executor.run_until_stalled();
        assert_eq!(executor.pending(), 1);
        assert!(!done.load(Ordering::SeqCst));

        gate.store(true, Ordering::SeqCst);
        assert!(!done.load(Ordering::SeqCst));
        executor.run_until_stalled();
        assert!(done.load(Ordering::SeqCst));
        assert_eq!(executor.pending(), 0);
    }

    #[test]
    fn inline_executor_runs_tasks_spawned_by_tasks() {
        let executor = Arc::new(InlineExecutor::new());
        let gate = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (wait, inner_wait) = (wait_for(&gate), wait_for(&gate));
        let (spawner, flag) = (Arc::clone(&executor), Arc::clone(&done));
        executor.spawn(Box::pin(async move {
            wait.await;
            spawner.spawn(Box::pin(async move {
                inner_wait.await;
                flag.store(true, Ordering::SeqCst);
            }));
        }));

        gate.store(true, Ordering::SeqCst);
        executor.run_until_stalled();
        assert!(done.load(Ordering::SeqCst));
        assert_eq!(executor.pending(), 0);
    }

    #[test]
    fn inline_executor_keeps_tasks_spawned_while_waiting() {
        let executor = Arc::new(InlineExecutor::new());
        let gate = Arc::new(AtomicBool::new(false));
        let (wait, spawner) = (wait_for(&gate), Arc::clone(&executor));
        executor.spawn(Box::pin(async move {
            wait.await;
            spawner.spawn(Box::pin(pending()));
        }));
        gate.store(true, Ordering::SeqCst);
        executor.run_until_stalled();
        assert_eq!(executor.pending(), 1);
    }

    #[test]
    fn inline_executor_stops_when_a_task_keeps_spawning_waiting_tasks() {
        let executor = Arc::new(InlineExecutor::new());
        let spawner = Arc::clone(&executor);
        executor.spawn(Box::pin(poll_fn(move |_| {
            spawner.spawn(Box::pin(pending()));
            Poll::<()>::Pending
        })));
        executor.run_until_stalled();
        assert_eq!(executor.pending(), 3);
    }

    #[test]
    fn doubles_describe_themselves() {
        let executor = InlineExecutor::new();
        executor.spawn(Box::pin(pending()));
        assert_eq!(format!("{executor:?}"), "InlineExecutor { pending: 1 }");
        let backend = FakeComputeBackend::new().with_solver("echo", Ok);
        assert_eq!(
            format!("{backend:?}"),
            r#"FakeComputeBackend { solvers: ["echo"] }"#
        );
    }

    #[test]
    fn fake_compute_backend_runs_registered_solvers() {
        let backend = FakeComputeBackend::new()
            .with_solver("echo", Ok)
            .with_solver("broken", |_| {
                Err(ComputeError::Failed {
                    message: "diverged".to_owned(),
                })
            });
        let dyn_backend: &dyn ComputeBackend = &backend;
        assert_eq!(
            now_or_never(dyn_backend.run("echo", vec![1, 2])),
            Some(Ok(vec![1, 2]))
        );
        assert_eq!(
            now_or_never(dyn_backend.run("broken", vec![])),
            Some(Err(ComputeError::Failed {
                message: "diverged".to_owned()
            }))
        );
        assert_eq!(
            now_or_never(dyn_backend.run("frame", vec![])),
            Some(Err(ComputeError::UnknownSolver {
                name: "frame".to_owned()
            }))
        );
    }
}
