use proptest::prelude::*;

use super::*;
use crate::{Cardinality, QuantityKind, Representation, Role, TypeDef};

fn attribute(name: &str, value: ValueType) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence: Presence::Optional,
        placement: Placement::Instance,
        doc: None,
    }
}

fn ifc(entity: &str) -> IfcMapping {
    IfcMapping {
        entity: entity.into(),
        predefined_type: None,
        object_type: None,
        type_entity: None,
    }
}

fn entity(name: &str, kind: EntityKind, ifc_entity: &str) -> Entity {
    Entity {
        name: name.into(),
        kind,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(ifc(ifc_entity)),
        attributes: Vec::new(),
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

/// A small, valid namespace each test breaks in one way.
fn sample() -> Namespace {
    let mut wall = entity("Wall", EntityKind::Element, "IfcWall");
    wall.doc = Some("A wall.".into());
    wall.property_sets = vec!["Pset_WallCommon".into()];
    wall.type_def = Some(TypeDef {
        name: "WallType".into(),
        doc: Some("A wall build-up.".into()),
    });
    if let Some(mapping) = &mut wall.ifc {
        mapping.predefined_type = Some("STANDARD".into());
        mapping.type_entity = Some("IfcWallType".into());
    }
    wall.attributes = vec![
        Attribute {
            presence: Presence::Required,
            ..attribute("axis", ValueType::Ref("Storey".into()))
        },
        Attribute {
            presence: Presence::Default(Value::Real(2.5)),
            placement: Placement::TypeOverridable,
            ..attribute("height", ValueType::Quantity(QuantityKind::Length))
        },
        Attribute {
            presence: Presence::Default(Value::List(vec![Value::Integer(1)])),
            placement: Placement::Type,
            ..attribute(
                "layer_counts",
                ValueType::List(Box::new(ValueType::Integer)),
            )
        },
    ];
    wall.representations = vec![Representation {
        lod: 100,
        context: "footprint".into(),
        generator: "wall::footprint".into(),
        depends_on: vec!["axis".into()],
    }];

    let storey = entity("Storey", EntityKind::Spatial, "IfcBuildingStorey");

    let mut containment = entity(
        "Containment",
        EntityKind::Relationship,
        "IfcRelContainedInSpatialStructure",
    );
    containment.roles = vec![
        Role {
            name: "structure".into(),
            targets: vec![RoleTarget::Kind(EntityKind::Spatial)],
            cardinality: Cardinality::One,
            exclusive: false,
        },
        Role {
            name: "elements".into(),
            targets: vec![RoleTarget::Entity("Wall".into())],
            cardinality: Cardinality::Many,
            exclusive: true,
        },
    ];

    let property_set = PropertySet {
        name: "Pset_WallCommon".into(),
        applies_to: vec!["Wall".into()],
        properties: vec![
            Attribute {
                presence: Presence::Default(Value::Bool(false)),
                ..attribute("IsExternal", ValueType::Bool)
            },
            Attribute {
                presence: Presence::Default(Value::Text("REI 60".into())),
                ..attribute("FireRating", ValueType::Text)
            },
        ],
    };

    Namespace {
        name: "core".into(),
        version: 1,
        entities: [wall, storey, containment]
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: [(property_set.name.clone(), property_set)].into(),
    }
}

fn wall(namespace: &mut Namespace) -> &mut Entity {
    namespace.entities.get_mut("Wall").unwrap()
}

fn errors_of(namespace: &Namespace) -> Vec<ValidationError> {
    validate(namespace).expect_err("namespace should be invalid")
}

fn entity_at(name: &str, item: Option<Item>) -> Location {
    Location {
        owner: Owner::Entity(name.into()),
        item,
    }
}

/// Asserts that validation reports exactly one error, at `location`, of `kind`.
fn assert_single(namespace: &Namespace, location: Location, kind: ValidationErrorKind) {
    assert_eq!(
        errors_of(namespace),
        vec![ValidationError { location, kind }]
    );
}

#[test]
fn accepts_the_sample() {
    assert_eq!(validate(&sample()), Ok(()));
}

#[test]
fn accepts_plugin_namespaces() {
    let mut namespace = sample();
    namespace.name = "plugin:acme-timber2".into();
    assert_eq!(validate(&namespace), Ok(()));
}

#[test]
fn rejects_bad_namespace_names_and_version_zero() {
    for name in [
        "Core",
        "plugin:",
        "plugin:Acme",
        "plugin:acme--x",
        "plugin:acme-",
        "x",
    ] {
        let mut namespace = sample();
        namespace.name = name.into();
        let errors = errors_of(&namespace);
        assert_eq!(errors.len(), 1, "{name}");
        assert!(
            matches!(&errors[0].kind, ValidationErrorKind::InvalidName { name: n, .. } if n == name)
        );
    }
    let mut namespace = sample();
    namespace.version = 0;
    assert_single(
        &namespace,
        Location {
            owner: Owner::Namespace,
            item: None,
        },
        ValidationErrorKind::ZeroVersion,
    );
}

#[test]
fn rejects_badly_named_definitions() {
    let mut namespace = sample();
    let mut beam = entity("beam", EntityKind::Element, "IfcBeam");
    beam.attributes = vec![
        attribute("Span", ValueType::Real),
        attribute("two__words", ValueType::Real),
        attribute(&"x".repeat(MAX_NAME_LEN + 1), ValueType::Real),
    ];
    namespace.entities.insert(beam.name.clone(), beam);
    let errors = errors_of(&namespace);
    let names: Vec<_> = errors
        .iter()
        .map(|error| match &error.kind {
            ValidationErrorKind::InvalidName { name, .. } => name.as_str(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(names.len(), 4);
    assert!(names.contains(&"beam"));
    assert!(names.contains(&"Span"));
    assert!(names.contains(&"two__words"));
}

#[test]
fn rejects_badly_named_property_sets_properties_and_contexts() {
    let mut namespace = sample();
    let mut property_set = namespace.property_sets.remove("Pset_WallCommon").unwrap();
    property_set.name = "_Pset".into();
    property_set.properties[0].name = "is_external".into();
    namespace
        .property_sets
        .insert(property_set.name.clone(), property_set);
    wall(&mut namespace).property_sets.clear();
    wall(&mut namespace).representations[0].context = "Body".into();
    wall(&mut namespace).representations[0].generator = "footprint".into();
    let errors = errors_of(&namespace);
    assert_eq!(errors.len(), 4, "{errors:#?}");
    assert!(
        errors
            .iter()
            .all(|e| matches!(e.kind, ValidationErrorKind::InvalidName { .. }))
    );
}

#[test]
fn rejects_long_descriptions() {
    let mut namespace = sample();
    let long = Some("x".repeat(MAX_DOC_LEN + 1));
    wall(&mut namespace).doc.clone_from(&long);
    wall(&mut namespace)
        .type_def
        .as_mut()
        .unwrap()
        .doc
        .clone_from(&long);
    wall(&mut namespace).attributes[0].doc = long;
    let errors = errors_of(&namespace);
    assert_eq!(
        errors
            .iter()
            .map(|e| e.location.clone())
            .collect::<Vec<_>>(),
        vec![
            entity_at("Wall", None),
            entity_at("Wall", Some(Item::Type)),
            entity_at("Wall", Some(Item::Attribute("axis".into()))),
        ]
    );
    assert!(
        errors
            .iter()
            .all(|e| e.kind == ValidationErrorKind::DocTooLong)
    );
}

#[test]
fn rejects_invalid_deprecation_versions() {
    for version in [0, 2] {
        let mut namespace = sample();
        wall(&mut namespace).deprecated = Some(version);
        assert_single(
            &namespace,
            entity_at("Wall", None),
            ValidationErrorKind::InvalidDeprecation { version },
        );
    }
    let mut namespace = sample();
    wall(&mut namespace).deprecated = Some(1);
    assert_eq!(validate(&namespace), Ok(()));
}

#[test]
fn rejects_type_names_that_clash_with_entities() {
    let mut namespace = sample();
    wall(&mut namespace).type_def.as_mut().unwrap().name = "Storey".into();
    assert_single(
        &namespace,
        entity_at("Wall", Some(Item::Type)),
        ValidationErrorKind::Duplicate {
            name: "Storey".into(),
        },
    );
}

#[test]
fn rejects_invalid_type_names() {
    let mut namespace = sample();
    wall(&mut namespace).type_def.as_mut().unwrap().name = "wall_type".into();
    let errors = errors_of(&namespace);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].location, entity_at("Wall", Some(Item::Type)));
}

#[test]
fn requires_an_ifc_mapping() {
    let mut namespace = sample();
    namespace.entities.get_mut("Storey").unwrap().ifc = None;
    assert_single(
        &namespace,
        entity_at("Storey", None),
        ValidationErrorKind::MissingIfcMapping,
    );
}

#[test]
fn rejects_duplicate_ifc_mappings() {
    let mut namespace = sample();
    let mut floor = entity("Floor", EntityKind::Spatial, "IfcBuildingStorey");
    floor.doc = None;
    namespace.entities.insert(floor.name.clone(), floor);
    // Entities are checked in name order, so Storey is reported against Floor.
    assert_single(
        &namespace,
        entity_at("Storey", Some(Item::Ifc)),
        ValidationErrorKind::DuplicateIfcMapping {
            other: "Floor".into(),
        },
    );
}

#[test]
fn checks_ifc_names() {
    let mut namespace = sample();
    {
        let mapping = wall(&mut namespace).ifc.as_mut().unwrap();
        mapping.entity = "Wall".into();
        mapping.predefined_type = Some("standard".into());
        mapping.type_entity = Some("IfcwallType".into());
    }
    let errors = errors_of(&namespace);
    assert_eq!(errors.len(), 3, "{errors:#?}");
    assert!(
        errors
            .iter()
            .all(|e| e.location == entity_at("Wall", Some(Item::Ifc)))
    );
}

#[test]
fn checks_user_defined_object_types() {
    let at = || entity_at("Wall", Some(Item::Ifc));
    let mut namespace = sample();
    wall(&mut namespace).ifc.as_mut().unwrap().predefined_type = Some("USERDEFINED".into());
    assert_single(&namespace, at(), ValidationErrorKind::ObjectTypeMismatch);

    wall(&mut namespace).ifc.as_mut().unwrap().object_type = Some("Curtain wall".into());
    assert_eq!(validate(&namespace), Ok(()));

    wall(&mut namespace).ifc.as_mut().unwrap().object_type = Some("bad\nname".into());
    let errors = errors_of(&namespace);
    assert!(matches!(
        errors[0].kind,
        ValidationErrorKind::InvalidName { .. }
    ));

    let mut namespace = sample();
    wall(&mut namespace).ifc.as_mut().unwrap().object_type = Some("Curtain wall".into());
    assert_single(&namespace, at(), ValidationErrorKind::ObjectTypeMismatch);
}

#[test]
fn rejects_type_entity_without_type() {
    let mut namespace = sample();
    let storey = namespace.entities.get_mut("Storey").unwrap();
    storey.ifc.as_mut().unwrap().type_entity = Some("IfcBuildingStoreyType".into());
    assert_single(
        &namespace,
        entity_at("Storey", Some(Item::Ifc)),
        ValidationErrorKind::TypeEntityWithoutType,
    );
}

#[test]
fn checks_property_set_references() {
    let mut namespace = sample();
    wall(&mut namespace).property_sets = vec![
        "Pset_WallCommon".into(),
        "Pset_WallCommon".into(),
        "Pset_Missing".into(),
    ];
    let mut beam_set = namespace.property_sets["Pset_WallCommon"].clone();
    beam_set.name = "Pset_BeamCommon".into();
    beam_set.applies_to = vec!["Storey".into()];
    namespace
        .property_sets
        .insert(beam_set.name.clone(), beam_set);
    wall(&mut namespace)
        .property_sets
        .push("Pset_BeamCommon".into());
    let kinds: Vec<_> = errors_of(&namespace).into_iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ValidationErrorKind::Duplicate {
                name: "Pset_WallCommon".into()
            },
            ValidationErrorKind::UnknownPropertySet {
                name: "Pset_Missing".into()
            },
            ValidationErrorKind::PropertySetNotApplicable {
                name: "Pset_BeamCommon".into()
            },
        ]
    );
}

#[test]
fn checks_property_set_targets_and_properties() {
    let mut namespace = sample();
    let property_set = namespace.property_sets.get_mut("Pset_WallCommon").unwrap();
    property_set.applies_to = vec!["Wall".into(), "Wall".into(), "Door".into()];
    property_set.properties[1].name = "IsExternal".into();
    let owner = || Owner::PropertySet("Pset_WallCommon".into());
    assert_eq!(
        errors_of(&namespace),
        vec![
            ValidationError {
                location: Location {
                    owner: owner(),
                    item: None
                },
                kind: ValidationErrorKind::Duplicate {
                    name: "Wall".into()
                },
            },
            ValidationError {
                location: Location {
                    owner: owner(),
                    item: None
                },
                kind: ValidationErrorKind::UnknownEntity {
                    name: "Door".into()
                },
            },
            ValidationError {
                location: Location {
                    owner: owner(),
                    item: Some(Item::Property("IsExternal".into())),
                },
                kind: ValidationErrorKind::Duplicate {
                    name: "IsExternal".into()
                },
            },
        ]
    );
}

#[test]
fn rejects_type_placement_without_type() {
    let mut namespace = sample();
    let storey = namespace.entities.get_mut("Storey").unwrap();
    storey.attributes = vec![Attribute {
        placement: Placement::Type,
        ..attribute("elevation", ValueType::Quantity(QuantityKind::Length))
    }];
    assert_single(
        &namespace,
        entity_at("Storey", Some(Item::Attribute("elevation".into()))),
        ValidationErrorKind::PlacementWithoutType,
    );
}

#[test]
fn rejects_duplicate_attribute_and_role_names() {
    let mut namespace = sample();
    wall(&mut namespace)
        .attributes
        .push(attribute("axis", ValueType::Real));
    let containment = namespace.entities.get_mut("Containment").unwrap();
    containment
        .attributes
        .push(attribute("elements", ValueType::Integer));
    assert_eq!(
        errors_of(&namespace),
        vec![
            ValidationError {
                location: entity_at("Containment", Some(Item::Role("elements".into()))),
                kind: ValidationErrorKind::Duplicate {
                    name: "elements".into()
                },
            },
            ValidationError {
                location: entity_at("Wall", Some(Item::Attribute("axis".into()))),
                kind: ValidationErrorKind::Duplicate {
                    name: "axis".into()
                },
            },
        ]
    );
}

#[test]
fn checks_references_in_value_types() {
    let mut namespace = sample();
    wall(&mut namespace).attributes.push(attribute(
        "openings",
        ValueType::List(Box::new(ValueType::Ref("Opening".into()))),
    ));
    assert_single(
        &namespace,
        entity_at("Wall", Some(Item::Attribute("openings".into()))),
        ValidationErrorKind::UnknownEntity {
            name: "Opening".into(),
        },
    );
}

#[test]
fn rejects_enumerations_until_they_can_be_declared() {
    let mut namespace = sample();
    wall(&mut namespace)
        .attributes
        .push(attribute("side", ValueType::Enum("Side".into())));
    let errors = errors_of(&namespace);
    assert_eq!(
        errors[0].kind,
        ValidationErrorKind::Unsupported("enumeration value types")
    );
    assert_eq!(
        errors[0].to_string(),
        "entity Wall, attribute side: enumeration value types are not supported yet"
    );
}

#[test]
fn checks_defaults_against_value_types() {
    let cases = [
        (ValueType::Bool, Value::Integer(1)),
        (ValueType::Integer, Value::Real(1.0)),
        (ValueType::Real, Value::Integer(1)),
        (ValueType::Text, Value::Bool(true)),
        (ValueType::Ref("Storey".into()), Value::Text("x".into())),
        (
            ValueType::List(Box::new(ValueType::Real)),
            Value::List(vec![Value::Real(1.0), Value::Text("x".into())]),
        ),
        (ValueType::List(Box::new(ValueType::Real)), Value::Real(1.0)),
    ];
    for (value_type, default) in cases {
        let mut namespace = sample();
        wall(&mut namespace).attributes.push(Attribute {
            presence: Presence::Default(default),
            ..attribute("extra", value_type.clone())
        });
        assert_single(
            &namespace,
            entity_at("Wall", Some(Item::Attribute("extra".into()))),
            ValidationErrorKind::DefaultMismatch {
                expected: value_type,
            },
        );
    }
}

#[test]
fn limits_list_nesting_in_value_types() {
    let nested =
        |depth| (0..depth).fold(ValueType::Real, |inner, _| ValueType::List(Box::new(inner)));
    let mut namespace = sample();
    wall(&mut namespace)
        .attributes
        .push(attribute("deepest", nested(MAX_LIST_DEPTH)));
    assert_eq!(validate(&namespace), Ok(()));
    wall(&mut namespace)
        .attributes
        .push(attribute("too_deep", nested(MAX_LIST_DEPTH + 1)));
    assert_single(
        &namespace,
        entity_at("Wall", Some(Item::Attribute("too_deep".into()))),
        ValidationErrorKind::TooMany {
            what: "nested list levels",
            limit: MAX_LIST_DEPTH,
        },
    );
}

#[test]
fn rejects_non_finite_defaults() {
    for value in [f64::NAN, f64::INFINITY] {
        for (value_type, default) in [
            (ValueType::Real, Value::Real(value)),
            (
                ValueType::List(Box::new(ValueType::Real)),
                Value::List(vec![Value::Real(value)]),
            ),
        ] {
            let mut namespace = sample();
            wall(&mut namespace).attributes.push(Attribute {
                presence: Presence::Default(default),
                ..attribute("extra", value_type)
            });
            assert_single(
                &namespace,
                entity_at("Wall", Some(Item::Attribute("extra".into()))),
                ValidationErrorKind::NonFiniteDefault,
            );
        }
    }
}

#[test]
fn checks_roles() {
    let mut namespace = sample();
    let containment = namespace.entities.get_mut("Containment").unwrap();
    containment.roles[0].targets.clear();
    containment.roles[1].targets = vec![RoleTarget::Entity("Door".into())];
    containment.roles[1].name = "Elements".into();
    let errors = errors_of(&namespace);
    let kinds: Vec<_> = errors.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(kinds.len(), 3, "{errors:#?}");
    assert!(kinds.contains(&ValidationErrorKind::RoleWithoutTargets));
    assert!(kinds.contains(&ValidationErrorKind::UnknownEntity {
        name: "Door".into()
    }));
}

#[test]
fn only_relationships_have_roles() {
    let mut namespace = sample();
    let roles = std::mem::take(&mut namespace.entities.get_mut("Containment").unwrap().roles);
    wall(&mut namespace).roles = roles;
    assert_eq!(
        errors_of(&namespace)
            .into_iter()
            .map(|e| (e.location, e.kind))
            .collect::<Vec<_>>(),
        vec![
            (
                entity_at("Containment", None),
                ValidationErrorKind::RelationshipWithoutRoles
            ),
            (
                entity_at("Wall", None),
                ValidationErrorKind::RolesOutsideRelationship
            ),
        ]
    );
}

#[test]
fn checks_representations() {
    let mut namespace = sample();
    let mut duplicate = wall(&mut namespace).representations[0].clone();
    duplicate.depends_on = vec!["width".into()];
    wall(&mut namespace).representations.push(duplicate);
    let at = || entity_at("Wall", Some(Item::Representation(1)));
    assert_eq!(
        errors_of(&namespace),
        vec![
            ValidationError {
                location: at(),
                kind: ValidationErrorKind::DuplicateRepresentation {
                    lod: 100,
                    context: "footprint".into()
                },
            },
            ValidationError {
                location: at(),
                kind: ValidationErrorKind::UnknownDependency {
                    name: "width".into()
                },
            },
        ]
    );
    assert_eq!(
        errors_of(&namespace)[0].to_string(),
        "entity Wall, representation #2: another representation already has level of detail \
         100 in context \"footprint\""
    );
}

#[test]
fn limits_namespace_size() {
    let mut namespace = sample();
    let storey = namespace.entities["Storey"].clone();
    for index in 0..MAX_ENTITIES {
        let mut copy = storey.clone();
        copy.name = format!("Storey{index}");
        namespace.entities.insert(copy.name.clone(), copy);
    }
    let property_set = namespace.property_sets["Pset_WallCommon"].clone();
    for index in 0..MAX_PROPERTY_SETS {
        let mut copy = property_set.clone();
        copy.name = format!("Pset_{index}");
        namespace.property_sets.insert(copy.name.clone(), copy);
    }
    let kinds: Vec<_> = errors_of(&namespace).into_iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ValidationErrorKind::TooMany {
                what: "entities",
                limit: MAX_ENTITIES
            },
            ValidationErrorKind::TooMany {
                what: "property sets",
                limit: MAX_PROPERTY_SETS
            },
        ]
    );
}

