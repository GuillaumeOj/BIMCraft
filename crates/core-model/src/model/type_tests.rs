use super::tests::{NO_VALUES, STOREY, WALLS, id, model};
use super::*;

const PSET: &str = "Pset_WallCommon";
const COMPONENT: u128 = 500;
const WALL_TYPE: u128 = 600;
const OTHER_TYPE: u128 = 601;

fn real(value: f64) -> AttributeValue {
    AttributeValue::Real(value)
}

fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

fn bool(value: bool) -> AttributeValue {
    AttributeValue::Bool(value)
}

/// A wall component whose types are 0.2 m thick unless they say otherwise.
fn component() -> Component {
    Component {
        entity: "Wall".into(),
        library: LibraryRef {
            id: "acme/partition".into(),
            version: 3,
        },
        defaults: vec![("thickness".into(), real(0.2))],
    }
}

/// The test model with a wall component and a type made from it; every wall has that type.
fn typed() -> Model {
    let mut model = model();
    model.create_component(id(COMPONENT), component()).unwrap();
    model
        .create_type(id(WALL_TYPE), id(COMPONENT), NO_VALUES)
        .unwrap();
    for wall in WALLS {
        model.assign_type(id(wall), Some(id(WALL_TYPE))).unwrap();
    }
    model
}

fn height(model: &Model, wall: u128) -> Option<AttributeValue> {
    model.get(id(wall), "height").unwrap()
}

#[test]
fn types_are_made_from_components() {
    let model = typed();
    assert_eq!(model.component(id(COMPONENT)), Some(&component()));
    assert_eq!(model.component_of(id(WALL_TYPE)), Some(id(COMPONENT)));
    assert_eq!(model.entity_of(id(WALL_TYPE)), Some("WallType"));
    // The component's default, then the schema's.
    assert_eq!(model.get(id(WALL_TYPE), "thickness"), Ok(Some(real(0.2))));
    assert_eq!(model.get(id(WALL_TYPE), "height"), Ok(Some(real(2.5))));

    let mut model = model;
    model
        .create_type(id(OTHER_TYPE), id(COMPONENT), [("thickness", real(0.3))])
        .unwrap();
    assert_eq!(model.get(id(OTHER_TYPE), "thickness"), Ok(Some(real(0.3))));
}

#[test]
fn component_errors_store_nothing() {
    let mut model = typed();
    let with_entity = |entity: &str| Component {
        entity: entity.into(),
        ..component()
    };
    let with_defaults = |defaults: Vec<(&str, AttributeValue)>| Component {
        defaults: defaults
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
        ..component()
    };
    let cases = [
        (
            id(COMPONENT),
            component(),
            ModelError::Store(StoreError::DuplicateId(id(COMPONENT))),
        ),
        (
            id(STOREY),
            component(),
            ModelError::Store(StoreError::DuplicateId(id(STOREY))),
        ),
        (
            id(501),
            with_entity("Roof"),
            ModelError::Store(StoreError::UnknownEntity("Roof".into())),
        ),
        (
            id(501),
            with_entity("Storey"),
            ModelError::NoType("Storey".into()),
        ),
        (
            id(501),
            with_defaults(vec![("name", text("W"))]),
            ModelError::Store(StoreError::UnknownAttribute {
                entity: "WallType".into(),
                attribute: "name".into(),
            }),
        ),
        (
            id(501),
            with_defaults(vec![("thickness", text("thick"))]),
            ModelError::Store(StoreError::TypeMismatch("thickness".into())),
        ),
    ];
    for (raw, component, error) in cases {
        assert_eq!(model.create_component(raw, component), Err(error));
        assert_eq!(model.component(id(501)), None);
    }
}

