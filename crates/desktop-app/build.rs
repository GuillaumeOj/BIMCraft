//! Prepares the window's configuration, permissions and icons for Tauri.

// ARCH-02: a build script runs on the build machine, like the host itself, and reports errors to
// Cargo on its standard output.
#![allow(clippy::disallowed_methods)]

use std::io::{self, Write};

fn main() {
    // Only the API command is exposed to the window, and only through capabilities/.
    let manifest = tauri_build::AppManifest::new().commands(&["api"]);
    // Releases link the C runtime statically on Windows, so they run without the Visual C++
    // redistributable. Tauri does that by putting an empty msvcrt.lib in this script's output
    // folder, and Cargo links every workspace doctest with that folder on its search path, so the
    // doctests then fail to link: other builds (tests, development) use the runtime as a DLL.
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let windows = tauri_build::WindowsAttributes::new().static_vc_runtime(release);
    let attributes = tauri_build::Attributes::new()
        .app_manifest(manifest)
        .windows_attributes(windows);
    if let Err(error) = tauri_build::try_build(attributes) {
        let _ = writeln!(io::stdout(), "cargo::error={error:#}");
    }
}
