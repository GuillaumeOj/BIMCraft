//! Why a delta is refused.

/// A section of the format, named in errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// The strings.
    Strings,
    /// The added meshes.
    Meshes,
    /// The removed meshes.
    RemovedMeshes,
    /// The added or changed elements.
    Elements,
    /// The removed elements.
    RemovedElements,
}

/// Why a delta is refused. Messages carry positions and counts, never coordinates or model
/// content (SEC-03). `index` is the position in the section, from 0.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WireError {
    /// The input is larger than [`MAX_DELTA_BYTES`](crate::MAX_DELTA_BYTES).
    #[error("the delta is larger than {max} bytes")]
    TooLarge {
        /// The limit, in bytes.
        max: usize,
    },
    /// The input ends before the section it announces does.
    #[error("the delta ends too early")]
    Truncated,
    /// Bytes remain after the last section.
    #[error("{count} bytes follow the last section")]
    TrailingBytes {
        /// The number of bytes left.
        count: usize,
    },
    /// The first four bytes are not [`MAGIC`](crate::MAGIC).
    #[error("this is not a scene delta")]
    BadMagic,
    /// The version is not [`VERSION`](crate::VERSION).
    #[error("scene delta version {version} is not supported")]
    UnsupportedVersion {
        /// The version found.
        version: u16,
    },
    /// A flag bit other than `full` is set.
    #[error("the delta sets flags this reader does not know")]
    UnknownFlags,
    /// The reserved header field is not 0.
    #[error("the reserved header field is not 0")]
    ReservedNotZero,
    /// A revision is 2^53 or more.
    #[error("a revision is too large")]
    RevisionTooLarge,
    /// A full delta applies to a base revision.
    #[error("a full delta has a base revision")]
    FullWithBase,
    /// A section announces more entries than its limit.
    #[error("{section:?} announces {count} entries, the limit is {max}")]
    TooMany {
        /// The section.
        section: Section,
        /// The announced count.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// A string is longer than [`MAX_STRING_BYTES`](crate::MAX_STRING_BYTES).
    #[error("string {index} is {len} bytes long")]
    StringTooLong {
        /// Position of the string.
        index: usize,
        /// Its length, in bytes.
        len: usize,
    },
    /// A string is not UTF-8.
    #[error("string {index} is not UTF-8")]
    InvalidUtf8 {
        /// Position of the string.
        index: usize,
    },
    /// A mesh has no vertices or no triangles.
    #[error("mesh {index} is empty")]
    EmptyMesh {
        /// Position of the mesh.
        index: usize,
    },
    /// A mesh has more vertices or indices than its limit.
    #[error("mesh {index} announces {count} vertices or indices, the limit is {max}")]
    MeshTooLarge {
        /// Position of the mesh.
        index: usize,
        /// The announced count.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// A coordinate of a mesh (origin, vertex or bounds) is not finite or lies beyond
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("mesh {index} has a coordinate out of range")]
    MeshCoordinate {
        /// Position of the mesh.
        index: usize,
    },
    /// A normal is not of unit length.
    #[error("mesh {index} has a normal that is not a unit vector (vertex {vertex})")]
    InvalidNormal {
        /// Position of the mesh.
        index: usize,
        /// Position of the vertex.
        vertex: usize,
    },
    /// The index count is not a multiple of 3.
    #[error("mesh {index} has an index count that is not a multiple of 3")]
    IndexCountNotMultipleOfThree {
        /// Position of the mesh.
        index: usize,
    },
    /// An index points past the vertices.
    #[error("mesh {index} has an index out of range (position {position})")]
    IndexOutOfRange {
        /// Position of the mesh.
        index: usize,
        /// Position of the index in the mesh's index list.
        position: usize,
    },
    /// The bounding box does not enclose every vertex.
    #[error("mesh {index} has a bounding box that does not enclose its vertices")]
    BoundsDoNotEnclose {
        /// Position of the mesh.
        index: usize,
    },
    /// Two added meshes have the same id.
    #[error("two added meshes share an id (the second is at {index})")]
    DuplicateMesh {
        /// Position of the second mesh.
        index: usize,
    },
    /// The translation of an element is not finite or lies beyond
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("element {index} has a translation out of range")]
    ElementTranslation {
        /// Position of the element.
        index: usize,
    },
    /// The rotation of an element is not finite, not orthonormal or a reflection.
    #[error("element {index} has a rotation that is not a proper orthonormal one")]
    InvalidRotation {
        /// Position of the element.
        index: usize,
    },
    /// The category or discipline of an element is not a string of the delta.
    #[error("element {index} names a string that does not exist")]
    StringIndexOutOfRange {
        /// Position of the element.
        index: usize,
    },
    /// Two added elements have the same id.
    #[error("two added elements share an id (the second is at {index})")]
    DuplicateElement {
        /// Position of the second element.
        index: usize,
    },
}
