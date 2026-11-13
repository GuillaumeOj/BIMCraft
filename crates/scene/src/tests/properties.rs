//! Property tests: over random command sequences with undo and redo, the incremental scene equals
//! one built from scratch after every step, and re-meshes only what the event can have reached.

use std::collections::{BTreeMap, BTreeSet};

use core_model::{AttributeValue, EntityId, Model};
use core_txn::{ChangeEvent, ModelCommand};
use proptest::prelude::*;

use super::fixture::{
    Calls, Harness, add_participant, assign, batch, component, contain, create, delete, generators,
    id, remove_participant, set, set_real, wall_type,
};
use crate::{ElementMesh, Scene, SceneError, SceneUpdate, View};

/// The objects the commands draw from: walls, finishes, one storey, one component, wall types
/// and containments. Many commands fail on a given model, as in `core-txn`'s tests.
const WALLS: [u128; 4] = [1, 2, 3, 4];
const FINISHES: [u128; 2] = [10, 11];
const STOREY: u128 = 20;
const COMPONENT: u128 = 30;
const TYPES: [u128; 2] = [31, 32];
const CONTAINMENTS: [u128; 2] = [40, 41];

fn pick<const N: usize>(from: [u128; N]) -> impl Strategy<Value = u128> {
    prop::sample::select(from.to_vec())
}

fn universe() -> Vec<u128> {
    [
        &WALLS[..],
        &FINISHES,
        &[STOREY, COMPONENT],
        &TYPES,
        &CONTAINMENTS,
    ]
    .concat()
}

#[derive(Clone, Debug)]
enum Op {
    Command(ModelCommand),
    Undo,
    Redo,
}

