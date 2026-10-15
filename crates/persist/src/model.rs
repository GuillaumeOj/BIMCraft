//! Saving the model into a project file and reading it back (PERS-01, ADR 0016).

use core_model::{Store, StoreError};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Statement, Transaction};

use crate::layout::{self, EntityTable};
use crate::migrate::migrate;
use crate::{Connection, PersistError, Schema, begin, codec, upgrade_format};

/// Writes every object of `store` into the project, replacing the objects saved before, and
/// records the schema version of `schema.namespace`, the namespace `store` was created with.
///
/// A file from an earlier version is migrated first (PERS-01). Everything runs in one
/// transaction, so an interrupted save leaves the previous content (PERS-02).
///
/// # Errors
///
/// [`PersistError::NewerSchema`] when the file holds this namespace in a newer version,
/// [`PersistError::Corrupt`] when its tables do not match the namespace,
/// [`PersistError::Model`] when the namespace is invalid or `store` holds an entity or
/// attribute it does not declare, the errors of a migration
/// ([`PersistError::MissingMigration`], [`PersistError::UnknownCustomStep`]),
/// [`PersistError::NotAProject`] for a database not prepared by
/// [`open_project`](crate::open_project), and [`PersistError::Database`] when `SQLite` fails.
pub fn save_model(
    conn: &mut Connection,
    schema: &Schema<'_>,
    store: &Store,
) -> Result<(), PersistError> {
    schema::validate(schema.namespace).map_err(StoreError::InvalidSchema)?;
    let tx = begin(conn)?;
    let tables = prepare_tables(&tx, schema)?;
    let mut inserts = Vec::with_capacity(tables.len());
    for table in &tables {
        tx.execute(&table.clear_sql(), [])?;
        inserts.push(tx.prepare(&table.insert_sql())?);
    }
    // One row buffer for the whole save: a model holds objects by the hundred thousand.
    let mut row = Vec::new();
    for (id, object) in store.ids().filter_map(|id| Some((id, store.entity(id)?))) {
        let index = tables
            .binary_search_by(|table| table.entity.cmp(&object.entity))
            .map_err(|_| StoreError::UnknownEntity(object.entity.clone()))?;
        let table = &tables[index];
        let mut values = object.values.into_iter().peekable();
        row.clear();
        row.push(Value::Blob(codec::id_to_sql(id)));
        for column in &table.columns {
            let value = values.next_if(|(name, _)| *name == column.name);
            row.push(value.map_or(Value::Null, |(_, value)| codec::to_sql(&value)));
        }
        if let Some((attribute, _)) = values.next() {
            return Err(StoreError::UnknownAttribute {
                entity: object.entity,
                attribute,
            }
            .into());
        }
        inserts[index].execute(rusqlite::params_from_iter(&row))?;
    }
    drop(inserts);
    tx.commit()?;
    Ok(())
}

/// Reads the objects of `schema.namespace` saved in the project into a new store.
///
/// A project that never saved this namespace gives an empty store. A file from an earlier
/// version is migrated in memory (PERS-01): the file itself is left unchanged until the next
/// save.
///
/// # Errors
///
/// [`PersistError::NewerSchema`] when the file holds this namespace in a newer version,
/// [`PersistError::Corrupt`] when its tables or values are malformed, [`PersistError::Model`]
/// when the namespace is invalid or the objects break its rules (a missing required value, a
/// duplicate id…), the errors of a migration, and [`PersistError::Database`] when `SQLite`
/// fails.
pub fn open_model(conn: &mut Connection, schema: &Schema<'_>) -> Result<Store, PersistError> {
    let mut store = Store::new(schema.namespace)?;
    // Never committed: migrations, and tables `prepare_tables` creates for a new namespace, are
    // rolled back.
    let tx = begin(conn)?;
    let mut values = Vec::new();
    for table in prepare_tables(&tx, schema)? {
        let mut select = tx.prepare(&table.select_sql())?;
        let mut rows = select.query([])?;
        while let Some(row) = rows.next()? {
            let id = codec::id_from_sql(row.get_ref(0)?)?;
            for (index, column) in (1..).zip(&table.columns) {
                if let Some(value) = codec::from_sql(row.get_ref(index)?, column.value_type)? {
                    values.push((column.name, value));
                }
            }
            store.insert(id, table.entity, values.drain(..))?;
        }
    }
    Ok(store)
}

