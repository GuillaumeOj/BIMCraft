use std::collections::BTreeMap;

use super::*;
use crate::{Attribute, Entity, EntityKind, Placement};

fn attribute(name: &str, value: &str, presence: Presence, placement: Placement) -> Attribute {
    Attribute {
        name: name.into(),
        value: value.parse().unwrap(),
        presence,
        placement,
        doc: None,
    }
}

fn optional(name: &str, value: &str) -> Attribute {
    attribute(name, value, Presence::Optional, Placement::Instance)
}

fn required(name: &str, value: &str) -> Attribute {
    attribute(name, value, Presence::Required, Placement::Instance)
}

fn entity(name: &str, attributes: Vec<Attribute>) -> Entity {
    Entity {
        name: name.into(),
        kind: EntityKind::Element,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: None,
        attributes,
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

fn namespace(version: u32, entities: Vec<Entity>) -> Namespace {
    Namespace {
        name: "core".into(),
        version,
        entities: entities
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::new(),
    }
}

fn wall(attributes: Vec<Attribute>) -> Entity {
    entity("Wall", attributes)
}

fn steps(before: &Namespace, declared: &[DeclaredStep], after: &Namespace) -> Vec<Step> {
    resolve(before, declared, after).unwrap().steps
}

fn errors(before: &Namespace, declared: &[DeclaredStep], after: &Namespace) -> Vec<MigrationError> {
    resolve(before, declared, after).unwrap_err()
}

fn stored(name: &str, value: &str) -> StoredAttribute {
    StoredAttribute {
        name: name.into(),
        value: value.parse().unwrap(),
    }
}

fn change_value(attribute: &str, value: &str, factor: f64) -> DeclaredStep {
    DeclaredStep::ChangeValue {
        entity: "Wall".into(),
        attribute: attribute.into(),
        value: value.parse().unwrap(),
        factor,
    }
}

fn set_value(attribute: &str, value: Value) -> DeclaredStep {
    DeclaredStep::SetValue {
        entity: "Wall".into(),
        attribute: attribute.into(),
        value,
    }
}

#[test]
fn a_version_without_stored_changes_needs_no_step() {
    let mut after = wall(vec![optional("name", "text")]);
    after.doc = Some("Documented now.".into());
    after.attributes.push(attribute(
        "fire_rating",
        "text",
        Presence::Optional,
        Placement::Type,
    ));
    let migration = resolve(
        &namespace(1, vec![wall(vec![optional("name", "text")])]),
        &[],
        &namespace(2, vec![after]),
    )
    .unwrap();
    assert_eq!(
        migration,
        Migration {
            version: 2,
            steps: Vec::new()
        }
    );
}

#[test]
fn additions_and_removals_are_inferred_in_order() {
    let before = namespace(
        1,
        vec![
            wall(vec![
                optional("name", "text"),
                optional("old", "integer"),
                optional("shared", "bool"),
            ]),
            entity("Slab", vec![optional("depth", "real")]),
        ],
    );
    let after = namespace(
        2,
        vec![
            wall(vec![
                optional("name", "text"),
                attribute(
                    "height",
                    "quantity:length",
                    Presence::Default(Value::Real(2.5)),
                    Placement::Instance,
                ),
                optional("tags", "list:text"),
                attribute("shared", "bool", Presence::Optional, Placement::Type),
                attribute(
                    "offset",
                    "real",
                    Presence::Default(Value::Real(0.0)),
                    Placement::TypeOverridable,
                ),
            ]),
            entity(
                "Beam",
                vec![
                    required("span", "quantity:length"),
                    attribute("profile", "text", Presence::Optional, Placement::Type),
                ],
            ),
        ],
    );
    assert_eq!(
        steps(&before, &[], &after),
        [
            Step::AddEntity {
                entity: "Beam".into(),
                attributes: vec![stored("span", "quantity:length")],
            },
            Step::AddAttribute {
                entity: "Wall".into(),
                attribute: stored("height", "quantity:length"),
                default: Some(Value::Real(2.5)),
            },
            Step::AddAttribute {
                entity: "Wall".into(),
                attribute: stored("tags", "list:text"),
                default: None,
            },
            Step::AddAttribute {
                entity: "Wall".into(),
                attribute: stored("offset", "real"),
                default: None,
            },
            Step::RemoveAttribute {
                entity: "Wall".into(),
                attribute: "old".into(),
            },
            Step::RemoveAttribute {
                entity: "Wall".into(),
                attribute: "shared".into(),
            },
            Step::RemoveEntity {
                entity: "Slab".into(),
            },
        ]
    );
}

#[test]
fn renames_keep_data_and_references() {
    let before = namespace(
        1,
        vec![
            entity("Storey", vec![optional("elevation", "real")]),
            wall(vec![
                required("base", "ref:Storey"),
                optional("hosts", "list:ref:Storey"),
                optional("base_offset", "real"),
            ]),
        ],
    );
    let after = namespace(
        2,
        vec![
            entity("Level", vec![optional("elevation", "real")]),
            wall(vec![
                required("base", "ref:Level"),
                optional("hosts", "list:ref:Level"),
                optional("bottom_offset", "real"),
            ]),
        ],
    );
    let declared = [
        DeclaredStep::RenameEntity {
            old_name: "Storey".into(),
            new_name: "Level".into(),
        },
        DeclaredStep::RenameAttribute {
            entity: "Wall".into(),
            old_name: "base_offset".into(),
            new_name: "bottom_offset".into(),
        },
    ];
    assert_eq!(
        steps(&before, &declared, &after),
        [
            Step::RenameEntity {
                from: "Storey".into(),
                to: "Level".into(),
            },
            Step::RenameAttribute {
                entity: "Wall".into(),
                from: "base_offset".into(),
                to: "bottom_offset".into(),
            },
        ]
    );
}

#[test]
fn value_type_changes_need_a_conversion() {
    let before = namespace(1, vec![wall(vec![optional("thickness", "real")])]);
    let after = namespace(
        2,
        vec![wall(vec![optional("thickness", "quantity:length")])],
    );
    assert_eq!(
        errors(&before, &[], &after),
        [MigrationError::TypeChanged {
            entity: "Wall".into(),
            attribute: "thickness".into(),
            found: ValueType::Real,
            expected: "quantity:length".parse().unwrap(),
        }]
    );
    assert_eq!(
        steps(
            &before,
            &[change_value("thickness", "quantity:length", 0.001)],
            &after
        ),
        [Step::ChangeValue {
            entity: "Wall".into(),
            attribute: "thickness".into(),
            from: ValueType::Real,
            to: "quantity:length".parse().unwrap(),
            factor: 0.001,
        }]
    );
}

#[test]
fn required_values_need_a_value_for_existing_objects() {
    let before = namespace(1, vec![wall(vec![optional("name", "text")])]);
    let after = namespace(
        2,
        vec![wall(vec![
            required("name", "text"),
            required("layer_count", "integer"),
        ])],
    );
    let unfilled = |attribute: &str| MigrationError::Unfilled {
        entity: "Wall".into(),
        attribute: attribute.into(),
    };
    assert_eq!(
        errors(&before, &[], &after),
        [unfilled("layer_count"), unfilled("name")]
    );
    let declared = [
        set_value("name", Value::Text("Unnamed".into())),
        set_value("layer_count", Value::Integer(1)),
    ];
    assert_eq!(steps(&before, &declared, &after).len(), 3);
}

#[test]
fn custom_steps_take_over_their_entities() {
    let before = namespace(1, vec![wall(vec![optional("layers", "text")])]);
    let after = namespace(2, vec![wall(vec![required("layers", "list:text")])]);
    let declared = [DeclaredStep::Custom {
        entities: vec!["Wall".into()],
        function: "migrations::v2::split_layers".into(),
    }];
    assert_eq!(
        steps(&before, &declared, &after),
        [Step::Custom {
            entities: vec!["Wall".into()],
            function: "migrations::v2::split_layers".into(),
        }]
    );
}

#[test]
fn steps_naming_missing_or_taken_items_are_refused() {
    let before = namespace(
        1,
        vec![
            wall(vec![optional("name", "text"), optional("tag", "text")]),
            entity("Slab", Vec::new()),
        ],
    );
    let after = namespace(
        2,
        vec![
            wall(vec![optional("name", "text"), optional("tag", "text")]),
            entity("Slab", Vec::new()),
        ],
    );
    let declared = [
        DeclaredStep::RenameEntity {
            old_name: "Door".into(),
            new_name: "Window".into(),
        },
        DeclaredStep::RenameEntity {
            old_name: "Slab".into(),
            new_name: "Wall".into(),
        },
        DeclaredStep::RenameAttribute {
            entity: "Wall".into(),
            old_name: "label".into(),
            new_name: "x".into(),
        },
        DeclaredStep::RenameAttribute {
            entity: "Wall".into(),
            old_name: "tag".into(),
            new_name: "name".into(),
        },
        set_value("height", Value::Real(1.0)),
        set_value("name", Value::Integer(1)),
        set_value("name", Value::List(vec![Value::Real(f64::NAN)])),
        change_value("name", "integer", 1.0),
        DeclaredStep::Custom {
            entities: vec!["Roof".into()],
            function: "f".into(),
        },
    ];
    assert_eq!(
        errors(&before, &declared, &after),
        [
            MigrationError::UnknownEntity {
                step: 1,
                entity: "Door".into()
            },
            MigrationError::NameTaken {
                step: 2,
                name: "Wall".into()
            },
            MigrationError::UnknownAttribute {
                step: 3,
                entity: "Wall".into(),
                attribute: "label".into()
            },
            MigrationError::NameTaken {
                step: 4,
                name: "name".into()
            },
            MigrationError::UnknownAttribute {
                step: 5,
                entity: "Wall".into(),
                attribute: "height".into()
            },
            MigrationError::ValueMismatch {
                step: 6,
                expected: ValueType::Text
            },
            MigrationError::ValueMismatch {
                step: 7,
                expected: ValueType::Text
            },
            MigrationError::Inconvertible {
                step: 8,
                from: ValueType::Text,
                to: ValueType::Integer,
                factor: 1.0
            },
            MigrationError::UnknownEntity {
                step: 9,
                entity: "Roof".into()
            },
        ]
    );
}

#[test]
fn only_consecutive_versions_of_one_namespace_migrate() {
    let v1 = namespace(1, Vec::new());
    assert_eq!(
        errors(&v1, &[], &namespace(3, Vec::new())),
        [MigrationError::VersionGap { from: 1, to: 3 }]
    );
    let mut other = namespace(2, Vec::new());
    other.name = "plugin:acme".into();
    assert_eq!(errors(&v1, &[], &other), [MigrationError::NamespaceChanged]);
    let many = vec![
        DeclaredStep::Custom {
            entities: Vec::new(),
            function: "f".into()
        };
        MAX_STEPS + 1
    ];
    assert_eq!(
        errors(&v1, &many, &namespace(2, Vec::new())),
        [MigrationError::TooManySteps]
    );
    let last = namespace(u32::MAX, Vec::new());
    assert!(matches!(
        errors(&last, &[], &namespace(1, Vec::new()))[..],
        [MigrationError::VersionGap { .. }]
    ));
}

#[test]
fn conversions_follow_the_value_kinds() {
    let convert = |from: &str, to: &str, factor: f64| {
        converts(&from.parse().unwrap(), &to.parse().unwrap(), factor)
    };
    assert!(convert("integer", "real", 1.0));
    assert!(convert("real", "quantity:length", 0.001));
    assert!(convert("integer", "quantity:area", 1e-6));
    assert!(convert("list:real", "list:quantity:length", 0.001));
    assert!(convert("text", "enum:Side", 1.0));
    assert!(convert("enum:Side", "text", 1.0));
    assert!(convert("ref:Storey", "ref:Level", 1.0));
    assert!(convert("bool", "bool", 1.0));
    assert!(convert("integer", "integer", 1.0));
    assert!(!convert("real", "integer", 1.0));
    assert!(!convert("text", "bool", 1.0));
    assert!(!convert("list:text", "text", 1.0));
    assert!(!convert("integer", "integer", 2.0));
    assert!(!convert("real", "real", 0.0));
    assert!(!convert("real", "real", f64::INFINITY));
}

#[test]
fn messages_name_schema_items_and_the_fix() {
    assert_eq!(
        MigrationError::Unfilled {
            entity: "Wall".into(),
            attribute: "name".into()
        }
        .to_string(),
        "attribute \"name\" of \"Wall\" is required but objects may have no value: add a set_value or custom step"
    );
    assert_eq!(
        MigrationError::Inconvertible {
            step: 2,
            from: ValueType::Text,
            to: ValueType::Integer,
            factor: 1.0
        }
        .to_string(),
        "step 2: values of type text cannot become integer with factor 1"
    );
}

fn typed_wall(attributes: Vec<Attribute>) -> Entity {
    Entity {
        type_def: Some(crate::TypeDef {
            name: "WallType".into(),
            doc: None,
        }),
        ..wall(attributes)
    }
}

#[test]
fn type_objects_are_stored_in_their_own_table() {
    let type_attribute =
        |name: &str, value: &str, presence| attribute(name, value, presence, Placement::Type);
    let untyped = namespace(1, vec![wall(vec![optional("name", "text")])]);
    let typed = namespace(
        2,
        vec![typed_wall(vec![
            optional("name", "text"),
            type_attribute("fire_rating", "text", Presence::Optional),
            attribute(
                "height",
                "real",
                Presence::Default(Value::Real(2.5)),
                Placement::TypeOverridable,
            ),
        ])],
    );
    assert_eq!(
        steps(&untyped, &[], &typed),
        [
            Step::AddAttribute {
                entity: "Wall".into(),
                attribute: stored("height", "real"),
                default: None,
            },
            Step::AddEntity {
                entity: "WallType".into(),
                attributes: vec![stored("fire_rating", "text"), stored("height", "real")],
            },
        ]
    );

    let mut changed = typed.clone();
    changed.version = 3;
    let wall = changed.entities.get_mut("Wall").unwrap();
    wall.attributes[1] = type_attribute("fire_rating", "integer", Presence::Optional);
    wall.attributes
        .push(type_attribute("code", "text", Presence::Required));
    assert_eq!(
        errors(&typed, &[], &changed),
        [
            MigrationError::Unfilled {
                entity: "WallType".into(),
                attribute: "code".into(),
            },
            MigrationError::TypeChanged {
                entity: "WallType".into(),
                attribute: "fire_rating".into(),
                found: ValueType::Text,
                expected: ValueType::Integer,
            },
        ]
    );
}
