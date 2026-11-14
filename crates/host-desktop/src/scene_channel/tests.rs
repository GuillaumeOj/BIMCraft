use core_model::{AttributeValue, EntityId, Model};
use core_txn::{Document, ModelCommand};
use scene_wire::{SceneDelta, decode};
use serde_json::{Value, json};

use super::*;
use crate::DocumentHost;

fn call(host: &DocumentHost, method: &str, params: &Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let handled = host.handle(&request.to_string());
    serde_json::from_str(&handled.response.unwrap()).unwrap()
}

/// Asserts two points agree to a nanometre.
fn assert_near(actual: [f64; 3], expected: [f64; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} is not {expected:?}");
    }
}

fn id(n: u128) -> EntityId {
    EntityId::new(n)
}

fn reals(values: &[f64]) -> AttributeValue {
    AttributeValue::List(values.iter().map(|v| AttributeValue::Real(*v)).collect())
}

fn place_block(host: &DocumentHost, n: u128, at: [f64; 3], size: [f64; 3]) {
    let command = ModelCommand::Create {
        id: id(n),
        entity: "Block".into(),
        values: vec![
            ("location".into(), reals(&at)),
            ("rotation".into(), AttributeValue::Real(0.0)),
            ("size_x".into(), AttributeValue::Real(size[0])),
            ("size_y".into(), AttributeValue::Real(size[1])),
            ("size_z".into(), AttributeValue::Real(size[2])),
        ],
    };
    let mut params = serde_json::to_value(&command).unwrap();
    params.as_object_mut().unwrap().remove("command");
    let response = call(host, "model.create", &params);
    assert!(response.get("error").is_none(), "{response}");
}

fn move_blocks(host: &DocumentHost, ids: &[u128], by: [f64; 3]) {
    let ids: Vec<String> = ids.iter().map(|n| format!("{n:032x}")).collect();
    let response = call(
        host,
        "elements.move",
        &json!({ "ids": ids, "translation": by }),
    );
    assert!(response.get("error").is_none(), "{response}");
}

fn delete(host: &DocumentHost, n: u128) {
    let response = call(host, "model.delete", &json!({ "id": format!("{n:032x}") }));
    assert!(response.get("error").is_none(), "{response}");
}

fn history(host: &DocumentHost, method: &str) {
    let response = call(host, method, &json!({}));
    assert!(response.get("error").is_none(), "{response}");
}

fn delta(host: &DocumentHost, since: Option<u64>) -> SceneDelta {
    decode(&host.scene(since)).unwrap()
}

fn element_ids(delta: &SceneDelta) -> Vec<u128> {
    delta
        .elements
        .iter()
        .map(|e| u128::from_le_bytes(e.id))
        .collect()
}

fn removed_ids(delta: &SceneDelta) -> Vec<u128> {
    delta
        .removed_elements
        .iter()
        .map(|id| u128::from_le_bytes(*id))
        .collect()
}

const UNIT: [f64; 3] = [1.0, 1.0, 1.0];

#[test]
fn an_empty_document_has_an_empty_full_scene() {
    let host = DocumentHost::open_new().unwrap();
    let first = delta(&host, None);
    assert!(first.full);
    assert_eq!((first.revision, first.base), (0, 0));
    assert!(first.elements.is_empty() && first.meshes.is_empty());
    // A viewport that holds revision 0 is current.
    let current = delta(&host, Some(0));
    assert!(!current.full && current.elements.is_empty());
}

