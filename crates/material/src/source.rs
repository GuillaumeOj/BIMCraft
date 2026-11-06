//! Where a value comes from: a design-code document and a clause number (REP-05).

use std::fmt;

use thiserror::Error;

/// The longest document or clause text accepted, in characters.
pub const MAX_TEXT_LEN: usize = 200;

/// Separates the document from the clause in a source's text form.
const SEPARATOR: &str = ", ";

/// The document and clause a value comes from, such as `EN 1992-1-1:2004` and `Table 3.1`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Source {
    document: String,
    clause: String,
}

/// Why a source was rejected. Messages never repeat the text (SEC-03).
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SourceError {
    /// The document is empty.
    #[error("the source names no document")]
    EmptyDocument,
    /// The clause is empty.
    #[error("the source names no clause")]
    EmptyClause,
    /// The document contains `", "`, which separates it from the clause in text form.
    #[error("the source document contains \", \"")]
    SeparatorInDocument,
    /// The document or clause is longer than [`MAX_TEXT_LEN`] characters.
    #[error("the source text is longer than {MAX_TEXT_LEN} characters")]
    TooLong,
}

impl Source {
    /// A source citing `clause` of `document`; surrounding spaces are trimmed.
    ///
    /// # Errors
    ///
    /// Fails when either part is empty or too long, or the document contains `", "`.
    pub fn new(document: &str, clause: &str) -> Result<Self, SourceError> {
        let document = bounded_text(document)?.ok_or(SourceError::EmptyDocument)?;
        let clause = bounded_text(clause)?.ok_or(SourceError::EmptyClause)?;
        if document.contains(SEPARATOR) {
            return Err(SourceError::SeparatorInDocument);
        }
        Ok(Self {
            document: document.to_owned(),
            clause: clause.to_owned(),
        })
    }

    /// Reads the text form written by [`Display`](fmt::Display): `<document>, <clause>`.
    ///
    /// # Errors
    ///
    /// Fails when the text has no clause, or like [`Source::new`].
    pub fn parse(text: &str) -> Result<Self, SourceError> {
        let (document, clause) = text.split_once(SEPARATOR).ok_or(SourceError::EmptyClause)?;
        Self::new(document, clause)
    }

    /// The design-code document, with its edition.
    #[must_use]
    pub fn document(&self) -> &str {
        &self.document
    }

    /// The clause, table or annex number within the document.
    #[must_use]
    pub fn clause(&self) -> &str {
        &self.clause
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.document, self.clause)
    }
}

/// `text` without surrounding spaces, or `None` when nothing is left.
///
/// # Errors
///
/// Fails with [`SourceError::TooLong`] when the trimmed text is longer than [`MAX_TEXT_LEN`]
/// characters.
pub(crate) fn bounded_text(text: &str) -> Result<Option<&str>, SourceError> {
    let text = text.trim();
    if text.chars().count() > MAX_TEXT_LEN {
        return Err(SourceError::TooLong);
    }
    Ok((!text.is_empty()).then_some(text))
}

/// A value with the source it comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct Sourced<T> {
    /// The value.
    pub value: T,
    /// Where the value comes from.
    pub source: Source,
}

impl<T> Sourced<T> {
    /// `value`, citing `source`.
    pub const fn new(value: T, source: Source) -> Self {
        Self { value, source }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_round_trips_through_its_text_form() {
        let source = Source::new(" EN 1992-1-1:2004 ", "Table 3.1").unwrap();
        assert_eq!(source.document(), "EN 1992-1-1:2004");
        assert_eq!(source.clause(), "Table 3.1");
        assert_eq!(source.to_string(), "EN 1992-1-1:2004, Table 3.1");
        assert_eq!(Source::parse(&source.to_string()), Ok(source));
    }

    #[test]
    fn a_clause_may_contain_the_separator() {
        let source = Source::parse("EN 1992-1-1:2004, 3.2.7(4), Figure 3.8").unwrap();
        assert_eq!(source.clause(), "3.2.7(4), Figure 3.8");
    }

    #[test]
    fn incomplete_or_oversized_sources_are_rejected() {
        let long = "x".repeat(MAX_TEXT_LEN + 1);
        assert_eq!(Source::new(" ", "3.1"), Err(SourceError::EmptyDocument));
        assert_eq!(Source::new("EN 1990", ""), Err(SourceError::EmptyClause));
        assert_eq!(
            Source::new("EN, 1990", "3.1"),
            Err(SourceError::SeparatorInDocument)
        );
        assert_eq!(Source::new(&long, "3.1"), Err(SourceError::TooLong));
        assert_eq!(Source::new("EN 1990", &long), Err(SourceError::TooLong));
        assert_eq!(Source::parse("EN 1990"), Err(SourceError::EmptyClause));
        assert_eq!(
            SourceError::TooLong.to_string(),
            "the source text is longer than 200 characters"
        );
    }
}
