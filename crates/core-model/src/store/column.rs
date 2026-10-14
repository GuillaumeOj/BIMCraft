use schema::ValueType;

use super::value::{AttributeValue, ValueError, conforms, finite};
use crate::EntityId;

/// The values of one attribute across every row of a table, in one contiguous vector of their
/// Rust type plus a presence bitmap, so a scan reads only that attribute (CORE-05).
#[derive(Clone, Debug)]
pub(super) struct Column {
    cells: Cells,
    /// The item type of a list column; the column's own value type otherwise, unused.
    items: ValueType,
}

#[derive(Clone, Debug)]
enum Cells {
    Bool(Typed<bool>),
    Integer(Typed<i64>),
    Real(Typed<f64>),
    Text(Typed<Box<str>>),
    Ref(Typed<EntityId>),
    List(Typed<Box<[AttributeValue]>>),
}

/// Runs `$body` on the typed cells of a column, whatever their Rust type.
macro_rules! each {
    ($cells:expr, $typed:ident => $body:expr) => {
        match $cells {
            Cells::Bool($typed) => $body,
            Cells::Integer($typed) => $body,
            Cells::Real($typed) => $body,
            Cells::Text($typed) => $body,
            Cells::Ref($typed) => $body,
            Cells::List($typed) => $body,
        }
    };
}

impl Column {
    /// An empty column for values of `value_type`.
    pub(super) fn new(value_type: &ValueType) -> Self {
        let cells = match value_type {
            ValueType::Bool => Cells::Bool(Typed::default()),
            ValueType::Integer => Cells::Integer(Typed::default()),
            ValueType::Real | ValueType::Quantity(_) => Cells::Real(Typed::default()),
            ValueType::Text | ValueType::Enum(_) => Cells::Text(Typed::default()),
            ValueType::Ref(_) => Cells::Ref(Typed::default()),
            ValueType::List(_) => Cells::List(Typed::default()),
        };
        let items = match value_type {
            ValueType::List(items) => (**items).clone(),
            other => other.clone(),
        };
        Self { cells, items }
    }

    /// Appends a row.
    pub(super) fn push(&mut self, value: Option<AttributeValue>) -> Result<(), ValueError> {
        let items = &self.items;
        each!(&mut self.cells, cells => cells.push(decode(value, items)?));
        Ok(())
    }

    /// Removes the last row, undoing a [`push`](Self::push).
    pub(super) fn pop(&mut self) {
        each!(&mut self.cells, cells => cells.pop());
    }

    /// The value in `row`, if any.
    pub(super) fn get(&self, row: usize) -> Option<AttributeValue> {
        each!(&self.cells, cells => cells.get(row))
    }

    /// Replaces the value in `row` and returns the previous one.
    pub(super) fn replace(
        &mut self,
        row: usize,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ValueError> {
        let items = &self.items;
        Ok(each!(&mut self.cells, cells => cells.replace(row, decode(value, items)?)))
    }

    /// Removes `row` by moving the last row into its place, and returns its value.
    pub(super) fn swap_remove(&mut self, row: usize) -> Option<AttributeValue> {
        each!(&mut self.cells, cells => cells.swap_remove(row))
    }

    /// Heap memory owned by the column.
    pub(super) fn heap_bytes(&self) -> usize {
        each!(&self.cells, cells => cells.heap_bytes())
    }
}

fn decode<T: Cell>(
    value: Option<AttributeValue>,
    items: &ValueType,
) -> Result<Option<T>, ValueError> {
    value.map(|value| T::from_value(value, items)).transpose()
}

/// A Rust type a column stores its values as.
trait Cell: Sized {
    /// The placeholder stored in rows without a value.
    fn filler() -> Self;

    /// Converts a value; `items` is the item type of a list column.
    fn from_value(value: AttributeValue, items: &ValueType) -> Result<Self, ValueError>;

    fn to_value(&self) -> AttributeValue;

    /// Converts the cell back into a value, reusing its allocation.
    fn into_value(self) -> AttributeValue {
        self.to_value()
    }

    /// Heap memory owned by the cell, not counting the cell itself.
    fn heap_bytes(&self) -> usize {
        0
    }
}

/// A `Copy` cell; `$check` accepts or rejects the unwrapped value.
macro_rules! copy_cell {
    ($type:ty, $variant:ident, $filler:expr, $check:expr) => {
        impl Cell for $type {
            fn filler() -> Self {
                $filler
            }

            fn from_value(value: AttributeValue, _: &ValueType) -> Result<Self, ValueError> {
                match value {
                    AttributeValue::$variant(value) => $check(value),
                    _ => Err(ValueError::Mismatch),
                }
            }

            fn to_value(&self) -> AttributeValue {
                AttributeValue::$variant(*self)
            }
        }
    };
}

copy_cell!(bool, Bool, false, Ok);
copy_cell!(i64, Integer, 0, Ok);
copy_cell!(f64, Real, 0.0, finite);
copy_cell!(EntityId, Ref, EntityId::new(0), Ok);

impl Cell for Box<str> {
    fn filler() -> Self {
        Self::default()
    }

