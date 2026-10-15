//! Conversion between attribute values and `SQLite` values (ADR 0016).
//!
//! Scalars use `SQLite`'s own types. References are 16-byte big-endian blobs. Lists are blobs in
//! a compact encoding driven by the attribute's value type, so they carry no type tags: a `u32`
//! item count, then each item — `bool` as one byte, `integer` and `real` as 8 little-endian
//! bytes, `text` as a `u32` byte length and UTF-8, `ref` as 16 bytes, a nested list the same way.
//!
//! Everything read from a file is hostile (AGENTS.md §8): a value of the wrong type, a malformed
//! blob or trailing bytes is [`PersistError::Corrupt`], never a panic.

use core_model::{AttributeValue, EntityId};
use rusqlite::types::{Value, ValueRef};
use schema::ValueType;

use crate::PersistError;

/// The `SQLite` value storing `value`.
pub(crate) fn to_sql(value: &AttributeValue) -> Value {
    match value {
        AttributeValue::Bool(value) => Value::Integer(i64::from(*value)),
        AttributeValue::Integer(value) => Value::Integer(*value),
        AttributeValue::Real(value) => Value::Real(*value),
        AttributeValue::Text(value) => Value::Text(value.clone()),
        AttributeValue::Ref(id) => Value::Blob(id_to_sql(*id)),
        AttributeValue::List(items) => {
            let mut bytes = Vec::new();
            encode_list(items, &mut bytes);
            Value::Blob(bytes)
        }
    }
}

/// The bytes storing an object id.
pub(crate) fn id_to_sql(id: EntityId) -> Vec<u8> {
    id.get().to_be_bytes().to_vec()
}

/// Reads an object id.
pub(crate) fn id_from_sql(value: ValueRef<'_>) -> Result<EntityId, PersistError> {
    match value {
        ValueRef::Blob(bytes) => id_from_bytes(bytes),
        _ => Err(PersistError::Corrupt),
    }
}

/// Reads the value of an attribute of type `value_type`; `NULL` is an absent value.
pub(crate) fn from_sql(
    value: ValueRef<'_>,
    value_type: &ValueType,
) -> Result<Option<AttributeValue>, PersistError> {
    let value = match (value, value_type) {
        (ValueRef::Null, _) => return Ok(None),
        (ValueRef::Integer(value), ValueType::Bool) => AttributeValue::Bool(bool_from(value)?),
        (ValueRef::Integer(value), ValueType::Integer) => AttributeValue::Integer(value),
        (ValueRef::Real(value), ValueType::Real | ValueType::Quantity(_)) => {
            AttributeValue::Real(value)
        }
        (ValueRef::Text(bytes), ValueType::Text | ValueType::Enum(_)) => {
            AttributeValue::Text(text_from(bytes)?)
        }
        (ValueRef::Blob(bytes), ValueType::Ref(_)) => AttributeValue::Ref(id_from_bytes(bytes)?),
        (ValueRef::Blob(bytes), ValueType::List(items)) => {
            let mut reader = Reader(bytes);
            let list = reader.list(items)?;
            if !reader.0.is_empty() {
                return Err(PersistError::Corrupt);
            }
            list
        }
        _ => return Err(PersistError::Corrupt),
    };
    Ok(Some(value))
}

fn encode_list(items: &[AttributeValue], bytes: &mut Vec<u8>) {
    encode_len(items.len(), bytes);
    for item in items {
        match item {
            AttributeValue::Bool(value) => bytes.push(u8::from(*value)),
            AttributeValue::Integer(value) => bytes.extend(value.to_le_bytes()),
            AttributeValue::Real(value) => bytes.extend(value.to_le_bytes()),
            AttributeValue::Text(value) => {
                encode_len(value.len(), bytes);
                bytes.extend(value.as_bytes());
            }
            AttributeValue::Ref(id) => bytes.extend(id.get().to_be_bytes()),
            AttributeValue::List(items) => encode_list(items, bytes),
        }
    }
}

/// Writes a length as a little-endian `u32`. `SQLite` caps a blob at 2³¹ − 1 bytes, so any list
/// or text that can be stored fits.
fn encode_len(len: usize, bytes: &mut Vec<u8>) {
    bytes.extend(u32::try_from(len).unwrap_or(u32::MAX).to_le_bytes());
}

fn bool_from(value: i64) -> Result<bool, PersistError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(PersistError::Corrupt),
    }
}

fn text_from(bytes: &[u8]) -> Result<String, PersistError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| PersistError::Corrupt)
}

fn id_from_bytes(bytes: &[u8]) -> Result<EntityId, PersistError> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| PersistError::Corrupt)?;
    Ok(EntityId::new(u128::from_be_bytes(bytes)))
}

