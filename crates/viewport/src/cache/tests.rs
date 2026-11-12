use super::*;
use crate::test_support::{Call, FakeSink, box_mesh, cubes, element, id};

fn run(
    cache: &mut SceneCache,
    sink: &mut FakeSink,
    delta: &SceneDelta,
) -> Result<Applied, SceneError> {
    cache.apply(delta, sink)
}

fn incremental(revision: u64, base: u64) -> SceneDelta {
    SceneDelta {
        revision,
        base,
        strings: vec!["Block".to_owned(), "core".to_owned()],
        ..SceneDelta::default()
    }
}

#[test]
fn a_full_delta_fills_the_cache_and_the_renderer() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    assert_eq!(cache.revision(), None);
    let applied = run(
        &mut cache,
        &mut sink,
        &cubes(3, [0.0; 3], &[[0.0; 3], [2.0, 0.0, 0.0]]),
    )
    .unwrap();
    assert_eq!(cache.revision(), Some(3));
    assert_eq!(applied.touched, crate::test_support::ids(&[1, 2]));
    assert_eq!(
        sink.take_calls(),
        [
            Call::AddMesh(7),
            Call::AddObject(id(1), 7),
            Call::AddObject(id(2), 7)
        ]
    );
    assert!(cache.contains(id(1)));
    let (min, max) = cache.bounds(id(2)).unwrap();
    assert_eq!((min, max), ([1.5, -0.5, -0.5], [2.5, 0.5, 0.5]));
    // A full delta replaces everything held.
    run(&mut cache, &mut sink, &cubes(4, [0.0; 3], &[[5.0; 3]])).unwrap();
    assert_eq!(
        sink.take_calls(),
        [
            Call::RemoveObject(id(1)),
            Call::RemoveObject(id(2)),
            Call::RemoveMesh(7),
            Call::AddMesh(7),
            Call::AddObject(id(1), 7),
        ]
    );
}

#[test]
fn an_incremental_delta_moves_adds_and_removes() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    run(
        &mut cache,
        &mut sink,
        &cubes(1, [0.0; 3], &[[0.0; 3], [2.0, 0.0, 0.0]]),
    )
    .unwrap();
    sink.take_calls();
    // Element 1 moves: only its placement changes. Element 2 goes; element 3 comes with a new
    // mesh; mesh 7 stays in use.
    let mut delta = incremental(2, 1);
    delta.elements = vec![element(1, 7, [0.0, 3.0, 0.0]), element(3, 9, [0.0; 3])];
    delta.meshes = vec![box_mesh(9, [0.0; 3], [0.0; 3], [1.0; 3])];
    delta.removed_elements = vec![id(2).0, id(42).0];
    let applied = run(&mut cache, &mut sink, &delta).unwrap();
    assert_eq!(applied.touched, crate::test_support::ids(&[1, 2, 3]));
    assert_eq!(applied.recreated, crate::test_support::ids(&[2, 3]));
    assert_eq!(
        sink.take_calls(),
        [
            Call::RemoveObject(id(2)),
            Call::AddMesh(9),
            Call::SetPlacement(id(1), [0.0, 3.0, 0.0]),
            Call::AddObject(id(3), 9),
        ]
    );
    // The same placement again changes nothing in the renderer; a new mesh re-creates the
    // object; a mesh sent twice is uploaded once.
    let mut delta = incremental(3, 2);
    delta.elements = vec![element(1, 7, [0.0, 3.0, 0.0]), element(3, 7, [0.0; 3])];
    delta.meshes = vec![box_mesh(7, [0.0; 3], [-0.5; 3], [0.5; 3])];
    delta.removed_meshes = vec![9];
    run(&mut cache, &mut sink, &delta).unwrap();
    assert_eq!(
        sink.take_calls(),
        [
            Call::RemoveObject(id(3)),
            Call::AddObject(id(3), 7),
            Call::RemoveMesh(9),
        ]
    );
    assert_eq!(sink.objects.get(&id(3)).map(|object| object.0), Some(7));
    assert_eq!(cache.revision(), Some(3));
}

