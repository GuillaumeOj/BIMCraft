use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use platform::{BoxFuture, ComputeBackend, ComputeError};

/// A [`ComputeBackend`] that runs each solver as a child process, so a crashing solver never
/// takes the model down (spec §9).
///
/// A run starts the solver's program, writes the input to its standard input, and returns what it
/// writes to its standard output once it exits successfully. Its error output is discarded,
/// because it may quote the model (SEC-03).
#[derive(Clone, Debug, Default)]
pub struct ProcessCompute {
    solvers: BTreeMap<String, PathBuf>,
}

impl ProcessCompute {
    /// Creates a backend with no solvers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the executable `program` as the solver `name`, replacing any solver already
    /// registered under it.
    #[must_use]
    pub fn with_solver(mut self, name: impl Into<String>, program: impl Into<PathBuf>) -> Self {
        self.solvers.insert(name.into(), program.into());
        self
    }
}

impl ComputeBackend for ProcessCompute {
    fn run<'a>(
        &'a self,
        solver: &'a str,
        input: Vec<u8>,
    ) -> BoxFuture<'a, Result<Vec<u8>, ComputeError>> {
        Box::pin(async move {
            let program = self
                .solvers
                .get(solver)
                .ok_or_else(|| ComputeError::UnknownSolver {
                    name: solver.to_owned(),
                })?;
            run_process(program, &input)
        })
    }
}

/// Runs `program` with `input` on its standard input and returns its standard output.
fn run_process(program: &Path, input: &[u8]) -> Result<Vec<u8>, ComputeError> {
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| failed("could not start the solver", &error))?;
    let stdin = child.stdin.take();
    // The input is written from a second thread while this one reads the output: writing it all
    // first would block forever once the solver fills its output pipe.
    let output = thread::scope(|scope| {
        scope.spawn(move || {
            if let Some(mut stdin) = stdin {
                // A solver may exit without reading all its input; its exit status tells.
                let _ = stdin.write_all(input);
            }
        });
        child.wait_with_output()
    })
    .map_err(|error| failed("lost contact with the solver", &error))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(ComputeError::Failed {
            message: format!("the solver stopped with {}", output.status),
        })
    }
}

/// Describes `error` by its kind only: the full message can repeat the program's path (SEC-03).
fn failed(context: &str, error: &io::Error) -> ComputeError {
    ComputeError::Failed {
        message: format!("{context}: {}", error.kind()),
    }
}