/// Makes the file's tables match `schema`: brings the file format up to date, then creates the
/// entity tables when the file never held the namespace, or migrates them from an earlier
/// version, and checks them.
fn prepare_tables<'a>(
    tx: &Transaction<'_>,
    schema: &Schema<'a>,
) -> Result<Vec<EntityTable<'a>>, PersistError> {
    upgrade_format(tx)?;
    let namespace = schema.namespace;
    let tables = layout::tables(namespace);
    match stored_version(tx, &namespace.name)? {
        None => {
            for table in &tables {
                tx.execute(&table.create_sql(), [])?;
            }
            tx.execute(
                "INSERT INTO schema_namespace (name, version) VALUES (?1, ?2)",
                (&namespace.name, namespace.version),
            )?;
        }
        Some(found) if found > namespace.version => {
            return Err(PersistError::NewerSchema {
                namespace: namespace.name.clone(),
                found,
                supported: namespace.version,
            });
        }
        Some(found) => {
            if found < namespace.version {
                migrate(tx, schema, found)?;
            }
            let mut info =
                tx.prepare("SELECT name, type, pk FROM pragma_table_info(?1) ORDER BY name")?;
            for table in &tables {
                check_columns(&mut info, table)?;
            }
        }
    }
    Ok(tables)
}

/// The schema version of `namespace` recorded in the file, if it holds that namespace.
fn stored_version(tx: &Transaction<'_>, namespace: &str) -> Result<Option<u32>, PersistError> {
    let version: Option<i64> = tx
        .query_row(
            "SELECT version FROM schema_namespace WHERE name = ?1",
            [namespace],
            |row| row.get(0),
        )
        .optional()?;
    version
        .map(|version| u32::try_from(version).map_err(|_| PersistError::Corrupt))
        .transpose()
}