    fn from_value(value: AttributeValue, _: &ValueType) -> Result<Self, ValueError> {
        match value {
            AttributeValue::Text(text) => Ok(text.into_boxed_str()),
            _ => Err(ValueError::Mismatch),
        }
    }

    fn to_value(&self) -> AttributeValue {
        AttributeValue::Text(self.to_string())
    }

    fn into_value(self) -> AttributeValue {
        AttributeValue::Text(self.into_string())
    }

    fn heap_bytes(&self) -> usize {
        self.len()
    }
}

impl Cell for Box<[AttributeValue]> {
    fn filler() -> Self {
        Self::default()
    }

    fn from_value(value: AttributeValue, items: &ValueType) -> Result<Self, ValueError> {
        let AttributeValue::List(list) = value else {
            return Err(ValueError::Mismatch);
        };
        list.iter().try_for_each(|item| conforms(item, items))?;
        Ok(list.into_boxed_slice())
    }

    fn to_value(&self) -> AttributeValue {
        AttributeValue::List(self.to_vec())
    }

    fn into_value(self) -> AttributeValue {
        AttributeValue::List(self.into_vec())
    }

    fn heap_bytes(&self) -> usize {
        size_of_val::<[AttributeValue]>(self)
            + self.iter().map(AttributeValue::heap_bytes).sum::<usize>()
    }
}

/// The dense values of one column and a bit per row telling whether the row has a value.
#[derive(Clone, Debug)]
struct Typed<T> {
    values: Vec<T>,
    present: Vec<u64>,
}

impl<T> Default for Typed<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            present: Vec::new(),
        }
    }
}

impl<T: Cell> Typed<T> {
    fn push(&mut self, value: Option<T>) {
        let row = self.values.len();
        if row.is_multiple_of(64) {
            self.present.push(0);
        }
        self.set_present(row, value.is_some());
        self.values.push(value.unwrap_or_else(T::filler));
    }

    fn pop(&mut self) {
        self.values.pop();
        self.trim_bitmap();
    }

    fn get(&self, row: usize) -> Option<AttributeValue> {
        self.is_present(row).then(|| self.values[row].to_value())
    }

    fn replace(&mut self, row: usize, value: Option<T>) -> Option<AttributeValue> {
        let was_present = self.is_present(row);
        self.set_present(row, value.is_some());
        let previous = std::mem::replace(&mut self.values[row], value.unwrap_or_else(T::filler));
        was_present.then(|| previous.into_value())
    }

    fn swap_remove(&mut self, row: usize) -> Option<AttributeValue> {
        let was_present = self.is_present(row);
        let last = self.values.len() - 1;
        self.set_present(row, self.is_present(last));
        self.set_present(last, false);
        let removed = self.values.swap_remove(row);
        self.trim_bitmap();
        was_present.then(|| removed.into_value())
    }

    fn heap_bytes(&self) -> usize {
        self.values.capacity() * size_of::<T>()
            + self.present.capacity() * size_of::<u64>()
            + self.values.iter().map(T::heap_bytes).sum::<usize>()
    }

    fn is_present(&self, row: usize) -> bool {
        self.present[row / 64] & (1 << (row % 64)) != 0
    }

    fn set_present(&mut self, row: usize, present: bool) {
        let word = &mut self.present[row / 64];
        if present {
            *word |= 1 << (row % 64);
        } else {
            *word &= !(1 << (row % 64));
        }
    }

    /// Drops bitmap words past the last row, keeping one word per started block of 64 rows.
    fn trim_bitmap(&mut self) {
        self.present.truncate(self.values.len().div_ceil(64));
    }
}

#[cfg(test)]
mod tests {
    use schema::QuantityKind;

    use super::*;

    fn filled(value_type: &ValueType, values: &[Option<AttributeValue>]) -> Column {
        let mut column = Column::new(value_type);
        for value in values {
            column.push(value.clone()).unwrap();
        }
        column
    }

    fn rows(column: &Column, count: usize) -> Vec<Option<AttributeValue>> {
        (0..count).map(|row| column.get(row)).collect()
    }

