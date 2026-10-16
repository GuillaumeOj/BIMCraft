//! Desktop host: implements the `platform` traits with the operating system's files, threads,
//! clock and processes (ARCH-02).
//!
//! - [`FileStorage`] keeps each value in a file under a root directory.
//! - [`RayonExecutor`] runs tasks on a rayon thread pool.
//! - [`SystemClock`] reads the system's wall clock.
//! - [`ProcessCompute`] runs each solver as a child process (spec §9).
//! - [`DocumentHost`] serves a document to the window over JSON-RPC, and [`ipc`] carries the
//!   messages over Tauri's IPC (UI-01, API-04). The `desktop-app` crate opens the window.

// ARCH-02: the host is where the core meets the operating system, so it opts out of the APIs that
// clippy.toml bans in core crates. See docs/dev/quality.md.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

mod clock;
mod compute;
mod document;
mod executor;
pub mod ipc;
mod storage;

pub use clock::SystemClock;
pub use compute::ProcessCompute;
pub use document::{DocumentHost, OpenError};
pub use executor::{ExecutorError, RayonExecutor};
pub use storage::FileStorage;
