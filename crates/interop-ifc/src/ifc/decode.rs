//! What the generated entity classes decode STEP values with.
//!
//! Each generated type implements [`Decode`] in a few calls to these helpers, so the logic lives
//! here once and the generated code stays small. Decoding checks what the schema states about
//! each value: its kind, whether it may be absent or derived, aggregate sizes, enumeration items
//! and select members. It does not evaluate `WHERE` rules or resolve references.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

pub use crate::step::{InstanceBody, Limits, Parameter, Record, StepReader};

/// Why a record could not be decoded into an entity.
///
/// Messages never quote the file content: it can hold personal data (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The record names no entity of the schema, or an abstract one.
    #[error("unknown entity")]
    UnknownEntity,
    /// A complex instance, made of several records; these are not decoded yet.
    #[error("complex instances are not supported")]
    ComplexInstance,
    /// The record has the wrong number of attributes.
    #[error("expected {expected} attributes, found {found}")]
    WrongArity {
        /// How many the entity has.
        expected: usize,
        /// How many the record has.
        found: usize,
    },
    /// A value where the entity derives the attribute, which must be written `*`.
    #[error("attribute {index} is derived and must be `*`")]
    NotDerived {
        /// The 0-based attribute position.
        index: usize,
    },
    /// An attribute whose value does not match its type.
    #[error("attribute {index}: {error}")]
    Attribute {
        /// The 0-based attribute position.
        index: usize,
        /// What is wrong with the value.
        error: ValueError,
    },
}

/// Why a value does not match its type.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    /// `$` where a value is required.
    #[error("missing value")]
    Missing,
    /// `*` where a value is required.
    #[error("unexpected `*`")]
    Derived,
    /// A value of another kind than the type requires.
    #[error("expected {expected}, found {found}")]
    WrongKind {
        /// What the type requires.
        expected: &'static str,
        /// What the value is.
        found: &'static str,
    },
    /// An enumeration value that is not an item of the enumeration.
    #[error("not an item of the enumeration")]
    UnknownItem,
    /// A typed value whose type is not a member of the select.
    #[error("not a member of the select")]
    NotInSelect,
    /// An aggregate with too few or too many elements.
    #[error("expected {}, found {found}", expected_count(*min, *max))]
    WrongCount {
        /// The fewest elements allowed.
        min: usize,
        /// The most elements allowed, if bounded.
        max: Option<usize>,
        /// How many there are.
        found: usize,
    },
}

/// "1 to 2 elements", or "at least 2 elements" when unbounded.
fn expected_count(min: usize, max: Option<usize>) -> String {
    match max {
        Some(max) => format!("{min} to {max} elements"),
        None => format!("at least {min} elements"),
    }
}

/// A value that can be read from a STEP parameter.
pub trait Decode: Sized {
    /// Reads `parameter` as a value of this type.
    ///
    /// # Errors
    ///
    /// Returns a [`ValueError`] when the parameter does not hold a value of this type.
    fn decode(parameter: &Parameter) -> Result<Self, ValueError>;
}

/// Reads `parameter` as a `T`; the decoder of every non-aggregate type.
///
/// # Errors
///
/// Returns a [`ValueError`] when the parameter does not hold a `T`.
pub fn decode<T: Decode>(parameter: &Parameter) -> Result<T, ValueError> {
    T::decode(parameter)
}

/// The error for a parameter that is not of the `expected` kind. It describes what was found
/// without quoting it.
fn wrong_kind(expected: &'static str, parameter: &Parameter) -> ValueError {
    let found = match parameter {
        Parameter::Null => return ValueError::Missing,
        Parameter::Derived => return ValueError::Derived,
        Parameter::Integer(_) => "an integer",
        Parameter::Real(_) => "a real",
        Parameter::String(_) => "a string",
        Parameter::Enumeration(_) => "an enumeration",
        Parameter::Binary(_) => "a binary",
        Parameter::Reference(_) => "a reference",
        Parameter::Typed { .. } => "a typed value",
        Parameter::List(_) => "a list",
    };
    ValueError::WrongKind { expected, found }
}

