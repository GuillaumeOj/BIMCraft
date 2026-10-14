//! `ProcessCompute` running the `test-solver` fixture as a real child process.

use host_desktop::ProcessCompute;
use platform::testing::now_or_never;
use platform::{ComputeBackend, ComputeError};

const TEST_SOLVER: &str = env!("CARGO_BIN_EXE_test-solver");

#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn run(backend: &dyn ComputeBackend, solver: &str, input: &[u8]) -> Result<Vec<u8>, ComputeError> {
    now_or_never(backend.run(solver, input.to_vec())).unwrap()
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn returns_the_solver_output() {
    let backend = ProcessCompute::new().with_solver("echo", TEST_SOLVER);
    assert_eq!(run(&backend, "echo", b"frame"), Ok(b"frame".to_vec()));
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn streams_input_larger_than_a_pipe_buffer() {
    let backend = ProcessCompute::new().with_solver("echo", TEST_SOLVER);
    let input: Vec<u8> = (0..4_000_000_u32).map(|i| i.to_le_bytes()[0]).collect();
    assert_eq!(run(&backend, "echo", &input), Ok(input));
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reports_a_failing_solver() {
    let backend = ProcessCompute::new().with_solver("echo", TEST_SOLVER);
    let Err(ComputeError::Failed { message }) = run(&backend, "echo", b"fail") else {
        panic!("expected the solver to fail");
    };
    assert!(message.contains('3'), "{message}");
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reports_unknown_solvers() {
    let backend = ProcessCompute::new().with_solver("echo", TEST_SOLVER);
    assert_eq!(
        run(&backend, "frame", b""),
        Err(ComputeError::UnknownSolver {
            name: "frame".to_owned()
        })
    );
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reports_a_solver_that_cannot_start_without_its_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("no-such-solver");
    let backend = ProcessCompute::new().with_solver("frame", &missing);

    let Err(ComputeError::Failed { message }) = run(&backend, "frame", b"") else {
        panic!("expected the solver not to start");
    };
    assert!(
        message.starts_with("could not start the solver"),
        "{message}"
    );
    assert!(!message.contains("no-such-solver"), "{message}");
}