#[test]
fn the_first_delta_is_the_full_scene_with_shared_meshes() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    place_block(&host, 2, [5.0, 0.0, 0.0], UNIT);
    place_block(&host, 3, [9.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let full = delta(&host, None);
    assert!(full.full);
    assert_eq!(full.revision, 3);
    assert_eq!(element_ids(&full), [1, 2, 3]);
    // Two of the blocks have the same size, so three elements draw two meshes.
    assert_eq!(full.meshes.len(), 2);
    assert_eq!(full.elements[0].mesh, full.elements[1].mesh);
    assert_ne!(full.elements[0].mesh, full.elements[2].mesh);
    assert_eq!(full.strings, ["Block", "core"]);
    assert!(
        full.elements
            .iter()
            .all(|e| (e.category, e.discipline) == (0, 1))
    );
    assert_near(full.elements[1].translation, [5.0, 0.0, 0.0]);
}

#[test]
fn creating_an_element_sends_it_alone() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    place_block(&host, 2, [3.0, 0.0, 0.0], [2.0, 1.0, 1.0]);
    let update = delta(&host, Some(seen));
    assert!(!update.full);
    assert_eq!((update.base, update.revision), (seen, seen + 1));
    assert_eq!(element_ids(&update), [2]);
    assert_eq!(update.meshes.len(), 1);
    assert!(update.removed_meshes.is_empty() && update.removed_elements.is_empty());
}

fn host_revision(host: &DocumentHost) -> u64 {
    delta(host, None).revision
}

#[test]
fn a_move_sends_a_placement_and_no_mesh() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    place_block(&host, 2, [5.0, 0.0, 0.0], UNIT);
    let seen = host_revision(&host);
    move_blocks(&host, &[1], [0.0, 10.0, 0.0]);
    let update = delta(&host, Some(seen));
    assert_eq!(element_ids(&update), [1]);
    assert!(update.meshes.is_empty() && update.removed_meshes.is_empty());
    assert_near(update.elements[0].translation, [0.0, 10.0, 0.0]);
    assert_eq!(update.revision, seen + 1);
}

#[test]
fn a_move_remeshes_only_the_elements_it_moves() {
    let host = DocumentHost::open_new().unwrap();
    for n in 1..=5 {
        place_block(&host, n, [0.0; 3], UNIT);
    }
    let seen = host_revision(&host);
    move_blocks(&host, &[2, 4], [1.0, 0.0, 0.0]);
    let update = delta(&host, Some(seen));
    assert_eq!(element_ids(&update), [2, 4]);
    assert!(update.meshes.is_empty());
}

#[test]
fn deleting_an_element_removes_it_and_its_mesh_when_nothing_else_uses_it() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    place_block(&host, 2, [5.0, 0.0, 0.0], UNIT);
    place_block(&host, 3, [9.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let full = delta(&host, None);
    let unit_mesh = full.elements[0].mesh;
    let big_mesh = full.elements[2].mesh;
    let seen = full.revision;

    delete(&host, 1);
    let update = delta(&host, Some(seen));
    assert_eq!(removed_ids(&update), [1]);
    assert!(update.removed_meshes.is_empty(), "block 2 still uses it");

    delete(&host, 3);
    let update = delta(&host, Some(seen));
    assert_eq!(removed_ids(&update), [1, 3]);
    assert_eq!(update.removed_meshes, [big_mesh]);
    delete(&host, 2);
    let update = delta(&host, Some(seen));
    assert!(update.removed_meshes.contains(&unit_mesh));
    assert!(delta(&host, None).elements.is_empty());
}

#[test]
fn undo_and_redo_are_deltas_too() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);

    move_blocks(&host, &[1], [4.0, 0.0, 0.0]);
    history(&host, "history.undo");
    let after_undo = delta(&host, Some(seen));
    assert_eq!(after_undo.revision, seen + 2);
    assert_eq!(element_ids(&after_undo), [1]);
    assert_near(after_undo.elements[0].translation, [0.0; 3]);

    history(&host, "history.redo");
    let after_redo = delta(&host, Some(seen));
    assert_near(after_redo.elements[0].translation, [4.0, 0.0, 0.0]);

    // Undoing the creation removes the element; redoing it brings it back, with its mesh.
    history(&host, "history.undo");
    history(&host, "history.undo");
    let gone = delta(&host, Some(seen));
    assert_eq!(removed_ids(&gone), [1]);
    assert!(gone.elements.is_empty());
    assert_eq!(gone.removed_meshes.len(), 1);
    history(&host, "history.redo");
    let back = delta(&host, None);
    assert_eq!(element_ids(&back), [1]);
    assert_eq!(back.meshes.len(), 1);
}

