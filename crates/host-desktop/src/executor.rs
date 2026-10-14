use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use platform::{BoxFuture, TaskExecutor};
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

/// Why a [`RayonExecutor`] could not be created.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ExecutorError(#[from] ThreadPoolBuildError);

/// A [`TaskExecutor`] that runs each task on a rayon thread pool.
///
/// A task keeps its worker thread until it finishes, even while it waits, so tasks should do
/// computation rather than wait on slow I/O.
#[derive(Debug)]
pub struct RayonExecutor {
    pool: ThreadPool,
}

impl RayonExecutor {
    /// Starts a pool of `threads` worker threads, or one per logical CPU if `threads` is 0.
    ///
    /// # Errors
    ///
    /// Fails if the operating system refuses to start the threads.
    pub fn new(threads: usize) -> Result<Self, ExecutorError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("bimcraft-worker-{index}"))
            .build()?;
        Ok(Self { pool })
    }

    /// Returns how many worker threads the pool runs.
    #[must_use]
    pub fn threads(&self) -> usize {
        self.pool.current_num_threads()
    }
}

impl TaskExecutor for RayonExecutor {
    fn spawn(&self, task: BoxFuture<'static, ()>) {
        self.pool.spawn(move || block_on(task));
    }
}

/// Runs `future` to completion on the current thread, parking it while the future waits.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        // A wake that arrives before `park` leaves a token, so `park` returns at once and no wake
        // is lost. A spurious return only costs one extra poll.
        thread::park();
    }
}

/// Wakes a task by unparking the thread that runs it.
struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn runs_every_spawned_task_on_a_worker_thread() {
        let executor = RayonExecutor::new(2).unwrap();
        assert_eq!(executor.threads(), 2);
        let (sender, receiver) = mpsc::channel();
        for task in 0..10 {
            let sender = sender.clone();
            executor.spawn(Box::pin(async move {
                let worker = thread::current().name().map(str::to_owned);
                sender.send((task, worker)).unwrap();
            }));
        }
        drop(sender);
        let mut done: Vec<_> = receiver.iter().collect();
        done.sort();
        assert_eq!(done.len(), 10);
        for (expected, (task, worker)) in done.into_iter().enumerate() {
            assert_eq!(task, expected);
            assert!(worker.unwrap().starts_with("bimcraft-worker-"));
        }
    }

    #[test]
    fn zero_threads_means_one_per_cpu() {
        let executor = RayonExecutor::new(0).unwrap();
        assert!(executor.threads() >= 1);
    }

    #[test]
    fn block_on_resumes_a_future_woken_before_it_parks() {
        let mut polls = 0;
        let output = block_on(poll_fn(|context| {
            polls += 1;
            if polls < 3 {
                context.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(polls)
            }
        }));
        assert_eq!(output, 3);
    }

    #[test]
    fn spawned_task_resumes_when_woken_from_another_thread() {
        let executor = RayonExecutor::new(1).unwrap();
        let (waker_sender, waker_receiver) = mpsc::channel::<Waker>();
        let (done_sender, done_receiver) = mpsc::channel();
        let (go_sender, go_receiver) = mpsc::channel::<()>();
        executor.spawn(Box::pin(async move {
            poll_fn(move |context| {
                if go_receiver.try_recv().is_ok() {
                    Poll::Ready(())
                } else {
                    waker_sender.send(context.waker().clone()).unwrap();
                    Poll::Pending
                }
            })
            .await;
            done_sender.send(()).unwrap();
        }));

        let waker = waker_receiver.recv().unwrap();
        go_sender.send(()).unwrap();
        waker.wake();
        done_receiver.recv().unwrap();
    }
}
