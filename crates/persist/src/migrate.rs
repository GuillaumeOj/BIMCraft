//! Brings the entity tables of a file from an earlier schema version of a namespace to the
//! current one (PERS-01, ADR 0002, ADR 0016).
//!
//! Each [`Step`] of a resolved migration becomes table changes: renames are `ALTER TABLE`,
//! additions create tables or columns (filled with the default), removals drop them, value
//! changes rewrite a column, and custom steps call code the host provides.

use core_model::AttributeValue;
use rusqlite::ErrorCode;
use rusqlite::types::Value as SqlValue;
use schema::{Step, ValueType};

use crate::layout::{Column, EntityTable, quote, sql_type, table_name};
use crate::{PersistError, Schema, Transaction, codec};

/// Runs every migration from version `from` of the namespace to its current version.
///
/// A step the file does not fit (a missing table or column) means the file does not hold the
/// version it claims: [`PersistError::Corrupt`].
pub(crate) fn migrate(
    tx: &Transaction<'_>,
    schema: &Schema<'_>,
    from: u32,
) -> Result<(), PersistError> {
    let namespace = &schema.namespace.name;
    for version in from + 1..=schema.namespace.version {
        let migration = schema
            .migrations
            .iter()
            .find(|migration| migration.version == version)
            .ok_or_else(|| PersistError::MissingMigration {
                namespace: namespace.clone(),
                version,
            })?;
        for step in &migration.steps {
            apply(tx, schema, step).map_err(not_fitting_is_corrupt)?;
        }
    }
    tx.execute(
        "UPDATE schema_namespace SET version = ?1 WHERE name = ?2",
        (schema.namespace.version, namespace),
    )?;
    Ok(())
}

/// A missing table or column is `SQLITE_ERROR`: the file does not hold the version it claims.
/// Other failures, such as a full disk, keep their own error.
fn not_fitting_is_corrupt(error: PersistError) -> PersistError {
    match error {
        PersistError::Database(error) if error.sqlite_error_code() == Some(ErrorCode::Unknown) => {
            PersistError::Corrupt
        }
        other => other,
    }
}

fn apply(tx: &Transaction<'_>, schema: &Schema<'_>, step: &Step) -> Result<(), PersistError> {
    let namespace = &schema.namespace.name;
    let table = |entity: &str| quote(&table_name(namespace, entity));
    match step {
        Step::RenameEntity { from, to } => {
            tx.execute(
                &format!("ALTER TABLE {} RENAME TO {}", table(from), table(to)),
                [],
            )?;
        }
        Step::RenameAttribute { entity, from, to } => rename_column(tx, &table(entity), from, to)?,
        Step::AddEntity { entity, attributes } => {
            let columns = attributes.iter().map(Column::from).collect();
            tx.execute(
                &EntityTable::new(namespace, entity, columns).create_sql(),
                [],
            )?;
        }
        Step::AddAttribute {
            entity,
            attribute,
            default,
        } => {
            add_column(tx, &table(entity), &attribute.name, &attribute.value)?;
            if let Some(default) = default {
                let sql = format!(
                    "UPDATE {} SET {} = ?1",
                    table(entity),
                    quote(&attribute.name)
                );
                tx.execute(&sql, [codec::to_sql(&default.into())])?;
            }
        }
        Step::ChangeValue {
            entity,
            attribute,
            from,
            to,
            factor,
        } => change_value(tx, &table(entity), attribute, from, to, *factor)?,
        Step::SetValue {
            entity,
            attribute,
            value,
        } => {
            let column = quote(attribute);
            let sql = format!(
                "UPDATE {} SET {column} = ?1 WHERE {column} IS NULL",
                table(entity)
            );
            tx.execute(&sql, [codec::to_sql(&value.into())])?;
        }
        Step::Custom { function, .. } => {
            let (_, run) = schema
                .custom_steps
                .iter()
                .find(|(name, _)| name == function)
                .ok_or_else(|| PersistError::UnknownCustomStep(function.clone()))?;
            run(tx)?;
        }
        Step::RemoveEntity { entity } => {
            tx.execute(&format!("DROP TABLE {}", table(entity)), [])?;
        }
        Step::RemoveAttribute { entity, attribute } => drop_column(tx, &table(entity), attribute)?,
    }
    Ok(())
}

fn rename_column(
    tx: &Transaction<'_>,
    table: &str,
    from: &str,
    to: &str,
) -> Result<(), PersistError> {
    let sql = format!(
        "ALTER TABLE {table} RENAME COLUMN {} TO {}",
        quote(from),
        quote(to)
    );
    tx.execute(&sql, [])?;
    Ok(())
}

