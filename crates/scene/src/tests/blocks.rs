//! The built-in `block::body` generator and the placement it returns.

use core_model::{AttributeValue, EntityId, Model};
use core_txn::{Document, ModelCommand, Origin};
use schema_gen::{Source, parse_sources};

use crate::builtin::{self, BLOCK_BODY, block_body};
use crate::{Generated, GeneratorError, Generators, Placement, Scene, View};

fn model() -> Model {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: include_str!("../../../../schema/core/namespace.toml").into(),
        },
        Source {
            path: "entities/block.toml".into(),
            contents: include_str!("../../../../schema/core/entities/block.toml").into(),
        },
    ];
    Model::new(&parse_sources(&sources).unwrap()).unwrap()
}

fn reals(values: &[f64]) -> AttributeValue {
    AttributeValue::List(values.iter().map(|v| AttributeValue::Real(*v)).collect())
}

fn block(
    document: &mut Document<ModelCommand>,
    n: u128,
    location: AttributeValue,
    rotation: f64,
    size: [f64; 3],
) -> EntityId {
    let id = EntityId::new(n);
    let command = ModelCommand::Create {
        id,
        entity: "Block".into(),
        values: vec![
            ("location".into(), location),
            ("rotation".into(), AttributeValue::Real(rotation)),
            ("size_x".into(), AttributeValue::Real(size[0])),
            ("size_y".into(), AttributeValue::Real(size[1])),
            ("size_z".into(), AttributeValue::Real(size[2])),
        ],
    };
    document.execute(command, Origin::Ui).unwrap();
    id
}

fn generate(document: &Document<ModelCommand>, id: EntityId) -> Result<Generated, GeneratorError> {
    block_body(document.model(), id)
}

fn document() -> Document<ModelCommand> {
    Document::new(model())
}

#[test]
fn a_block_is_a_local_box_and_a_placement() {
    let mut d = document();
    let id = block(&mut d, 1, reals(&[10.0, 20.0, 3.0]), 0.0, [2.0, 4.0, 6.0]);
    let generated = generate(&d, id).unwrap();
    assert_eq!(generated.solids.len(), 1);
    assert!((generated.solids[0].volume() - 48.0).abs() < 1e-9);
    assert_eq!(
        generated.placement,
        Placement::about_z(0.0, [10.0, 20.0, 3.0])
    );
    // Centred in x and y, standing on z = 0.
    let (min, max) = bounds(&generated);
    assert_eq!((min, max), ([-1.0, -2.0, 0.0], [1.0, 2.0, 6.0]));
}

fn bounds(generated: &Generated) -> ([f64; 3], [f64; 3]) {
    let mesh = generated.solids[0].tessellate().unwrap();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for (i, c) in [p.x, p.y, p.z].into_iter().enumerate() {
            min[i] = min[i].min(c);
            max[i] = max[i].max(c);
        }
    }
    (min, max)
}

#[test]
fn the_rotation_turns_the_block_about_the_vertical_axis() {
    let mut d = document();
    let id = block(
        &mut d,
        1,
        reals(&[1.0, 2.0, 3.0]),
        std::f64::consts::FRAC_PI_2,
        [2.0, 4.0, 1.0],
    );
    let placement = generate(&d, id).unwrap().placement;
    let corner = placement.apply([1.0, 2.0, 0.0]);
    assert!((corner[0] - (-1.0)).abs() < 1e-12);
    assert!((corner[1] - 3.0).abs() < 1e-12);
    assert!((corner[2] - 3.0).abs() < 1e-12);
}

#[test]
fn blocks_of_one_size_have_one_local_geometry() {
    let mut d = document();
    let a = block(&mut d, 1, reals(&[0.0; 3]), 0.0, [1.0, 2.0, 3.0]);
    let b = block(&mut d, 2, reals(&[50.0, 0.0, 0.0]), 1.0, [1.0, 2.0, 3.0]);
    let (a, b) = (generate(&d, a).unwrap(), generate(&d, b).unwrap());
    assert_eq!(
        a.solids[0].tessellate().unwrap(),
        b.solids[0].tessellate().unwrap()
    );
    assert_ne!(a.placement, b.placement);
}