#[test]
fn type_errors_store_nothing() {
    let mut model = typed();
    let mut bare = component();
    bare.defaults.clear();
    model.create_component(id(501), bare).unwrap();
    let cases = [
        (
            id(WALL_TYPE),
            id(COMPONENT),
            ModelError::Store(StoreError::DuplicateId(id(WALL_TYPE))),
        ),
        (
            id(COMPONENT),
            id(COMPONENT),
            ModelError::Store(StoreError::DuplicateId(id(COMPONENT))),
        ),
        (
            id(OTHER_TYPE),
            id(999),
            ModelError::ComponentNotFound(id(999)),
        ),
        // Neither the component nor the call gives the required thickness.
        (
            id(OTHER_TYPE),
            id(501),
            ModelError::Store(StoreError::MissingRequired("thickness".into())),
        ),
    ];
    for (type_id, component, error) in cases {
        assert_eq!(model.create_type(type_id, component, NO_VALUES), Err(error));
    }
    assert!(!model.contains(id(OTHER_TYPE)));
}

#[test]
fn ids_are_shared_by_objects_and_components() {
    let mut model = typed();
    let taken = Err(ModelError::Store(StoreError::DuplicateId(id(COMPONENT))));
    assert_eq!(model.insert(id(COMPONENT), "Storey", NO_VALUES), taken);
    let roles = [
        ("structure", vec![id(STOREY)]),
        ("elements", vec![id(WALLS[0])]),
    ];
    assert_eq!(
        model.relate(id(COMPONENT), "Containment", roles, NO_VALUES),
        taken
    );
}

#[test]
fn types_without_component_are_stored_directly() {
    let mut model = model();
    model
        .insert(id(WALL_TYPE), "WallType", [("thickness", real(0.15))])
        .unwrap();
    assert_eq!(model.component_of(id(WALL_TYPE)), None);
    model
        .assign_type(id(WALLS[0]), Some(id(WALL_TYPE)))
        .unwrap();
    assert_eq!(model.get(id(WALLS[0]), "thickness"), Ok(Some(real(0.15))));
}

#[test]
fn instances_inherit_from_their_type() {
    let model = typed();
    for wall in WALLS {
        assert_eq!(model.type_of(id(wall)), Some(id(WALL_TYPE)));
        assert_eq!(model.get(id(wall), "thickness"), Ok(Some(real(0.2))));
        assert_eq!(height(&model, wall), Some(real(2.5)));
    }
    assert_eq!(
        model.instances_of(id(WALL_TYPE)).collect::<Vec<_>>(),
        WALLS.map(id)
    );
    assert_eq!(model.instances_of(id(STOREY)).count(), 0);
}

#[test]
fn untyped_instances_read_defaults() {
    let model = model();
    assert_eq!(model.type_of(id(WALLS[0])), None);
    assert_eq!(height(&model, WALLS[0]), Some(real(2.5)));
    assert_eq!(model.get(id(WALLS[0]), "thickness"), Ok(None));
    assert_eq!(
        model.property(id(WALLS[0]), PSET, "IsExternal"),
        Ok(Some(bool(true)))
    );
    assert_eq!(model.property(id(WALLS[0]), PSET, "FireRating"), Ok(None));
}

#[test]
fn overriding_a_non_overridable_value_is_rejected() {
    let mut model = typed();
    let wall = id(WALLS[0]);
    assert_eq!(
        model.set(wall, "thickness", Some(real(0.5))),
        Err(ModelError::NotOverridable {
            owner: "Wall".into(),
            name: "thickness".into(),
        })
    );
    assert_eq!(
        model.set_property(wall, PSET, "FireRating", Some(text("REI 30"))),
        Err(ModelError::NotOverridable {
            owner: PSET.into(),
            name: "FireRating".into(),
        })
    );
    assert_eq!(model.get(wall, "thickness"), Ok(Some(real(0.2))));
    // And instance values do not belong on the type.
    assert_eq!(
        model.set(id(WALL_TYPE), "name", Some(text("W"))),
        Err(ModelError::NotOnType {
            owner: "Wall".into(),
            name: "name".into(),
        })
    );
    assert_eq!(
        model.set_property(id(WALL_TYPE), PSET, "Reference", Some(text("W"))),
        Err(ModelError::NotOnType {
            owner: PSET.into(),
            name: "Reference".into(),
        })
    );
}