fn drop_column(tx: &Transaction<'_>, table: &str, name: &str) -> Result<(), PersistError> {
    tx.execute(
        &format!("ALTER TABLE {table} DROP COLUMN {}", quote(name)),
        [],
    )?;
    Ok(())
}

fn add_column(
    tx: &Transaction<'_>,
    table: &str,
    name: &str,
    value_type: &ValueType,
) -> Result<(), PersistError> {
    let sql = format!(
        "ALTER TABLE {table} ADD COLUMN {} {}",
        quote(name),
        sql_type(value_type)
    );
    tx.execute(&sql, [])?;
    Ok(())
}

/// Rewrites a column with its values converted from `from` to `to`, in a new column that then
/// takes the old one's place, since the column type may change.
fn change_value(
    tx: &Transaction<'_>,
    table: &str,
    attribute: &str,
    from: &ValueType,
    to: &ValueType,
    factor: f64,
) -> Result<(), PersistError> {
    // `~` never appears in a schema name, so the temporary column cannot clash.
    let temporary = format!("{attribute}~");
    add_column(tx, table, &temporary, to)?;
    if matches!(from, ValueType::List(_)) {
        convert_rows(tx, table, attribute, &temporary, (from, to), factor)?;
    } else {
        convert_scalars(tx, table, attribute, &temporary, (from, to), factor)?;
    }
    drop_column(tx, table, attribute)?;
    rename_column(tx, table, &temporary, attribute)
}

/// Converts a scalar column in one statement: reals are multiplied by `factor`, other values
/// copied. Values first must have the storage class of `from`, as a malformed file could hold
/// text where a number belongs, which SQL arithmetic would silently turn into 0.
fn convert_scalars(
    tx: &Transaction<'_>,
    table: &str,
    attribute: &str,
    temporary: &str,
    (from, to): (&ValueType, &ValueType),
    factor: f64,
) -> Result<(), PersistError> {
    let column = quote(attribute);
    // `typeof` names the storage classes of the STRICT column types, in lower case.
    let class = sql_type(from).to_ascii_lowercase();
    let malformed: bool = tx.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE {column} IS NOT NULL AND typeof({column}) <> ?1)"
        ),
        [&class],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(PersistError::Corrupt);
    }
    let set = format!("UPDATE {table} SET {} = {column}", quote(temporary));
    if matches!(to, ValueType::Real | ValueType::Quantity(_)) {
        tx.execute(&format!("{set} * ?1"), [factor])?;
    } else {
        tx.execute(&set, [])?;
    }
    Ok(())
}

/// Converts a list column row by row, since each blob must be decoded.
fn convert_rows(
    tx: &Transaction<'_>,
    table: &str,
    attribute: &str,
    temporary: &str,
    (from, to): (&ValueType, &ValueType),
    factor: f64,
) -> Result<(), PersistError> {
    let rows = {
        let column = quote(attribute);
        let mut select = tx.prepare(&format!(
            "SELECT id, {column} FROM {table} WHERE {column} IS NOT NULL"
        ))?;
        let mut rows = select.query([])?;
        let mut converted = Vec::new();
        while let Some(row) = rows.next()? {
            let id: Vec<u8> = row.get(0)?;
            let value = codec::from_sql(row.get_ref(1)?, from)?
                .map(|value| convert(value, to, factor))
                .transpose()?;
            converted.push((
                id,
                value.map_or(SqlValue::Null, |value| codec::to_sql(&value)),
            ));
        }
        converted
    };
    let mut update = tx.prepare(&format!(
        "UPDATE {table} SET {} = ?1 WHERE id = ?2",
        quote(temporary)
    ))?;
    for (id, value) in rows {
        update.execute((value, id))?;
    }
    Ok(())
}

/// Converts a value as [`schema::converts`] allows: numbers to reals multiplied by `factor`,
/// text to text, references kept, lists item by item.
fn convert(
    value: AttributeValue,
    to: &ValueType,
    factor: f64,
) -> Result<AttributeValue, PersistError> {
    Ok(match (value, to) {
        // Integers fit a real exactly up to 2^53; larger counts lose precision like any real.
        #[allow(clippy::cast_precision_loss)]
        (AttributeValue::Integer(number), ValueType::Real | ValueType::Quantity(_)) => {
            AttributeValue::Real(number as f64 * factor)
        }
        (AttributeValue::Real(number), ValueType::Real | ValueType::Quantity(_)) => {
            AttributeValue::Real(number * factor)
        }
        (AttributeValue::List(items), ValueType::List(inner)) => AttributeValue::List(
            items
                .into_iter()
                .map(|item| convert(item, inner, factor))
                .collect::<Result<_, _>>()?,
        ),
        (value @ AttributeValue::Bool(_), ValueType::Bool)
        | (value @ AttributeValue::Integer(_), ValueType::Integer)
        | (value @ AttributeValue::Text(_), ValueType::Text | ValueType::Enum(_))
        | (value @ AttributeValue::Ref(_), ValueType::Ref(_)) => value,
        _ => return Err(PersistError::Corrupt),
    })
}