#[test]
fn a_viewport_further_behind_than_the_log_gets_the_full_scene() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    for step in 0..LOG_LENGTH {
        let by = if step % 2 == 0 { 1.0 } else { -1.0 };
        move_blocks(&host, &[1], [by, 0.0, 0.0]);
    }
    // The log keeps the last LOG_LENGTH revisions: `seen` is the one just before them.
    assert!(!delta(&host, Some(seen)).full);
    move_blocks(&host, &[1], [1.0, 0.0, 0.0]);
    let behind = delta(&host, Some(seen));
    assert!(behind.full);
    assert_eq!(behind.base, 0);
    assert_eq!(behind.elements.len(), 1);
}

#[test]
fn an_unknown_revision_gets_the_full_scene() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    assert!(delta(&host, Some(1_000)).full);
    assert!(delta(&host, None).full);
}

#[test]
fn a_command_that_changes_nothing_in_the_scene_keeps_the_revision() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    // The element is re-meshed to the same mesh and placement.
    let response = call(
        &host,
        "model.set",
        &json!({ "id": format!("{:032x}", 1), "attribute": "size_x", "value": { "Real": 1.0 } }),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(host_revision(&host), seen);
    let update = delta(&host, Some(seen));
    assert!(update.elements.is_empty() && update.meshes.is_empty());
}

#[test]
fn changes_since_a_revision_are_the_union_of_the_revisions_after_it() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    place_block(&host, 2, [5.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    move_blocks(&host, &[2], [1.0, 0.0, 0.0]);
    delete(&host, 1);
    let update = delta(&host, Some(seen));
    assert_eq!(element_ids(&update), [2]);
    assert_eq!(removed_ids(&update), [1]);
    assert_eq!(update.meshes.len(), 1, "the big mesh, once");
    assert_eq!(update.removed_meshes.len(), 1, "the unit mesh");
    assert_near(update.elements[0].translation, [6.0, 0.0, 0.0]);
    assert_eq!((update.base, update.revision), (seen, seen + 3));
}

#[test]
fn a_mesh_created_and_dropped_between_two_revisions_is_never_sent() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    place_block(&host, 2, [5.0, 0.0, 0.0], [3.0, 3.0, 3.0]);
    delete(&host, 2);
    let update = delta(&host, Some(seen));
    assert!(update.meshes.is_empty() && update.removed_meshes.is_empty());
    assert_eq!(removed_ids(&update), [2]);
}

#[test]
fn two_geometries_with_one_hash_keep_their_own_meshes() {
    let mut document = Document::new(Model::new(&crate::core_namespace().unwrap()).unwrap());
    for (n, size) in [(1, 1.0), (2, 2.0), (3, 1.0)] {
        let create = ModelCommand::Create {
            id: id(n),
            entity: "Block".into(),
            values: vec![
                ("location".into(), reals(&[0.0; 3])),
                ("size_x".into(), AttributeValue::Real(size)),
                ("size_y".into(), AttributeValue::Real(size)),
                ("size_z".into(), AttributeValue::Real(size)),
            ],
        };
        document.execute(create, core_txn::Origin::Ui).unwrap();
    }
    let model = document.model();
    let channel = SceneChannel::with_hash(model, |_| 7);
    let full = channel.delta(None);
    assert_eq!(full.meshes.len(), 2);
    let ids: BTreeSet<u64> = full.meshes.iter().map(|m| m.id).collect();
    assert_eq!(ids, BTreeSet::from([7, 8]));
    assert_eq!(full.elements[0].mesh, full.elements[2].mesh);
    assert_ne!(full.elements[0].mesh, full.elements[1].mesh);
    assert_eq!(channel.mesh_count(), 2);
}

#[test]
fn identical_geometry_hashes_alike_and_different_geometry_differently() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    place_block(&host, 2, [8.0, 0.0, 0.0], UNIT);
    place_block(&host, 3, [0.0; 3], [1.0, 1.0, 2.0]);
    let full = delta(&host, None);
    assert_eq!(full.meshes.len(), 2);
    assert_eq!(
        full.meshes
            .iter()
            .map(|m| m.id)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
}

