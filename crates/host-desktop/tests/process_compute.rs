//! `ProcessCompute` running the `test-solver` fixture as a real child process.

use host_desktop::ProcessCompute;
use platform::testing::now_or_never;
use platform::{ComputeBackend, ComputeError};

const TEST_SOLVER: &str = env!("CARGO_BIN_EXE_test-solver");

#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn run(
    backend: &dyn ComputeBackend,
    solver: &str,
    input: Vec<u8>,
) -> Result<Vec<u8>, ComputeError> {
    now_or_never(backend.run(solver, input)).unwrap()
}

/// A backend whose `echo` solver is the `test-solver` fixture.
fn echo_backend() -> ProcessCompute {
    ProcessCompute::new().with_solver("echo", TEST_SOLVER)
}

#[test]
fn returns_the_solver_output() {
    let backend = echo_backend();
    assert_eq!(
        run(&backend, "echo", b"frame".to_vec()),
        Ok(b"frame".to_vec())
    );
}

#[test]
fn streams_input_larger_than_a_pipe_buffer() {
    let backend = echo_backend();
    let input: Vec<u8> = (0..1_u32 << 20).map(|i| i.to_le_bytes()[0]).collect();
    assert_eq!(run(&backend, "echo", input.clone()), Ok(input));
}

#[test]
fn reports_a_failing_solver() {
    let backend = echo_backend();
    let Err(ComputeError::Failed { message }) = run(&backend, "echo", b"fail".to_vec()) else {
        panic!("expected the solver to fail");
    };
    assert!(message.contains('3'), "{message}");
}

#[test]
fn reports_unknown_solvers() {
    let backend = echo_backend();
    assert_eq!(
        run(&backend, "frame", Vec::new()),
        Err(ComputeError::UnknownSolver {
            name: "frame".to_owned()
        })
    );
}

#[test]
fn reports_a_solver_that_cannot_start_without_its_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("no-such-solver");
    let backend = ProcessCompute::new().with_solver("frame", &missing);

    let Err(ComputeError::Failed { message }) = run(&backend, "frame", Vec::new()) else {
        panic!("expected the solver not to start");
    };
    assert!(
        message.starts_with("could not start the solver"),
        "{message}"
    );
    assert!(!message.contains("no-such-solver"), "{message}");
}
