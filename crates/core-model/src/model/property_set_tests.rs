use schema::{Placement, Presence, PropertySet, ValueType};

use super::tests::{STOREY, WALLS, id, model, wall_common};
use super::*;
use crate::units::{Area, AreaUnit, Dimension, Length, LengthUnit};

const BUILT_IN: &str = "Pset_WallCommon";
const CUSTOM: &str = "Acme_WallChecks";

fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

fn bool(value: bool) -> AttributeValue {
    AttributeValue::Bool(value)
}

/// The test model with a custom set that has the same properties as the built-in one.
fn with_custom_set() -> Model {
    let mut model = model();
    model
        .define_property_set(PropertySet {
            name: CUSTOM.into(),
            ..wall_common()
        })
        .unwrap();
    model
}

fn reference() -> (&'static str, AttributeValue) {
    ("Reference", text("W-01"))
}

#[test]
fn built_in_and_custom_sets_are_one_mechanism() {
    let model = with_custom_set();
    assert_eq!(
        model.property_set(CUSTOM).map(|set| &set.properties),
        model.property_set(BUILT_IN).map(|set| &set.properties)
    );
    for set in [BUILT_IN, CUSTOM] {
        let mut model = model.clone();
        let wall = id(WALLS[0]);
        model
            .attach_property_set(wall, set, [reference(), ("IsExternal", bool(false))])
            .unwrap();
        assert_eq!(
            model.property(wall, set, "Reference"),
            Ok(Some(text("W-01")))
        );
        assert_eq!(
            model.property(wall, set, "IsExternal"),
            Ok(Some(bool(false)))
        );
        // Defaults fill properties without a value.
        assert_eq!(
            model.property(wall, set, "LoadBearing"),
            Ok(Some(bool(false)))
        );
        assert_eq!(
            model.property(wall, set, "Width"),
            Ok(Some(AttributeValue::Real(0.2)))
        );
        assert_eq!(model.property(wall, set, "FireRating"), Ok(None));
        assert_eq!(
            model.set_property(wall, set, "LoadBearing", Some(bool(true))),
            Ok(Some(bool(false)))
        );
        assert_eq!(
            model.set_property(wall, set, "IsExternal", Some(text("yes"))),
            Err(ModelError::PropertyValue {
                set: set.into(),
                error: StoreError::TypeMismatch("IsExternal".into()),
            })
        );
        assert_eq!(model.property_sets_of(wall).collect::<Vec<_>>(), [set]);
    }
}

#[test]
fn custom_sets_are_validated_like_built_in_ones() {
    let mut model = model();
    assert_eq!(
        model.define_property_set(wall_common()),
        Err(ModelError::PropertySetExists(BUILT_IN.into()))
    );
    let invalid = [
        PropertySet {
            name: CUSTOM.into(),
            applies_to: vec!["Roof".into()],
            ..wall_common()
        },
        PropertySet {
            name: "1st set".into(),
            ..wall_common()
        },
    ];
    for set in invalid {
        let name = set.name.clone();
        assert!(matches!(
            model.define_property_set(set),
            Err(ModelError::InvalidPropertySet(errors)) if !errors.is_empty()
        ));
        assert_eq!(model.property_set(&name), None);
    }
    // A rejected set left nothing behind: the same name can be defined afterwards.
    model
        .define_property_set(PropertySet {
            name: CUSTOM.into(),
            ..wall_common()
        })
        .unwrap();
    assert!(model.property_set(CUSTOM).is_some());
}

