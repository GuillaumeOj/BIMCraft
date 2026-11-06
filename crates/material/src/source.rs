//! Values with the document and clause they come from (REP-05, ADR 0018).

use core_model::{Source, SourceError};

/// The longest material name or design code accepted, in characters.
pub const MAX_TEXT_LEN: usize = core_model::MAX_SOURCE_LEN;

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
    fn text_is_trimmed_and_bounded() {
        assert_eq!(bounded_text(" C30/37 "), Ok(Some("C30/37")));
        assert_eq!(bounded_text("  "), Ok(None));
        assert_eq!(
            bounded_text(&"x".repeat(MAX_TEXT_LEN + 1)),
            Err(SourceError::TooLong)
        );
    }
}
