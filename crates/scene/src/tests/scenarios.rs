//! Re-meshing counts after each kind of edit (the acceptance criteria of GEO-05 and CORE-08).
//!
//! Each step asserts two things: what the update reports, and the generator calls that really
//! ran, so a lazy `mesh()` or a hidden recomputation cannot pass unnoticed.

use std::collections::BTreeSet;

use core_model::{AttributeValue, EntityId, Model};
use core_txn::ModelCommand;
use geom::TessellationError;

use super::fixture::{
    Calls, Harness, PSET, add_participant, assign, batch, component, contain, create, delete, id,
    ids, prism, real, remove_participant, set, set_real, untriangulable_solid, wall_type, walls,
};
use crate::{ElementMesh, Generated, GeneratorError, Generators, SceneError, View};

/// Walls `1..=count` and a built scene, ready for edits.
fn scene_of(count: u128) -> Harness {
    let mut h = Harness::new();
    h.execute(walls(count));
    h.rebuild();
    h
}

fn mesh_of(h: &mut Harness, raw: u128) -> Option<ElementMesh> {
    h.scene.mesh(h.doc.model(), id(raw)).cloned()
}

/// The meshes of walls `1..=count`.
fn meshes(h: &mut Harness, count: u128) -> Vec<Option<ElementMesh>> {
    (1..=count).map(|raw| mesh_of(h, raw)).collect()
}

#[test]
fn the_first_build_meshes_every_wall() {
    let mut h = Harness::new();
    h.execute(walls(5));
    let update = h.scene.rebuild(h.doc.model());
    assert_eq!(update.added, ids([1, 2, 3, 4, 5]));
    assert_eq!(update.remeshed(), 5);
    assert!(update.changed.is_empty() && update.removed.is_empty() && update.failed.is_empty());
    assert_eq!(h.calls.elements(), (1..=5).map(id).collect::<Vec<_>>());
}

#[test]
fn editing_one_wall_remeshes_exactly_that_wall() {
    let mut h = scene_of(4);
    let before = meshes(&mut h, 4);
    let update = h.run(set_real(2, "height", 3.0));
    assert_eq!(update.changed, ids([2]));
    assert_eq!(update.remeshed(), 1);
    assert_eq!(h.calls.elements(), vec![id(2)]);
    let after = meshes(&mut h, 4);
    assert_ne!(after[1], before[1], "the edited wall has a new mesh");
    for index in [0, 2, 3] {
        assert_eq!(after[index], before[index], "the others keep their mesh");
    }
    // Reading the meshes did not generate anything.
    assert!(h.calls.take().is_empty());
}

