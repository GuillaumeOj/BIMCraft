//! Autosave writes a recovery copy, not the project, and an interrupted autosave keeps the
//! previous copy whole (PERS-02, NFR-06, issue #38).

use core_model::{AttributeValue, EntityId, Store};
use persist::{
    Autosave, AutosaveInterval, Connection, PersistError, Schema, autosave_model, autosaved_at,
    open_model, open_project, save_model, set_property,
};
use platform::Clock;
use platform::Timestamp;
use platform::testing::ManualClock;
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, ValueType,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn namespace() -> Namespace {
    let space = Entity {
        name: "Space".into(),
        kind: EntityKind::Spatial,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: "IfcSpace".into(),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes: vec![Attribute {
            name: "name".into(),
            value: ValueType::Text,
            presence: Presence::Optional,
            placement: Placement::Instance,
            doc: None,
        }],
        roles: Vec::new(),
        representations: Vec::new(),
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [("Space".to_owned(), space)].into(),
        property_sets: std::collections::BTreeMap::new(),
    }
}

/// `count` spaces named after `label`.
fn spaces(label: &str, count: u128) -> Result<Store> {
    let mut store = Store::new(&namespace())?;
    for raw in 0..count {
        let name = AttributeValue::Text(format!("{label} {raw:04}"));
        store.insert(EntityId::new(raw), "Space", [("name", name)])?;
    }
    Ok(store)
}

/// The names of the spaces in `store`, sorted by id.
fn names(store: &Store) -> Result<Vec<Option<AttributeValue>>> {
    let mut ids: Vec<_> = store.ids().collect();
    ids.sort();
    let names = ids.into_iter().map(|id| store.get(id, "name"));
    Ok(names.collect::<std::result::Result<_, _>>()?)
}

fn new_file() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    open_project(&mut conn)?;
    Ok(conn)
}

fn at(millis: u64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

#[test]
fn autosave_writes_the_recovery_copy_and_leaves_the_project_alone() -> Result<()> {
    let namespace = namespace();
    let schema = Schema::new(&namespace);
    let mut project = new_file()?;
    let saved = spaces("Saved", 2)?;
    save_model(&mut project, &schema, &saved)?;
    let mut recovery = new_file()?;

    // The host's loop: a change, then the timer fires one interval later.
    let clock = ManualClock::new(at(1_000));
    let mut autosave = Autosave::new(AutosaveInterval::DEFAULT);
    let edited = spaces("Edited", 3)?;
    autosave.changed(clock.now());
    clock.advance(AutosaveInterval::DEFAULT.as_millis());
    let wrote = autosave.run(clock.now(), || {
        autosave_model(&mut recovery, &schema, &edited, clock.now())
    })?;
    assert!(wrote);

    assert_eq!(
        names(&open_model(&mut recovery, &schema)?)?,
        names(&edited)?
    );
    assert_eq!(
        autosaved_at(&recovery)?,
        Some(at(1_000 + AutosaveInterval::DEFAULT.as_millis()))
    );
    assert_eq!(names(&open_model(&mut project, &schema)?)?, names(&saved)?);
    assert_eq!(autosaved_at(&project)?, None);
    Ok(())
}

#[test]
fn an_interrupted_autosave_keeps_the_previous_copy() -> Result<()> {
    let namespace = namespace();
    let schema = Schema::new(&namespace);
    let mut recovery = new_file()?;
    let before = spaces("Before", 20)?;
    let after = spaces("After", 30)?;
    autosave_model(&mut recovery, &schema, &before, at(1))?;

    let mut steps = 1;
    let mut interruptions = 0;
    loop {
        // Interrupts once, so the rollback that follows runs to its end.
        let mut fired = false;
        recovery.progress_handler(
            steps,
            Some(move || {
                let interrupt = !fired;
                fired = true;
                interrupt
            }),
        )?;
        let result = autosave_model(&mut recovery, &schema, &after, at(2));
        recovery.progress_handler(0, None::<fn() -> bool>)?;
        if result.is_ok() {
            break;
        }
        interruptions += 1;
        assert!(recovery.is_autocommit(), "after {steps} steps");
        assert_eq!(
            names(&open_model(&mut recovery, &schema)?)?,
            names(&before)?,
            "after {steps} steps"
        );
        assert_eq!(autosaved_at(&recovery)?, Some(at(1)), "after {steps} steps");
        steps += steps / 4 + 1;
    }
    assert!(interruptions > 10, "only {interruptions} interruptions");
    assert_eq!(names(&open_model(&mut recovery, &schema)?)?, names(&after)?);
    assert_eq!(autosaved_at(&recovery)?, Some(at(2)));
    Ok(())
}

#[test]
fn a_damaged_autosave_time_is_reported() -> Result<()> {
    let recovery = new_file()?;
    set_property(&recovery, "autosaved_at", "yesterday")?;
    assert!(matches!(
        autosaved_at(&recovery),
        Err(PersistError::Corrupt)
    ));
    Ok(())
}

#[test]
fn autosave_refuses_an_invalid_schema() -> Result<()> {
    let mut invalid = namespace();
    invalid.name = String::new();
    let mut recovery = new_file()?;
    let result = autosave_model(
        &mut recovery,
        &Schema::new(&invalid),
        &spaces("Any", 1)?,
        at(0),
    );
    assert!(matches!(result, Err(PersistError::Model(_))), "{result:?}");
    assert_eq!(autosaved_at(&recovery)?, None);
    Ok(())
}
