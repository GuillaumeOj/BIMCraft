use std::fmt;

/// A place in a schema: 1-based line and column, with columns counted in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: usize,
    /// Character offset within the line, starting at 1.
    pub column: usize,
}

impl Position {
    /// The first character of the input.
    pub const START: Self = Self { line: 1, column: 1 };
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// A problem found while reading a schema or generating code from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// What went wrong.
    pub kind: ErrorKind,
    /// Where it went wrong, when the problem is at one place of the schema.
    pub position: Option<Position>,
}

impl Error {
    pub(crate) const fn at(kind: ErrorKind, position: Position) -> Self {
        Self {
            kind,
            position: Some(position),
        }
    }

    pub(crate) const fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            position: None,
        }
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(position) = self.position {
            write!(f, "{position}: ")?;
        }
        write!(f, "{}", self.kind)
    }
}

/// The kinds of [`Error`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    /// The schema file could not be read.
    #[error("{0}")]
    Io(String),
    /// The schema is larger than [`MAX_INPUT_LEN`](crate::MAX_INPUT_LEN).
    #[error("schema larger than the limit of {limit} bytes")]
    InputTooLarge {
        /// The limit in force.
        limit: usize,
    },
    /// A character that cannot start a token.
    #[error("unexpected character {0:?}")]
    UnexpectedCharacter(char),
    /// A string literal with no closing delimiter.
    #[error("unterminated string")]
    UnterminatedString,
    /// An embedded remark `(* … *)` with no closing `*)`.
    #[error("unterminated remark")]
    UnterminatedRemark,
    /// An encoded string literal whose content is not groups of eight hexadecimal digits.
    #[error("malformed encoded string")]
    InvalidEncodedString,
    /// A binary literal `%…` with a digit other than 0 or 1, or no digit.
    #[error("malformed binary literal")]
    InvalidBinary,
    /// An integer or real literal that does not fit in 64 bits.
    #[error("number out of range")]
    NumberOutOfRange,
    /// A token, or the end of the input, where the grammar expects something else.
    #[error("expected {expected}, found {found}")]
    UnexpectedToken {
        /// What the grammar expects.
        expected: &'static str,
        /// What was found instead.
        found: String,
    },
    /// Constructs nested deeper than [`MAX_DEPTH`](crate::MAX_DEPTH).
    #[error("constructs nested deeper than the limit of {limit}")]
    TooDeep {
        /// The limit in force.
        limit: usize,
    },
    /// A part of the EXPRESS language this parser does not read.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// Two declarations, attributes or items with the same name.
    #[error("`{0}` is declared more than once")]
    Duplicate(String),
    /// A reference to a name that is not declared, or not of the expected kind.
    #[error("`{name}` is not a declared {expected}")]
    Undefined {
        /// The name referred to.
        name: String,
        /// What it should name, such as "entity" or "type".
        expected: &'static str,
    },
    /// An entity that is, directly or through others, its own supertype.
    #[error("`{0}` is its own supertype")]
    SupertypeCycle(String),
    /// A type that refers to itself without an aggregate in between.
    #[error("`{0}` is defined in terms of itself")]
    TypeCycle(String),
    /// An attribute redeclared with `SELF\Entity.attribute` that the entity does not inherit.
    #[error("`{entity}` does not inherit an attribute `{attribute}` to redeclare")]
    UnknownRedeclaration {
        /// The redeclaring entity.
        entity: String,
        /// The attribute it names.
        attribute: String,
    },
    /// A generic type (`GENERIC`, `AGGREGATE`…) outside a function, procedure or rule.
    #[error("generic type in `{0}`; generic types are only for parameters")]
    GenericType(String),
    /// Aggregate bounds whose upper bound is below the lower one.
    #[error("invalid aggregate bounds in `{0}`")]
    InvalidBounds(String),
    /// A valid schema that cannot be turned into Rust.
    #[error("{0}")]
    Rust(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_the_position_when_known() {
        let kind = || ErrorKind::Duplicate("IfcWall".into());
        assert_eq!(
            Error::new(kind()).to_string(),
            "`IfcWall` is declared more than once"
        );
        let position = Position { line: 3, column: 7 };
        assert_eq!(
            Error::at(kind(), position).to_string(),
            "line 3, column 7: `IfcWall` is declared more than once"
        );
        assert_eq!(Position::START.to_string(), "line 1, column 1");
    }

    #[test]
    fn every_kind_describes_itself() {
        let cases = [
            (ErrorKind::Io("gone".into()), "gone"),
            (
                ErrorKind::InputTooLarge { limit: 8 },
                "schema larger than the limit of 8 bytes",
            ),
            (
                ErrorKind::UnexpectedCharacter('~'),
                "unexpected character '~'",
            ),
            (ErrorKind::UnterminatedString, "unterminated string"),
            (ErrorKind::UnterminatedRemark, "unterminated remark"),
            (ErrorKind::InvalidEncodedString, "malformed encoded string"),
            (ErrorKind::InvalidBinary, "malformed binary literal"),
            (ErrorKind::NumberOutOfRange, "number out of range"),
            (
                ErrorKind::UnexpectedToken {
                    expected: "`;`",
                    found: "`)`".into(),
                },
                "expected `;`, found `)`",
            ),
            (
                ErrorKind::TooDeep { limit: 4 },
                "constructs nested deeper than the limit of 4",
            ),
            (
                ErrorKind::Unsupported("USE FROM"),
                "USE FROM is not supported",
            ),
            (
                ErrorKind::Undefined {
                    name: "IfcFoo".into(),
                    expected: "type",
                },
                "`IfcFoo` is not a declared type",
            ),
            (
                ErrorKind::SupertypeCycle("A".into()),
                "`A` is its own supertype",
            ),
            (
                ErrorKind::TypeCycle("A".into()),
                "`A` is defined in terms of itself",
            ),
            (
                ErrorKind::UnknownRedeclaration {
                    entity: "B".into(),
                    attribute: "x".into(),
                },
                "`B` does not inherit an attribute `x` to redeclare",
            ),
            (
                ErrorKind::GenericType("A".into()),
                "generic type in `A`; generic types are only for parameters",
            ),
            (
                ErrorKind::InvalidBounds("A".into()),
                "invalid aggregate bounds in `A`",
            ),
            (ErrorKind::Rust("clash".into()), "clash"),
        ];
        for (kind, message) in cases {
            assert_eq!(kind.to_string(), message);
        }
    }
}