#[test]
fn limits_definition_size() {
    let too_many = MAX_ITEMS + 1;
    let mut namespace = sample();
    wall(&mut namespace).attributes = (0..too_many)
        .map(|index| attribute(&format!("a{index}"), ValueType::Real))
        .collect();
    let containment = namespace.entities.get_mut("Containment").unwrap();
    containment.roles[0].targets = vec![RoleTarget::Kind(EntityKind::Spatial); too_many];
    namespace
        .entities
        .get_mut("Storey")
        .unwrap()
        .representations = vec![
        Representation {
            lod: 100,
            context: "body".into(),
            generator: "storey::body".into(),
            depends_on: vec!["x".into(); too_many],
        };
        1
    ];
    namespace
        .property_sets
        .get_mut("Pset_WallCommon")
        .unwrap()
        .properties = (0..too_many)
        .map(|index| attribute(&format!("P{index}"), ValueType::Real))
        .collect();
    let errors = errors_of(&namespace);
    let kinds: Vec<_> = errors.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            ValidationErrorKind::TooMany {
                what: "targets",
                limit: MAX_ITEMS
            },
            ValidationErrorKind::TooMany {
                what: "dependencies",
                limit: MAX_ITEMS
            },
            ValidationErrorKind::TooMany {
                what: "attributes",
                limit: MAX_ITEMS
            },
            ValidationErrorKind::TooMany {
                what: "properties",
                limit: MAX_ITEMS
            },
        ],
        "{errors:#?}"
    );
}