#[test]
fn invalid_inputs_give_no_geometry() {
    let mut d = document();
    let cases = [
        (reals(&[0.0, 0.0]), [1.0; 3]),
        (reals(&[0.0; 4]), [1.0; 3]),
        (reals(&[0.0, 0.0, 2e9]), [1.0; 3]),
        (AttributeValue::Text("x".into()), [1.0; 3]),
        (
            AttributeValue::List(vec![
                AttributeValue::Real(0.0),
                AttributeValue::Text("y".into()),
                AttributeValue::Real(0.0),
            ]),
            [1.0; 3],
        ),
        (reals(&[0.0; 3]), [0.0, 1.0, 1.0]),
        (reals(&[0.0; 3]), [1.0, -1.0, 1.0]),
    ];
    for (n, (location, size)) in cases.into_iter().enumerate() {
        let n = u128::try_from(n).unwrap() + 1;
        // The schema may already refuse a malformed value: only stored blocks are checked.
        let id = EntityId::new(n);
        let command = ModelCommand::Create {
            id,
            entity: "Block".into(),
            values: vec![
                ("location".into(), location),
                ("size_x".into(), AttributeValue::Real(size[0])),
                ("size_y".into(), AttributeValue::Real(size[1])),
                ("size_z".into(), AttributeValue::Real(size[2])),
            ],
        };
        if d.execute(command, Origin::Ui).is_ok() {
            assert_eq!(
                generate(&d, id).unwrap_err(),
                GeneratorError::InvalidInput,
                "case {n}"
            );
        }
    }
}

#[test]
fn a_block_without_a_size_lacks_an_input() {
    let mut d = document();
    let id = block(&mut d, 1, reals(&[0.0; 3]), 0.0, [1.0; 3]);
    let unset = ModelCommand::Set {
        id,
        attribute: "size_z".into(),
        value: None,
    };
    if d.execute(unset, Origin::Ui).is_ok() {
        assert_eq!(generate(&d, id).unwrap_err(), GeneratorError::MissingInput);
    }
}

#[test]
fn a_missing_element_is_invalid() {
    let d = document();
    assert_eq!(
        generate(&d, EntityId::new(9)).unwrap_err(),
        GeneratorError::InvalidInput
    );
}

#[test]
fn the_scene_meshes_blocks_and_a_move_changes_only_the_placement() {
    let mut generators = Generators::new();
    builtin::register(&mut generators);
    assert!(format!("{generators:?}").contains(BLOCK_BODY));
    let mut d = document();
    let a = block(&mut d, 1, reals(&[0.0; 3]), 0.0, [1.0, 1.0, 1.0]);
    let mut scene = Scene::new(generators, View::new("body", u16::MAX));
    let update = scene.rebuild(d.model());
    assert_eq!(update.added.len(), 1);
    let before = scene.mesh(d.model(), a).unwrap().clone();

    let event = d
        .execute(
            ModelCommand::Set {
                id: a,
                attribute: "location".into(),
                value: Some(reals(&[5.0, 0.0, 0.0])),
            },
            Origin::Ui,
        )
        .unwrap();
    scene.apply(&event, d.model());
    let update = scene.update(d.model());
    assert_eq!(update.changed.len(), 1);
    let after = scene.mesh(d.model(), a).unwrap();
    assert_eq!(after.mesh, before.mesh);
    assert_eq!(after.placement, Placement::about_z(0.0, [5.0, 0.0, 0.0]));
}

#[test]
fn the_identity_placement_leaves_points_alone() {
    assert_eq!(Placement::default(), Placement::IDENTITY);
    let moved = Placement::about_z(0.0, [1.0, 1.0, 1.0]).apply([1.0, 2.0, 3.0]);
    assert!((moved[0] - 2.0).abs() + (moved[1] - 3.0).abs() + (moved[2] - 4.0).abs() < 1e-12);
}