impl Decode for i64 {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::Integer(value) => Ok(*value),
            _ => Err(wrong_kind("an integer", parameter)),
        }
    }
}

impl Decode for f64 {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::Real(value) => Ok(*value),
            _ => Err(wrong_kind("a real", parameter)),
        }
    }
}

impl Decode for String {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::String(value) => Ok(value.clone()),
            _ => Err(wrong_kind("a string", parameter)),
        }
    }
}

impl Decode for bool {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        enumeration(parameter, &[("T", true), ("F", false)])
    }
}

/// An EXPRESS `NUMBER`: an integer or a real, kept as written so it can be written back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    /// Written as an integer.
    Integer(i64),
    /// Written as a real.
    Real(f64),
}

impl Decode for Number {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::Integer(value) => Ok(Self::Integer(*value)),
            Parameter::Real(value) => Ok(Self::Real(*value)),
            _ => Err(wrong_kind("a number", parameter)),
        }
    }
}

/// An EXPRESS `LOGICAL`: true, false or unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Logical {
    /// `.T.`
    True,
    /// `.F.`
    False,
    /// `.U.`
    Unknown,
}

impl Decode for Logical {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        enumeration(
            parameter,
            &[("T", Self::True), ("F", Self::False), ("U", Self::Unknown)],
        )
    }
}

/// An EXPRESS `BINARY` value, as written: hexadecimal digits, the first giving the number of
/// unused bits in the last digit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binary(pub String);

impl Decode for Binary {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::Binary(value) => Ok(Self(value.clone())),
            _ => Err(wrong_kind("a binary", parameter)),
        }
    }
}

/// A reference to an instance of `T`, or of one of its subtypes, by its instance name.
///
/// References are not resolved when decoding: a file streams one instance at a time, and the
/// one referred to may come later.
pub struct Ref<T> {
    id: u64,
    entity: PhantomData<fn() -> T>,
}

impl<T> Ref<T> {
    /// A reference to the instance `#id`.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self {
            id,
            entity: PhantomData,
        }
    }

    /// The instance name: `7` for `#7`.
    #[must_use]
    pub const fn id(self) -> u64 {
        self.id
    }
}

// Written out because derives would require `T` itself to implement each trait.
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Ref<T> {}

impl<T> PartialEq for Ref<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Ref<T> {}

impl<T> Hash for Ref<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.id)
    }
}

impl<T> Decode for Ref<T> {
    fn decode(parameter: &Parameter) -> Result<Self, ValueError> {
        match parameter {
            Parameter::Reference(id) => Ok(Self::new(*id)),
            _ => Err(wrong_kind("a reference", parameter)),
        }
    }
}

/// Reads an enumeration value, given each item's STEP spelling, such as `NOTDEFINED`.
///
/// # Errors
///
/// Returns a [`ValueError`] when the parameter is not an enumeration value, or not one of
/// `items`.
pub fn enumeration<T: Copy>(parameter: &Parameter, items: &[(&str, T)]) -> Result<T, ValueError> {
    let Parameter::Enumeration(value) = parameter else {
        return Err(wrong_kind("an enumeration", parameter));
    };
    items
        .iter()
        .find(|(name, _)| name == value)
        .map(|&(_, item)| item)
        .ok_or(ValueError::UnknownItem)
}

/// The value of a select: a reference to an entity instance, or a value tagged with its type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selected<'a> {
    /// `#id`
    Instance(u64),
    /// `TYPENAME(value)`, with the type name in upper case.
    Typed(&'a str, &'a Parameter),
}