/// Reads list items from the front of a blob.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], PersistError> {
        let (head, rest) = self.0.split_first_chunk().ok_or(PersistError::Corrupt)?;
        self.0 = rest;
        Ok(*head)
    }

    fn take_slice(&mut self, len: usize) -> Result<&[u8], PersistError> {
        let (head, rest) = self.0.split_at_checked(len).ok_or(PersistError::Corrupt)?;
        self.0 = rest;
        Ok(head)
    }

    fn len(&mut self) -> Result<usize, PersistError> {
        usize::try_from(u32::from_le_bytes(self.take()?)).map_err(|_| PersistError::Corrupt)
    }

    /// A list of `items`. The depth is bounded by the validated value type
    /// ([`schema::MAX_LIST_DEPTH`]) and every item takes at least one byte, so a hostile count
    /// cannot make the loop outlast the blob.
    fn list(&mut self, items: &ValueType) -> Result<AttributeValue, PersistError> {
        let count = self.len()?;
        let mut list = Vec::with_capacity(count.min(self.0.len()));
        for _ in 0..count {
            list.push(self.item(items)?);
        }
        Ok(AttributeValue::List(list))
    }

    fn item(&mut self, value_type: &ValueType) -> Result<AttributeValue, PersistError> {
        Ok(match value_type {
            ValueType::Bool => {
                let [byte] = self.take()?;
                AttributeValue::Bool(bool_from(byte.into())?)
            }
            ValueType::Integer => AttributeValue::Integer(i64::from_le_bytes(self.take()?)),
            ValueType::Real | ValueType::Quantity(_) => {
                AttributeValue::Real(f64::from_le_bytes(self.take()?))
            }
            ValueType::Text | ValueType::Enum(_) => {
                let len = self.len()?;
                AttributeValue::Text(text_from(self.take_slice(len)?)?)
            }
            ValueType::Ref(_) => AttributeValue::Ref(id_from_bytes(&self.take::<16>()?)?),
            ValueType::List(items) => self.list(items)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use schema::QuantityKind;

    use super::*;

    fn list_of(value_type: ValueType) -> ValueType {
        ValueType::List(Box::new(value_type))
    }

    fn round_trip(value: &AttributeValue, value_type: &ValueType) -> Option<AttributeValue> {
        let sql = to_sql(value);
        from_sql(ValueRef::from(&sql), value_type).unwrap()
    }

    fn read(value: &Value, value_type: &ValueType) -> Result<Option<AttributeValue>, PersistError> {
        from_sql(ValueRef::from(value), value_type)
    }

    #[test]
    fn every_value_type_round_trips() {
        let id = EntityId::new(0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF);
        let length = ValueType::Quantity(QuantityKind::Length);
        let cases = [
            (AttributeValue::Bool(true), ValueType::Bool),
            (AttributeValue::Integer(-7), ValueType::Integer),
            (AttributeValue::Real(2.5), ValueType::Real),
            (AttributeValue::Real(-0.25), length.clone()),
            (AttributeValue::Text("Mur".into()), ValueType::Text),
            (
                AttributeValue::Text("A".into()),
                ValueType::Enum("Kind".into()),
            ),
            (AttributeValue::Ref(id), ValueType::Ref("Wall".into())),
            (
                AttributeValue::List(vec![
                    AttributeValue::List(vec![AttributeValue::Bool(false)]),
                    AttributeValue::List(Vec::new()),
                ]),
                list_of(list_of(ValueType::Bool)),
            ),
            (
                AttributeValue::List(vec![
                    AttributeValue::Integer(i64::MIN),
                    AttributeValue::Integer(i64::MAX),
                ]),
                list_of(ValueType::Integer),
            ),
            (
                AttributeValue::List(vec![
                    AttributeValue::Real(-0.0),
                    AttributeValue::Real(1e-300),
                ]),
                list_of(length),
            ),
            (
                AttributeValue::List(vec![
                    AttributeValue::Text(String::new()),
                    AttributeValue::Text("béton".into()),
                ]),
                list_of(ValueType::Text),
            ),
            (
                AttributeValue::List(vec![AttributeValue::Ref(id)]),
                list_of(ValueType::Ref("Wall".into())),
            ),
        ];
        for (value, value_type) in cases {
            assert_eq!(round_trip(&value, &value_type), Some(value), "{value_type}");
        }
        assert_eq!(read(&Value::Null, &ValueType::Text).unwrap(), None);
        assert_eq!(id_from_sql(ValueRef::Blob(&id_to_sql(id))).unwrap(), id);
    }

    #[test]
    fn values_of_the_wrong_shape_are_corrupt() {
        let bools = list_of(ValueType::Bool);
        let texts = list_of(ValueType::Text);
        let cases = [
            (Value::Integer(2), ValueType::Bool),
            (Value::Text("1".into()), ValueType::Integer),
            (Value::Integer(1), ValueType::Real),
            (Value::Blob(vec![0xFF]), ValueType::Text),
            (Value::Text("x".into()), ValueType::Ref("Wall".into())),
            (Value::Blob(vec![0; 15]), ValueType::Ref("Wall".into())),
            (Value::Text("x".into()), bools.clone()),
            // A count without items, an item out of range, and trailing bytes.
            (Value::Blob(vec![1, 0, 0, 0]), bools.clone()),
            (Value::Blob(vec![1, 0, 0, 0, 2]), bools.clone()),
            (Value::Blob(vec![0, 0, 0, 0, 0]), bools),
            (Value::Blob(vec![0, 0, 0]), texts.clone()),
            // A text longer than the blob, and one that is not UTF-8.
            (
                Value::Blob(vec![1, 0, 0, 0, 9, 0, 0, 0, b'a']),
                texts.clone(),
            ),
            (Value::Blob(vec![1, 0, 0, 0, 1, 0, 0, 0, 0xFF]), texts),
            // A hostile count is refused without allocating for it.
            (Value::Blob(vec![0xFF; 4]), list_of(ValueType::Integer)),
        ];
        for (value, value_type) in cases {
            let error = read(&value, &value_type).unwrap_err();
            assert!(
                matches!(error, PersistError::Corrupt),
                "{value:?} as {value_type}"
            );
        }
        assert!(matches!(
            id_from_sql(ValueRef::Integer(1)),
            Err(PersistError::Corrupt)
        ));
    }
}
