use core_model::{AttributeValue, EntityId, Source};
use core_txn::{Document, ModelCommand, Origin};

use super::super::tests::{ID, s355, stored};
use super::*;
use crate::Category;

const ORIGINAL: EntityId = EntityId::new(70);
const OTHER_ORIGINAL: EntityId = EntityId::new(71);

fn run(document: &mut Document<ModelCommand>, command: ModelCommand) {
    document.execute(command, Origin::Ui).unwrap();
}

fn library(value: f64) -> Sourced<f64> {
    Sourced::new(value, Source::new("EN 1993-1-1:2005", "Table 3.1").unwrap())
}

fn entered(value: f64) -> Sourced<f64> {
    Sourced::new(value, Source::user())
}

fn yield_strength(document: &Document<ModelCommand>) -> Option<Sourced<f64>> {
    read(document.model(), ID)
        .unwrap()
        .property(Property::YieldStrength)
        .cloned()
}

#[test]
fn an_edited_value_is_entered_by_the_user_and_keeps_the_library_original() {
    let mut document = stored();
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        345e6,
        ORIGINAL,
    );
    run(&mut document, command.unwrap());
    assert_eq!(yield_strength(&document), Some(entered(345e6)));
    let kept = originals(document.model(), ID).unwrap();
    assert_eq!(
        kept,
        vec![Original {
            id: ORIGINAL,
            property: Property::YieldStrength,
            value: library(355e6),
        }]
    );

    // A second edit keeps the first original, not the user's earlier value.
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        340e6,
        OTHER_ORIGINAL,
    );
    run(&mut document, command.unwrap());
    assert_eq!(yield_strength(&document), Some(entered(340e6)));
    assert_eq!(originals(document.model(), ID).unwrap(), kept);
    assert!(!document.model().contains(OTHER_ORIGINAL));
}

#[test]
fn reset_restores_the_library_value_and_source() {
    let mut document = stored();
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        345e6,
        ORIGINAL,
    );
    run(&mut document, command.unwrap());
    let command = reset_value(document.model(), ID, Property::YieldStrength).unwrap();
    run(&mut document, command);
    assert_eq!(read(document.model(), ID), Ok(s355()));
    assert_eq!(originals(document.model(), ID), Ok(Vec::new()));
    assert!(!document.model().contains(ORIGINAL));

    // Undo brings the edit back, with its original; redo resets again.
    document.undo().unwrap();
    assert_eq!(yield_strength(&document), Some(entered(345e6)));
    assert_eq!(originals(document.model(), ID).unwrap().len(), 1);
    document.redo().unwrap();
    assert_eq!(read(document.model(), ID), Ok(s355()));
}

#[test]
fn a_value_the_library_never_gave_has_nothing_to_reset() {
    let mut document = stored();
    let command = set_value(
        document.model(),
        ID,
        Property::ThermalConductivity,
        50.0,
        ORIGINAL,
    );
    let command = command.unwrap();
    assert!(matches!(&command, ModelCommand::Batch { commands } if commands.len() == 1));
    run(&mut document, command);
    assert_eq!(
        reset_value(document.model(), ID, Property::ThermalConductivity),
        Err(EditError::NothingToReset(Property::ThermalConductivity))
    );
    assert_eq!(
        reset_value(document.model(), ID, Property::YieldStrength),
        Err(EditError::NothingToReset(Property::YieldStrength))
    );
}

#[test]
fn edits_that_break_the_material_are_refused() {
    let document = stored();
    // fy above fu.
    assert_eq!(
        set_value(
            document.model(),
            ID,
            Property::YieldStrength,
            600e6,
            ORIGINAL
        ),
        Err(EditError::Invalid(MaterialError::Inconsistent {
            property: Property::UltimateStrength,
            below: Property::YieldStrength,
        }))
    );
    assert_eq!(
        set_value(
            document.model(),
            ID,
            Property::YieldStrength,
            -1.0,
            ORIGINAL
        ),
        Err(EditError::Invalid(MaterialError::OutOfRange(
            Property::YieldStrength
        )))
    );
    assert_eq!(
        set_value(
            document.model(),
            ID,
            Property::CharacteristicCompressiveStrength,
            1.0,
            ORIGINAL
        ),
        Err(EditError::Invalid(MaterialError::PropertyNotForCategory {
            category: Category::Steel,
            property: Property::CharacteristicCompressiveStrength,
        }))
    );
    assert_eq!(
        set_value(
            document.model(),
            ORIGINAL,
            Property::YieldStrength,
            1.0,
            ORIGINAL
        ),
        Err(EditError::Read(ReadError::NotAMaterial))
    );
    assert_eq!(
        reset_value(document.model(), ORIGINAL, Property::YieldStrength),
        Err(EditError::Read(ReadError::NotAMaterial))
    );
}

#[test]
fn a_reset_that_no_longer_fits_is_refused() {
    let mut document = stored();
    // fu 490 → 600 (entered), then fy 355 → 550 (entered); restoring fu = 490 would put it
    // below fy.
    let command = set_value(
        document.model(),
        ID,
        Property::UltimateStrength,
        600e6,
        ORIGINAL,
    );
    run(&mut document, command.unwrap());
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        550e6,
        OTHER_ORIGINAL,
    );
    run(&mut document, command.unwrap());
    assert_eq!(
        reset_value(document.model(), ID, Property::UltimateStrength),
        Err(EditError::Invalid(MaterialError::Inconsistent {
            property: Property::UltimateStrength,
            below: Property::YieldStrength,
        }))
    );
}

