use crate::BoxFuture;

/// Why a [`ComputeBackend`] run failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComputeError {
    /// The backend does not provide a solver with this name.
    #[error("unknown solver `{name}`")]
    UnknownSolver {
        /// The requested solver name.
        name: String,
    },
    /// The solver started but did not produce a result.
    #[error("solver failed: {message}")]
    Failed {
        /// Host-provided description, without model content or personal data.
        message: String,
    },
}

/// Runs heavy computations outside the core: child processes on desktop, Web Workers or a
/// self-hosted server in the browser (spec §9).
pub trait ComputeBackend {
    /// Runs the solver called `solver` on the serialised `input` and returns its serialised output.
    fn run<'a>(
        &'a self,
        solver: &'a str,
        input: Vec<u8>,
    ) -> BoxFuture<'a, Result<Vec<u8>, ComputeError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_describe_themselves() {
        let unknown = ComputeError::UnknownSolver {
            name: "frame".to_owned(),
        };
        assert_eq!(unknown.to_string(), "unknown solver `frame`");
        let failed = ComputeError::Failed {
            message: "exit code 3".to_owned(),
        };
        assert_eq!(failed.to_string(), "solver failed: exit code 3");
    }
}
