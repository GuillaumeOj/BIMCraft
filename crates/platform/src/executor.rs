use crate::BoxFuture;

/// Runs background work: a thread pool on desktop, Web Workers or the event loop in the browser.
///
/// Tasks are not required to be `Send`, because browser futures are not. The desktop host will
/// refine this when it needs to move work across threads.
pub trait TaskExecutor {
    /// Starts `task` and returns without waiting for it to finish.
    fn spawn(&self, task: BoxFuture<'static, ()>);
}
