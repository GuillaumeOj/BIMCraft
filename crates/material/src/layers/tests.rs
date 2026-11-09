use core_model::units::LengthUnit::Millimetre;

use super::*;

fn layer(raw: u128, function: LayerFunction, millimetres: f64) -> Layer {
    Layer {
        id: EntityId::new(raw),
        material: EntityId::new(100 + raw),
        thickness: Length::new(millimetres, Millimetre),
        function,
        priority: function.default_priority(),
        core: function.default_core(),
        name: None,
    }
}

/// A 320 mm exterior wall: 20 mm render, 120 mm insulation, 160 mm concrete, 20 mm plaster.
fn wall() -> Vec<Layer> {
    vec![
        layer(1, LayerFunction::Finish, 20.0),
        layer(2, LayerFunction::Insulation, 120.0),
        layer(3, LayerFunction::Structure, 160.0),
        layer(4, LayerFunction::Finish, 20.0),
    ]
}

fn build(layers: Vec<Layer>) -> Result<LayerSet, LayerSetError> {
    LayerSet::new("Exterior wall", Usage::Wall, None, layers)
}

#[test]
fn a_build_up_adds_its_layers_and_centres_on_its_core() {
    let set = LayerSet::new(
        " Exterior wall 320 ",
        Usage::Wall,
        Some(" Insulated "),
        wall(),
    )
    .unwrap();
    assert_eq!(set.name(), "Exterior wall 320");
    assert_eq!(set.description(), Some("Insulated"));
    assert_eq!(set.usage(), Usage::Wall);
    assert_eq!(set.layers().len(), 4);
    assert!((set.total_thickness().get(Millimetre) - 320.0).abs() < 1e-9);
    // Core from 140 mm to 300 mm: its centre is 220 mm from the exterior face.
    assert!((set.geometry().core_centre() - 0.22).abs() < 1e-12);
    assert_eq!(set.geometry().layers()[2].id, layer_id(2));
}

#[test]
fn functions_have_defaults_and_names() {
    let defaults = [
        (LayerFunction::Structure, 1, true),
        (LayerFunction::Substrate, 2, false),
        (LayerFunction::Insulation, 3, false),
        (LayerFunction::AirGap, 3, false),
        (LayerFunction::Membrane, 4, false),
        (LayerFunction::Finish, 5, false),
    ];
    for (function, priority, core) in defaults {
        assert_eq!(function.default_priority(), priority, "{function:?}");
        assert_eq!(function.default_core(), core, "{function:?}");
        assert_eq!(LayerFunction::from_name(function.name()), Some(function));
    }
    assert_eq!(LayerFunction::from_name("plaster"), None);
    for usage in Usage::ALL {
        assert_eq!(Usage::from_name(usage.name()), Some(usage));
    }
    assert_eq!(Usage::from_name("roof"), None);
}

// The IFC priority runs 0 to 100 with the higher one winning: 1 → 100, 2 → 75, 3 → 50, 4 → 25,
// 5 → 0 (ADR 0019), and an import takes the nearest of the five.
#[test]
fn priorities_map_to_ifc_and_back() {
    let table = [(1, 100), (2, 75), (3, 50), (4, 25), (5, 0)];
    for (priority, ifc) in table {
        assert_eq!(ifc_priority(priority), ifc);
        assert_eq!(priority_from_ifc(ifc), priority);
    }
    assert_eq!(priority_from_ifc(88), 1);
    assert_eq!(priority_from_ifc(87), 2);
    assert_eq!(priority_from_ifc(12), 5);
    assert_eq!(priority_from_ifc(13), 4);
    assert_eq!(priority_from_ifc(255), 1);
}