#[cfg(test)]
mod tests {
    use core_model::EntityId;
    use schema::{Migration, StoredAttribute, Value};

    use super::*;
    use crate::test_support::namespace;
    use crate::{Connection, open_project};

    fn project() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        open_project(&mut conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE \"core/Wall\" (id BLOB PRIMARY KEY NOT NULL, \"width\" INTEGER) STRICT;
             INSERT INTO \"core/Wall\" VALUES (x'00000000000000000000000000000001', 250);
             INSERT INTO schema_namespace VALUES ('core', 0);",
        )
        .unwrap();
        conn
    }

    fn run(
        conn: &mut Connection,
        steps: Vec<Step>,
        custom: &[(&str, crate::CustomStep)],
    ) -> Result<(), PersistError> {
        let namespace = namespace();
        let migrations = [Migration { version: 1, steps }];
        let schema = Schema {
            namespace: &namespace,
            migrations: &migrations,
            custom_steps: custom,
        };
        let tx = conn.transaction()?;
        migrate(&tx, &schema, 0)?;
        tx.commit()?;
        Ok(())
    }

    fn width(conn: &Connection) -> SqlValue {
        conn.query_row("SELECT \"width\" FROM \"core/Wall\"", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn values_change_type_and_scale() {
        let mut conn = project();
        let step = Step::ChangeValue {
            entity: "Wall".into(),
            attribute: "width".into(),
            from: ValueType::Integer,
            to: "quantity:length".parse().unwrap(),
            factor: 0.5,
        };
        run(&mut conn, vec![step], &[]).unwrap();
        assert_eq!(width(&conn), SqlValue::Real(125.0));
        let version: u32 = conn
            .query_row("SELECT version FROM schema_namespace", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn missing_values_are_set_and_custom_code_runs() {
        fn double(tx: &Transaction<'_>) -> Result<(), PersistError> {
            tx.execute("UPDATE \"core/Wall\" SET \"width\" = \"width\" * 2", [])?;
            Ok(())
        }
        let mut conn = project();
        conn.execute("UPDATE \"core/Wall\" SET \"width\" = NULL", [])
            .unwrap();
        let steps = vec![
            Step::SetValue {
                entity: "Wall".into(),
                attribute: "width".into(),
                value: Value::Integer(100),
            },
            Step::Custom {
                entities: vec!["Wall".into()],
                function: "double".into(),
            },
        ];
        run(&mut conn, steps, &[("double", double)]).unwrap();
        assert_eq!(width(&conn), SqlValue::Integer(200));
    }

    #[test]
    fn structure_steps_rename_add_and_remove() {
        let mut conn = project();
        let steps = vec![
            Step::RenameEntity {
                from: "Wall".into(),
                to: "Partition".into(),
            },
            Step::RenameAttribute {
                entity: "Partition".into(),
                from: "width".into(),
                to: "thickness".into(),
            },
            Step::AddAttribute {
                entity: "Partition".into(),
                attribute: StoredAttribute {
                    name: "height".into(),
                    value: ValueType::Real,
                },
                default: Some(Value::Real(2.5)),
            },
            Step::AddAttribute {
                entity: "Partition".into(),
                attribute: StoredAttribute {
                    name: "name".into(),
                    value: ValueType::Text,
                },
                default: None,
            },
            Step::RemoveAttribute {
                entity: "Partition".into(),
                attribute: "thickness".into(),
            },
            Step::AddEntity {
                entity: "Slab".into(),
                attributes: vec![StoredAttribute {
                    name: "depth".into(),
                    value: ValueType::Real,
                }],
            },
            Step::RemoveEntity {
                entity: "Slab".into(),
            },
        ];
        run(&mut conn, steps, &[]).unwrap();
        let row: (f64, Option<String>) = conn
            .query_row(
                "SELECT \"height\", \"name\" FROM \"core/Partition\"",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, (2.5, None));
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name IN ('core/Wall', 'core/Slab')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
    }

    #[test]
    fn list_values_change_item_by_item() {
        let mut conn = project();
        let lengths = AttributeValue::List(vec![AttributeValue::Integer(4)]);
        conn.execute_batch("ALTER TABLE \"core/Wall\" ADD COLUMN \"layers\" BLOB")
            .unwrap();
        conn.execute(
            "UPDATE \"core/Wall\" SET \"layers\" = ?1",
            [codec::to_sql(&lengths)],
        )
        .unwrap();
        let length: ValueType = "quantity:length".parse().unwrap();
        let step = Step::ChangeValue {
            entity: "Wall".into(),
            attribute: "layers".into(),
            from: ValueType::List(Box::new(ValueType::Integer)),
            to: ValueType::List(Box::new(length.clone())),
            factor: 0.5,
        };
        run(&mut conn, vec![step], &[]).unwrap();
        let stored: SqlValue = conn
            .query_row("SELECT \"layers\" FROM \"core/Wall\"", [], |row| row.get(0))
            .unwrap();
        let read = codec::from_sql((&stored).into(), &ValueType::List(Box::new(length)));
        assert_eq!(
            read.unwrap(),
            Some(AttributeValue::List(vec![AttributeValue::Real(2.0)]))
        );
    }

    #[test]
    fn scalars_of_the_wrong_storage_class_are_corrupt() {
        let mut conn = Connection::open_in_memory().unwrap();
        open_project(&mut conn).unwrap();
        // Not STRICT, as a hostile file may be: text sits where a number belongs.
        conn.execute_batch(
            "CREATE TABLE \"core/Wall\" (id BLOB PRIMARY KEY NOT NULL, \"width\" INTEGER);
             INSERT INTO \"core/Wall\" VALUES (x'01', 'wide');
             INSERT INTO schema_namespace VALUES ('core', 0);",
        )
        .unwrap();
        let step = Step::ChangeValue {
            entity: "Wall".into(),
            attribute: "width".into(),
            from: ValueType::Integer,
            to: ValueType::Real,
            factor: 0.5,
        };
        assert!(matches!(
            run(&mut conn, vec![step], &[]),
            Err(PersistError::Corrupt)
        ));
    }

    #[test]
    fn failures_other_than_a_missing_item_keep_their_error() {
        fn duplicate(tx: &Transaction<'_>) -> Result<(), PersistError> {
            tx.execute("INSERT INTO schema_namespace VALUES ('core', 5)", [])?;
            Ok(())
        }
        let mut conn = project();
        let step = Step::Custom {
            entities: Vec::new(),
            function: "duplicate".into(),
        };
        let error = run(&mut conn, vec![step], &[("duplicate", duplicate)]).unwrap_err();
        assert!(matches!(error, PersistError::Database(_)), "{error:?}");
    }

    #[test]
    fn migrations_that_do_not_fit_the_file_fail() {
        let mut conn = project();
        let missing_table = Step::RemoveEntity {
            entity: "Roof".into(),
        };
        assert!(matches!(
            run(&mut conn, vec![missing_table], &[]),
            Err(PersistError::Corrupt)
        ));
        let custom = Step::Custom {
            entities: Vec::new(),
            function: "absent".into(),
        };
        let error = run(&mut conn, vec![custom], &[]).unwrap_err();
        assert!(matches!(&error, PersistError::UnknownCustomStep(name) if name == "absent"));
        assert_eq!(
            error.to_string(),
            "the migration step \"absent\" is not available"
        );

        let namespace = namespace();
        let schema = Schema::new(&namespace);
        let tx = conn.transaction().unwrap();
        let error = migrate(&tx, &schema, 0).unwrap_err();
        assert!(matches!(
            &error,
            PersistError::MissingMigration { namespace, version: 1 } if namespace == "core"
        ));
        assert_eq!(
            error.to_string(),
            "no migration of the \"core\" schema to version 1"
        );
    }

    #[test]
    fn conversions_follow_the_value_kinds() {
        let length: ValueType = "quantity:length".parse().unwrap();
        let id = EntityId::new(7);
        let cases = [
            (
                AttributeValue::Integer(3),
                length.clone(),
                AttributeValue::Real(1.5),
            ),
            (
                AttributeValue::Real(2.0),
                ValueType::Real,
                AttributeValue::Real(1.0),
            ),
            (
                AttributeValue::List(vec![AttributeValue::Real(1000.0)]),
                ValueType::List(Box::new(length)),
                AttributeValue::List(vec![AttributeValue::Real(500.0)]),
            ),
            (
                AttributeValue::Bool(true),
                ValueType::Bool,
                AttributeValue::Bool(true),
            ),
            (
                AttributeValue::Integer(5),
                ValueType::Integer,
                AttributeValue::Integer(5),
            ),
            (
                AttributeValue::Text("A".into()),
                ValueType::Enum("Side".into()),
                AttributeValue::Text("A".into()),
            ),
            (
                AttributeValue::Ref(id),
                ValueType::Ref("Level".into()),
                AttributeValue::Ref(id),
            ),
        ];
        for (value, to, expected) in cases {
            assert_eq!(convert(value, &to, 0.5).unwrap(), expected, "{to}");
        }
        assert!(matches!(
            convert(AttributeValue::Text("A".into()), &ValueType::Integer, 1.0),
            Err(PersistError::Corrupt)
        ));
    }
}