/// Checks the columns of `table` with `info`, the prepared `pragma_table_info` query.
fn check_columns(info: &mut Statement<'_>, table: &EntityTable<'_>) -> Result<(), PersistError> {
    let found = info
        .query_map([&table.name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<(String, String, i64)>, _>>()?;
    if found == table.expected_columns() {
        Ok(())
    } else {
        Err(PersistError::Corrupt)
    }
}

#[cfg(test)]
mod tests {
    use core_model::{AttributeValue, EntityId};
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{entity, namespace, object, sorted_objects, store_strategy};
    use crate::{Schema, open_project};

    fn new_project() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        open_project(&mut conn).unwrap();
        conn
    }

    fn saved(store: &Store) -> Connection {
        let mut conn = new_project();
        save_model(&mut conn, &Schema::new(&namespace()), store).unwrap();
        conn
    }

    fn wall(store: &mut Store, raw: u128) {
        store
            .insert(
                EntityId::new(raw),
                "Wall",
                [("axis", AttributeValue::Ref(EntityId::new(1)))],
            )
            .unwrap();
    }

    fn table_count(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'core/%'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn a_project_without_a_model_opens_empty_and_unchanged() {
        let mut conn = new_project();
        let store = open_model(&mut conn, &Schema::new(&namespace())).unwrap();
        assert!(store.is_empty());
        assert_eq!(table_count(&conn), 0);
    }

    #[test]
    fn saving_again_replaces_the_previous_objects() {
        let namespace = namespace();
        let mut store = Store::new(&namespace).unwrap();
        wall(&mut store, 10);
        wall(&mut store, 11);
        let mut conn = saved(&store);
        store.remove(EntityId::new(10)).unwrap();
        save_model(&mut conn, &Schema::new(&namespace), &store).unwrap();
        let reopened = open_model(&mut conn, &Schema::new(&namespace)).unwrap();
        assert_eq!(sorted_objects(&reopened), sorted_objects(&store));
        assert_eq!(table_count(&conn), 3);
    }

    #[test]
    fn the_namespace_version_is_recorded() {
        let conn = saved(&Store::new(&namespace()).unwrap());
        let version: u32 = conn
            .query_row(
                "SELECT version FROM schema_namespace WHERE name = 'core'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, namespace().version);
    }

    #[test]
    fn newer_namespace_versions_are_refused() {
        let mut conn = saved(&Store::new(&namespace()).unwrap());
        conn.execute("UPDATE schema_namespace SET version = version + 1", [])
            .unwrap();
        let error = open_model(&mut conn, &Schema::new(&namespace())).unwrap_err();
        assert!(matches!(
            &error,
            PersistError::NewerSchema { namespace, found: 2, supported: 1 } if namespace == "core"
        ));
        assert_eq!(
            error.to_string(),
            "the project uses version 2 of the \"core\" schema; this version reads up to 1"
        );
        let error = save_model(
            &mut conn,
            &Schema::new(&namespace()),
            &Store::new(&namespace()).unwrap(),
        );
        assert!(matches!(error, Err(PersistError::NewerSchema { .. })));
    }

    #[test]
    fn older_versions_without_migrations_are_refused() {
        let mut conn = saved(&Store::new(&namespace()).unwrap());
        conn.execute("UPDATE schema_namespace SET version = 0", [])
            .unwrap();
        let error = open_model(&mut conn, &Schema::new(&namespace())).unwrap_err();
        assert!(matches!(
            error,
            PersistError::MissingMigration { version: 1, .. }
        ));
    }

    #[test]
    fn malformed_files_are_corrupt() {
        let table = "\"core/Wall\"";
        let cases = [
            "UPDATE schema_namespace SET version = -1".to_owned(),
            format!("DROP TABLE {table}"),
            format!("ALTER TABLE {table} ADD COLUMN extra TEXT"),
            format!("ALTER TABLE {table} DROP COLUMN tags"),
            format!("UPDATE {table} SET id = x'00'"),
            format!("UPDATE {table} SET is_external = 7"),
            format!("UPDATE {table} SET tags = x'FFFFFFFF'"),
        ];
        let namespace = namespace();
        let mut store = Store::new(&namespace).unwrap();
        wall(&mut store, 10);
        for sql in cases {
            let mut conn = saved(&store);
            conn.execute_batch(&sql).unwrap();
            let error = open_model(&mut conn, &Schema::new(&namespace)).unwrap_err();
            assert!(matches!(error, PersistError::Corrupt), "{sql}: {error:?}");
            assert_eq!(error.to_string(), "the project file is damaged");
        }
    }

    #[test]
    fn objects_breaking_the_schema_are_refused() {
        let namespace = namespace();
        let mut store = Store::new(&namespace).unwrap();
        wall(&mut store, 10);
        for sql in [
            "UPDATE \"core/Wall\" SET axis = NULL",
            "INSERT INTO \"core/Storey\" (id) SELECT id FROM \"core/Wall\"",
            "UPDATE \"core/Wall\" SET height = 9e999",
        ] {
            let mut conn = saved(&store);
            conn.execute_batch(sql).unwrap();
            let error = open_model(&mut conn, &Schema::new(&namespace)).unwrap_err();
            assert!(matches!(error, PersistError::Model(_)), "{sql}: {error:?}");
        }
    }

    #[test]
    fn a_store_from_another_namespace_is_refused() {
        let mut bigger = namespace();
        bigger
            .entities
            .insert("Door".into(), entity("Door", Vec::new()));
        let mut store = Store::new(&bigger).unwrap();
        store
            .insert(EntityId::new(1), "Door", Vec::<(&str, _)>::new())
            .unwrap();
        let mut conn = new_project();
        let error = save_model(&mut conn, &Schema::new(&namespace()), &store).unwrap_err();
        assert!(matches!(
            error,
            PersistError::Model(StoreError::UnknownEntity(entity)) if entity == "Door"
        ));

        let mut smaller = namespace();
        smaller
            .entities
            .get_mut("Wall")
            .unwrap()
            .attributes
            .retain(|attribute| attribute.name != "name");
        let mut store = Store::new(&namespace()).unwrap();
        store.insert(EntityId::new(10), "Wall", object()).unwrap();
        let error = save_model(&mut conn, &Schema::new(&smaller), &store).unwrap_err();
        assert!(matches!(
            error,
            PersistError::Model(StoreError::UnknownAttribute { attribute, .. }) if attribute == "name"
        ));
        assert_eq!(table_count(&conn), 0);
    }

    #[test]
    fn invalid_namespaces_are_refused() {
        let mut invalid = namespace();
        invalid.version = 0;
        let store = Store::new(&namespace()).unwrap();
        let mut conn = new_project();
        for error in [
            save_model(&mut conn, &Schema::new(&invalid), &store).unwrap_err(),
            open_model(&mut conn, &Schema::new(&invalid)).unwrap_err(),
        ] {
            assert!(matches!(
                error,
                PersistError::Model(StoreError::InvalidSchema(_))
            ));
            assert_eq!(error.to_string(), "the project does not fit the schema");
        }
    }

    #[test]
    fn every_value_type_survives_a_save() {
        let namespace = namespace();
        let mut store = Store::new(&namespace).unwrap();
        store.insert(EntityId::new(7), "Wall", object()).unwrap();
        let layers = AttributeValue::List(vec![AttributeValue::Real(0.2)]);
        store
            .insert(EntityId::new(8), "WallType", [("layers", layers)])
            .unwrap();
        let mut conn = saved(&store);
        let reopened = open_model(&mut conn, &Schema::new(&namespace)).unwrap();
        assert_eq!(sorted_objects(&reopened), sorted_objects(&store));
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config())]

        /// Save, open, compare: the model comes back identical.
        #[test]
        fn save_then_open_gives_the_same_model(store in store_strategy()) {
            let mut conn = saved(&store);
            let reopened = open_model(&mut conn, &Schema::new(&namespace())).unwrap();
            prop_assert_eq!(sorted_objects(&reopened), sorted_objects(&store));
        }
    }
}