/// Reads the value of a select, for the generated code to match against its members.
///
/// # Errors
///
/// Returns a [`ValueError`] when the parameter is neither a reference nor a typed value.
pub fn select(parameter: &Parameter) -> Result<Selected<'_>, ValueError> {
    match parameter {
        Parameter::Reference(id) => Ok(Selected::Instance(*id)),
        Parameter::Typed { type_name, value } => Ok(Selected::Typed(type_name, value)),
        _ => Err(wrong_kind("a reference or a typed value", parameter)),
    }
}

/// A decoder for an aggregate of `min` to `max` elements, each read by `element`.
pub fn aggregate<T>(
    min: usize,
    max: Option<usize>,
    element: impl Fn(&Parameter) -> Result<T, ValueError>,
) -> impl Fn(&Parameter) -> Result<Vec<T>, ValueError> {
    move |parameter| {
        let Parameter::List(items) = parameter else {
            return Err(wrong_kind("a list", parameter));
        };
        let found = items.len();
        if found < min || max.is_some_and(|max| found > max) {
            return Err(ValueError::WrongCount { min, max, found });
        }
        let mut values = Vec::with_capacity(found);
        for item in items {
            values.push(element(item)?);
        }
        Ok(values)
    }
}

/// A decoder for the elements of an `ARRAY OF OPTIONAL`, which may be `$`.
pub fn nullable<T>(
    element: impl Fn(&Parameter) -> Result<T, ValueError>,
) -> impl Fn(&Parameter) -> Result<Option<T>, ValueError> {
    move |parameter| match parameter {
        Parameter::Null => Ok(None),
        _ => element(parameter).map(Some),
    }
}

/// Reads the attributes of a record in order, skipping derived ones.
pub struct Attributes<'a> {
    parameters: &'a [Parameter],
    derived: &'static [usize],
    next: usize,
}

impl<'a> Attributes<'a> {
    /// Checks that `record` has `count` attributes and `*` at every position in `derived`.
    ///
    /// # Errors
    ///
    /// Returns a [`DecodeError`] when it does not.
    pub fn new(
        record: &'a Record,
        count: usize,
        derived: &'static [usize],
    ) -> Result<Self, DecodeError> {
        let parameters = record.parameters.as_slice();
        if parameters.len() != count {
            return Err(DecodeError::WrongArity {
                expected: count,
                found: parameters.len(),
            });
        }
        if let Some(&index) = derived
            .iter()
            .find(|&&index| parameters[index] != Parameter::Derived)
        {
            return Err(DecodeError::NotDerived { index });
        }
        Ok(Self {
            parameters,
            derived,
            next: 0,
        })
    }

    /// The next attribute that is not derived, with its position.
    fn next_parameter(&mut self) -> (usize, &'a Parameter) {
        while self.derived.contains(&self.next) {
            self.next += 1;
        }
        let index = self.next;
        self.next += 1;
        (index, &self.parameters[index])
    }

    /// Reads the next attribute, which must have a value.
    ///
    /// # Errors
    ///
    /// Returns a [`DecodeError`] when the value is absent or does not match its type.
    pub fn required<T>(
        &mut self,
        decode: impl Fn(&Parameter) -> Result<T, ValueError>,
    ) -> Result<T, DecodeError> {
        let (index, parameter) = self.next_parameter();
        decode(parameter).map_err(|error| DecodeError::Attribute { index, error })
    }