#[test]
fn locations_read_naturally() {
    let location = |owner, item| Location { owner, item }.to_string();
    assert_eq!(location(Owner::Namespace, None), "namespace");
    assert_eq!(
        location(Owner::Entity("Wall".into()), Some(Item::Type)),
        "entity Wall, [type]"
    );
    assert_eq!(
        location(Owner::Entity("Wall".into()), Some(Item::Ifc)),
        "entity Wall, [ifc]"
    );
    assert_eq!(
        location(
            Owner::PropertySet("Pset_WallCommon".into()),
            Some(Item::Property("IsExternal".into()))
        ),
        "property set Pset_WallCommon, property IsExternal"
    );
    assert_eq!(
        location(
            Owner::Entity("Containment".into()),
            Some(Item::Role("elements".into()))
        ),
        "entity Containment, role elements"
    );
}

#[test]
fn every_error_kind_has_a_message() {
    let kinds = [
        ValidationErrorKind::ZeroVersion,
        ValidationErrorKind::DocTooLong,
        ValidationErrorKind::PlacementWithoutType,
        ValidationErrorKind::NonFiniteDefault,
        ValidationErrorKind::RolesOutsideRelationship,
        ValidationErrorKind::RelationshipWithoutRoles,
        ValidationErrorKind::RoleWithoutTargets,
        ValidationErrorKind::MissingIfcMapping,
        ValidationErrorKind::ObjectTypeMismatch,
        ValidationErrorKind::TypeEntityWithoutType,
    ];
    for kind in kinds {
        assert!(!kind.to_string().is_empty());
    }
    assert_eq!(
        ValidationErrorKind::DefaultMismatch {
            expected: ValueType::Quantity(QuantityKind::Length)
        }
        .to_string(),
        "default value does not match the value type quantity:length"
    );
}