#[test]
fn every_delta_decodes_and_meshes_enclose_their_vertices() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [1e8, -1e8, 5.0], [10.0, 20.0, 30.0]);
    let full = delta(&host, None);
    let mesh = &full.meshes[0];
    assert_near(mesh.origin, [0.0, 0.0, 15.0]);
    assert_eq!(mesh.positions.len(), 24);
    assert_near(mesh.bounds_min.map(f64::from), [-5.0, -10.0, -15.0]);
    assert_near(mesh.bounds_max.map(f64::from), [5.0, 10.0, 15.0]);
}

#[test]
fn a_block_that_draws_nothing_is_not_sent() {
    let host = DocumentHost::open_new().unwrap();
    // A zero height gives no geometry: the element fails and the viewport never hears of it.
    place_block(&host, 1, [0.0; 3], [1.0, 1.0, 0.0]);
    assert!(delta(&host, None).elements.is_empty());
    assert_eq!(host_revision(&host), 0);
}

#[test]
fn an_element_that_starts_failing_is_removed() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0; 3], UNIT);
    let seen = host_revision(&host);
    let response = call(
        &host,
        "model.set",
        &json!({ "id": format!("{:032x}", 1), "attribute": "size_z", "value": { "Real": 0.0 } }),
    );
    assert!(response.get("error").is_none(), "{response}");
    let update = delta(&host, Some(seen));
    assert_eq!(removed_ids(&update), [1]);
}

#[test]
fn invalid_placements_are_refused() {
    let mut placement = Placement::about_z(0.3, [1.0, 2.0, 3.0]);
    assert!(placement_is_valid(&placement));
    placement.rotation[0] = 2.0;
    assert!(!placement_is_valid(&placement));
    let far = Placement::about_z(0.0, [2e9, 0.0, 0.0]);
    assert!(!placement_is_valid(&far));
    let mut reflection = Placement::IDENTITY;
    reflection.rotation[8] = -1.0;
    assert!(!placement_is_valid(&reflection));
    let mut skew = Placement::IDENTITY;
    skew.rotation[3] = 0.5;
    assert!(!placement_is_valid(&skew));
}

#[test]
fn malformed_meshes_are_refused() {
    use geom::{Point3, TriangleMesh, Vector3};
    let triangle = |indices: Vec<u32>, normals: usize| scene::ElementMesh {
        mesh: TriangleMesh {
            positions: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
            ],
            normals: vec![Vector3::new(0.0, 0.0, 1.0); normals],
            indices,
        },
        placement: Placement::IDENTITY,
    };
    assert!(wire_mesh(&triangle(vec![0, 1, 2], 3)).is_some());
    assert!(wire_mesh(&triangle(vec![0, 1, 2], 2)).is_none(), "normals");
    assert!(
        wire_mesh(&triangle(vec![0, 1], 3)).is_none(),
        "not triangles"
    );
    assert!(
        wire_mesh(&triangle(vec![0, 1, 3], 3)).is_none(),
        "index out of range"
    );
    assert!(wire_mesh(&scene::ElementMesh::default()).is_none(), "empty");
    let mut far = triangle(vec![0, 1, 2], 3);
    far.mesh.positions[0] = Point3::new(f64::NAN, 0.0, 0.0);
    assert!(wire_mesh(&far).is_none(), "not finite");
    let mut wide = triangle(vec![0, 1, 2], 3);
    wide.mesh.positions[1] = Point3::new(5e9, 0.0, 0.0);
    assert!(wire_mesh(&wide).is_none(), "beyond the world extent");
    let mut flat = triangle(vec![0, 1, 2], 3);
    flat.mesh.normals[0] = Vector3::new(0.0, 0.0, 0.0);
    assert!(wire_mesh(&flat).is_none(), "not a unit normal");
}

#[test]
fn a_block_wider_than_the_world_does_not_spoil_the_delta() {
    let host = DocumentHost::open_new().unwrap();
    place_block(&host, 1, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    place_block(&host, 2, [0.0, 0.0, 0.0], [5e9, 1.0, 1.0]);
    let full = delta(&host, None);
    assert_eq!(element_ids(&full), [1]);
}
