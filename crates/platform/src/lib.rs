//! Interfaces between the `BIMCraft` core and the platform it runs on.
//!
//! Core crates never touch the operating system directly (ARCH-02), so the same code builds for
//! desktop and for `wasm32-unknown-unknown` (ARCH-01). Each host (desktop, browser) implements
//! these traits with its own files, threads, clock and solver processes.
//!
//! Every trait is dyn-compatible, so the core can hold a host as `&dyn Storage` instead of
//! becoming generic over it. The signatures are deliberately minimal; the first crate that uses a
//! trait refines it.
//!
//! The `testing` feature adds the `testing` module: deterministic, in-memory implementations of every trait
//! for the tests of core crates.

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::pin::Pin;

mod clock;
mod compute;
mod executor;
mod storage;
#[cfg(feature = "testing")]
pub mod testing;

pub use clock::{Clock, Timestamp};
pub use compute::{ComputeBackend, ComputeError};
pub use executor::TaskExecutor;
pub use storage::{Storage, StorageError};

/// A boxed future, so traits can return futures and still be used as trait objects.
///
/// On native targets it is `Send`, so the desktop host can run it on a thread pool.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A boxed future, so traits can return futures and still be used as trait objects.
///
/// On `wasm32` it is not `Send`, because browser futures are not.
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails to compile if a trait stops being usable as a trait object.
    #[test]
    fn traits_are_dyn_compatible() {
        let _: Option<&dyn Clock> = None;
        let _: Option<&dyn ComputeBackend> = None;
        let _: Option<&dyn Storage> = None;
        let _: Option<&dyn TaskExecutor> = None;
    }
}