fn command() -> impl Strategy<Value = ModelCommand> {
    let height = (0..5u8).prop_map(f64::from);
    let thickness = (1..5u8).prop_map(|tenths| f64::from(tenths) / 10.0);
    let offset = (0..3u8).prop_map(|tenths| f64::from(tenths) / 10.0);
    let name = prop::option::of(prop_oneof![Just("unmeshable"), Just("plain")])
        .prop_map(|name| name.map(|name| AttributeValue::Text(name.into())));
    prop_oneof![
        3 => pick(WALLS).prop_map(|wall| create(wall, "Wall", Vec::new())),
        2 => pick(FINISHES).prop_map(|finish| create(finish, "Finish", Vec::new())),
        1 => Just(create(STOREY, "Storey", Vec::new())),
        1 => Just(component(COMPONENT)),
        1 => pick(TYPES).prop_map(|kind| wall_type(kind, COMPONENT)),
        2 => (pick(WALLS), prop::option::of(pick(TYPES))).prop_map(|(wall, kind)| assign(wall, kind)),
        3 => (pick(WALLS), height).prop_map(|(wall, height)| set_real(wall, "height", height)),
        1 => (pick(TYPES), thickness).prop_map(|(kind, value)| set_real(kind, "thickness", value)),
        2 => (pick(FINISHES), offset).prop_map(|(finish, value)| set_real(finish, "offset", value)),
        2 => (pick(WALLS), prop::option::of(pick(FINISHES))).prop_map(|(wall, finish)| {
            set(wall, "finish", finish.map(|finish| AttributeValue::Ref(id(finish))))
        }),
        1 => (pick(WALLS), name).prop_map(|(wall, name)| set(wall, "name", name)),
        2 => prop::sample::select(universe()).prop_map(delete),
        2 => (pick(CONTAINMENTS), prop::collection::vec(pick(WALLS), 1..3))
            .prop_map(|(relationship, walls)| contain(relationship, STOREY, &walls)),
        2 => (pick(CONTAINMENTS), pick(WALLS)).prop_map(|(relationship, wall)| add_participant(relationship, wall)),
        2 => (pick(CONTAINMENTS), pick(WALLS)).prop_map(|(relationship, wall)| remove_participant(relationship, wall)),
        1 => prop::collection::vec(pick(WALLS), 1..3)
            .prop_map(|walls| batch(walls.into_iter().map(|wall| set_real(wall, "height", 1.0)).collect())),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => command().prop_map(Op::Command),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
    ]
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 300,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// What the objects an element can depend on were, at any time: its type, the relationships it
/// takes part in and its finish. A scene that reads more than this is re-meshing without reason.
#[derive(Default)]
struct Seen(BTreeMap<EntityId, BTreeSet<EntityId>>);

impl Seen {
    fn record(&mut self, model: &Model) {
        for element in model.ids() {
            let seen = self.0.entry(element).or_default();
            seen.extend(model.type_of(element));
            seen.extend(
                model
                    .relationships_of(element)
                    .map(|role| role.relationship),
            );
            if let Ok(Some(AttributeValue::Ref(finish))) = model.get(element, "finish") {
                seen.insert(finish);
            }
        }
    }
}

/// The elements an event can have reached: the objects it lists, the elements that depended on
/// one of them, and the participants of the relationships it lists, as they are now.
fn reach(event: &ChangeEvent, model: &Model, seen: &Seen) -> BTreeSet<EntityId> {
    let touched: BTreeSet<EntityId> = [&event.created, &event.modified, &event.deleted]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let mut reach = touched.clone();
    reach.extend(
        seen.0
            .iter()
            .filter(|(_, depends)| !depends.is_disjoint(&touched))
            .map(|(element, _)| *element),
    );
    for &relationship in event.created.iter().chain(&event.modified) {
        for role in ["structure", "elements"] {
            reach.extend(model.participants(relationship, role).into_iter().flatten());
        }
    }
    reach
}

/// What a viewport that applies the updates holds.
#[derive(Default)]
struct Viewport {
    meshes: BTreeMap<EntityId, ElementMesh>,
    failed: BTreeMap<EntityId, SceneError>,
}

impl Viewport {
    fn apply(&mut self, update: &SceneUpdate, scene: &mut Scene, model: &Model) {
        for &element in update.removed.iter().chain(update.failed.keys()) {
            self.meshes.remove(&element);
        }
        for &element in update.added.iter().chain(&update.changed) {
            self.failed.remove(&element);
            if let Some(mesh) = scene.mesh(model, element) {
                self.meshes.insert(element, mesh.clone());
            }
        }
        for element in &update.removed {
            self.failed.remove(element);
        }
        self.failed.extend(update.failed.clone());
    }
}

/// The scene a model builds from scratch, as a viewport holds it.
fn rebuilt(model: &Model) -> Viewport {
    let mut scene = Scene::new(generators(&Calls::default()), View::new("body", 100));
    let update = scene.rebuild(model);
    let mut viewport = Viewport::default();
    viewport.apply(&update, &mut scene, model);
    viewport
}

fn run(ops: &[Op], flush_every: usize) -> Result<(), TestCaseError> {
    let mut h = Harness::new();
    h.rebuild();
    let (mut seen, mut viewport) = (Seen::default(), Viewport::default());
    let mut events = Vec::new();
    for (step, op) in ops.iter().enumerate() {
        let before = h.doc.model().clone();
        let result = match op {
            Op::Command(command) => h
                .doc
                .execute(command.clone(), core_txn::Origin::Ui)
                .map_err(drop),
            Op::Undo => h.doc.undo().map_err(drop),
            Op::Redo => h.doc.redo().map_err(drop),
        };
        let Ok(event) = result else { continue };
        seen.record(&before);
        seen.record(h.doc.model());
        h.scene.apply(&event, h.doc.model());
        events.push(event);
        if (step + 1) % flush_every != 0 {
            continue;
        }
        h.calls.take();
        let update = h.scene.update(h.doc.model());
        let generated = h.calls.elements();
        // Each element is generated once, exactly when the update reports it.
        let reported: BTreeSet<EntityId> = update
            .added
            .iter()
            .chain(&update.changed)
            .chain(update.failed.keys())
            .copied()
            .collect();
        prop_assert_eq!(
            generated.iter().copied().collect::<BTreeSet<_>>(),
            reported.clone()
        );
        prop_assert_eq!(generated.len(), reported.len());
        // Only elements the events can have reached are meshed again.
        let reached: BTreeSet<EntityId> = events
            .iter()
            .flat_map(|event| reach(event, h.doc.model(), &seen))
            .collect();
        prop_assert!(
            reported.is_subset(&reached),
            "{reported:?} outside {reached:?}"
        );
        events.clear();
        viewport.apply(&update, &mut h.scene, h.doc.model());
        let fresh = rebuilt(h.doc.model());
        prop_assert_eq!(&viewport.meshes, &fresh.meshes);
        prop_assert_eq!(&viewport.failed, &fresh.failed);
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_incremental_scene_equals_a_rebuilt_one_after_every_step(
        ops in prop::collection::vec(op(), 1..40),
    ) {
        run(&ops, 1)?;
    }

    #[test]
    fn several_events_before_an_update_give_the_same_scene(
        ops in prop::collection::vec(op(), 1..40),
        flush_every in 2..5usize,
    ) {
        run(&ops, flush_every)?;
    }
}