#[test]
fn invalid_build_ups_are_rejected_with_the_reason() {
    let with = |index: usize, change: fn(&mut Layer)| {
        let mut layers = wall();
        change(&mut layers[index]);
        build(layers)
    };
    let long = "x".repeat(MAX_TEXT_LEN + 1);
    let none = |_| None;
    let first = |name: &str, layers: Vec<Layer>| {
        LayerSet::problems(name, None, &layers, none)
            .first()
            .copied()
    };
    assert_eq!(first(" ", wall()), Some(LayerSetError::EmptyName));
    assert_eq!(first(&long, wall()), Some(LayerSetError::TooLong));
    assert_eq!(first("A\nB", wall()), Some(LayerSetError::ControlCharacter));
    assert_eq!(build(Vec::new()), Err(LayerSetError::NoLayers));
    let many = (0..=MAX_LAYERS as u128)
        .map(|raw| layer(raw, LayerFunction::Finish, 1.0))
        .collect();
    assert_eq!(build(many), Err(LayerSetError::TooManyLayers));
    let mut named = wall();
    named[0].name = Some(long);
    assert_eq!(build(named), Err(LayerSetError::TooLong));
    let thin = with(1, |layer| layer.thickness = Length::from_si(1e-7));
    assert_eq!(thin, Err(LayerSetError::InvalidThickness(2)));
    let nan = with(0, |layer| layer.thickness = Length::from_si(f64::NAN));
    assert_eq!(nan, Err(LayerSetError::InvalidThickness(1)));
    let zero = with(1, |layer| layer.thickness = Length::from_si(0.0));
    assert_eq!(zero, Err(LayerSetError::InvalidThickness(2)));
    assert_eq!(
        with(3, |layer| layer.priority = 6),
        Err(LayerSetError::InvalidPriority(4))
    );
    assert_eq!(
        with(3, |layer| layer.priority = 0),
        Err(LayerSetError::InvalidPriority(4))
    );
    assert_eq!(
        with(3, |layer| layer.id = EntityId::new(1)),
        Err(LayerSetError::DuplicateLayer(4))
    );
    assert_eq!(
        with(0, |layer| layer.core = true),
        Err(LayerSetError::CoreNotContiguous)
    );
    let thick = (1..=3)
        .map(|raw| layer(raw, LayerFunction::Finish, 4e6))
        .collect();
    assert_eq!(build(thick), Err(LayerSetError::TooThick));
}

#[test]
fn a_membrane_may_have_no_thickness_and_adds_nothing_to_the_geometry() {
    let mut layers = wall();
    let mut membrane = layer(5, LayerFunction::Membrane, 0.0);
    membrane.thickness = Length::from_si(0.0);
    layers.insert(2, membrane);
    let set = build(layers).unwrap();
    assert_eq!(set.layers().len(), 5);
    assert!((set.total_thickness().get(Millimetre) - 320.0).abs() < 1e-9);
    // The kernel's ids are the positions in the full list, so the structure is layer 3 + 1.
    assert_eq!(set.geometry().layers().len(), 4);
    assert_eq!(set.geometry().layers()[2].id, layer_id(3));
    // A vanishing, negative or core membrane is not valid, nor a build-up of membranes only.
    let mut vanishing = wall();
    let mut thin = layer(5, LayerFunction::Membrane, 0.0);
    thin.thickness = Length::from_si(1e-9);
    vanishing.push(thin);
    assert_eq!(build(vanishing), Err(LayerSetError::InvalidThickness(5)));
    let mut negative = layer(5, LayerFunction::Membrane, 0.0);
    negative.thickness = Length::from_si(-1.0);
    assert_eq!(
        build(vec![wall().remove(0), negative]),
        Err(LayerSetError::InvalidThickness(2))
    );
    let mut core = layer(5, LayerFunction::Membrane, 0.0);
    core.thickness = Length::from_si(0.0);
    core.core = true;
    assert_eq!(build(vec![core]), Err(LayerSetError::InvalidThickness(1)));
    let mut only = layer(6, LayerFunction::Membrane, 0.0);
    only.thickness = Length::from_si(0.0);
    assert_eq!(build(vec![only]), Err(LayerSetError::InvalidThickness(1)));
}

