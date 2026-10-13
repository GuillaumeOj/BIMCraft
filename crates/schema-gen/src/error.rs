use std::fmt;

/// A problem found while reading definitions or generating code from them.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// The definition file, relative to the namespace directory, when the problem is in one.
    pub file: Option<String>,
    /// 1-based line in `file`, when known.
    pub line: Option<usize>,
    /// 1-based column in `file`, when known.
    pub column: Option<usize>,
    /// What went wrong.
    pub kind: ErrorKind,
}

/// What went wrong.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ErrorKind {
    /// A file or directory could not be read.
    #[error("{0}")]
    Io(String),
    /// A file is not valid TOML, or does not have the expected keys and value types.
    #[error("{0}")]
    Syntax(String),
    /// A definition breaks a rule of the file format that TOML cannot express.
    #[error("{0}")]
    Definition(String),
    /// The namespace built from the definitions is inconsistent.
    #[error("{0}")]
    Validation(Box<schema::ValidationError>),
    /// The namespace is valid but cannot be turned into Rust.
    #[error("{0}")]
    Rust(String),
}

impl Error {
    /// An error with no position.
    pub(crate) const fn new(kind: ErrorKind) -> Self {
        Self {
            file: None,
            line: None,
            column: None,
            kind,
        }
    }

    /// An error in `file`, at a byte offset of its `contents` when one is known.
    pub(crate) fn at(file: &str, contents: &str, offset: Option<usize>, kind: ErrorKind) -> Self {
        let (line, column) = offset
            .map(|offset| line_and_column(contents, offset))
            .unzip();
        Self {
            file: Some(file.to_owned()),
            line,
            column,
            kind,
        }
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            f.write_str(file)?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            if let Some(column) = self.column {
                write!(f, ":{column}")?;
            }
            f.write_str(": ")?;
        }
        write!(f, "{}", self.kind)
    }
}

/// 1-based line and column (in characters) of a byte offset; offsets past the end, or inside a
/// character, count as the nearest earlier character.
fn line_and_column(contents: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(contents.len());
    while !contents.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &contents[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_one_based() {
        let contents = "a = 1\nbé = 2\n";
        assert_eq!(line_and_column(contents, 0), (1, 1));
        assert_eq!(line_and_column(contents, 6), (2, 1));
        assert_eq!(line_and_column(contents, 9), (2, 3));
        // Inside the two-byte `é`, and past the end.
        assert_eq!(line_and_column(contents, 8), (2, 2));
        assert_eq!(line_and_column(contents, 100), (3, 1));
    }

    #[test]
    fn displays_the_position_it_knows() {
        let kind = || ErrorKind::Definition("bad".into());
        assert_eq!(Error::new(kind()).to_string(), "bad");
        assert_eq!(
            Error::at("entities/wall.toml", "", None, kind()).to_string(),
            "entities/wall.toml: bad"
        );
        assert_eq!(
            Error::at("entities/wall.toml", "x\ny", Some(2), kind()).to_string(),
            "entities/wall.toml:2:1: bad"
        );
    }
}