#[test]
fn inconsistent_deltas_change_nothing() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    // Before any full delta, an incremental one does not apply.
    assert_eq!(
        run(&mut cache, &mut sink, &incremental(2, 1)),
        Err(SceneError::BaseMismatch)
    );
    run(&mut cache, &mut sink, &cubes(5, [0.0; 3], &[[0.0; 3]])).unwrap();
    sink.take_calls();
    assert_eq!(
        run(&mut cache, &mut sink, &incremental(7, 6)),
        Err(SceneError::BaseMismatch)
    );
    let mut unknown_mesh = incremental(6, 5);
    unknown_mesh.elements = vec![element(2, 8, [0.0; 3])];
    assert_eq!(
        run(&mut cache, &mut sink, &unknown_mesh),
        Err(SceneError::UnknownMesh)
    );
    let mut in_use = incremental(6, 5);
    in_use.removed_meshes = vec![7];
    assert_eq!(
        run(&mut cache, &mut sink, &in_use),
        Err(SceneError::MeshInUse)
    );
    // Removed and placed again in the same delta.
    let mut removed_then_used = incremental(6, 5);
    removed_then_used.removed_meshes = vec![7];
    removed_then_used.removed_elements = vec![id(1).0];
    removed_then_used.elements = vec![element(2, 7, [0.0; 3])];
    assert_eq!(
        run(&mut cache, &mut sink, &removed_then_used),
        Err(SceneError::UnknownMesh)
    );
    // A full delta must carry its meshes.
    let mut full = cubes(9, [0.0; 3], &[[0.0; 3]]);
    full.meshes.clear();
    assert_eq!(
        run(&mut cache, &mut sink, &full),
        Err(SceneError::UnknownMesh)
    );
    assert_eq!(sink.take_calls().len(), 0);
    assert_eq!(cache.revision(), Some(5));
    // Removing the last user of a mesh with the mesh is consistent.
    let mut both = incremental(6, 5);
    both.removed_meshes = vec![7];
    both.removed_elements = vec![id(1).0];
    run(&mut cache, &mut sink, &both).unwrap();
    assert_eq!(
        sink.take_calls(),
        [Call::RemoveObject(id(1)), Call::RemoveMesh(7)]
    );
}

#[test]
fn a_mesh_removed_and_sent_again_keeps_its_objects() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    run(&mut cache, &mut sink, &cubes(1, [0.0; 3], &[[0.0; 3]])).unwrap();
    sink.take_calls();
    let mut delta = incremental(2, 1);
    delta.removed_meshes = vec![7];
    delta.meshes = vec![box_mesh(7, [0.0; 3], [-0.5; 3], [0.5; 3])];
    run(&mut cache, &mut sink, &delta).unwrap();
    assert_eq!(sink.take_calls().len(), 0);
    assert!(sink.objects.contains_key(&id(1)));
    assert!(cache.element(id(1)).is_some());
}

#[test]
fn a_refusal_of_the_renderer_drops_everything() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    run(&mut cache, &mut sink, &cubes(1, [0.0; 3], &[[0.0; 3]])).unwrap();
    sink.objects_before_failure = Some(1);
    let mut delta = incremental(2, 1);
    delta.elements = vec![element(2, 7, [1.0; 3]), element(3, 7, [2.0; 3])];
    let error = run(&mut cache, &mut sink, &delta).unwrap_err();
    assert!(
        matches!(error, SceneError::Render(SinkError::Render(_))),
        "{error:?}"
    );
    assert_eq!(cache.revision(), None);
    assert!(!cache.contains(id(1)) && !cache.contains(id(2)));
    assert!(sink.objects.is_empty() && sink.meshes.is_empty());
    // The next full delta starts again.
    sink.objects_before_failure = None;
    run(&mut cache, &mut sink, &cubes(3, [0.0; 3], &[[0.0; 3]])).unwrap();
    assert_eq!(cache.revision(), Some(3));
}

#[test]
fn spheres_enclose_the_elements() {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    assert_eq!(cache.sphere(None), None);
    run(
        &mut cache,
        &mut sink,
        &cubes(1, [1e7, 0.0, 0.0], &[[0.0; 3], [4.0, 0.0, 0.0]]),
    )
    .unwrap();
    let sphere = cache.sphere(None).unwrap();
    assert_eq!(sphere.centre, [1e7 + 2.0, 0.0, 0.0]);
    assert!((sphere.radius - libm::sqrt(25.0 + 1.0 + 1.0) / 2.0).abs() < 1e-9);
    let one = cache.sphere(Some(&[id(2)])).unwrap();
    assert_eq!(one.centre, [1e7 + 4.0, 0.0, 0.0]);
    assert_eq!(cache.sphere(Some(&[id(9)])), None);
}

#[test]
fn rotations_arrive_column_major() {
    // A quarter turn about z: x → y.
    let quarter = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let placed = placement(&quarter, [1.0, 2.0, 3.0]);
    assert_eq!(apply_rotation(&placed, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
    assert_eq!(placed.translation, [1.0, 2.0, 3.0]);
}

fn apply_rotation(placement: &Placement, v: Vec3) -> Vec3 {
    apply(&placement.rotation, v)
}

#[test]
fn only_feature_edges_are_kept() {
    let cube = box_mesh(1, [0.0; 3], [0.0; 3], [1.0; 3]);
    let edges = feature_edges(&cube.positions, &cube.indices);
    // Each face keeps its 4 border edges, not its diagonal: the vertices are not shared
    // between faces (flat shading).
    assert_eq!(edges.len(), 24);
    // A shared, folded edge is a feature; a shared, flat one is not.
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 1.0],
    ];
    let folded = feature_edges(&positions, &[0, 1, 2, 1, 0, 3]);
    assert!(folded.contains(&[0, 1]));
    let flat = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
    ];
    let kept = feature_edges(&flat, &[0, 1, 2, 1, 0, 3]);
    assert!(!kept.contains(&[0, 1]));
    assert_eq!(kept.len(), 4);
}