/// A model whose `Block` is defined by `block.toml` with `from` replaced by `to`: the schema's
/// type check stops the core definition from storing the values the generator still has to
/// refuse, as a plugin schema or a future version could.
fn model_with(from: &str, to: &str) -> Model {
    let definition = include_str!("../../../../schema/core/entities/block.toml");
    assert!(definition.contains(from), "{from} is not in the definition");
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: include_str!("../../../../schema/core/namespace.toml").into(),
        },
        Source {
            path: "entities/block.toml".into(),
            contents: definition.replacen(from, to, 1),
        },
    ];
    Model::new(&parse_sources(&sources).unwrap()).unwrap()
}

fn create(model: &mut Model, values: Vec<(&str, AttributeValue)>) -> EntityId {
    let id = EntityId::new(1);
    model.insert(id, "Block", values).unwrap();
    id
}

fn real(v: f64) -> AttributeValue {
    AttributeValue::Real(v)
}

#[test]
fn values_of_another_type_are_invalid_input() {
    let sizes = || {
        vec![
            ("size_x", real(1.0)),
            ("size_y", real(1.0)),
            ("size_z", real(1.0)),
        ]
    };
    // A location that is not a list.
    let mut model = model_with("value = \"list:quantity:length\"", "value = \"text\"");
    let mut values = sizes();
    values.push(("location", AttributeValue::Text("here".into())));
    let id = create(&mut model, values);
    assert_eq!(
        block_body(&model, id).unwrap_err(),
        GeneratorError::InvalidInput
    );
    // A list of something else than lengths.
    let mut model = model_with("value = \"list:quantity:length\"", "value = \"list:text\"");
    let mut values = sizes();
    values.push((
        "location",
        AttributeValue::List(vec![AttributeValue::Text("x".into()); 3]),
    ));
    let id = create(&mut model, values);
    assert_eq!(
        block_body(&model, id).unwrap_err(),
        GeneratorError::InvalidInput
    );
    // A size that is not a number, and a rotation that is not.
    for (attribute, from) in [
        ("size_x", "name = \"size_x\"\nvalue = \"quantity:length\""),
        (
            "rotation",
            "value = \"quantity:plane_angle\"\ndefault = 0.0",
        ),
    ] {
        let to = from
            .replace("quantity:length", "text")
            .replace("quantity:plane_angle\"\ndefault = 0.0", "text\"");
        let mut model = model_with(from, &to);
        let mut values: Vec<_> = sizes()
            .into_iter()
            .filter(|(n, _)| *n != attribute)
            .collect();
        values.push(("location", reals(&[0.0; 3])));
        values.push((attribute, AttributeValue::Text("wide".into())));
        let id = create(&mut model, values);
        assert_eq!(
            block_body(&model, id).unwrap_err(),
            GeneratorError::InvalidInput,
            "{attribute}"
        );
    }
}

#[test]
fn an_unset_rotation_is_zero_and_an_unset_size_is_missing() {
    // Without the schema's default, an unset rotation is still read as no rotation.
    let mut model = model_with("default = 0.0\n", "");
    let values = vec![
        ("location", reals(&[1.0, 2.0, 3.0])),
        ("size_x", real(1.0)),
        ("size_y", real(1.0)),
        ("size_z", real(1.0)),
    ];
    let id = create(&mut model, values);
    let placement = block_body(&model, id).unwrap().placement;
    assert_eq!(placement, Placement::about_z(0.0, [1.0, 2.0, 3.0]));
    // Where a size is optional, leaving it out lacks an input.
    let mut model = model_with(
        "name = \"size_x\"\nvalue = \"quantity:length\"\nrequired = true",
        "name = \"size_x\"\nvalue = \"quantity:length\"",
    );
    let values = vec![
        ("location", reals(&[0.0; 3])),
        ("size_y", real(1.0)),
        ("size_z", real(1.0)),
    ];
    let id = create(&mut model, values);
    assert_eq!(
        block_body(&model, id).unwrap_err(),
        GeneratorError::MissingInput
    );
}
