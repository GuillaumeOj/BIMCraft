//! The in-memory form of a delta.

/// What changed in the scene since a revision (`api-contract.md` §1).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SceneDelta {
    /// The viewport drops everything it holds before applying the delta.
    pub full: bool,
    /// The host's scene revision after this delta.
    pub revision: u64,
    /// The revision this delta applies to; 0 when `full`.
    pub base: u64,
    /// Category and discipline names, which elements refer to by position.
    pub strings: Vec<String>,
    /// Meshes added; a mesh is immutable once sent. Ids are unique within the delta.
    pub meshes: Vec<Mesh>,
    /// Ids of meshes no element refers to after this delta.
    pub removed_meshes: Vec<u64>,
    /// Elements added or changed. Ids are unique within the delta.
    pub elements: Vec<Element>,
    /// Ids of elements removed.
    pub removed_elements: Vec<[u8; 16]>,
}

/// A triangle mesh with `f32` vertices relative to an `f64` origin (ADR 0020).
///
/// Of the same shape as the renderer's mesh data, so the viewport converts without copying
/// more than it must.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Content hash chosen by the host; unique per document.
    pub id: u64,
    /// Origin of the local coordinates, in metres.
    pub origin: [f64; 3],
    /// Vertex positions relative to `origin`, in metres.
    pub positions: Vec<[f32; 3]>,
    /// One unit normal per position.
    pub normals: Vec<[f32; 3]>,
    /// Triangle list: three indices into `positions` per triangle.
    pub indices: Vec<u32>,
    /// Minimum corner of the local bounding box, for culling and level of detail.
    pub bounds_min: [f32; 3],
    /// Maximum corner of the local bounding box.
    pub bounds_max: [f32; 3],
}

/// An element: a mesh and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// The element's id (an `EntityId`, 128 bits).
    pub id: [u8; 16],
    /// A mesh id sent in this delta or earlier.
    pub mesh: u64,
    /// Column-major rotation: orthonormal with determinant +1, never a reflection.
    pub rotation: [f64; 9],
    /// Translation, in metres.
    pub translation: [f64; 3],
    /// Position of the category name in `strings`.
    pub category: u32,
    /// Position of the discipline name in `strings`.
    pub discipline: u32,
}
