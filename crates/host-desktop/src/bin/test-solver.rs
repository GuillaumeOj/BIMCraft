//! Test fixture for `ProcessCompute`: a stand-in solver run as a child process.
//!
//! It copies its standard input to its standard output, or exits with status 3 when the input is
//! `fail`.

// The fixture is a host-side process, like the real solvers.
#![allow(clippy::disallowed_methods)]

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        return ExitCode::from(2);
    }
    if input == b"fail" {
        return ExitCode::from(3);
    }
    match io::stdout().write_all(&input) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}
