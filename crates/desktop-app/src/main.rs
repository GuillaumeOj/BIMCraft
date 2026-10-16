//! The desktop program: the web interface in a native window, on a new document (UI-01).
//!
//! `host-desktop` holds the document and its IPC; this program only opens the window. See
//! docs/dev/desktop.md.

// ARCH-02: the program is the host. It reports to the terminal, and the code Tauri generates for
// it uses threads.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
// Release builds run as a windowed program on Windows, without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use host_desktop::{DocumentHost, ipc};

fn main() -> ExitCode {
    exit_code(run(), &mut io::stderr())
}

/// Opens the window on a new document and runs until it closes.
fn run() -> Result<(), Box<dyn Error>> {
    let host = DocumentHost::open_new()?;
    ipc::serve(tauri::Builder::default(), host).run(tauri::generate_context!())?;
    Ok(())
}

/// The program's exit status for `result`, telling `log` why it failed: the window did not open,
/// so nothing else can.
fn exit_code(result: Result<(), Box<dyn Error>>, log: &mut impl Write) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(log, "BIMCraft could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_window_exits_successfully_and_quietly() {
        let mut log = Vec::new();
        assert_eq!(exit_code(Ok(()), &mut log), ExitCode::SUCCESS);
        assert!(log.is_empty());
    }

    #[test]
    fn a_window_that_cannot_open_says_why() {
        let mut log = Vec::new();
        let error = host_desktop::OpenError::Schema(Vec::new()).into();
        assert_eq!(exit_code(Err(error), &mut log), ExitCode::FAILURE);
        assert_eq!(
            String::from_utf8(log).unwrap(),
            "BIMCraft could not start: the built-in core schema is invalid: []\n"
        );
    }
}
