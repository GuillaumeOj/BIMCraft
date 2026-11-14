//! What the viewport tells its host, and what the host asks of it (`api-contract.md` §2): plain
//! Rust types, which the browser glue maps to and from JavaScript values.

use render::SnapKind;

/// The id of an element: an `EntityId`, 16 bytes little-endian, as the scene channel sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(pub [u8; 16]);

/// The tool the pointer drives in the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Hover, click to select, drag a selected element to move it.
    #[default]
    Select,
    /// Base point, then target point: moves the selection.
    Move,
    /// Corner, corner, height: places a block.
    PlaceBlock,
}

/// Where a tool stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStep {
    /// Nothing in progress.
    Idle,
    /// The move waits for its base point.
    BasePoint,
    /// The move waits for its target point.
    TargetPoint,
    /// The block waits for its first corner.
    FirstCorner,
    /// The block waits for the opposite corner.
    SecondCorner,
    /// The block waits for its height.
    Height,
}

/// A preset direction of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Looking down, north up.
    Top,
    /// Looking north.
    Front,
    /// Looking south.
    Back,
    /// Looking east, from the west.
    Left,
    /// Looking west, from the east.
    Right,
    /// From the south-east, above.
    IsoSe,
    /// From the south-west, above.
    IsoSw,
    /// From the north-east, above.
    IsoNe,
    /// From the north-west, above.
    IsoNw,
}

impl Preset {
    /// The name of the preset in the contract.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Front => "front",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::IsoSe => "iso_se",
            Self::IsoSw => "iso_sw",
            Self::IsoNe => "iso_ne",
            Self::IsoNw => "iso_nw",
        }
    }
}

/// How the view is projected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectionMode {
    /// Perspective, objects shrink with the distance.
    #[default]
    Perspective,
    /// Parallel (orthographic).
    Parallel,
}

impl ProjectionMode {
    /// The name of the projection in the contract.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Perspective => "perspective",
            Self::Parallel => "parallel",
        }
    }
}

/// A view command, from buttons, menus and shortcuts outside the canvas. The section box,
/// clipping planes and visibility commands join with VP-03.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewCommand {
    /// Frames every element.
    ZoomFit,
    /// Frames the selection; nothing without one.
    ZoomSelection,
    /// Looks from a preset direction, then frames every element.
    ViewPreset(Preset),
    /// Switches the projection.
    Projection(ProjectionMode),
    /// Selects these elements; unknown ids are ignored.
    Select(Vec<ElementId>),
    /// Clears the selection.
    ClearSelection,
    /// Esc: cancels the tool step, else clears the selection.
    Cancel,
    /// A typed distance, in metres, confirmed at once, during a move or a placement.
    TypedDistance(f64),
    /// Turns snapping on or off.
    Snapping(bool),
}

/// The key of an announcement for the live region; the UI translates it (UI-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnouncementKey {
    /// The snap went to a vertex.
    SnapEndpoint,
    /// The snap went to the middle of an edge.
    SnapMidpoint,
    /// The snap went to an edge.
    SnapEdge,
    /// The snap went to a face.
    SnapFace,
    /// The snap went to the ground plane.
    SnapGround,
    /// `count` elements are selected.
    SelectionCount,
    /// Nothing is selected any more.
    SelectionCleared,
    /// A move was sent: `distance`.
    MoveDistance,
    /// A move was cancelled.
    MoveCancelled,
    /// The footprint of a block was set: `width`, `depth`.
    PlaceSize,
    /// A block was sent: `height`.
    PlaceHeight,
    /// A preset view: `name`.
    ViewPreset,
    /// A projection: `projection`.
    ViewProjection,
}

impl AnnouncementKey {
    /// The key in the contract.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SnapEndpoint => "snap.endpoint",
            Self::SnapMidpoint => "snap.midpoint",
            Self::SnapEdge => "snap.edge",
            Self::SnapFace => "snap.face",
            Self::SnapGround => "snap.ground",
            Self::SelectionCount => "selection.count",
            Self::SelectionCleared => "selection.cleared",
            Self::MoveDistance => "move.distance",
            Self::MoveCancelled => "move.cancelled",
            Self::PlaceSize => "place.size",
            Self::PlaceHeight => "place.height",
            Self::ViewPreset => "view.preset",
            Self::ViewProjection => "view.projection",
        }
    }

    /// The announcement of a snap of `kind`.
    #[must_use]
    pub const fn of_snap(kind: SnapKind) -> Self {
        match kind {
            SnapKind::Endpoint => Self::SnapEndpoint,
            SnapKind::Midpoint => Self::SnapMidpoint,
            SnapKind::Edge => Self::SnapEdge,
            SnapKind::Face => Self::SnapFace,
            _ => Self::SnapGround,
        }
    }
}