#[test]
fn objects_carry_the_sets_they_have_values_for() {
    let mut model = with_custom_set();
    let wall = id(WALLS[0]);
    assert_eq!(model.property_sets_of(wall).count(), 0);
    // Reading never attaches; defaults show through.
    assert_eq!(
        model.property(wall, BUILT_IN, "IsExternal"),
        Ok(Some(bool(true)))
    );
    assert_eq!(model.property(wall, BUILT_IN, "Reference"), Ok(None));
    // Clearing a property of a set the object does not carry changes nothing.
    assert_eq!(
        model.set_property(wall, BUILT_IN, "IsExternal", None),
        Ok(None)
    );
    assert_eq!(model.property_sets_of(wall).count(), 0);

    model
        .attach_property_set(wall, CUSTOM, [reference()])
        .unwrap();
    model
        .attach_property_set(wall, BUILT_IN, [reference()])
        .unwrap();
    assert_eq!(
        model.property_sets_of(wall).collect::<Vec<_>>(),
        [CUSTOM, BUILT_IN]
    );
    assert_eq!(
        model.detach_property_set(wall, CUSTOM),
        Ok(vec![
            ("Reference".into(), text("W-01")),
            ("LoadBearing".into(), bool(false)),
            ("Width".into(), AttributeValue::Real(0.2)),
        ])
    );
    assert_eq!(model.property_sets_of(wall).collect::<Vec<_>>(), [BUILT_IN]);
}

#[test]
fn setting_a_property_attaches_its_set() {
    let mut model = model();
    // Sets without required properties attach on the first value.
    let mut optional = wall_common();
    optional.name = CUSTOM.into();
    optional.properties[0].presence = Presence::Optional;
    model.define_property_set(optional).unwrap();
    let wall = id(WALLS[1]);
    assert_eq!(
        model.set_property(wall, CUSTOM, "IsExternal", Some(bool(false))),
        Ok(None)
    );
    assert_eq!(model.property_sets_of(wall).collect::<Vec<_>>(), [CUSTOM]);
    assert_eq!(
        model.property(wall, CUSTOM, "IsExternal"),
        Ok(Some(bool(false)))
    );
    // Clearing an override brings the default back.
    assert_eq!(
        model.set_property(wall, CUSTOM, "IsExternal", None),
        Ok(Some(bool(false)))
    );
    assert_eq!(
        model.property(wall, CUSTOM, "IsExternal"),
        Ok(Some(bool(true)))
    );

    // A set with a required property needs it before anything else.
    assert_eq!(
        model.set_property(wall, BUILT_IN, "IsExternal", Some(bool(false))),
        Err(ModelError::PropertyValue {
            set: BUILT_IN.into(),
            error: StoreError::MissingRequired("Reference".into()),
        })
    );
    assert_eq!(
        model.set_property(wall, BUILT_IN, "Reference", Some(text("W-02"))),
        Ok(None)
    );
    assert_eq!(
        model.set_property(wall, BUILT_IN, "Reference", None),
        Err(ModelError::PropertyValue {
            set: BUILT_IN.into(),
            error: StoreError::MissingRequired("Reference".into()),
        })
    );
}

