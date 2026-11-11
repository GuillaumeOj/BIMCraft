//! Triangle meshes with `f32` vertices relative to an `f64` origin (GEO-07).

use geom::Point3;

/// The largest number of vertices of a mesh. It fits `u32` indices, and the vertex buffers stay
/// within the default buffer size of the GPU (256 MiB).
pub const MAX_MESH_VERTICES: usize = 1 << 24;

/// The largest number of indices of a mesh: three per triangle, 16 777 216 triangles.
pub const MAX_MESH_INDICES: usize = 3 << 24;

/// How far from unit length a normal may be: the rounding of a normal computed in `f64` and
/// stored in `f32` is many orders of magnitude smaller.
const NORMAL_LENGTH_TOLERANCE: f32 = 1e-3;

/// A triangle mesh whose `f32` vertices are relative to an `f64` origin (GEO-07, VP-04).
///
/// Keeping the local coordinates small keeps their precision: a 100 m element centred on its
/// origin stores its vertices to better than 10 µm.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshData {
    /// Origin of the local coordinates, in metres, in the coordinates the placement maps to the
    /// world.
    pub origin: [f64; 3],
    /// Vertex positions relative to `origin`, in metres.
    pub positions: Vec<[f32; 3]>,
    /// One unit normal per position.
    pub normals: Vec<[f32; 3]>,
    /// Triangle list: three indices into `positions` per triangle.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Builds a mesh from `f64` positions; its origin is the centre of their bounding box.
    /// The normals and indices are moved into the mesh, not copied.
    ///
    /// # Errors
    ///
    /// A [`MeshError`] when a position is not finite or lies beyond
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE), or when the mesh fails
    /// [`MeshData::validate`].
    pub fn from_world(
        positions: &[Point3],
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
    ) -> Result<Self, MeshError> {
        check_counts(positions.len(), normals.len(), indices.len())?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for (vertex, point) in positions.iter().enumerate() {
            let coordinates = [point.x, point.y, point.z];
            if !coordinates.iter().all(|&c| crate::in_world(c)) {
                return Err(MeshError::NonFinitePosition { vertex });
            }
            for axis in 0..3 {
                min[axis] = min[axis].min(coordinates[axis]);
                max[axis] = max[axis].max(coordinates[axis]);
            }
        }
        // Halving each bound before adding cannot overflow and keeps the centre exact enough.
        let origin: [f64; 3] = core::array::from_fn(|axis| min[axis] / 2.0 + max[axis] / 2.0);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "local coordinates are small, f32 is their storage precision"
        )]
        let local = positions
            .iter()
            .map(|p| core::array::from_fn(|axis| ([p.x, p.y, p.z][axis] - origin[axis]) as f32))
            .collect();
        // The origin and the local positions lie within the world bounds by construction: the
        // origin is between bounds already checked, and each local coordinate is at most half
        // their spread, which rounds to at most `MAX_WORLD_COORDINATE`, exact in `f32`.
        check_normals(&normals)?;
        check_indices(&indices, positions.len())?;
        Ok(Self {
            origin,
            positions: local,
            normals,
            indices,
        })
    }

    /// Checks that the mesh can be drawn: sizes within bounds, one normal per position, finite
    /// values, local coordinates within [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE),
    /// unit normals, whole triangles and indices in range.
    ///
    /// # Errors
    ///
    /// The first [`MeshError`] found.
    pub fn validate(&self) -> Result<(), MeshError> {
        check_counts(self.positions.len(), self.normals.len(), self.indices.len())?;
        if !self.origin.iter().all(|c| c.is_finite()) {
            return Err(MeshError::NonFiniteOrigin);
        }
        if !self.origin.iter().all(|&c| crate::in_world(c)) {
            return Err(MeshError::OriginOutOfRange);
        }
        if let Some(vertex) = self
            .positions
            .iter()
            .position(|p| !p.iter().all(|&c| crate::in_world(f64::from(c))))
        {
            return Err(MeshError::NonFinitePosition { vertex });
        }
        check_normals(&self.normals)?;
        check_indices(&self.indices, self.positions.len())
    }
}

fn check_normals(normals: &[[f32; 3]]) -> Result<(), MeshError> {
    match normals.iter().position(|n| !is_unit(*n)) {
        Some(vertex) => Err(MeshError::InvalidNormal { vertex }),
        None => Ok(()),
    }
}