    /// Reads the next attribute, which may be `$`.
    ///
    /// # Errors
    ///
    /// Returns a [`DecodeError`] when the value does not match its type.
    pub fn optional<T>(
        &mut self,
        decode: impl Fn(&Parameter) -> Result<T, ValueError>,
    ) -> Result<Option<T>, DecodeError> {
        self.required(nullable(decode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::test_support::{record, typed};

    fn enumeration_value(value: &str) -> Parameter {
        Parameter::Enumeration(value.into())
    }

    #[test]
    fn decodes_simple_values() {
        assert_eq!(decode::<i64>(&Parameter::Integer(3)), Ok(3));
        assert_eq!(decode::<f64>(&Parameter::Real(1.5)), Ok(1.5));
        assert_eq!(
            decode::<String>(&Parameter::String("a".into())),
            Ok("a".into())
        );
        assert_eq!(decode::<bool>(&enumeration_value("T")), Ok(true));
        assert_eq!(decode::<bool>(&enumeration_value("F")), Ok(false));
        assert_eq!(
            decode::<bool>(&enumeration_value("U")),
            Err(ValueError::UnknownItem)
        );
        assert_eq!(
            decode::<Logical>(&enumeration_value("U")),
            Ok(Logical::Unknown)
        );
        assert_eq!(
            decode::<Number>(&Parameter::Integer(2)),
            Ok(Number::Integer(2))
        );
        assert_eq!(
            decode::<Number>(&Parameter::Real(2.0)),
            Ok(Number::Real(2.0))
        );
        assert_eq!(
            decode::<Binary>(&Parameter::Binary("0F".into())),
            Ok(Binary("0F".into()))
        );
        assert_eq!(
            decode::<Ref<String>>(&Parameter::Reference(7)),
            Ok(Ref::new(7))
        );
    }

    #[test]
    fn rejects_values_of_the_wrong_kind() {
        let wrong = |expected, found| ValueError::WrongKind { expected, found };
        let integer = Parameter::Integer(1);
        assert_eq!(
            decode::<i64>(&Parameter::Real(1.0)),
            Err(wrong("an integer", "a real"))
        );
        assert_eq!(decode::<f64>(&integer), Err(wrong("a real", "an integer")));
        assert_eq!(
            decode::<String>(&integer),
            Err(wrong("a string", "an integer"))
        );
        assert_eq!(
            decode::<bool>(&integer),
            Err(wrong("an enumeration", "an integer"))
        );
        assert_eq!(
            decode::<Number>(&Parameter::String(String::new())),
            Err(wrong("a number", "a string"))
        );
        assert_eq!(
            decode::<Binary>(&enumeration_value("T")),
            Err(wrong("a binary", "an enumeration"))
        );
        assert_eq!(
            decode::<Ref<i64>>(&Parameter::Binary("0".into())),
            Err(wrong("a reference", "a binary"))
        );
        assert_eq!(
            decode::<i64>(&Parameter::Reference(1)),
            Err(wrong("an integer", "a reference"))
        );
        assert_eq!(
            decode::<i64>(&typed("A", integer.clone())),
            Err(wrong("an integer", "a typed value"))
        );
        assert_eq!(
            decode::<i64>(&Parameter::List(Vec::new())),
            Err(wrong("an integer", "a list"))
        );
        assert_eq!(decode::<i64>(&Parameter::Null), Err(ValueError::Missing));
        assert_eq!(decode::<i64>(&Parameter::Derived), Err(ValueError::Derived));
    }

    #[test]
    fn references_compare_and_print_by_id() {
        let a: Ref<String> = Ref::new(4);
        let b = a;
        assert_eq!(a.clone(), b);
        assert_eq!(a.id(), 4);
        assert_eq!(format!("{a:?}"), "#4");
        let mut set = std::collections::HashSet::new();
        set.insert(a);
        assert!(set.contains(&Ref::new(4)));
    }

    #[test]
    fn reads_selects() {
        assert_eq!(select(&Parameter::Reference(3)), Ok(Selected::Instance(3)));
        let value = typed("IFCLABEL", Parameter::String("a".into()));
        assert_eq!(
            select(&value),
            Ok(Selected::Typed("IFCLABEL", &Parameter::String("a".into())))
        );
        assert_eq!(
            select(&Parameter::Integer(1)),
            Err(ValueError::WrongKind {
                expected: "a reference or a typed value",
                found: "an integer"
            })
        );
    }

    #[test]
    fn reads_aggregates() {
        let list = |values: &[i64]| {
            Parameter::List(values.iter().copied().map(Parameter::Integer).collect())
        };
        let bounded = aggregate(1, Some(2), decode::<i64>);
        assert_eq!(bounded(&list(&[1, 2])), Ok(vec![1, 2]));
        assert_eq!(
            bounded(&list(&[])),
            Err(ValueError::WrongCount {
                min: 1,
                max: Some(2),
                found: 0
            })
        );
        assert_eq!(
            bounded(&list(&[1, 2, 3])),
            Err(ValueError::WrongCount {
                min: 1,
                max: Some(2),
                found: 3
            })
        );
        assert_eq!(
            bounded(&Parameter::Integer(1)),
            Err(ValueError::WrongKind {
                expected: "a list",
                found: "an integer"
            })
        );
        let unbounded = aggregate(0, None, nullable(decode::<i64>));
        let mixed = Parameter::List(vec![Parameter::Null, Parameter::Integer(4)]);
        assert_eq!(unbounded(&mixed), Ok(vec![None, Some(4)]));
        let strings = Parameter::List(vec![Parameter::String("a".into())]);
        assert_eq!(
            aggregate(0, None, decode::<i64>)(&strings),
            Err(ValueError::WrongKind {
                expected: "an integer",
                found: "a string"
            })
        );
    }

    #[test]
    fn reads_attributes_in_order() {
        let record = record(
            "X",
            vec![
                Parameter::Integer(1),
                Parameter::Derived,
                Parameter::Null,
                Parameter::Real(2.0),
            ],
        );
        let mut attributes = Attributes::new(&record, 4, &[1]).unwrap();
        assert_eq!(attributes.required(decode::<i64>), Ok(1));
        assert_eq!(attributes.optional(decode::<f64>), Ok(None));
        assert_eq!(attributes.optional(decode::<f64>), Ok(Some(2.0)));
    }

    #[test]
    fn rejects_mismatched_records() {
        let record = record("X", vec![Parameter::Null, Parameter::Integer(1)]);
        assert!(matches!(
            Attributes::new(&record, 3, &[]),
            Err(DecodeError::WrongArity {
                expected: 3,
                found: 2
            })
        ));
        assert!(matches!(
            Attributes::new(&record, 2, &[1]),
            Err(DecodeError::NotDerived { index: 1 })
        ));
        let mut attributes = Attributes::new(&record, 2, &[]).unwrap();
        assert_eq!(
            attributes.required(decode::<i64>),
            Err(DecodeError::Attribute {
                index: 0,
                error: ValueError::Missing
            })
        );
        assert_eq!(
            attributes.optional(decode::<String>),
            Err(DecodeError::Attribute {
                index: 1,
                error: ValueError::WrongKind {
                    expected: "a string",
                    found: "an integer"
                }
            })
        );
    }

    #[test]
    fn errors_describe_themselves_without_content() {
        let cases = [
            (DecodeError::UnknownEntity, "unknown entity"),
            (
                DecodeError::ComplexInstance,
                "complex instances are not supported",
            ),
            (
                DecodeError::WrongArity {
                    expected: 2,
                    found: 3,
                },
                "expected 2 attributes, found 3",
            ),
            (
                DecodeError::NotDerived { index: 1 },
                "attribute 1 is derived and must be `*`",
            ),
            (
                DecodeError::Attribute {
                    index: 0,
                    error: ValueError::Missing,
                },
                "attribute 0: missing value",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
        let cases = [
            (ValueError::Derived, "unexpected `*`"),
            (ValueError::UnknownItem, "not an item of the enumeration"),
            (ValueError::NotInSelect, "not a member of the select"),
            (
                ValueError::WrongCount {
                    min: 1,
                    max: Some(2),
                    found: 0,
                },
                "expected 1 to 2 elements, found 0",
            ),
            (
                ValueError::WrongCount {
                    min: 2,
                    max: None,
                    found: 1,
                },
                "expected at least 2 elements, found 1",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}
