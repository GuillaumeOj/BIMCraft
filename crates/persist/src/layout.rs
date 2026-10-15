//! The tables of a namespace: one `STRICT` table per entity and one per type, one column per
//! attribute their objects store, like the store's tables (ADR 0016).

use schema::{Namespace, StoredAttribute, ValueType};

/// The table that holds the objects of one entity or one type.
#[derive(Debug)]
pub(crate) struct EntityTable<'a> {
    /// The entity or type, as the store names it (`Wall`, `WallType`).
    pub entity: &'a str,
    /// Unquoted table name, `<namespace>/<Entity>`.
    pub name: String,
    /// The attributes its objects store, in declaration order, as the store holds them.
    pub columns: Vec<Column<'a>>,
}

/// The column of one attribute.
#[derive(Debug)]
pub(crate) struct Column<'a> {
    /// The attribute name.
    pub name: &'a str,
    /// The attribute's value type, which decides the column type and how values are read.
    pub value_type: &'a ValueType,
}

impl<'a> From<&'a StoredAttribute> for Column<'a> {
    fn from(attribute: &'a StoredAttribute) -> Self {
        Self {
            name: &attribute.name,
            value_type: &attribute.value,
        }
    }
}

/// The tables of `namespace` as [`schema::stored_tables`] lays them out, sorted by name like
/// the store sorts its own.
pub(crate) fn tables(namespace: &Namespace) -> Vec<EntityTable<'_>> {
    let mut tables: Vec<_> = schema::stored_tables(namespace)
        .into_iter()
        .map(|table| {
            let columns = table
                .attributes
                .into_iter()
                .map(|attribute| Column {
                    name: &attribute.name,
                    value_type: &attribute.value,
                })
                .collect();
            EntityTable::new(&namespace.name, table.name, columns)
        })
        .collect();
    tables.sort_by(|a, b| a.entity.cmp(b.entity));
    tables
}

/// Unquoted name of the table of `entity` in `namespace`: `<namespace>/<Entity>`.
pub(crate) fn table_name(namespace: &str, entity: &str) -> String {
    format!("{namespace}/{entity}")
}

impl<'a> EntityTable<'a> {
    /// The table of `entity` in `namespace` with these columns.
    pub fn new(namespace: &str, entity: &'a str, columns: Vec<Column<'a>>) -> Self {
        Self {
            entity,
            name: table_name(namespace, entity),
            columns,
        }
    }

    /// `CREATE TABLE` statement of the table.
    pub fn create_sql(&self) -> String {
        let columns: Vec<_> =
            std::iter::once("id BLOB PRIMARY KEY NOT NULL".to_owned())
                .chain(self.columns.iter().map(|column| {
                    format!("{} {}", quote(column.name), sql_type(column.value_type))
                }))
                .collect();
        format!(
            "CREATE TABLE {} ({}) STRICT",
            quote(&self.name),
            columns.join(", ")
        )
    }

    /// `INSERT` statement taking the id then every column, in order. Columns are named, so
    /// their order in the file does not matter: migrations append the columns they add.
    pub fn insert_sql(&self) -> String {
        let placeholders = ", ?".repeat(self.columns.len());
        format!(
            "INSERT INTO {} ({}) VALUES (?{placeholders})",
            quote(&self.name),
            self.column_list()
        )
    }

    fn column_list(&self) -> String {
        let columns: Vec<_> = std::iter::once("id".to_owned())
            .chain(self.columns.iter().map(|column| quote(column.name)))
            .collect();
        columns.join(", ")
    }

    /// `SELECT` statement reading the id then every column, in order.
    pub fn select_sql(&self) -> String {
        format!("SELECT {} FROM {}", self.column_list(), quote(&self.name))
    }

    /// `DELETE` statement emptying the table.
    pub fn clear_sql(&self) -> String {
        format!("DELETE FROM {}", quote(&self.name))
    }

    /// What `pragma_table_info` reports for the table, sorted by column name: name, declared
    /// type and primary key position of each column.
    pub fn expected_columns(&self) -> Vec<(String, String, i64)> {
        let mut columns: Vec<_> = std::iter::once(("id".to_owned(), "BLOB".to_owned(), 1))
            .chain(self.columns.iter().map(|column| {
                (
                    column.name.to_owned(),
                    sql_type(column.value_type).to_owned(),
                    0,
                )
            }))
            .collect();
        columns.sort();
        columns
    }
}

/// The `STRICT` column type holding values of `value_type`.
pub(crate) fn sql_type(value_type: &ValueType) -> &'static str {
    match value_type {
        ValueType::Bool | ValueType::Integer => "INTEGER",
        ValueType::Real | ValueType::Quantity(_) => "REAL",
        ValueType::Text | ValueType::Enum(_) => "TEXT",
        ValueType::Ref(_) | ValueType::List(_) => "BLOB",
    }
}

/// Quotes an SQL identifier. Schema names are validated ASCII, but quoting keeps a hostile name
/// from ever becoming SQL.
pub(crate) fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use schema::QuantityKind;

    use super::*;
    use crate::test_support::namespace;

    #[test]
    fn one_table_per_entity_and_per_type() {
        let namespace = namespace();
        let tables = tables(&namespace);
        let names: Vec<_> = tables.iter().map(|table| table.name.as_str()).collect();
        assert_eq!(names, ["core/Storey", "core/Wall", "core/WallType"]);
        let wall_type = &tables[2];
        assert_eq!(wall_type.entity, "WallType");
        let columns: Vec<_> = wall_type.columns.iter().map(|column| column.name).collect();
        assert_eq!(columns, ["height", "layers"]);
        let wall = &tables[1];
        assert_eq!(wall.entity, "Wall");
        let columns: Vec<_> = wall.columns.iter().map(|column| column.name).collect();
        assert_eq!(
            columns,
            [
                "axis",
                "height",
                "flags",
                "layer_count",
                "name",
                "is_external",
                "tags",
                "ratio",
                "hosts",
                "counts",
                "offsets"
            ]
        );
    }

    #[test]
    fn statements_quote_every_identifier() {
        let namespace = namespace();
        let storey = &tables(&namespace)[0];
        assert_eq!(
            storey.create_sql(),
            "CREATE TABLE \"core/Storey\" (id BLOB PRIMARY KEY NOT NULL, \"elevation\" REAL) STRICT"
        );
        assert_eq!(
            storey.insert_sql(),
            "INSERT INTO \"core/Storey\" (id, \"elevation\") VALUES (?, ?)"
        );
        assert_eq!(
            storey.select_sql(),
            "SELECT id, \"elevation\" FROM \"core/Storey\""
        );
        assert_eq!(storey.clear_sql(), "DELETE FROM \"core/Storey\"");
        assert_eq!(quote("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn value_types_map_to_strict_column_types() {
        let cases = [
            (ValueType::Bool, "INTEGER"),
            (ValueType::Integer, "INTEGER"),
            (ValueType::Real, "REAL"),
            (ValueType::Quantity(QuantityKind::Length), "REAL"),
            (ValueType::Text, "TEXT"),
            (ValueType::Enum("Kind".into()), "TEXT"),
            (ValueType::Ref("Wall".into()), "BLOB"),
            (ValueType::List(Box::new(ValueType::Bool)), "BLOB"),
        ];
        for (value_type, expected) in cases {
            assert_eq!(sql_type(&value_type), expected, "{value_type}");
        }
    }
}