#[test]
fn every_problem_is_reported_at_once_with_its_layer() {
    let mut layers = wall();
    layers[0].thickness = Length::from_si(0.0);
    layers[1].priority = 9;
    layers[2].id = EntityId::new(1);
    layers[3].function = LayerFunction::AirGap;
    layers[3].material = EntityId::new(500);
    let category = |id: EntityId| match id.get() {
        500 => Some(Category::Steel),
        _ => Some(Category::Generic),
    };
    let problems = LayerSet::problems("", None, &layers, category);
    assert_eq!(
        problems,
        [
            LayerSetError::EmptyName,
            LayerSetError::InvalidThickness(1),
            LayerSetError::InvalidPriority(2),
            LayerSetError::DuplicateLayer(3),
            LayerSetError::InvalidMaterial(4),
        ]
    );
    assert!(LayerSet::problems("W", None, &wall(), category).is_empty());
    // An unknown material is the caller's to report: `category_of` says None only for objects
    // that are not materials, and the model layer turns that into UnknownMaterial.
    assert_eq!(
        LayerSetError::UnknownMaterial(2).to_string(),
        "the material of layer 2 is not a material of the project"
    );
}

#[test]
fn thermal_resistance_sums_the_layers_that_have_a_conductivity() {
    let set = build(wall()).unwrap();
    // R = d / λ: render 0.02 / 1.0 = 0.02, insulation 0.12 / 0.04 = 3.0, concrete 0.16 / 2.0 = 0.08,
    // and the plaster has no conductivity.
    let lambdas = [Some(1.0), Some(0.04), Some(2.0), None];
    let thermal = set.thermal(|layer| lambdas[usize::try_from(layer.id.get() - 1).unwrap()]);
    let expected = [Some(0.02), Some(3.0), Some(0.08), None];
    for (got, want) in thermal.layers.iter().zip(expected) {
        match (got, want) {
            (Some(got), Some(want)) => assert!((got - want).abs() < 1e-12),
            (None, None) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!((thermal.total.unwrap() - 3.1).abs() < 1e-12);
    assert_eq!(thermal.layers_without_data, 1);
}

#[test]
fn air_gaps_and_unusable_conductivities_have_no_resistance() {
    let mut layers = wall();
    layers[1].function = LayerFunction::AirGap;
    let set = build(layers).unwrap();
    let thermal = set.thermal(|layer| match layer.id.get() {
        1 => Some(0.0),
        2 => Some(0.026),
        3 => Some(f64::NAN),
        _ => Some(1.0),
    });
    assert_eq!(thermal.layers_without_data, 3);
    assert!((thermal.total.unwrap() - 0.02).abs() < 1e-12);
    let none = set.thermal(|_| None);
    assert_eq!(none.total, None);
    assert_eq!(none.layers_without_data, 4);
}

#[test]
fn errors_give_positions_not_values() {
    assert_eq!(
        LayerSetError::InvalidPriority(2).to_string(),
        "layer 2 has a priority outside 1 to 5"
    );
    assert_eq!(
        LayerSetError::TooManyLayers.to_string(),
        "the build-up has more than 32 layers"
    );
    assert_eq!(
        LayerSetError::CoreNotContiguous.to_string(),
        "the core layers are not next to each other"
    );
    assert_eq!(
        LayerSetError::InvalidMaterial(3).to_string(),
        "the material of air gap 3 is not a generic material"
    );
}

#[test]
fn names_and_descriptions_are_bounded() {
    let layers = wall();
    let problems = |name: &str, description: Option<&str>| {
        LayerSet::problems(name, description, &layers, |_| None)
    };
    assert_eq!(problems(&"x".repeat(MAX_NAME_LEN), None), []);
    assert_eq!(
        problems(&"x".repeat(MAX_NAME_LEN + 1), None),
        [LayerSetError::TooLong]
    );
    assert_eq!(
        problems("W", Some(&"x".repeat(MAX_TEXT_LEN + 1))),
        [LayerSetError::TooLong]
    );
    assert_eq!(
        problems("W", Some("a\nb")),
        [LayerSetError::ControlCharacter]
    );
    assert_eq!(problems("W", Some(" ")), []);
    assert_eq!(
        LayerSet::new("W", Usage::Wall, Some("a\nb"), wall()),
        Err(LayerSetError::ControlCharacter)
    );
    assert_eq!(
        LayerSet::new("W", Usage::Wall, Some(" "), wall())
            .unwrap()
            .description(),
        None
    );
    assert_eq!(
        LayerSetError::DuplicateName.to_string(),
        "another build-up has this name"
    );
}