#[test]
fn keys_must_match_names() {
    let mut namespace = sample();
    let storey = namespace.entities.remove("Storey").unwrap();
    namespace.entities.insert("Level".into(), storey);
    let property_set = namespace.property_sets.remove("Pset_WallCommon").unwrap();
    namespace
        .property_sets
        .insert("Pset_Other".into(), property_set);
    let errors = errors_of(&namespace);
    let key_errors: Vec<_> = errors
        .iter()
        .filter(|error| matches!(error.kind, ValidationErrorKind::KeyMismatch { .. }))
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        key_errors,
        [
            "entity Storey: listed under the name \"Level\", not its own",
            "property set Pset_WallCommon: listed under the name \"Pset_Other\", not its own",
        ]
    );
}

#[test]
fn entity_kind_names() {
    let names: Vec<_> = EntityKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(names, ["element", "spatial", "relationship"]);
}

fn arbitrary_value_type() -> impl Strategy<Value = ValueType> {
    let leaf = prop_oneof![
        Just(ValueType::Bool),
        Just(ValueType::Integer),
        Just(ValueType::Real),
        Just(ValueType::Text),
        "[A-Za-z]{0,6}".prop_map(ValueType::Ref),
        "[A-Za-z]{0,6}".prop_map(ValueType::Enum),
    ];
    leaf.prop_recursive(3, 8, 1, |inner| {
        inner.prop_map(|value| ValueType::List(Box::new(value)))
    })
}

