//! Where a value comes from: a document and a clause, or the user (REP-05, ADR 0018).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::AttributeValue;

/// The longest document or clause accepted, in characters.
pub const MAX_SOURCE_LEN: usize = 200;

/// Where a value comes from: the document and clause it is taken from, such as
/// `EN 1992-1-1:2004` and `Table 3.1`, or the user who entered it.
///
/// A document and a clause are trimmed, non-empty, at most [`MAX_SOURCE_LEN`] characters and
/// free of control characters, which also keeps standards text from being pasted in (REP-05).
/// In JSON, a cited source is `{"document": …, "clause": …}` and a user's value is
/// `{"entered_by_user": true}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "SourceParts", into = "SourceParts")]
pub struct Source(Origin);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Origin {
    Cited {
        document: Box<str>,
        clause: Box<str>,
    },
    User,
}

/// A [`Source`] as serialised, before it is checked.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum SourceParts {
    Cited(Citation),
    User(EnteredByUser),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Citation {
    document: String,
    clause: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnteredByUser {
    entered_by_user: True,
}

/// The JSON `true`, the only value `entered_by_user` takes.
struct True;

impl Serialize for True {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for True {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("entered_by_user is always true"))
        }
    }
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
    /// A part is longer than [`MAX_SOURCE_LEN`] characters.
    #[error("the source text is longer than {MAX_SOURCE_LEN} characters")]
    TooLong,
    /// A part contains a control character, such as a line break.
    #[error("the source text contains a control character")]
    ControlCharacter,
}

impl Source {
    /// A source citing `clause` of `document`; surrounding spaces are trimmed.
    ///
    /// # Errors
    ///
    /// Fails when a part is empty, too long or contains a control character.
    pub fn new(document: &str, clause: &str) -> Result<Self, SourceError> {
        let document = part(document)?.ok_or(SourceError::EmptyDocument)?;
        let clause = part(clause)?.ok_or(SourceError::EmptyClause)?;
        Ok(Self(Origin::Cited {
            document: document.into(),
            clause: clause.into(),
        }))
    }

    /// The source of a value the user entered instead of taking it from a document.
    #[must_use]
    pub const fn user() -> Self {
        Self(Origin::User)
    }

    /// Whether the user entered the value.
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self.0, Origin::User)
    }

    /// The document, with its edition; `None` for a value the user entered.
    #[must_use]
    pub fn document(&self) -> Option<&str> {
        match &self.0 {
            Origin::Cited { document, .. } => Some(document),
            Origin::User => None,
        }
    }

    /// The clause, table or annex number within the document; `None` for a value the user
    /// entered.
    #[must_use]
    pub fn clause(&self) -> Option<&str> {
        match &self.0 {
            Origin::Cited { clause, .. } => Some(clause),
            Origin::User => None,
        }
    }

    /// Heap memory owned by the source.
    pub(crate) fn heap_bytes(&self) -> usize {
        match &self.0 {
            Origin::Cited { document, clause } => document.len() + clause.len(),
            Origin::User => 0,
        }
    }
}

impl TryFrom<SourceParts> for Source {
    type Error = SourceError;

    fn try_from(parts: SourceParts) -> Result<Self, SourceError> {
        match parts {
            SourceParts::Cited(citation) => Self::new(&citation.document, &citation.clause),
            SourceParts::User(_) => Ok(Self::user()),
        }
    }
}

impl From<Source> for SourceParts {
    fn from(source: Source) -> Self {
        match source.0 {
            Origin::Cited { document, clause } => Self::Cited(Citation {
                document: document.into(),
                clause: clause.into(),
            }),
            Origin::User => Self::User(EnteredByUser {
                entered_by_user: True,
            }),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Origin::Cited { document, clause } => write!(f, "{document}, {clause}"),
            Origin::User => f.write_str("entered by user"),
        }
    }
}

/// `text` trimmed, or `None` when nothing is left.
fn part(text: &str) -> Result<Option<&str>, SourceError> {
    let text = text.trim();
    if text.chars().count() > MAX_SOURCE_LEN {
        return Err(SourceError::TooLong);
    }
    if text.chars().any(char::is_control) {
        return Err(SourceError::ControlCharacter);
    }
    Ok((!text.is_empty()).then_some(text))
}

/// The value of a sourced attribute or property, with its source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedValue {
    /// The plain value; never itself sourced.
    pub value: AttributeValue,
    /// Where the value comes from.
    pub source: Source,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_keeps_its_trimmed_parts() {
        let source = Source::new(" EN 1992-1-1:2004 ", "Table 3.1").unwrap();
        assert_eq!(source.document(), Some("EN 1992-1-1:2004"));
        assert_eq!(source.clause(), Some("Table 3.1"));
        assert!(!source.is_user());
        assert_eq!(source.to_string(), "EN 1992-1-1:2004, Table 3.1");
        assert_eq!(source.heap_bytes(), 25);
    }

    #[test]
    fn a_value_entered_by_the_user_cites_nothing() {
        let source = Source::user();
        assert!(source.is_user());
        assert_eq!(source.document(), None);
        assert_eq!(source.clause(), None);
        assert_eq!(source.to_string(), "entered by user");
        assert_eq!(source.heap_bytes(), 0);
        assert_ne!(source, Source::new("EN 1990", "3.1").unwrap());
    }

    #[test]
    fn sources_have_their_json_shapes() {
        let cited = Source::new("EN 1990", "3.1").unwrap();
        let json = |source: &Source| serde_json::to_string(source).unwrap();
        assert_eq!(json(&cited), r#"{"document":"EN 1990","clause":"3.1"}"#);
        assert_eq!(json(&Source::user()), r#"{"entered_by_user":true}"#);
        for source in [cited, Source::user()] {
            assert_eq!(
                serde_json::from_str::<Source>(&json(&source)).unwrap(),
                source
            );
        }
        for invalid in [
            r#"{"entered_by_user":false}"#,
            r#"{"entered_by_user":true,"document":"EN 1990"}"#,
            r#"{"document":"EN 1990","clause":"3.1","entered_by_user":true}"#,
            r#"{"document":"EN 1990"}"#,
            r"{}",
        ] {
            assert!(
                serde_json::from_str::<Source>(invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn incomplete_or_suspicious_sources_are_rejected() {
        let long = "x".repeat(MAX_SOURCE_LEN + 1);
        assert_eq!(Source::new(" ", "3.1"), Err(SourceError::EmptyDocument));
        assert_eq!(Source::new("EN 1990", ""), Err(SourceError::EmptyClause));
        assert_eq!(Source::new(&long, "3.1"), Err(SourceError::TooLong));
        assert_eq!(
            Source::new("EN 1990", "3.1\n(2)"),
            Err(SourceError::ControlCharacter)
        );
        assert_eq!(
            SourceError::TooLong.to_string(),
            "the source text is longer than 200 characters"
        );
    }

    #[test]
    fn sources_are_checked_when_read_from_json() {
        let value = AttributeValue::sourced(
            AttributeValue::Real(30e6),
            Source::new("EN 1992-1-1:2004", "Table 3.1").unwrap(),
        );
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<AttributeValue>(&json).unwrap(),
            value
        );
        let empty = json.replace("Table 3.1", " ");
        assert!(serde_json::from_str::<AttributeValue>(&empty).is_err());
    }
}
