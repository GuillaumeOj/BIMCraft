use core_model::ModelError;
use core_model::units::LengthUnit::Millimetre;
use core_txn::{Document, Origin};

use super::*;
use crate::test_support::model;
use crate::{Library, commands};

const CONCRETE: EntityId = EntityId::new(1);
const INSULATION: EntityId = EntityId::new(2);
const SET: EntityId = EntityId::new(10);

fn layer(raw: u128, material: EntityId, function: LayerFunction, millimetres: f64) -> Layer {
    Layer {
        id: EntityId::new(raw),
        material,
        thickness: Length::new(millimetres, Millimetre),
        function,
        priority: function.default_priority(),
        core: function.default_core(),
        name: function.default_core().then(|| "Structure".to_owned()),
    }
}

fn wall() -> LayerSet {
    LayerSet::new(
        "Exterior wall",
        Usage::Wall,
        Some("Insulated concrete"),
        vec![
            layer(11, INSULATION, LayerFunction::Insulation, 120.0),
            layer(12, CONCRETE, LayerFunction::Structure, 200.0),
        ],
    )
    .unwrap()
}

fn run(document: &mut Document<ModelCommand>, command: ModelCommand) {
    document.execute(command, Origin::Ui).unwrap();
}

/// A model with two materials from the library and the wall build-up.
fn stored() -> Document<ModelCommand> {
    let library = Library::builtin().unwrap();
    let mut document = Document::new(model());
    let c30 = library.get("en1992-1-1/C30/37").unwrap();
    // Any material object will do for the build-up.
    let s235 = library.get("en1993-1-1/S235").unwrap();
    for (id, material) in [(CONCRETE, c30), (INSULATION, s235)] {
        run(&mut document, commands(id, material));
    }
    run(&mut document, layer_set_commands(SET, &wall()));
    document
}

#[test]
fn a_build_up_round_trips_through_the_model() {
    let document = stored();
    let set = read_layer_set(document.model(), SET).unwrap();
    assert_eq!(set, wall());
    assert!((set.total_thickness().get(Millimetre) - 320.0).abs() < 1e-9);
    assert_eq!(
        layer_ids(document.model(), SET),
        Ok(vec![EntityId::new(11), EntityId::new(12)])
    );
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
fn replacing_a_build_up_keeps_the_layers_it_still_lists() {
    let mut document = stored();
    // Layer 12 keeps its identity but is resized and renamed, layer 11 goes, layer 13 is new, and
    // the build-up is renamed with no description.
    let mut structure = layer(12, CONCRETE, LayerFunction::Structure, 250.0);
    structure.name = None;
    let changed = LayerSet::new(
        "Wall 300",
        Usage::Slab,
        None,
        vec![
            structure,
            layer(13, INSULATION, LayerFunction::Finish, 50.0),
        ],
    )
    .unwrap();
    let command = replace_layer_set(document.model(), SET, &changed).unwrap();
    run(&mut document, command);
    assert_eq!(read_layer_set(document.model(), SET), Ok(changed));
    assert!(!document.model().contains(EntityId::new(11)));
    assert_eq!(
        layer_ids(document.model(), SET),
        Ok(vec![EntityId::new(12), EntityId::new(13)])
    );
    // One undo step puts everything back.
    document.undo().unwrap();
    assert_eq!(read_layer_set(document.model(), SET), Ok(wall()));
    assert_eq!(
        replace_layer_set(document.model(), CONCRETE, &wall()),
        Err(LayerSetReadError::NotALayerSet)
    );
}

#[test]
fn a_build_up_and_its_layers_are_deleted_in_one_command() {
    let mut document = stored();
    // The generic delete refuses a layer the build-up lists and a material a layer uses.
    for id in [EntityId::new(11), CONCRETE] {
        let result = document.execute(ModelCommand::Delete { id }, Origin::Ui);
        assert_eq!(
            result.unwrap_err().to_string(),
            ModelError::StillReferenced { count: 1 }.to_string()
        );
    }
    let command = delete_layer_set(document.model(), SET).unwrap();
    run(&mut document, command);
    for raw in [10, 11, 12] {
        assert!(!document.model().contains(EntityId::new(raw)));
    }
    assert!(document.model().contains(CONCRETE));
    document.undo().unwrap();
    assert_eq!(read_layer_set(document.model(), SET), Ok(wall()));
    assert_eq!(
        delete_layer_set(document.model(), CONCRETE),
        Err(LayerSetReadError::NotALayerSet)
    );
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
            LayerSetReadError::Invalid(LayerSetError::NoLayers),
        ),
        (
            layer_attribute("material", AttributeValue::Ref(EntityId::new(11))),
            LayerSetReadError::NotAMaterial(2),
        ),
        (
            layer_attribute("priority", AttributeValue::Integer(200)),
            LayerSetReadError::Invalid(LayerSetError::InvalidPriority(2)),
        ),
        (
            layer_attribute("priority", AttributeValue::Integer(-3)),
            LayerSetReadError::Invalid(LayerSetError::InvalidPriority(2)),
        ),
        (
            layer_attribute("thickness", AttributeValue::Real(0.0)),
            LayerSetReadError::Invalid(LayerSetError::InvalidThickness(2)),
        ),
    ];
    for (command, error) in cases {
        let mut document = stored();
        run(&mut document, command);
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
    assert_eq!(
        LayerSetReadError::Unknown.to_string(),
        "the build-up holds a value this version does not know"
    );
}