fn arbitrary_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Integer),
        any::<f64>().prop_map(Value::Real),
        ".{0,8}".prop_map(Value::Text),
    ];
    leaf.prop_recursive(3, 8, 4, |inner| {
        prop::collection::vec(inner, 0..4).prop_map(Value::List)
    })
}

fn arbitrary_attribute() -> impl Strategy<Value = Attribute> {
    (
        "[A-Za-z_]{0,8}",
        arbitrary_value_type(),
        prop_oneof![
            Just(Presence::Required),
            Just(Presence::Optional),
            arbitrary_value().prop_map(Presence::Default),
        ],
        prop_oneof![
            Just(Placement::Instance),
            Just(Placement::Type),
            Just(Placement::TypeOverridable),
        ],
    )
        .prop_map(|(name, value, presence, placement)| Attribute {
            name,
            value,
            presence,
            placement,
            doc: None,
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        rng_seed: prop::test_runner::RngSeed::Fixed(0x5C4E_3A02),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Hostile models never make validation panic, and errors come out sorted.
    #[test]
    fn never_panics_on_arbitrary_attributes(
        attributes in prop::collection::vec(arbitrary_attribute(), 0..6),
        with_type in any::<bool>(),
    ) {
        let mut namespace = sample();
        let target = wall(&mut namespace);
        target.attributes = attributes;
        if !with_type {
            target.type_def = None;
            target.ifc.as_mut().unwrap().type_entity = None;
        }
        if let Err(errors) = validate(&namespace) {
            prop_assert!(errors.windows(2).all(|pair| pair[0].location <= pair[1].location));
        }
    }
}
