use schema::{Value, ValueType};

use crate::EntityId;

/// A value held in an attribute of a stored object.
///
/// Quantities are plain reals in the SI unit of their kind (CORE-09), and enumeration values are
/// text, like schema defaults.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AttributeValue {
    /// A `bool` value.
    Bool(bool),
    /// An `integer` value.
    Integer(i64),
    /// A `real` or `quantity:<kind>` value; never NaN or infinite.
    Real(f64),
    /// A `text` or `enum:<Name>` value.
    Text(String),
    /// A `ref:<Entity>` value.
    Ref(EntityId),
    /// A `list:<value type>` value.
    List(Vec<AttributeValue>),
}

impl AttributeValue {
    /// Heap memory owned by the value, not counting the value itself.
    pub(super) fn heap_bytes(&self) -> usize {
        match self {
            Self::Text(text) => text.capacity(),
            Self::List(items) => {
                items.capacity() * size_of::<Self>()
                    + items.iter().map(Self::heap_bytes).sum::<usize>()
            }
            Self::Bool(_) | Self::Integer(_) | Self::Real(_) | Self::Ref(_) => 0,
        }
    }
}

impl From<&Value> for AttributeValue {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bool(value) => Self::Bool(*value),
            Value::Integer(value) => Self::Integer(*value),
            Value::Real(value) => Self::Real(*value),
            Value::Text(value) => Self::Text(value.clone()),
            Value::List(items) => Self::List(items.iter().map(Self::from).collect()),
        }
    }
}

/// Why a value cannot be stored in an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueError {
    /// The value does not have the attribute's value type.
    Mismatch,
    /// A real is NaN or infinite.
    NonFinite,
}

/// Checks that `value`, lists included, has the value type `value_type`.
pub(super) fn conforms(value: &AttributeValue, value_type: &ValueType) -> Result<(), ValueError> {
    match (value, value_type) {
        (AttributeValue::Real(number), ValueType::Real | ValueType::Quantity(_)) => {
            finite(*number).map(drop)
        }
        (AttributeValue::List(items), ValueType::List(inner)) => {
            items.iter().try_for_each(|item| conforms(item, inner))
        }
        (AttributeValue::Bool(_), ValueType::Bool)
        | (AttributeValue::Integer(_), ValueType::Integer)
        | (AttributeValue::Text(_), ValueType::Text | ValueType::Enum(_))
        | (AttributeValue::Ref(_), ValueType::Ref(_)) => Ok(()),
        _ => Err(ValueError::Mismatch),
    }
}

/// Rejects NaN and infinities, which compare unequal to themselves and are never valid SI values.
pub(super) fn finite(number: f64) -> Result<f64, ValueError> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ValueError::NonFinite)
    }
}

#[cfg(test)]
mod tests {
    use schema::QuantityKind;

    use super::*;

    #[test]
    fn schema_values_convert() {
        let value = Value::List(vec![
            Value::Bool(true),
            Value::Integer(3),
            Value::Real(2.5),
            Value::Text("REI 60".into()),
        ]);
        assert_eq!(
            AttributeValue::from(&value),
            AttributeValue::List(vec![
                AttributeValue::Bool(true),
                AttributeValue::Integer(3),
                AttributeValue::Real(2.5),
                AttributeValue::Text("REI 60".into()),
            ])
        );
    }

    #[test]
    fn values_conform_to_their_types() {
        let length = ValueType::Quantity(QuantityKind::Length);
        let cases = [
            (AttributeValue::Bool(true), ValueType::Bool),
            (AttributeValue::Integer(1), ValueType::Integer),
            (AttributeValue::Real(1.0), ValueType::Real),
            (AttributeValue::Real(1.0), length.clone()),
            (AttributeValue::Text("a".into()), ValueType::Text),
            (
                AttributeValue::Text("A".into()),
                ValueType::Enum("Kind".into()),
            ),
            (
                AttributeValue::Ref(EntityId::new(1)),
                ValueType::Ref("Wall".into()),
            ),
            (
                AttributeValue::List(vec![AttributeValue::Real(1.0)]),
                ValueType::List(Box::new(length)),
            ),
        ];
        for (value, value_type) in cases {
            assert_eq!(conforms(&value, &value_type), Ok(()), "{value_type}");
        }
    }

    #[test]
    fn mismatches_and_non_finite_reals_are_rejected() {
        let reals = ValueType::List(Box::new(ValueType::Real));
        assert_eq!(
            conforms(&AttributeValue::Integer(1), &ValueType::Real),
            Err(ValueError::Mismatch)
        );
        assert_eq!(
            conforms(
                &AttributeValue::List(vec![AttributeValue::Bool(true)]),
                &reals
            ),
            Err(ValueError::Mismatch)
        );
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                conforms(
                    &AttributeValue::List(vec![AttributeValue::Real(number)]),
                    &reals
                ),
                Err(ValueError::NonFinite)
            );
        }
    }

    #[test]
    fn heap_bytes_count_text_and_list_items() {
        assert_eq!(AttributeValue::Integer(1).heap_bytes(), 0);
        assert_eq!(
            AttributeValue::Text(String::with_capacity(10)).heap_bytes(),
            10
        );
        let list = AttributeValue::List(vec![AttributeValue::Text("abc".into())]);
        assert_eq!(list.heap_bytes(), size_of::<AttributeValue>() + 3);
    }
}
