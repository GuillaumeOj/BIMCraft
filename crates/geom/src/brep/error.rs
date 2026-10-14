//! Why a boundary representation is rejected.

/// Why a set of faces does not bound a valid solid.
///
/// Variants name the face, vertex or shell at fault by index, never by coordinates: coordinates
/// are model content and must stay out of logs and crash reports (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum BrepError {
    /// No face was given.
    #[error("the solid has no faces")]
    Empty,
    /// A ring refers to a vertex index that does not exist.
    #[error("face {face} refers to a vertex that does not exist")]
    IndexOutOfRange {
        /// Index of the face.
        face: usize,
    },
    /// A ring has fewer than three vertices or visits a vertex twice.
    #[error("face {face} has a ring with fewer than three distinct vertices")]
    DegenerateRing {
        /// Index of the face.
        face: usize,
    },
    /// The links of a ring do not form one closed cycle.
    #[error("face {face} has a ring whose links do not close")]
    BrokenRing {
        /// Index of the face.
        face: usize,
    },
    /// Two rings run along the same edge in the same direction: a face is flipped, or more than
    /// two faces meet at the edge.
    #[error("an edge of face {face} is used twice in the same direction")]
    NonManifoldEdge {
        /// Index of the face.
        face: usize,
    },
    /// An edge has no neighbouring face: the boundary is not closed.
    #[error("an edge of face {face} has no neighbouring face")]
    OpenEdge {
        /// Index of the face.
        face: usize,
    },
    /// The two sides of an edge do not refer to each other.
    #[error("an edge of face {face} does not match its neighbour")]
    InconsistentTwin {
        /// Index of the face.
        face: usize,
    },
    /// An edge is shorter than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE).
    #[error("face {face} has an edge shorter than the linear tolerance")]
    ShortEdge {
        /// Index of the face.
        face: usize,
    },
    /// A coordinate is NaN or infinite.
    #[error("vertex {vertex} has a coordinate that is not a finite number")]
    NonFinite {
        /// Index of the vertex.
        vertex: usize,
    },
    /// A coordinate is outside [`MODEL_EXTENT`](crate::MODEL_EXTENT), or so close to zero
    /// (below [`MIN_COORDINATE`](crate::MIN_COORDINATE)) that exact predicates cannot handle it.
    #[error("vertex {vertex} is outside the model extent")]
    OutsideModelExtent {
        /// Index of the vertex.
        vertex: usize,
    },
    /// A vertex belongs to no face.
    #[error("vertex {vertex} belongs to no face")]
    IsolatedVertex {
        /// Index of the vertex.
        vertex: usize,
    },
    /// The faces around a vertex form more than one fan, as when two solids touch at a point.
    #[error("vertex {vertex} joins faces that do not form a single fan")]
    NonManifoldVertex {
        /// Index of the vertex.
        vertex: usize,
    },
    /// A face is narrower than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE).
    #[error("face {face} is narrower than the linear tolerance")]
    DegenerateFace {
        /// Index of the face.
        face: usize,
    },
    /// A vertex of a face lies farther than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE) from
    /// the face plane.
    #[error("face {face} is not planar within the linear tolerance")]
    NonPlanarFace {
        /// Index of the face.
        face: usize,
    },
    /// A hole turns the same way as its face's outer ring.
    #[error("face {face} has a hole that turns the same way as its outer boundary")]
    MisorientedHole {
        /// Index of the face.
        face: usize,
    },
    /// The Euler–Poincaré formula gives no valid genus for a shell.
    #[error("shell {shell} breaks the Euler–Poincaré formula")]
    EulerViolation {
        /// Index of the shell.
        shell: usize,
    },
    /// A shell encloses a volume thinner than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE) on
    /// average.
    #[error("shell {shell} encloses no volume above the linear tolerance")]
    ThinShell {
        /// Index of the shell.
        shell: usize,
    },
    /// The solid encloses no positive volume: its faces point inwards, or its voids are larger
    /// than its outside.
    #[error("the solid encloses no positive volume: its faces point inwards")]
    WrongOrientation,
}