fn check_indices(indices: &[u32], vertex_count: usize) -> Result<(), MeshError> {
    if !indices.len().is_multiple_of(3) {
        return Err(MeshError::IndexCountNotMultipleOfThree {
            count: indices.len(),
        });
    }
    match indices
        .iter()
        .enumerate()
        .find(|&(_, &index)| usize::try_from(index).map_or(true, |i| i >= vertex_count))
    {
        Some((position, &index)) => Err(MeshError::IndexOutOfRange {
            position,
            index,
            vertex_count,
        }),
        None => Ok(()),
    }
}

/// Rejects empty, oversized and mismatched meshes before anything is read or allocated.
fn check_counts(vertices: usize, normals: usize, indices: usize) -> Result<(), MeshError> {
    if vertices == 0 || indices == 0 {
        return Err(MeshError::Empty);
    }
    if vertices > MAX_MESH_VERTICES {
        return Err(MeshError::TooManyVertices {
            count: vertices,
            max: MAX_MESH_VERTICES,
        });
    }
    if indices > MAX_MESH_INDICES {
        return Err(MeshError::TooManyIndices {
            count: indices,
            max: MAX_MESH_INDICES,
        });
    }
    if normals != vertices {
        return Err(MeshError::LengthMismatch {
            positions: vertices,
            normals,
        });
    }
    Ok(())
}

fn is_unit(normal: [f32; 3]) -> bool {
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    // A NaN length fails the comparison, an infinite one too.
    (length - 1.0).abs() <= NORMAL_LENGTH_TOLERANCE
}