#[test]
fn changing_a_type_value_updates_every_non_overridden_instance() {
    let mut model = typed();
    let [first, second, third] = WALLS;
    assert_eq!(model.set(id(second), "height", Some(real(3.0))), Ok(None));
    assert_eq!(
        model
            .inheriting_instances(id(WALL_TYPE), "height")
            .unwrap()
            .collect::<Vec<_>>(),
        [id(first), id(third)]
    );
    assert_eq!(
        model
            .inheriting_instances(id(WALL_TYPE), "thickness")
            .unwrap()
            .count(),
        3
    );

    assert_eq!(
        model.set(id(WALL_TYPE), "height", Some(real(2.8))),
        Ok(Some(real(2.5)))
    );
    model
        .set(id(WALL_TYPE), "thickness", Some(real(0.25)))
        .unwrap();
    assert_eq!(height(&model, first), Some(real(2.8)));
    assert_eq!(height(&model, second), Some(real(3.0)));
    assert_eq!(height(&model, third), Some(real(2.8)));
    for wall in WALLS {
        assert_eq!(model.get(id(wall), "thickness"), Ok(Some(real(0.25))));
    }

    // Clearing the override brings the type's value back.
    assert_eq!(model.set(id(second), "height", None), Ok(Some(real(3.0))));
    assert_eq!(height(&model, second), Some(real(2.8)));
}

#[test]
fn instances_inherit_type_properties() {
    let mut model = typed();
    let [first, second, _] = WALLS.map(id);
    // A type holds type and type_overridable properties.
    model
        .attach_property_set(
            id(WALL_TYPE),
            PSET,
            [("FireRating", text("REI 60")), ("IsExternal", bool(false))],
        )
        .unwrap();
    assert_eq!(
        model.attach_property_set(id(WALL_TYPE), PSET, [("FireRating", text("REI 60"))]),
        Err(ModelError::AlreadyAttached(PSET.into()))
    );
    model
        .attach_property_set(second, PSET, [("Reference", text("W-02"))])
        .unwrap();
    model
        .set_property(second, PSET, "IsExternal", Some(bool(true)))
        .unwrap();

    assert_eq!(
        model.property(first, PSET, "FireRating"),
        Ok(Some(text("REI 60")))
    );
    assert_eq!(
        model.property(first, PSET, "IsExternal"),
        Ok(Some(bool(false)))
    );
    assert_eq!(
        model.property(second, PSET, "IsExternal"),
        Ok(Some(bool(true)))
    );
    // Instance properties come from the instance only.
    assert_eq!(
        model.property(first, PSET, "LoadBearing"),
        Ok(Some(bool(false)))
    );
    assert_eq!(
        model.property(id(WALL_TYPE), PSET, "Reference"),
        Err(ModelError::NotOnType {
            owner: PSET.into(),
            name: "Reference".into(),
        })
    );

    model
        .set_property(id(WALL_TYPE), PSET, "FireRating", Some(text("REI 90")))
        .unwrap();
    model
        .set_property(id(WALL_TYPE), PSET, "IsExternal", Some(bool(true)))
        .unwrap();
    for wall in [first, second] {
        assert_eq!(
            model.property(wall, PSET, "FireRating"),
            Ok(Some(text("REI 90")))
        );
        assert_eq!(
            model.property(wall, PSET, "IsExternal"),
            Ok(Some(bool(true)))
        );
    }
    assert_eq!(
        model.property_sets_of(id(WALL_TYPE)).collect::<Vec<_>>(),
        [PSET]
    );
    assert_eq!(
        model
            .detach_property_set(id(WALL_TYPE), PSET)
            .map(|values| values.len()),
        Ok(2)
    );
    // Without the type's values, instances fall back to the defaults.
    assert_eq!(
        model.property(first, PSET, "IsExternal"),
        Ok(Some(bool(true)))
    );
    assert_eq!(model.property(first, PSET, "FireRating"), Ok(None));
}