    /// One present value of each column kind, with its value type.
    fn samples() -> Vec<(ValueType, AttributeValue)> {
        vec![
            (ValueType::Bool, AttributeValue::Bool(true)),
            (ValueType::Integer, AttributeValue::Integer(-4)),
            (
                ValueType::Quantity(QuantityKind::Length),
                AttributeValue::Real(2.5),
            ),
            (ValueType::Text, AttributeValue::Text("REI 60".into())),
            (
                ValueType::Ref("Wall".into()),
                AttributeValue::Ref(EntityId::new(9)),
            ),
            (
                ValueType::List(Box::new(ValueType::Integer)),
                AttributeValue::List(vec![AttributeValue::Integer(1)]),
            ),
        ]
    }

    #[test]
    fn every_kind_stores_reads_and_replaces_values() {
        for (value_type, value) in samples() {
            let mut column = filled(&value_type, &[Some(value.clone()), None]);
            assert_eq!(rows(&column, 2), [Some(value.clone()), None]);
            assert_eq!(column.replace(1, Some(value.clone())), Ok(None));
            assert_eq!(column.replace(0, None), Ok(Some(value.clone())));
            assert_eq!(rows(&column, 2), [None, Some(value)]);
        }
    }

    #[test]
    fn every_kind_rejects_other_kinds() {
        for (value_type, _) in samples() {
            for (other_type, other) in samples() {
                if other_type != value_type {
                    let mut column = Column::new(&value_type);
                    assert_eq!(column.push(Some(other.clone())), Err(ValueError::Mismatch));
                    assert_eq!(column.heap_bytes(), 0, "{value_type} unchanged");
                }
            }
        }
    }

    #[test]
    fn reals_must_be_finite() {
        let mut column = filled(&ValueType::Real, &[None]);
        assert_eq!(
            column.push(Some(AttributeValue::Real(f64::NAN))),
            Err(ValueError::NonFinite)
        );
        assert_eq!(
            column.replace(0, Some(AttributeValue::Real(f64::INFINITY))),
            Err(ValueError::NonFinite)
        );
        assert_eq!(column.get(0), None);
    }

    #[test]
    fn list_items_are_checked() {
        let mut column = Column::new(&ValueType::List(Box::new(ValueType::Integer)));
        let wrong =
            AttributeValue::List(vec![AttributeValue::Integer(1), AttributeValue::Bool(true)]);
        assert_eq!(column.push(Some(wrong)), Err(ValueError::Mismatch));
    }

    #[test]
    fn enumerations_are_text() {
        let column = filled(
            &ValueType::Enum("Kind".into()),
            &[Some(AttributeValue::Text("A".into()))],
        );
        assert_eq!(column.get(0), Some(AttributeValue::Text("A".into())));
    }

    #[test]
    fn swap_remove_moves_the_last_row_across_bitmap_words() {
        // 130 rows span three bitmap words; even rows have a value.
        let values: Vec<_> = (0..130)
            .map(|row| (row % 2 == 0).then_some(AttributeValue::Integer(row)))
            .collect();
        let mut column = filled(&ValueType::Integer, &values);
        let mut expected = values;

        for row in [3, 0, 64, 126] {
            assert_eq!(column.swap_remove(row), expected.swap_remove(row));
            assert_eq!(rows(&column, expected.len()), expected);
        }
        // Down to 64 rows, the third bitmap word is gone.
        while expected.len() > 64 {
            assert_eq!(column.swap_remove(0), expected.swap_remove(0));
        }
        assert_eq!(each!(&column.cells, cells => cells.present.len()), 1);
        assert_eq!(rows(&column, 64), expected);
    }

    #[test]
    fn pop_undoes_push() {
        let mut column = filled(&ValueType::Integer, &[Some(AttributeValue::Integer(1))]);
        column.push(Some(AttributeValue::Integer(2))).unwrap();
        column.pop();
        column.push(None).unwrap();
        assert_eq!(rows(&column, 2), [Some(AttributeValue::Integer(1)), None]);
    }

    #[test]
    fn heap_bytes_count_values_bitmap_and_owned_data() {
        let one = |value_type: &ValueType, value: AttributeValue| {
            filled(value_type, &[Some(value)]).heap_bytes()
        };
        assert_eq!(Column::new(&ValueType::Text).heap_bytes(), 0);
        assert!(filled(&ValueType::Integer, &[None]).heap_bytes() > 0);

        let text = |value: &str| AttributeValue::Text(value.into());
        assert_eq!(
            one(&ValueType::Text, text("abc")) - one(&ValueType::Text, text("")),
            3
        );
        let texts = ValueType::List(Box::new(ValueType::Text));
        assert_eq!(
            one(&texts, AttributeValue::List(vec![text("ab")]))
                - one(&texts, AttributeValue::List(Vec::new())),
            size_of::<AttributeValue>() + 2
        );
    }
}
