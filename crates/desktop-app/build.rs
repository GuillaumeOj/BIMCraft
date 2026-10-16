//! Prepares the window's configuration, permissions and icons for Tauri.

// ARCH-02: a build script runs on the build machine, like the host itself, and reports errors to
// Cargo on its standard output.
#![allow(clippy::disallowed_methods)]

use std::io::{self, Write};

fn main() {
    // Only the API command is exposed to the window, and only through capabilities/.
    let manifest = tauri_build::AppManifest::new().commands(&["api"]);
    let attributes = tauri_build::Attributes::new().app_manifest(manifest);
    if let Err(error) = tauri_build::try_build(attributes) {
        let _ = writeln!(io::stdout(), "cargo::error={error:#}");
    }
}