#[test]
fn editing_a_type_remeshes_its_instances() {
    let mut h = scene_of(4);
    h.run(component(10));
    h.run(wall_type(11, 10));
    h.run(assign(1, Some(11)));
    h.run(assign(2, Some(11)));
    h.calls.take();
    let update = h.run(set_real(11, "thickness", 0.4));
    assert_eq!(update.changed, ids([1, 2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
    // Untyping one instance changes only it; deleting the type changes the rest.
    assert_eq!(h.run(assign(1, None)).changed, ids([1]));
    assert_eq!(h.run(delete(11)).changed, ids([2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
}

#[test]
fn editing_an_input_remeshes_the_walls_that_read_it() {
    let mut h = Harness::new();
    h.execute(batch(vec![
        walls(4),
        create(10, "Finish", Vec::new()),
        create(11, "Finish", Vec::new()),
        set(1, "finish", Some(AttributeValue::Ref(id(10)))),
        set(2, "finish", Some(AttributeValue::Ref(id(10)))),
        set(3, "finish", Some(AttributeValue::Ref(id(11)))),
    ]));
    h.rebuild();
    let update = h.run(set_real(10, "offset", 0.05));
    assert_eq!(update.changed, ids([1, 2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
    // Dependencies follow the model: once wall 2 reads the other finish, editing the first one
    // no longer reaches it.
    h.run(set(2, "finish", Some(AttributeValue::Ref(id(11)))));
    h.calls.take();
    assert_eq!(h.run(set_real(10, "offset", 0.1)).changed, ids([1]));
    assert_eq!(h.calls.elements(), vec![id(1)]);
    assert_eq!(h.run(set_real(11, "offset", 0.1)).changed, ids([2, 3]));
}

#[test]
fn a_command_that_changes_no_geometry_remeshes_nothing() {
    let mut h = scene_of(3);
    h.run(create(20, "Storey", Vec::new()));
    let update = h.run(set_real(20, "elevation", 3.0));
    assert_eq!(update.remeshed(), 0);
    assert!(h.calls.take().is_empty());
    // A second update with no new event has nothing to do either.
    let again = h.scene.update(h.doc.model());
    assert_eq!(again, crate::SceneUpdate::default());
    assert!(h.calls.take().is_empty());
}

#[test]
fn creating_a_wall_adds_it_and_deleting_it_removes_it() {
    let mut h = scene_of(2);
    let update = h.run(create(7, "Wall", Vec::new()));
    assert_eq!(update.added, ids([7]));
    assert_eq!(h.calls.elements(), vec![id(7)]);
    let update = h.run(delete(7));
    assert_eq!(update.removed, ids([7]));
    assert_eq!(update.remeshed(), 0);
    assert!(h.calls.take().is_empty());
    assert_eq!(mesh_of(&mut h, 7), None);
    // Objects that are not elements with a representation are not meshed.
    let update = h.run(create(20, "Storey", Vec::new()));
    assert_eq!(update, crate::SceneUpdate::default());
}

#[test]
fn undo_and_redo_remesh_exactly_what_they_change() {
    let mut h = scene_of(3);
    assert_eq!(h.run(set_real(2, "height", 4.0)).changed, ids([2]));
    assert_eq!(h.undo().changed, ids([2]));
    assert_eq!(h.calls.elements(), vec![id(2), id(2)]);
    assert_eq!(h.redo().changed, ids([2]));
    assert_eq!(h.calls.elements(), vec![id(2)]);
    // Creation undone is a removal, redone an addition.
    assert_eq!(h.run(create(7, "Wall", Vec::new())).added, ids([7]));
    assert_eq!(h.undo().removed, ids([7]));
    assert_eq!(h.redo().added, ids([7]));
    assert_eq!(h.calls.elements(), vec![id(7), id(7)]);
}

#[test]
fn a_relationship_remeshes_its_participants() {
    let mut h = scene_of(4);
    h.run(create(20, "Storey", Vec::new()));
    let update = h.run(contain(30, 20, &[1, 2]));
    assert_eq!(update.changed, ids([1, 2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
    // Adding a participant changes the relationship, which its participants depend on.
    let update = h.run(add_participant(30, 3));
    assert_eq!(update.changed, ids([1, 2, 3]));
    // A participant that leaves still depends on what it read: it is meshed again too.
    let update = h.run(remove_participant(30, 3));
    assert_eq!(update.changed, ids([1, 2, 3]));
    // Deleting the relationship reaches the walls that took part, and no other.
    h.calls.take();
    assert_eq!(h.run(delete(30)).changed, ids([1, 2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
}

#[test]
fn an_element_whose_generator_fails_is_reported_and_the_others_are_meshed() {
    let mut h = scene_of(3);
    let update = h.run(batch(vec![
        set_real(1, "height", 0.0),
        set_real(3, "height", 5.0),
    ]));
    assert_eq!(update.changed, ids([3]));
    assert_eq!(
        update.failed.keys().copied().collect::<Vec<_>>(),
        vec![id(1)]
    );
    assert!(matches!(
        update.failed[&id(1)],
        SceneError::Generator(GeneratorError::Extrusion(_))
    ));
    assert_eq!(mesh_of(&mut h, 1), None, "a failed element has no mesh");
    assert!(mesh_of(&mut h, 3).is_some());
    // Fixed, it is meshed again: it had no mesh, so it is added.
    let update = h.run(set_real(1, "height", 2.0));
    assert_eq!(update.added, ids([1]));
    assert!(update.failed.is_empty());
    // A mesh lost to a failure is dropped from the viewport through `failed`, and a failed
    // element that is deleted is removed.
    assert_eq!(h.run(set_real(2, "height", 0.0)).failed.len(), 1);
    assert!(mesh_of(&mut h, 2).is_none());
    assert_eq!(h.run(delete(2)).removed, ids([2]));
}

#[test]
fn a_failed_element_is_meshed_again_when_what_it_read_comes_back() {
    let mut h = Harness::new();
    h.execute(batch(vec![
        walls(2),
        create(10, "Finish", Vec::new()),
        set(1, "finish", Some(AttributeValue::Ref(id(10)))),
    ]));
    h.rebuild();
    // The finish is gone and the wall still points at it: no input, no mesh.
    let update = h.run(delete(10));
    assert_eq!(
        update.failed.get(&id(1)),
        Some(&SceneError::Generator(GeneratorError::MissingInput))
    );
    assert_eq!(h.calls.elements(), vec![id(1)]);
    // The generator reported no inputs as it failed, but the scene kept the ones it had read.
    let update = h.run(create(10, "Finish", vec![("offset", real(0.1))]));
    assert_eq!(update.added, ids([1]));
    assert_eq!(h.calls.elements(), vec![id(1)]);
}

#[test]
fn a_solid_that_cannot_be_tessellated_fails_its_element_only() {
    let mut h = scene_of(3);
    let update = h.run(set(
        2,
        "name",
        Some(AttributeValue::Text("unmeshable".into())),
    ));
    assert!(update.changed.is_empty());
    let Some(error @ SceneError::Tessellation { solid: 0, source }) = update.failed.get(&id(2))
    else {
        panic!("expected a tessellation error, got {:?}", update.failed);
    };
    assert!(matches!(source, TessellationError::Untriangulable { .. }));
    // The message names the solid and the face, never coordinates (SEC-03).
    assert!(!error.to_string().contains('.'));
    assert!(mesh_of(&mut h, 2).is_none());
    assert!(mesh_of(&mut h, 1).is_some() && mesh_of(&mut h, 3).is_some());
}

#[test]
fn a_solid_that_cannot_be_tessellated_still_depends_on_what_its_generator_read() {
    let mut generators = Generators::new();
    // No attribute of the wall refers to finish 99: only the generator's `inputs` say so.
    generators.register("wall::body", |_: &Model, _: EntityId| {
        Ok(Generated {
            solids: vec![untriangulable_solid()],
            inputs: BTreeSet::from([id(99)]),
        })
    });
    let mut h = Harness::with_generators(generators, View::new("body", 100), Calls::default());
    h.execute(batch(vec![
        create(1, "Wall", Vec::new()),
        create(99, "Finish", Vec::new()),
    ]));
    assert!(h.rebuild().failed.contains_key(&id(1)));
    // The generator ran and said what it read, so a change to it meshes the wall again.
    let update = h.run(set_real(99, "offset", 0.5));
    assert!(update.failed.contains_key(&id(1)));
}

#[test]
fn a_representation_without_a_generator_fails_its_elements() {
    let mut h =
        Harness::with_generators(Generators::new(), View::new("body", 100), Calls::default());
    h.execute(walls(2));
    let update = h.rebuild();
    let expected = SceneError::NoGenerator {
        generator: "wall::body".into(),
    };
    assert_eq!(update.failed.len(), 2);
    assert!(update.failed.values().all(|error| *error == expected));
    assert_eq!(
        expected.to_string(),
        "no generator is registered for `wall::body`"
    );
}

#[test]
fn the_view_picks_the_representation_by_context_and_level_of_detail() {
    let column = || create(1, "Column", Vec::new());
    let path_for = |view: View| {
        let mut h = Harness::viewing(view);
        h.execute(column());
        h.scene.rebuild(h.doc.model());
        h.calls.take()
    };
    let path = |calls: &[(String, _)]| calls.first().map(|(path, _): &(String, _)| path.clone());
    assert_eq!(
        path(&path_for(View::new("body", 100))).as_deref(),
        Some("column::coarse")
    );
    assert_eq!(
        path(&path_for(View::new("body", 200))).as_deref(),
        Some("column::coarse")
    );
    assert_eq!(
        path(&path_for(View::new("body", 300))).as_deref(),
        Some("column::fine")
    );
    assert_eq!(
        path(&path_for(View::new("body", 500))).as_deref(),
        Some("column::fine")
    );
    assert_eq!(
        path(&path_for(View::new("axis", 100))).as_deref(),
        Some("column::axis")
    );
    // Below the lowest level of detail, or in a context it has none for, it is not meshed.
    assert_eq!(path(&path_for(View::new("body", 50))), None);
    assert_eq!(path(&path_for(View::new("footprint", 100))), None);
}

#[test]
fn mesh_meshes_a_stale_element_on_demand_and_the_update_still_reports_it() {
    let mut h = scene_of(3);
    let before = mesh_of(&mut h, 1);
    let event = h.execute(set_real(1, "height", 3.0));
    h.scene.apply(&event, h.doc.model());
    // Nothing was computed by `apply`.
    assert!(h.calls.take().is_empty());
    let fresh = mesh_of(&mut h, 1);
    assert_ne!(fresh, before);
    assert_eq!(h.calls.elements(), vec![id(1)]);
    // The update reports it without generating it again.
    let update = h.scene.update(h.doc.model());
    assert_eq!(update.changed, ids([1]));
    assert!(h.calls.take().is_empty());
    assert_eq!(mesh_of(&mut h, 1), fresh);
}

#[test]
fn an_element_deleted_and_recreated_before_an_update_is_changed_not_removed() {
    let mut h = scene_of(2);
    let deleted = h.execute(delete(1));
    h.scene.apply(&deleted, h.doc.model());
    let undone = h.doc.undo().expect("something to undo");
    h.scene.apply(&undone, h.doc.model());
    let update = h.scene.update(h.doc.model());
    assert_eq!(update.changed, ids([1]));
    assert!(update.removed.is_empty() && update.added.is_empty());
}

#[test]
fn an_element_that_is_gone_when_the_scene_updates_is_dropped() {
    let mut h = scene_of(2);
    let event = h.execute(set_real(1, "height", 3.0));
    h.scene.apply(&event, h.doc.model());
    // The scene is updated against a model that no longer has the wall.
    let mut other = Harness::new();
    other.execute(create(2, "Wall", Vec::new()));
    let update = h.scene.update(other.doc.model());
    assert_eq!(update.removed, ids([1]));
    assert!(update.remeshed() == 0 && h.calls.take().is_empty());
}

#[test]
fn rebuilding_meshes_everything_again_and_reports_what_is_gone() {
    let mut h = scene_of(3);
    let update = h.scene.rebuild(h.doc.model());
    assert_eq!(update.changed, ids([1, 2, 3]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2), id(3)]);
    // Elements the model lost since are removed, the others are changed.
    let mut other = Harness::new();
    other.execute(create(2, "Wall", Vec::new()));
    let update = h.scene.rebuild(other.doc.model());
    assert_eq!(update.removed, ids([1, 3]));
    assert_eq!(update.changed, ids([2]));
}

#[test]
fn an_element_with_several_solids_gets_one_merged_mesh() {
    let mut generators = Generators::new();
    generators.register("wall::body", |_: &Model, _| {
        Ok(Generated {
            solids: vec![prism(1.0, 1.0, 1.0)?, prism(2.0, 2.0, 2.0)?],
            ..Generated::default()
        })
    });
    let mut h = Harness::with_generators(generators, View::new("body", 100), Calls::default());
    h.execute(create(1, "Wall", Vec::new()));
    h.scene.rebuild(h.doc.model());
    let mesh = mesh_of(&mut h, 1).expect("a mesh").mesh;
    assert_eq!(mesh.triangle_count(), 24);
    assert_eq!(mesh.positions.len(), 48);
    assert_eq!(mesh.normals.len(), 48);
    // The second box's triangles point at its own positions.
    assert!(mesh.indices[36..].iter().all(|&index| index >= 24));
}

#[test]
fn an_object_a_generator_reports_reading_makes_its_element_stale() {
    let mut h = Harness::viewing(View::new("body", 300));
    h.execute(batch(vec![
        create(1, "Column", Vec::new()),
        create(2, "Column", Vec::new()),
        create(99, "Finish", Vec::new()),
    ]));
    h.rebuild();
    // No attribute of a column refers to finish 99: only the generator's `inputs` say so.
    let update = h.run(set_real(99, "offset", 0.5));
    assert_eq!(update.changed, ids([1, 2]));
    assert_eq!(h.calls.elements(), vec![id(1), id(2)]);
    // The finish is gone: the columns are meshed again without it, and again when it comes back.
    assert_eq!(h.run(delete(99)).changed, ids([1, 2]));
    assert_eq!(h.run(create(99, "Finish", Vec::new())).changed, ids([1, 2]));
}

#[test]
fn an_object_a_property_refers_to_makes_its_element_stale() {
    let mut h = Harness::new();
    h.execute(batch(vec![walls(2), create(10, "Finish", Vec::new())]));
    h.execute(ModelCommand::AttachPropertySet {
        id: id(1),
        set: PSET.into(),
        values: vec![("AcousticFinish".into(), AttributeValue::Ref(id(10)))],
    });
    h.rebuild();
    // The generator reads no property: the reference alone ties the wall to the finish.
    let update = h.run(set_real(10, "offset", 0.05));
    assert_eq!(update.changed, ids([1]));
    assert_eq!(h.calls.elements(), vec![id(1)]);
}