/// A value of an announcement or a label: raw metres or counts, or a name; the UI formats them
/// for the locale.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A number.
    Number(f64),
    /// A name, never user text.
    Text(&'static str),
}

/// Something for the live region.
#[derive(Clone, Debug, PartialEq)]
pub struct Announcement {
    /// What to say.
    pub key: AnnouncementKey,
    /// The values the sentence names.
    pub values: Vec<(&'static str, Value)>,
}

/// The key of the label drawn next to a preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelKey {
    /// The distance of a move: `distance`.
    MoveDistance,
    /// The footprint of a block: `width`, `depth`.
    PlaceSize,
    /// The height of a block: `height`.
    PlaceHeight,
}

impl LabelKey {
    /// The key in the contract.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MoveDistance => "move.distance",
            Self::PlaceSize => "place.size",
            Self::PlaceHeight => "place.height",
        }
    }
}

/// The text chip next to a preview: the renderer draws no text, so the UI does.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewLabel {
    /// Where, in CSS pixels from the top left of the canvas.
    pub at: [f64; 2],
    /// What it says.
    pub key: LabelKey,
    /// The values it names.
    pub values: Vec<(&'static str, Value)>,
    /// The value being typed, if any.
    pub typed: Option<String>,
}

/// A finished action for the core, sent by the UI as a command (UI-01).
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// `elements.move`.
    Move {
        /// The elements, in the order they were selected.
        ids: Vec<ElementId>,
        /// The translation, in metres.
        translation: [f64; 3],
    },
    /// `model.create` of a `Block`.
    PlaceBlock {
        /// The centre of its base, in metres.
        location: [f64; 3],
        /// About z, in radians.
        rotation: f64,
        /// Width, depth and height, in metres.
        size: [f64; 3],
    },
}

/// The view state mirrored to the UI's panels. Visibility, the section box and the clipping
/// planes join with VP-03.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewState {
    /// The projection.
    pub projection: ProjectionMode,
    /// Whether snapping applies now: off after the `snapping` command or while Alt is held.
    pub snapping: bool,
    /// The tool.
    pub tool: Tool,
}

/// Why the view stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKey {
    /// The renderer could not hold the scene: the viewport dropped it and waits for a full one.
    SceneRejected,
}

/// What the viewport tells its host.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewportEvent {
    /// The selection changed.
    Selection(Vec<ElementId>),
    /// The hovered element changed.
    Hover(Option<ElementId>),
    /// The tool or its step changed.
    ToolState(Tool, ToolStep),
    /// Something for the live region.
    Announce(Announcement),
    /// A finished action.
    Intent(Intent),
    /// The view state changed.
    ViewState(ViewState),
    /// The preview label changed; `None` hides it.
    PreviewLabel(Option<PreviewLabel>),
    /// The view stopped.
    Error(ErrorKey),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_contract() {
        assert_eq!(Preset::IsoNw.name(), "iso_nw");
        assert_eq!(ProjectionMode::Parallel.name(), "parallel");
        assert_eq!(
            AnnouncementKey::SelectionCleared.as_str(),
            "selection.cleared"
        );
        assert_eq!(LabelKey::PlaceSize.as_str(), "place.size");
        let snaps = [
            SnapKind::Endpoint,
            SnapKind::Midpoint,
            SnapKind::Edge,
            SnapKind::Face,
            SnapKind::Ground,
        ]
        .map(|kind| AnnouncementKey::of_snap(kind).as_str());
        assert_eq!(
            snaps,
            [
                "snap.endpoint",
                "snap.midpoint",
                "snap.edge",
                "snap.face",
                "snap.ground"
            ]
        );
    }
}