/// An object, a set and values to attach, and the error attaching them must fail with.
type AttachCase = (
    EntityId,
    &'static str,
    Vec<(&'static str, AttributeValue)>,
    ModelError,
);

/// Attachments to a fresh wall that must fail, one per check.
fn attach_error_cases() -> Vec<AttachCase> {
    let wall = id(WALLS[0]);
    let value_error = |error| ModelError::PropertyValue {
        set: BUILT_IN.into(),
        error,
    };
    vec![
        (
            id(999),
            BUILT_IN,
            vec![reference()],
            ModelError::Store(StoreError::NotFound(id(999))),
        ),
        (
            wall,
            "Pset_Unknown",
            vec![reference()],
            ModelError::UnknownPropertySet("Pset_Unknown".into()),
        ),
        (
            id(STOREY),
            BUILT_IN,
            vec![reference()],
            ModelError::NotApplicable {
                set: BUILT_IN.into(),
                entity: "Storey".into(),
            },
        ),
        (
            wall,
            BUILT_IN,
            vec![reference(), ("Colour", text("red"))],
            ModelError::UnknownProperty {
                set: BUILT_IN.into(),
                property: "Colour".into(),
            },
        ),
        (
            wall,
            BUILT_IN,
            vec![reference(), ("FireRating", text("REI 60"))],
            ModelError::NotOverridable {
                owner: BUILT_IN.into(),
                name: "FireRating".into(),
            },
        ),
        (
            wall,
            BUILT_IN,
            vec![reference(), reference()],
            value_error(StoreError::DuplicateAttribute("Reference".into())),
        ),
        (
            wall,
            BUILT_IN,
            vec![("IsExternal", bool(true))],
            value_error(StoreError::MissingRequired("Reference".into())),
        ),
        (
            wall,
            BUILT_IN,
            vec![reference(), ("LoadBearing", text("yes"))],
            value_error(StoreError::TypeMismatch("LoadBearing".into())),
        ),
        (
            wall,
            BUILT_IN,
            vec![reference(), ("Width", AttributeValue::Real(f64::NAN))],
            value_error(StoreError::NonFinite("Width".into())),
        ),
    ]
}

#[test]
fn invalid_values_return_typed_errors_and_change_nothing() {
    let mut model = model();
    let wall = id(WALLS[0]);
    let cases = attach_error_cases();
    for (object, set, values, error) in cases {
        assert_eq!(model.attach_property_set(object, set, values), Err(error));
        assert_eq!(model.property_sets_of(wall).count(), 0);
    }

    model
        .attach_property_set(wall, BUILT_IN, [reference()])
        .unwrap();
    assert_eq!(
        model.attach_property_set(wall, BUILT_IN, [reference()]),
        Err(ModelError::AlreadyAttached(BUILT_IN.into()))
    );
    assert_eq!(
        model.set_property(wall, BUILT_IN, "FireRating", Some(text("REI 60"))),
        Err(ModelError::NotOverridable {
            owner: BUILT_IN.into(),
            name: "FireRating".into(),
        })
    );
    assert_eq!(
        model.property(wall, BUILT_IN, "Colour"),
        Err(ModelError::UnknownProperty {
            set: BUILT_IN.into(),
            property: "Colour".into(),
        })
    );
    assert_eq!(
        model.detach_property_set(id(WALLS[1]), BUILT_IN),
        Err(ModelError::NotAttached(BUILT_IN.into()))
    );
    assert_eq!(
        model.property(wall, BUILT_IN, "Reference"),
        Ok(Some(text("W-01")))
    );
}

#[test]
fn quantities_carry_their_unit() {
    let mut model = model();
    let wall = id(WALLS[0]);
    model
        .attach_property_set(wall, BUILT_IN, [reference()])
        .unwrap();
    assert_eq!(
        model.quantity_property::<LengthUnit>(wall, BUILT_IN, "Width"),
        Ok(Some(Length::new(200.0, LengthUnit::Millimetre)))
    );
    let width = Length::new(250.0, LengthUnit::Millimetre);
    assert_eq!(
        model.set_quantity_property(wall, BUILT_IN, "Width", Some(width)),
        Ok(Some(Length::new(0.2, LengthUnit::Metre)))
    );
    // Stored in SI (CORE-09).
    assert_eq!(
        model.property(wall, BUILT_IN, "Width"),
        Ok(Some(AttributeValue::Real(0.25)))
    );
    assert_eq!(
        model.quantity_property::<LengthUnit>(wall, BUILT_IN, "Width"),
        Ok(Some(width))
    );
    let area_mismatch = |property: &str| ModelError::QuantityMismatch {
        property: property.into(),
        dimension: Dimension::Area,
    };
    assert_eq!(
        model.quantity_property::<AreaUnit>(wall, BUILT_IN, "Width"),
        Err(area_mismatch("Width"))
    );
    assert_eq!(
        model.set_quantity_property(
            wall,
            BUILT_IN,
            "Width",
            Some(Area::new(1.0, AreaUnit::SquareMetre))
        ),
        Err(area_mismatch("Width"))
    );
    assert_eq!(
        model.quantity_property::<AreaUnit>(wall, BUILT_IN, "IsExternal"),
        Err(area_mismatch("IsExternal"))
    );
    assert_eq!(
        model.quantity_property::<LengthUnit>(wall, "Pset_Unknown", "Width"),
        Err(ModelError::UnknownPropertySet("Pset_Unknown".into()))
    );
    assert_eq!(
        model.quantity_property::<LengthUnit>(wall, BUILT_IN, "Height"),
        Err(ModelError::UnknownProperty {
            set: BUILT_IN.into(),
            property: "Height".into(),
        })
    );
    assert_eq!(
        model.property(wall, BUILT_IN, "Width"),
        Ok(Some(AttributeValue::Real(0.25)))
    );
}

#[test]
fn quantity_properties_without_value_read_as_none() {
    let mut model = model();
    let mut set = wall_common();
    set.name = CUSTOM.into();
    set.properties[4].presence = Presence::Optional;
    model.define_property_set(set).unwrap();
    let wall = id(WALLS[0]);
    assert_eq!(
        model.quantity_property::<LengthUnit>(wall, CUSTOM, "Width"),
        Ok(None)
    );
    model
        .attach_property_set(wall, CUSTOM, [reference()])
        .unwrap();
    assert_eq!(
        model.set_quantity_property::<LengthUnit>(wall, CUSTOM, "Width", None),
        Ok(None)
    );
}

#[test]
fn deleted_objects_take_their_properties_with_them() {
    let mut model = with_custom_set();
    let wall = id(WALLS[0]);
    for set in [BUILT_IN, CUSTOM] {
        model
            .attach_property_set(wall, set, [reference(), ("IsExternal", bool(false))])
            .unwrap();
    }
    model
        .attach_property_set(id(WALLS[1]), BUILT_IN, [("Reference", text("W-02"))])
        .unwrap();
    let before = model.clone();

    let removal = model.remove(wall).unwrap();
    assert_eq!(model.property_sets_of(wall).count(), 0);
    // The other wall's row moved but kept its values.
    assert_eq!(
        model.property(id(WALLS[1]), BUILT_IN, "Reference"),
        Ok(Some(text("W-02")))
    );
    model.restore(removal).unwrap();
    for set in [BUILT_IN, CUSTOM] {
        for property in ["Reference", "IsExternal", "LoadBearing", "Width"] {
            assert_eq!(
                model.property(wall, set, property),
                before.property(wall, set, property)
            );
        }
    }
    assert_eq!(
        model.property_sets_of(wall).collect::<Vec<_>>(),
        [CUSTOM, BUILT_IN]
    );
    assert_eq!(
        model.property(id(WALLS[1]), BUILT_IN, "Reference"),
        Ok(Some(text("W-02")))
    );
}

#[test]
fn type_placed_properties_are_not_stored_on_instances() {
    let set = wall_common();
    let fire_rating = &set.properties[3];
    assert_eq!(fire_rating.placement, Placement::Type);
    assert_eq!(fire_rating.value, ValueType::Text);
    let mut model = model();
    let wall = id(WALLS[0]);
    model
        .attach_property_set(wall, BUILT_IN, [reference()])
        .unwrap();
    assert_eq!(model.property(wall, BUILT_IN, "FireRating"), Ok(None));
}

#[test]
fn property_errors_name_sets_and_properties_but_never_values() {
    let messages = [
        (
            ModelError::UnknownPropertySet("Pset_X".into()),
            "unknown property set \"Pset_X\"",
        ),
        (
            ModelError::PropertySetExists("Pset_X".into()),
            "property set \"Pset_X\" already exists",
        ),
        (
            ModelError::InvalidPropertySet(Vec::new()),
            "the property set does not pass validation",
        ),
        (
            ModelError::NotApplicable {
                set: "Pset_X".into(),
                entity: "Storey".into(),
            },
            "property set \"Pset_X\" does not apply to entity \"Storey\"",
        ),
        (
            ModelError::UnknownProperty {
                set: "Pset_X".into(),
                property: "Colour".into(),
            },
            "property set \"Pset_X\" has no property \"Colour\"",
        ),
        (
            ModelError::AlreadyAttached("Pset_X".into()),
            "the object already carries property set \"Pset_X\"",
        ),
        (
            ModelError::NotAttached("Pset_X".into()),
            "the object does not carry property set \"Pset_X\"",
        ),
        (
            ModelError::PropertyValue {
                set: "Pset_X".into(),
                error: StoreError::TypeMismatch("Width".into()),
            },
            "property set \"Pset_X\": attribute \"Width\" does not accept a value of this type",
        ),
        (
            ModelError::NotOverridable {
                owner: "Pset_X".into(),
                name: "FireRating".into(),
            },
            "\"FireRating\" of \"Pset_X\" is set on the type and cannot be overridden",
        ),
        (
            ModelError::QuantityMismatch {
                property: "Width".into(),
                dimension: Dimension::Area,
            },
            "property \"Width\" does not hold quantities of area",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}
