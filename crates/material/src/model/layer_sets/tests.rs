use core_model::units::LengthUnit::Millimetre;
use core_txn::{Document, Origin};

use super::*;
use crate::test_support::model;
use crate::{Library, commands};

const CONCRETE: EntityId = EntityId::new(1);
const INSULATION: EntityId = EntityId::new(2);
const SET: EntityId = EntityId::new(10);

fn layer(raw: u128, material: EntityId, millimetres: f64, core: bool) -> Layer {
    Layer {
        id: EntityId::new(raw),
        material,
        thickness: Length::new(millimetres, Millimetre),
        priority: if core { 100 } else { 30 },
        core,
        name: core.then(|| "Structure".to_owned()),
    }
}

fn wall() -> LayerSet {
    LayerSet::new(
        "Exterior wall",
        vec![
            layer(11, INSULATION, 120.0, false),
            layer(12, CONCRETE, 200.0, true),
        ],
    )
    .unwrap()
}

/// A model with two materials from the library and the wall build-up.
fn stored() -> Document<ModelCommand> {
    let library = Library::builtin().unwrap();
    let mut document = Document::new(model());
    let c30 = library.get("en1992-1-1/C30/37").unwrap();
    // Any material object will do for the build-up.
    let s235 = library.get("en1993-1-1/S235").unwrap();
    for (id, material) in [(CONCRETE, c30), (INSULATION, s235)] {
        document
            .execute(commands(id, material), Origin::Ui)
            .unwrap();
    }
    document
        .execute(layer_set_commands(SET, &wall()), Origin::Ui)
        .unwrap();
    document
}

#[test]
fn a_build_up_round_trips_through_the_model() {
    let document = stored();
    let set = read_layer_set(document.model(), SET).unwrap();
    assert_eq!(set, wall());
    assert!((set.total_thickness().get(Millimetre) - 320.0).abs() < 1e-9);
}

#[test]
fn storing_a_build_up_is_undoable() {
    let mut document = stored();
    document.undo().unwrap();
    assert_eq!(
        read_layer_set(document.model(), SET),
        Err(LayerSetReadError::NotALayerSet)
    );
    assert_eq!(document.model().entity_of(EntityId::new(11)), None);
    document.redo().unwrap();
    assert_eq!(read_layer_set(document.model(), SET), Ok(wall()));
}

#[test]
fn broken_build_ups_are_rejected_with_the_reason() {
    let set = |attribute: &str, value| ModelCommand::Set {
        id: SET,
        attribute: attribute.into(),
        value: Some(value),
    };
    let layer_attribute = |attribute: &str, value| ModelCommand::Set {
        id: EntityId::new(12),
        attribute: attribute.into(),
        value: Some(value),
    };
    let layers = |ids: &[u128]| {
        AttributeValue::List(
            ids.iter()
                .map(|raw| AttributeValue::Ref(EntityId::new(*raw)))
                .collect(),
        )
    };
    let cases = [
        (
            set("layers", layers(&[11, 1])),
            LayerSetReadError::NotALayer(2),
        ),
        (
            set("layers", layers(&[])),
            LayerSetReadError::Invalid(LayerSetError::Empty),
        ),
        (
            layer_attribute("material", AttributeValue::Ref(EntityId::new(11))),
            LayerSetReadError::NotAMaterial(2),
        ),
        (
            layer_attribute("priority", AttributeValue::Integer(200)),
            LayerSetReadError::Invalid(LayerSetError::Priority(2)),
        ),
        (
            layer_attribute("thickness", AttributeValue::Real(0.0)),
            LayerSetReadError::Invalid(LayerSetError::Thickness(2)),
        ),
    ];
    for (command, error) in cases {
        let mut document = stored();
        document.execute(command, Origin::Ui).unwrap();
        assert_eq!(read_layer_set(document.model(), SET), Err(error));
    }
    assert_eq!(
        read_layer_set(stored().model(), CONCRETE),
        Err(LayerSetReadError::NotALayerSet)
    );
}

#[test]
fn errors_give_positions_not_values() {
    assert_eq!(
        LayerSetReadError::NotAMaterial(2).to_string(),
        "the material of layer 2 is not a material"
    );
    assert_eq!(
        LayerSetReadError::NotALayer(1).to_string(),
        "item 1 of the build-up is not a layer"
    );
}