#[test]
fn deleting_a_material_deletes_its_originals_in_one_step() {
    let mut document = stored();
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        345e6,
        ORIGINAL,
    );
    run(&mut document, command.unwrap());
    let command = delete(document.model(), ID).unwrap();
    run(&mut document, command);
    assert!(!document.model().contains(ID));
    assert!(!document.model().contains(ORIGINAL));
    document.undo().unwrap();
    assert!(document.model().contains(ORIGINAL));
    assert_eq!(yield_strength(&document), Some(entered(345e6)));
    assert_eq!(
        delete(document.model(), ORIGINAL),
        Err(EditError::Read(ReadError::NotAMaterial))
    );
}

#[test]
fn malformed_originals_are_skipped() {
    let mut document = stored();
    let original = |key: &str, value: AttributeValue| ModelCommand::Create {
        id: ORIGINAL,
        entity: ORIGINAL_ENTITY.into(),
        values: vec![
            ("material".into(), AttributeValue::Ref(ID)),
            ("key".into(), AttributeValue::Text(key.into())),
            ("value".into(), value),
        ],
    };
    let value = AttributeValue::sourced(AttributeValue::Real(1.0), Source::user());
    run(&mut document, original("not_a_property", value));
    run(
        &mut document,
        ModelCommand::Create {
            id: OTHER_ORIGINAL,
            entity: ORIGINAL_ENTITY.into(),
            values: vec![
                ("material".into(), AttributeValue::Ref(EntityId::new(99))),
                ("key".into(), AttributeValue::Text("fy".into())),
                (
                    "value".into(),
                    AttributeValue::sourced(AttributeValue::Real(1.0), Source::user()),
                ),
            ],
        },
    );
    assert_eq!(originals(document.model(), ID), Ok(Vec::new()));
}

#[test]
fn the_library_key_is_stored_and_read_back() {
    let mut document = Document::new(super::super::tests::model());
    let material = s355().with_library_key(" steel/S355 ").unwrap();
    assert_eq!(material.library_key(), Some("steel/S355"));
    run(&mut document, super::super::commands(ID, &material));
    assert_eq!(read(document.model(), ID), Ok(material.clone()));
    let edited = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        345e6,
        ORIGINAL,
    )
    .unwrap();
    run(&mut document, edited);
    assert_eq!(
        read(document.model(), ID).unwrap().library_key(),
        Some("steel/S355")
    );
    assert_eq!(
        s355().with_library_key(&"x".repeat(201)),
        Err(MaterialError::TooLong("library key"))
    );
    assert_eq!(material.with_library_key("").unwrap().library_key(), None);
}

#[test]
fn edit_errors_explain_without_values() {
    assert_eq!(
        EditError::NothingToReset(Property::YieldStrength).to_string(),
        "the property fy has no library value to restore"
    );
}

#[test]
fn the_generic_delete_refuses_a_material_that_originals_still_refer_to() {
    let mut document = stored();
    let command = set_value(
        document.model(),
        ID,
        Property::YieldStrength,
        345e6,
        ORIGINAL,
    );
    run(&mut document, command.unwrap());
    let before = document.model().clone();
    let history = document.history().count();

    let result = document.execute(ModelCommand::Delete { id: ID }, Origin::Ui);

    assert_eq!(
        result.unwrap_err().to_string(),
        "1 other object(s) still refer to the object"
    );
    assert_eq!(document.model(), &before);
    assert_eq!(document.history().count(), history);
    // The material's own delete removes the original first, then the material, in one step.
    let command = delete(document.model(), ID).unwrap();
    run(&mut document, command);
    assert!(!document.model().contains(ID));
    assert!(!document.model().contains(ORIGINAL));
    assert_eq!(document.history().count(), history + 1);
}

#[test]
fn several_values_are_checked_together_and_edited_in_one_batch() {
    let mut document = stored();
    let ids = |property: Property| match property {
        Property::YieldStrength => ORIGINAL,
        _ => OTHER_ORIGINAL,
    };
    // fy alone above fu would be refused; both raised together are valid.
    let alone = edit(
        document.model(),
        ID,
        &[(Property::YieldStrength, Edit::Set(600e6))],
        ids,
    );
    assert!(alone.is_err());
    let both = [
        (Property::YieldStrength, Edit::Set(500e6)),
        (Property::UltimateStrength, Edit::Set(650e6)),
        (Property::ThermalConductivity, Edit::Set(50.0)),
    ];
    let command = edit(document.model(), ID, &both, ids).unwrap();
    run(&mut document, command);
    let edited = read(document.model(), ID).unwrap();
    assert_eq!(
        edited.property(Property::YieldStrength),
        Some(&entered(500e6))
    );
    assert_eq!(
        edited.property(Property::UltimateStrength),
        Some(&entered(650e6))
    );
    // Two library values were kept; the conductivity the library never gave was not.
    let kept = originals(document.model(), ID).unwrap();
    assert_eq!(kept.len(), 2);

    // Reset restores both library values, and clears the value the library never gave.
    let all = [
        (Property::YieldStrength, Edit::Reset),
        (Property::UltimateStrength, Edit::Reset),
        (Property::ThermalConductivity, Edit::Reset),
        (Property::YoungModulus, Edit::Reset),
    ];
    let command = edit(document.model(), ID, &all, ids).unwrap();
    run(&mut document, command);
    assert_eq!(read(document.model(), ID), Ok(s355()));
    assert_eq!(originals(document.model(), ID), Ok(Vec::new()));
}

#[test]
fn an_edit_repeating_a_property_is_refused() {
    let document = stored();
    let twice = [
        (Property::YieldStrength, Edit::Set(300e6)),
        (Property::YieldStrength, Edit::Reset),
    ];
    assert_eq!(
        edit(document.model(), ID, &twice, |_| ORIGINAL),
        Err(EditError::Repeated(Property::YieldStrength))
    );
    assert_eq!(
        EditError::Repeated(Property::YieldStrength).to_string(),
        "the property fy is edited more than once"
    );
}