#[test]
fn assigning_types_checks_both_ends() {
    let mut model = typed();
    model
        .insert(id(OTHER_TYPE), "WallType", [("thickness", real(0.1))])
        .unwrap();
    let wall = id(WALLS[0]);
    assert_eq!(
        model.assign_type(wall, Some(id(OTHER_TYPE))),
        Ok(Some(id(WALL_TYPE)))
    );
    assert_eq!(model.get(wall, "thickness"), Ok(Some(real(0.1))));
    assert_eq!(model.instances_of(id(WALL_TYPE)).count(), 2);
    assert_eq!(model.assign_type(wall, None), Ok(Some(id(OTHER_TYPE))));
    assert_eq!(model.assign_type(wall, None), Ok(None));
    assert_eq!(model.instances_of(id(OTHER_TYPE)).count(), 0);

    let cases = [
        (
            id(999),
            Some(id(WALL_TYPE)),
            ModelError::Store(StoreError::NotFound(id(999))),
        ),
        (
            id(WALL_TYPE),
            Some(id(OTHER_TYPE)),
            ModelError::NotAnInstance,
        ),
        (
            id(STOREY),
            Some(id(WALL_TYPE)),
            ModelError::NoType("Storey".into()),
        ),
        (
            wall,
            Some(id(999)),
            ModelError::Store(StoreError::NotFound(id(999))),
        ),
        (
            wall,
            Some(id(STOREY)),
            ModelError::WrongType {
                entity: "Wall".into(),
                type_entity: "Storey".into(),
            },
        ),
        (
            wall,
            Some(id(WALLS[1])),
            ModelError::WrongType {
                entity: "Wall".into(),
                type_entity: "Wall".into(),
            },
        ),
    ];
    for (instance, type_id, error) in cases {
        assert_eq!(model.assign_type(instance, type_id), Err(error));
    }
    assert_eq!(model.type_of(wall), None);
}

#[test]
fn deleting_a_type_leaves_its_instances_untyped() {
    let mut model = typed();
    model.set(id(WALL_TYPE), "height", Some(real(3.1))).unwrap();
    let removal = model.remove(id(WALL_TYPE)).unwrap();
    for wall in WALLS {
        assert!(model.contains(id(wall)));
        assert_eq!(model.type_of(id(wall)), None);
        assert_eq!(height(&model, wall), Some(real(2.5)));
    }
    assert_eq!(model.remove_component(id(COMPONENT)), Ok(component()));
    assert_eq!(
        model.restore(removal.clone()),
        Err(ModelError::ComponentNotFound(id(COMPONENT)))
    );
    model.create_component(id(COMPONENT), component()).unwrap();
    model.restore(removal).unwrap();
    assert_eq!(model.component_of(id(WALL_TYPE)), Some(id(COMPONENT)));
    for wall in WALLS {
        assert_eq!(model.type_of(id(wall)), Some(id(WALL_TYPE)));
        assert_eq!(height(&model, wall), Some(real(3.1)));
    }
}

#[test]
fn deleting_an_instance_unlinks_it_from_its_type() {
    let mut model = typed();
    let wall = id(WALLS[1]);
    let removal = model.remove(wall).unwrap();
    assert_eq!(model.instances_of(id(WALL_TYPE)).count(), 2);
    model.restore(removal).unwrap();
    assert_eq!(model.type_of(wall), Some(id(WALL_TYPE)));
    assert_eq!(model.instances_of(id(WALL_TYPE)).count(), 3);
}

#[test]
fn restoring_type_links_needs_both_ends() {
    let mut model = typed();
    let removal = model.remove(id(WALLS[0])).unwrap();
    model.remove(id(WALL_TYPE)).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::Store(StoreError::NotFound(id(WALL_TYPE))))
    );
    assert!(!model.contains(id(WALLS[0])));

    let mut model = typed();
    let removal = model.remove(id(WALL_TYPE)).unwrap();
    model.remove(id(WALLS[2])).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::Store(StoreError::NotFound(id(WALLS[2]))))
    );
    assert!(!model.contains(id(WALL_TYPE)));
}