/// Why a mesh cannot be drawn. Messages name vertices and counts, never coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MeshError {
    /// The mesh has no vertices or no indices.
    #[error("the mesh has no vertices or no triangles")]
    Empty,
    /// The mesh has more vertices than [`MAX_MESH_VERTICES`].
    #[error("the mesh has {count} vertices, more than the {max} allowed")]
    TooManyVertices {
        /// Number of vertices.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// The mesh has more indices than [`MAX_MESH_INDICES`].
    #[error("the mesh has {count} indices, more than the {max} allowed")]
    TooManyIndices {
        /// Number of indices.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// The mesh does not have one normal per position.
    #[error("the mesh has {positions} positions but {normals} normals")]
    LengthMismatch {
        /// Number of positions.
        positions: usize,
        /// Number of normals.
        normals: usize,
    },
    /// The number of indices is not a multiple of three, so the last triangle is incomplete.
    #[error("the mesh has {count} indices, which is not a whole number of triangles")]
    IndexCountNotMultipleOfThree {
        /// Number of indices.
        count: usize,
    },
    /// An index refers to a vertex that does not exist.
    #[error("index {position} of the mesh refers to vertex {index}, but it has {vertex_count}")]
    IndexOutOfRange {
        /// Position of the index in the index list.
        position: usize,
        /// The index.
        index: u32,
        /// Number of vertices.
        vertex_count: usize,
    },
    /// The origin of the mesh is not finite.
    #[error("the origin of the mesh is not a finite point")]
    NonFiniteOrigin,
    /// The origin of the mesh lies beyond [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("the origin of the mesh lies too far from the world origin")]
    OriginOutOfRange,
    /// A position is not finite or has a coordinate beyond
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE): in the world for
    /// [`MeshData::from_world`], relative to the origin for [`MeshData::validate`].
    #[error("vertex {vertex} of the mesh is not a finite point within the world bounds")]
    NonFinitePosition {
        /// The vertex.
        vertex: usize,
    },
    /// A normal is not finite or not of unit length.
    #[error("the normal of vertex {vertex} of the mesh is not a unit vector")]
    InvalidNormal {
        /// The vertex.
        vertex: usize,
    },
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::MAX_WORLD_COORDINATE;

    const UP: [f32; 3] = [0.0, 0.0, 1.0];

    /// Exact comparison of points, bit for bit.
    fn bits(point: [f64; 3]) -> [u64; 3] {
        point.map(f64::to_bits)
    }

    fn triangle() -> MeshData {
        MeshData {
            origin: [1.0, 2.0, 3.0],
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![UP; 3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn a_valid_triangle_passes() {
        assert_eq!(triangle().validate(), Ok(()));
    }

    #[test]
    fn from_world_centres_the_origin_on_the_bounding_box() {
        let points = [
            Point3::new(1e6 + 1.0, 1e6 - 2.0, 100.0),
            Point3::new(1e6 + 3.0, 1e6 + 4.0, 100.0),
            Point3::new(1e6 + 2.0, 1e6, 102.0),
        ];
        let mesh = MeshData::from_world(&points, vec![UP; 3], vec![0, 1, 2]).unwrap();
        assert_eq!(bits(mesh.origin), bits([1e6 + 2.0, 1e6 + 1.0, 101.0]));
        assert_eq!(
            mesh.positions,
            vec![[-1.0, -3.0, -1.0], [1.0, 3.0, -1.0], [0.0, -1.0, 1.0]]
        );
        for (local, world) in mesh.positions.iter().zip(points) {
            assert_eq!(
                (f64::from(local[0]) + mesh.origin[0]).to_bits(),
                world.x.to_bits()
            );
            assert_eq!(
                (f64::from(local[1]) + mesh.origin[1]).to_bits(),
                world.y.to_bits()
            );
            assert_eq!(
                (f64::from(local[2]) + mesh.origin[2]).to_bits(),
                world.z.to_bits()
            );
        }
    }

    #[test]
    fn from_world_handles_the_extreme_world_bounds() {
        let points = [
            Point3::new(-MAX_WORLD_COORDINATE, 0.0, 0.0),
            Point3::new(MAX_WORLD_COORDINATE, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let mesh = MeshData::from_world(&points, vec![UP; 3], vec![0, 1, 2]).unwrap();
        assert_eq!(bits(mesh.origin), bits([0.0, 0.5, 0.0]));
        assert_eq!(mesh.validate(), Ok(()));
    }

    #[test]
    fn from_world_rejects_points_that_are_not_finite_or_too_far() {
        for bad in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            MAX_WORLD_COORDINATE * 2.0,
        ] {
            for axis in 0..3 {
                let mut points = [Point3::new(0.0, 0.0, 0.0); 3];
                let mut coordinates = [0.0; 3];
                coordinates[axis] = bad;
                points[2] = Point3::new(coordinates[0], coordinates[1], coordinates[2]);
                assert_eq!(
                    MeshData::from_world(&points, vec![UP; 3], vec![0, 1, 2]),
                    Err(MeshError::NonFinitePosition { vertex: 2 })
                );
            }
        }
    }

    #[test]
    fn from_world_checks_the_counts_first_and_validates_the_rest() {
        assert_eq!(
            MeshData::from_world(&[], Vec::new(), vec![0, 1, 2]),
            Err(MeshError::Empty)
        );
        let points = [Point3::new(0.0, 0.0, 0.0); 3];
        assert_eq!(
            MeshData::from_world(&points, vec![UP; 2], vec![0, 1, 2]),
            Err(MeshError::LengthMismatch {
                positions: 3,
                normals: 2
            })
        );
    }

    #[test]
    fn empty_meshes_are_rejected() {
        let mut mesh = triangle();
        mesh.indices.clear();
        assert_eq!(mesh.validate(), Err(MeshError::Empty));
        let mut mesh = triangle();
        mesh.positions.clear();
        assert_eq!(mesh.validate(), Err(MeshError::Empty));
    }

    #[test]
    fn oversized_meshes_are_rejected_without_reading_them() {
        assert_eq!(
            check_counts(MAX_MESH_VERTICES, MAX_MESH_VERTICES, MAX_MESH_INDICES),
            Ok(())
        );
        assert_eq!(
            check_counts(MAX_MESH_VERTICES + 1, MAX_MESH_VERTICES + 1, 3),
            Err(MeshError::TooManyVertices {
                count: MAX_MESH_VERTICES + 1,
                max: MAX_MESH_VERTICES
            })
        );
        assert_eq!(
            check_counts(3, 3, MAX_MESH_INDICES + 1),
            Err(MeshError::TooManyIndices {
                count: MAX_MESH_INDICES + 1,
                max: MAX_MESH_INDICES
            })
        );
    }

    #[test]
    fn the_limits_fit_gpu_indices_and_buffers() {
        assert!(u32::try_from(MAX_MESH_VERTICES).is_ok());
        assert!(u32::try_from(MAX_MESH_INDICES).is_ok());
        assert!(MAX_MESH_VERTICES * size_of::<[f32; 3]>() <= 256 << 20);
        assert!(MAX_MESH_INDICES * size_of::<u32>() <= 256 << 20);
    }

    #[test]
    fn a_missing_normal_is_rejected() {
        let mut mesh = triangle();
        mesh.normals.pop();
        assert_eq!(
            mesh.validate(),
            Err(MeshError::LengthMismatch {
                positions: 3,
                normals: 2
            })
        );
    }

    #[test]
    fn an_origin_that_is_not_finite_or_too_far_is_rejected() {
        for (bad, error) in [
            (f64::NAN, MeshError::NonFiniteOrigin),
            (f64::INFINITY, MeshError::NonFiniteOrigin),
            (-MAX_WORLD_COORDINATE * 1.5, MeshError::OriginOutOfRange),
        ] {
            for axis in 0..3 {
                let mut mesh = triangle();
                mesh.origin[axis] = bad;
                assert_eq!(mesh.validate(), Err(error.clone()));
            }
        }
    }

    #[test]
    fn a_position_that_is_not_finite_or_too_far_is_rejected() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, -2e9] {
            for axis in 0..3 {
                let mut mesh = triangle();
                mesh.positions[1][axis] = bad;
                assert_eq!(
                    mesh.validate(),
                    Err(MeshError::NonFinitePosition { vertex: 1 })
                );
            }
        }
    }

    #[test]
    fn normals_must_be_finite_unit_vectors() {
        for bad in [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.01],
            [f32::NAN, 0.0, 1.0],
            [0.0, f32::INFINITY, 0.0],
        ] {
            let mut mesh = triangle();
            mesh.normals[2] = bad;
            assert_eq!(mesh.validate(), Err(MeshError::InvalidNormal { vertex: 2 }));
        }
        let mut mesh = triangle();
        mesh.normals[0] = [0.6, 0.0, 0.8];
        mesh.normals[1] = [0.0, 0.0, 1.0005];
        assert_eq!(mesh.validate(), Ok(()));
    }

    #[test]
    fn incomplete_triangles_are_rejected() {
        let mut mesh = triangle();
        mesh.indices.push(0);
        assert_eq!(
            mesh.validate(),
            Err(MeshError::IndexCountNotMultipleOfThree { count: 4 })
        );
    }

    #[test]
    fn indices_out_of_range_are_rejected() {
        let mut mesh = triangle();
        mesh.indices = vec![0, 1, 2, 2, 3, 0];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::IndexOutOfRange {
                position: 4,
                index: 3,
                vertex_count: 3
            })
        );
        mesh.indices = vec![0, 1, u32::MAX];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::IndexOutOfRange {
                position: 2,
                index: u32::MAX,
                vertex_count: 3
            })
        );
    }

    #[test]
    fn messages_never_contain_coordinates() {
        let points = [
            Point3::new(123_456.789, 0.0, 0.0),
            Point3::new(0.0, 987_654.321, 0.0),
            Point3::new(0.0, 0.0, f64::NAN),
        ];
        let error = MeshData::from_world(&points, vec![UP; 3], vec![0, 1, 2]).unwrap_err();
        let message = error.to_string();
        assert_eq!(
            message,
            "vertex 2 of the mesh is not a finite point within the world bounds"
        );
        for error in [
            MeshError::Empty,
            MeshError::NonFiniteOrigin,
            MeshError::OriginOutOfRange,
            MeshError::InvalidNormal { vertex: 7 },
            MeshError::IndexCountNotMultipleOfThree { count: 4 },
            MeshError::TooManyVertices { count: 9, max: 8 },
            MeshError::TooManyIndices { count: 9, max: 8 },
            MeshError::LengthMismatch {
                positions: 3,
                normals: 2,
            },
            MeshError::IndexOutOfRange {
                position: 1,
                index: 5,
                vertex_count: 3,
            },
        ] {
            let message = error.to_string();
            assert!(!message.contains("123456") && !message.contains("987654"));
            assert!(!message.is_empty());
        }
    }

    fn any_f32() -> impl Strategy<Value = f32> {
        prop_oneof![
            -10.0f32..10.0,
            Just(f32::NAN),
            Just(f32::INFINITY),
            any::<f32>()
        ]
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Arbitrary meshes are accepted or rejected with an error, never a panic.
        #[test]
        fn validation_never_panics(
            origin in proptest::array::uniform3(any::<f64>()),
            positions in proptest::collection::vec(proptest::array::uniform3(any_f32()), 0..8),
            normals in proptest::collection::vec(proptest::array::uniform3(any_f32()), 0..8),
            indices in proptest::collection::vec(0u32..10, 0..12),
        ) {
            let mesh = MeshData { origin, positions, normals, indices };
            let _ = mesh.validate();
            let world: Vec<Point3> = mesh
                .positions
                .iter()
                .map(|p| Point3::new(f64::from(p[0]) + origin[0], f64::from(p[1]), f64::from(p[2])))
                .collect();
            // What from_world builds always passes the full validation it skips.
            if let Ok(built) = MeshData::from_world(&world, mesh.normals, mesh.indices) {
                prop_assert_eq!(built.validate(), Ok(()));
            }
        }
    }
}
