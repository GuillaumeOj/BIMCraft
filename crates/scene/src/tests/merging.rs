//! Merging the meshes of an element's solids, and its limit.

use geom::{Point3, TriangleMesh, Vector3};

use crate::SceneError;
use crate::mesh::merge_with_limit;

/// A mesh of `vertices` positions and one triangle over its first three.
fn mesh(vertices: usize, indices: &[u32]) -> TriangleMesh {
    TriangleMesh {
        positions: vec![Point3::new(0.0, 0.0, 0.0); vertices],
        normals: vec![Vector3::new(0.0, 0.0, 1.0); vertices],
        indices: indices.to_vec(),
    }
}

#[test]
fn indices_are_offset_by_the_vertices_before_them() {
    let merged = merge_with_limit(vec![mesh(3, &[0, 1, 2]), mesh(4, &[0, 2, 3])], 10).unwrap();
    assert_eq!(merged.positions.len(), 7);
    assert_eq!(merged.normals.len(), 7);
    assert_eq!(merged.indices, vec![0, 1, 2, 3, 5, 6]);
}

#[test]
fn nothing_merges_into_an_empty_mesh() {
    assert_eq!(
        merge_with_limit(Vec::new(), 10),
        Ok(TriangleMesh::default())
    );
    assert_eq!(
        merge_with_limit(vec![mesh(0, &[])], 0),
        Ok(TriangleMesh::default())
    );
}

#[test]
fn a_merge_past_the_limit_is_an_error_not_a_panic() {
    let parts = || vec![mesh(3, &[0, 1, 2]), mesh(3, &[0, 1, 2])];
    assert_eq!(
        merge_with_limit(parts(), 6).map(|m| m.positions.len()),
        Ok(6)
    );
    assert_eq!(
        merge_with_limit(parts(), 5),
        Err(SceneError::TooManyVertices)
    );
}

#[test]
fn an_index_that_overflows_once_offset_is_an_error() {
    let parts = vec![mesh(3, &[0, 1, 2]), mesh(3, &[u32::MAX, 1, 2])];
    assert_eq!(
        merge_with_limit(parts, 100),
        Err(SceneError::TooManyVertices)
    );
}

#[test]
fn the_error_says_what_is_wrong_without_content() {
    assert_eq!(
        SceneError::TooManyVertices.to_string(),
        "the meshes of the element's solids have too many vertices to merge"
    );
}
