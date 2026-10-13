use std::fmt;
use std::io;

/// A place in the input: 1-based line and column, with columns counted in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: u64,
    /// Byte offset within the line, starting at 1.
    pub column: u64,
}

impl Position {
    /// The first byte of the input.
    pub const START: Self = Self { line: 1, column: 1 };
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Why reading a STEP file failed, and where.
///
/// Messages never quote the file content: it can hold personal data (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at {position}")]
pub struct StepError {
    /// What went wrong.
    pub kind: StepErrorKind,
    /// Where it went wrong: the start of the offending token, or the offending byte.
    pub position: Position,
}

/// The kinds of [`StepError`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StepErrorKind {
    /// The underlying reader failed.
    #[error("reading the input failed ({0})")]
    Io(io::ErrorKind),
    /// A byte that cannot appear here, such as a control character or a non-ASCII byte.
    #[error("unexpected byte 0x{0:02X}")]
    UnexpectedByte(u8),
    /// A token, or the end of the input, where the grammar expects something else.
    #[error("expected {expected}, found {found}")]
    UnexpectedToken {
        /// What the grammar expects.
        expected: &'static str,
        /// What was found instead.
        found: &'static str,
    },
    /// A binary literal with a non-hexadecimal digit or a leading digit above 3.
    #[error("malformed binary literal")]
    InvalidBinary,
    /// A number or instance name that does not fit in 64 bits.
    #[error("number out of range")]
    NumberOutOfRange,
    /// A token longer than [`Limits::max_token_len`](super::Limits::max_token_len).
    #[error("token longer than the limit of {limit} bytes")]
    TokenTooLong {
        /// The limit in force.
        limit: usize,
    },
    /// An instance longer than [`Limits::max_instance_len`](super::Limits::max_instance_len).
    #[error("instance longer than the limit of {limit} bytes")]
    InstanceTooLong {
        /// The limit in force.
        limit: usize,
    },
    /// Parameters nested deeper than [`Limits::max_nesting_depth`](super::Limits::max_nesting_depth).
    #[error("parameters nested deeper than the limit of {limit}")]
    TooDeep {
        /// The limit in force.
        limit: usize,
    },
    /// More instances than [`Limits::max_instances`](super::Limits::max_instances).
    #[error("more instances than the limit of {limit}")]
    TooManyInstances {
        /// The limit in force.
        limit: usize,
    },
    /// Two instances with the same name.
    #[error("instance #{0} defined more than once")]
    DuplicateInstance(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_position_without_quoting_content() {
        let error = StepError {
            kind: StepErrorKind::UnexpectedToken {
                expected: "`;`",
                found: "a string",
            },
            position: Position {
                line: 3,
                column: 14,
            },
        };
        assert_eq!(
            error.to_string(),
            "expected `;`, found a string at line 3, column 14"
        );
    }

    #[test]
    fn every_kind_describes_itself() {
        let cases = [
            (
                StepErrorKind::Io(io::ErrorKind::UnexpectedEof),
                "reading the input failed (unexpected end of file)",
            ),
            (StepErrorKind::UnexpectedByte(0x07), "unexpected byte 0x07"),
            (StepErrorKind::InvalidBinary, "malformed binary literal"),
            (StepErrorKind::NumberOutOfRange, "number out of range"),
            (
                StepErrorKind::TokenTooLong { limit: 8 },
                "token longer than the limit of 8 bytes",
            ),
            (
                StepErrorKind::InstanceTooLong { limit: 9 },
                "instance longer than the limit of 9 bytes",
            ),
            (
                StepErrorKind::TooDeep { limit: 4 },
                "parameters nested deeper than the limit of 4",
            ),
            (
                StepErrorKind::TooManyInstances { limit: 2 },
                "more instances than the limit of 2",
            ),
            (
                StepErrorKind::DuplicateInstance(12),
                "instance #12 defined more than once",
            ),
        ];
        for (kind, message) in cases {
            assert_eq!(kind.to_string(), message);
        }
    }
}
