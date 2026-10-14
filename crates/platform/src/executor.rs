use crate::BoxFuture;

/// Runs background work: a thread pool on desktop, Web Workers or the event loop in the browser.
///
/// Tasks are [`BoxFuture`]s: `Send` on native targets, so a host can move them across threads, but
/// not on `wasm32`, because browser futures are not.
pub trait TaskExecutor {
    /// Starts `task` and returns without waiting for it to finish.
    fn spawn(&self, task: BoxFuture<'static, ()>);
}