#[test]
fn components_are_removed_once_unused() {
    let mut model = typed();
    assert_eq!(
        model.remove_component(id(COMPONENT)),
        Err(ModelError::ComponentInUse)
    );
    model.remove(id(WALL_TYPE)).unwrap();
    assert_eq!(model.remove_component(id(COMPONENT)), Ok(component()));
    assert_eq!(
        model.remove_component(id(COMPONENT)),
        Err(ModelError::ComponentNotFound(id(COMPONENT)))
    );
}

#[test]
fn type_errors_name_entities_but_never_ids() {
    let messages = [
        (
            ModelError::NoType("Storey".into()),
            "entity \"Storey\" has no type",
        ),
        (
            ModelError::NotAnInstance,
            "the object is a type, not an instance",
        ),
        (
            ModelError::WrongType {
                entity: "Wall".into(),
                type_entity: "Storey".into(),
            },
            "a \"Storey\" cannot be the type of a \"Wall\"",
        ),
        (
            ModelError::NotOnType {
                owner: "Wall".into(),
                name: "name".into(),
            },
            "\"name\" of \"Wall\" is set on each instance, not on the type",
        ),
        (
            ModelError::ComponentNotFound(id(0xABCD)),
            "no component with this id",
        ),
        (ModelError::ComponentInUse, "the component still has types"),
        (
            ModelError::InstanceRetyped(id(0xABCD)),
            "an instance of the type was given another type",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn unknown_attributes_are_rejected_on_both_sides() {
    let mut model = typed();
    for (object, entity) in [(WALLS[0], "Wall"), (WALL_TYPE, "WallType")] {
        let unknown = Err(ModelError::Store(StoreError::UnknownAttribute {
            entity: entity.into(),
            attribute: "colour".into(),
        }));
        assert_eq!(model.set(id(object), "colour", Some(text("red"))), unknown);
        assert_eq!(model.get(id(object), "colour"), unknown);
    }
}

#[test]
fn inheriting_instances_needs_an_attribute_the_type_holds() {
    let model = typed();
    for attribute in ["colour", "name"] {
        assert!(matches!(
            model.inheriting_instances(id(WALL_TYPE), attribute),
            Err(ModelError::Store(StoreError::UnknownAttribute { .. }))
        ));
    }
    assert!(matches!(
        model.inheriting_instances(id(999), "height"),
        Err(ModelError::Store(StoreError::NotFound(_)))
    ));
}

#[test]
fn types_do_not_read_instance_properties() {
    let model = typed();
    assert_eq!(
        model.property(id(WALL_TYPE), PSET, "LoadBearing"),
        Err(ModelError::NotOnType {
            owner: PSET.into(),
            name: "LoadBearing".into(),
        })
    );
}

#[test]
fn restore_fails_when_the_model_moved_on_around_types() {
    // An id taken by a component since the removal.
    let mut model = typed();
    let removal = model.remove(id(WALLS[0])).unwrap();
    model.create_component(id(WALLS[0]), component()).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::Store(StoreError::DuplicateId(id(WALLS[0]))))
    );

    // An instance of the removed type given another type since.
    let mut model = typed();
    let removal = model.remove(id(WALL_TYPE)).unwrap();
    model
        .insert(id(OTHER_TYPE), "WallType", [("thickness", real(0.1))])
        .unwrap();
    model
        .assign_type(id(WALLS[1]), Some(id(OTHER_TYPE)))
        .unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::InstanceRetyped(id(WALLS[1])))
    );
    assert!(!model.contains(id(WALL_TYPE)));
    assert_eq!(model.type_of(id(WALLS[1])), Some(id(OTHER_TYPE)));
}
