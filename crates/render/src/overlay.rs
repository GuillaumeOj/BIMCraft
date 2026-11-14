//! Transient geometry drawn over the model: the overlay of previews and snap markers (VP-01,
//! spec D5).
//!
//! An [`Overlay`] is a set of lines, snap markers, translucent boxes and ghosts of existing
//! objects, which the host rebuilds when a preview changes and hands to
//! [`Renderer::set_overlay`](crate::Renderer::set_overlay). Nothing in it is model state. The
//! renderer draws it after the opaque pass, in this order:
//!
//! 1. ghosts and translucent boxes, depth tested against the model and blended, so a model in
//!    front hides them;
//! 2. lines and markers, without depth test, so they stay visible behind the model. Each sits on
//!    a halo of the theme's halo colour (2 px around a line or a marker, and inside a marker),
//!    so it keeps its contrast over any model colour.
//!
//! Like the objects, everything is positioned in `f64` world coordinates and made camera-relative
//! on the CPU before it is rounded to `f32` (`docs/dev/adr/0020-camera-relative-rendering.md`);
//! a marker 10⁷ m from the origin sits on the pixel it should. A ghost is a draw of the mesh the
//! renderer already holds, with one more instance record: moving a preview uploads no mesh.

use crate::theme::{BOX_ALPHA, Theme};
use crate::transform::{Matrix, ViewFrame, camera_relative_model_view, to_gpu};
use crate::vector::{add, apply, dot, mul, scale, sub};
use crate::world::{Bucket, DrawList, INSTANCE_FLOATS, MeshId, ObjectId, World};
use crate::{DetailLevel, Placement, PlacementError, Projection};

/// The most primitives of each kind (lines, markers, boxes, ghosts) an overlay holds. A preview
/// is a handful of them; the bound keeps a faulty host from growing the buffers without limit.
pub const MAX_OVERLAY_ITEMS: usize = 65_536;

/// The widest line, in pixels.
pub const MAX_LINE_WIDTH: f32 = 64.0;

/// The thickness of the halo around lines and markers, on each side, in pixels.
const HALO_WIDTH: f32 = 2.0;

/// The stroke of a marker, in pixels (board 3.28: 3 px).
const MARKER_STROKE: f32 = 3.0;

/// The half size of a marker, in pixels: a square marker is 14 px wide.
const MARKER_HALF_SIZE: f32 = 7.0;

/// Floats of a primitive record: two camera-space points with the shape and size in their fourth
/// components, a colour, and the halo width, layer and stroke.
pub(crate) const PRIMITIVE_FLOATS: usize = 16;

/// Bytes of a primitive record.
pub(crate) const PRIMITIVE_STRIDE: u64 = PRIMITIVE_FLOATS as u64 * 4;

/// The `vec4` attributes of a primitive record.
pub(crate) const PRIMITIVE_ATTRIBUTES: usize = PRIMITIVE_FLOATS / 4;

/// Which colour of the [`Theme`] draws a primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tone {
    /// The snap colour: markers, and the lines that show where a snap leads.
    Snap,
    /// The preview colour: rubber bands, outlines of what will be placed.
    Preview,
    /// The selection colour: the section box.
    Selection,
    /// The hover colour: the face picked for a clipping plane.
    Hover,
    /// The cut colour: clipping planes.
    Cut,
}

/// What a snap marker says about the point: each kind has its own shape, so the colour is never
/// the only difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapKind {
    /// A vertex of a mesh: a square.
    Endpoint,
    /// The middle of an edge: a triangle.
    Midpoint,
    /// The nearest point of an edge: a short line.
    Edge,
    /// A point on a face: a diamond.
    Face,
    /// A point of the ground plane: a plus.
    Ground,
}

impl SnapKind {
    /// The shape code of the shader.
    fn shape(self) -> f32 {
        match self {
            Self::Endpoint => 1.0,
            Self::Midpoint => 2.0,
            Self::Edge => 3.0,
            Self::Face => 4.0,
            Self::Ground => 5.0,
        }
    }
}

/// Why something cannot be added to an [`Overlay`]. Messages never contain coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OverlayError {
    /// A coordinate is not finite or lies beyond [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("a point of the overlay is not finite or lies too far from the world origin")]
    PointOutOfRange,
    /// The width of a line is not between a fraction of a pixel and [`MAX_LINE_WIDTH`].
    #[error("the width of a line is not between 0.5 and {MAX_LINE_WIDTH} pixels")]
    InvalidWidth,
    /// The minimum of a box is above its maximum on an axis.
    #[error("the minimum of a box is above its maximum")]
    InvalidBox,
    /// The overlay already holds [`MAX_OVERLAY_ITEMS`] items of this kind.
    #[error("the overlay holds as many items of this kind as it can")]
    TooMany,
    /// The extra transform of a ghost is invalid.
    #[error(transparent)]
    Placement(#[from] PlacementError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Line {
    from: [f64; 3],
    to: [f64; 3],
    width: f32,
    tone: Tone,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Marker {
    at: [f64; 3],
    kind: SnapKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TranslucentBox {
    min: [f64; 3],
    max: [f64; 3],
    tone: Tone,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ghost {
    object: ObjectId,
    extra: Placement,
}

/// The temporary geometry of one frame: previews, markers and ghosts. See the
/// [module](self) for how it is drawn.
///
/// Clearing an overlay keeps its allocations, so the host can rebuild it each frame without
/// allocating.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    lines: Vec<Line>,
    markers: Vec<Marker>,
    boxes: Vec<TranslucentBox>,
    ghosts: Vec<Ghost>,
}

impl Overlay {
    /// An empty overlay.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes everything, keeping the allocations.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.markers.clear();
        self.boxes.clear();
        self.ghosts.clear();
    }

    /// Whether the overlay holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.markers.is_empty()
            && self.boxes.is_empty()
            && self.ghosts.is_empty()
    }

    /// Adds a line segment `width` pixels wide, on top of the model.
    ///
    /// # Errors
    ///
    /// [`OverlayError::PointOutOfRange`], [`OverlayError::InvalidWidth`] or
    /// [`OverlayError::TooMany`].
    pub fn line(
        &mut self,
        from: [f64; 3],
        to: [f64; 3],
        width: f32,
        tone: Tone,
    ) -> Result<(), OverlayError> {
        check_point(from)?;
        check_point(to)?;
        if !(0.5..=MAX_LINE_WIDTH).contains(&width) {
            return Err(OverlayError::InvalidWidth);
        }
        check_room(self.lines.len())?;
        self.lines.push(Line {
            from,
            to,
            width,
            tone,
        });
        Ok(())
    }

    /// Adds the segments between consecutive points. Nothing is added when any point is invalid
    /// or the polyline does not fit.
    ///
    /// # Errors
    ///
    /// As [`Overlay::line`].
    pub fn polyline(
        &mut self,
        points: &[[f64; 3]],
        width: f32,
        tone: Tone,
    ) -> Result<(), OverlayError> {
        let start = self.lines.len();
        let result = points
            .windows(2)
            .try_for_each(|pair| self.line(pair[0], pair[1], width, tone));
        if result.is_err() {
            self.lines.truncate(start);
        }
        result
    }

    /// Adds a snap marker at `at`, on top of the model, in the snap colour.
    ///
    /// # Errors
    ///
    /// [`OverlayError::PointOutOfRange`] or [`OverlayError::TooMany`].
    pub fn marker(&mut self, at: [f64; 3], kind: SnapKind) -> Result<(), OverlayError> {
        check_point(at)?;
        check_room(self.markers.len())?;
        self.markers.push(Marker { at, kind });
        Ok(())
    }

    /// Adds a translucent axis-aligned box from `min` to `max`, depth tested: the block being
    /// placed, later the section box.
    ///
    /// # Errors
    ///
    /// [`OverlayError::PointOutOfRange`], [`OverlayError::InvalidBox`] or
    /// [`OverlayError::TooMany`].
    pub fn translucent_box(
        &mut self,
        min: [f64; 3],
        max: [f64; 3],
        tone: Tone,
    ) -> Result<(), OverlayError> {
        check_point(min)?;
        check_point(max)?;
        if min.iter().zip(&max).any(|(low, high)| low > high) {
            return Err(OverlayError::InvalidBox);
        }
        check_room(self.boxes.len())?;
        self.boxes.push(TranslucentBox { min, max, tone });
        Ok(())
    }

    /// Adds a ghost of `object`: its mesh drawn translucent in the preview colour, depth tested,
    /// after its placement is followed by `extra` (a rotation and a translation in the world,
    /// the move preview). The object itself is not drawn differently; give it the
    /// [`DisplayState::Ghost`](crate::DisplayState::Ghost) state to turn it into the ghost left at
    /// the origin.
    ///
    /// Whether the object exists is checked by [`Renderer::set_overlay`](crate::Renderer::set_overlay).
    ///
    /// # Errors
    ///
    /// [`OverlayError::Placement`] or [`OverlayError::TooMany`].
    pub fn ghost(&mut self, object: ObjectId, extra: Placement) -> Result<(), OverlayError> {
        extra.validate()?;
        check_room(self.ghosts.len())?;
        self.ghosts.push(Ghost { object, extra });
        Ok(())
    }

    /// The records the overlay needs on the GPU: lines and markers (a halo and an ink each), and
    /// instances (its ghosts and boxes).
    pub(crate) fn record_counts(&self) -> (u64, u64) {
        let primitives = 2 * (self.lines.len() + self.markers.len());
        let instances = self.ghosts.len() + self.boxes.len();
        (primitives as u64, instances as u64)
    }

    /// The objects the ghosts show.
    pub(crate) fn ghost_objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.ghosts.iter().map(|ghost| ghost.object)
    }
}

fn check_point(point: [f64; 3]) -> Result<(), OverlayError> {
    if point.iter().all(|&value| crate::in_world(value)) {
        Ok(())
    } else {
        Err(OverlayError::PointOutOfRange)
    }
}

fn check_room(len: usize) -> Result<(), OverlayError> {
    if len < MAX_OVERLAY_ITEMS {
        Ok(())
    } else {
        Err(OverlayError::TooMany)
    }
}

/// The part of a prepared frame that is the overlay's own: the lines and markers. Kept between
/// frames to reuse its allocations. Ghosts and boxes go to the [`DrawList`], since they are
/// instances of meshes.
#[derive(Debug, Default)]
pub(crate) struct OverlayFrame {
    /// [`PRIMITIVE_FLOATS`] values per record: all the halos first, then all the inks, so a
    /// line never covers the halo of another with its own.
    pub(crate) primitives: Vec<f32>,
    /// The ghosts of the frame, sorted by mesh so each mesh is one draw call.
    ghosts: Vec<(MeshId, [f32; INSTANCE_FLOATS])>,
}

impl OverlayFrame {
    /// How many primitive records the frame draws.
    pub(crate) fn primitive_count(&self) -> u32 {
        // The overlay bounds its items far below u32::MAX / 2.
        u32::try_from(self.primitives.len() / PRIMITIVE_FLOATS).unwrap_or(u32::MAX)
    }

    pub(crate) fn primitive_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.primitives)
    }

    /// Prepares `overlay` for a frame seen through `view`, whose lines are kept within `clip`:
    /// writes the primitives here, and the ghosts and the boxes into the draw list after the
    /// ghost objects the world wrote.
    pub(crate) fn prepare(
        &mut self,
        overlay: &Overlay,
        theme: &Theme,
        view: &ViewFrame,
        clip: &ClipVolume,
        world: &World,
        list: &mut DrawList,
    ) {
        self.primitives.clear();
        for layer in [Layer::Halo, Layer::Ink] {
            for line in &overlay.lines {
                self.push_line(line, layer, theme, view, clip);
            }
            for marker in &overlay.markers {
                self.push_marker(marker, layer, theme, view, clip.near);
            }
        }
        self.prepare_ghosts(overlay, theme, view, world, list);
        prepare_boxes(overlay, theme, view, list);
    }

    fn push_line(
        &mut self,
        line: &Line,
        layer: Layer,
        theme: &Theme,
        view: &ViewFrame,
        clip: &ClipVolume,
    ) {
        let (from, to) = (to_camera(view, line.from), to_camera(view, line.to));
        let Some((from, to)) = clip.segment(from, to) else {
            return;
        };
        self.push_primitive(&Primitive {
            from,
            to,
            shape: 0.0,
            size: 0.0,
            stroke: line.width,
            color: theme_color(theme, layer, line.tone),
            layer,
        });
    }

    fn push_marker(
        &mut self,
        marker: &Marker,
        layer: Layer,
        theme: &Theme,
        view: &ViewFrame,
        near: f64,
    ) {
        let at = to_camera(view, marker.at);
        if at[2] > -near {
            return;
        }
        self.push_primitive(&Primitive {
            from: at,
            to: at,
            shape: marker.kind.shape(),
            size: MARKER_HALF_SIZE,
            stroke: MARKER_STROKE,
            color: theme_color(theme, layer, Tone::Snap),
            layer,
        });
    }

    fn push_primitive(&mut self, primitive: &Primitive) {
        let [from, to] = [primitive.from, primitive.to].map(|p| p.map(to_f32));
        let [r, g, b] = primitive.color;
        self.primitives.extend_from_slice(&[
            from[0],
            from[1],
            from[2],
            primitive.shape,
            to[0],
            to[1],
            to[2],
            primitive.size,
            r,
            g,
            b,
            1.0,
            match primitive.layer {
                Layer::Halo => HALO_WIDTH,
                Layer::Ink => 0.0,
            },
            primitive.layer.code(),
            primitive.stroke,
            0.0,
        ]);
    }

    /// Appends the instance of each ghost whose object still exists, grouped by mesh.
    fn prepare_ghosts(
        &mut self,
        overlay: &Overlay,
        theme: &Theme,
        view: &ViewFrame,
        world: &World,
        list: &mut DrawList,
    ) {
        self.ghosts.clear();
        let color = {
            let [r, g, b] = theme.tints().preview;
            [r, g, b, crate::theme::GHOST_ALPHA]
        };
        for ghost in &overlay.ghosts {
            let Some((mesh, origin, placement)) = world.ghost_source(ghost.object) else {
                continue;
            };
            let moved = follow(&placement, &ghost.extra);
            let model_view = camera_relative_model_view(view, origin, &moved);
            let mut record = [0.0; INSTANCE_FLOATS];
            record[..16].copy_from_slice(model_view.as_flattened());
            record[16..].copy_from_slice(&color);
            self.ghosts.push((mesh, record));
        }
        // Stable, so ghosts of one mesh keep the order they were added in.
        self.ghosts.sort_by_key(|&(mesh, _)| mesh);
        let mut run: Option<(MeshId, u32, u32)> = None;
        for &(mesh, ref record) in &self.ghosts {
            // The overlay bounds the ghosts, so instance indices fit u32.
            let index = u32::try_from(list.ghosts.len() / INSTANCE_FLOATS).unwrap_or(u32::MAX);
            list.ghosts.extend_from_slice(record);
            match &mut run {
                Some((current, _, count)) if *current == mesh => *count += 1,
                _ => {
                    flush(list, run.take());
                    run = Some((mesh, index, 1));
                }
            }
        }
        flush(list, run);
    }
}

fn flush(list: &mut DrawList, run: Option<(MeshId, u32, u32)>) {
    if let Some((mesh, first, count)) = run {
        list.ghost_buckets.push(Bucket {
            mesh,
            level: DetailLevel::Fine,
            first,
            count,
        });
    }
}

/// Appends the instance of each box: the unit cube `[-1, 1]³` scaled to its half extents, in
/// `f64`, then made camera-relative.
fn prepare_boxes(overlay: &Overlay, theme: &Theme, view: &ViewFrame, list: &mut DrawList) {
    let first = u32::try_from(list.ghosts.len() / INSTANCE_FLOATS).unwrap_or(u32::MAX);
    for item in &overlay.boxes {
        let centre = scale(add(item.min, item.max), 0.5);
        let half = scale(sub(item.max, item.min), 0.5);
        // Column `c` of the rotation is scaled by the half extent along axis `c`.
        let rotation = core::array::from_fn(|row| {
            core::array::from_fn(|column| view.rotation[row][column] * half[column])
        });
        let model_view: Matrix = to_gpu(&rotation, apply(&view.rotation, sub(centre, view.eye)));
        let [r, g, b] = tone_color(theme, item.tone);
        list.ghosts.extend_from_slice(model_view.as_flattened());
        list.ghosts.extend_from_slice(&[r, g, b, BOX_ALPHA]);
    }
    let count = u32::try_from(overlay.boxes.len()).unwrap_or(u32::MAX);
    list.boxes = (first, count);
}

/// What a record draws on top of the halos or under the ink.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Halo,
    Ink,
}

impl Layer {
    fn code(self) -> f32 {
        match self {
            Self::Halo => 0.0,
            Self::Ink => 1.0,
        }
    }
}

struct Primitive {
    from: [f64; 3],
    to: [f64; 3],
    shape: f32,
    size: f32,
    stroke: f32,
    color: [f32; 3],
    layer: Layer,
}

fn theme_color(theme: &Theme, layer: Layer, tone: Tone) -> [f32; 3] {
    match layer {
        Layer::Halo => theme.halo.linear(),
        Layer::Ink => tone_color(theme, tone),
    }
}

fn tone_color(theme: &Theme, tone: Tone) -> [f32; 3] {
    match tone {
        Tone::Snap => theme.snap,
        Tone::Preview => theme.preview,
        Tone::Selection => theme.selection,
        Tone::Hover => theme.hover,
        Tone::Cut => theme.cut,
    }
    .linear()
}

/// The placement that applies `base`, then `extra`: `extra · base`.
fn follow(base: &Placement, extra: &Placement) -> Placement {
    Placement {
        rotation: mul(&extra.rotation, &base.rotation),
        translation: add(apply(&extra.rotation, base.translation), extra.translation),
    }
}

/// The point as the camera sees it, in `f64`: the large world terms cancel before anything is
/// rounded. The camera looks along −z.
fn to_camera(view: &ViewFrame, point: [f64; 3]) -> [f64; 3] {
    apply(&view.rotation, sub(point, view.eye))
}

/// How much wider and taller than the view the region lines are clipped to is: the shader still
/// draws the parts of a line near the edges of the view exactly, while no point it receives lies
/// much beyond them.
const GUARD_BAND: f64 = 4.0;

/// The region of camera space a line of the overlay is clipped to, in `f64`, before anything is
/// rounded to `f32`: in front of the near plane and within the view enlarged by [`GUARD_BAND`].
/// A rubber band from a point near the eye to one 10⁷ m away thus reaches the GPU as a segment of
/// a few times the view, which `f32` holds exactly enough (ADR 0020).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipVolume {
    /// Distance of the near plane.
    pub(crate) near: f64,
    /// Planes `n · q + d ≥ 0` in camera coordinates, inside when every one holds.
    planes: [([f64; 3], f64); 5],
}

impl ClipVolume {
    /// The volume of a validated projection on a target of `aspect` (width over height).
    pub(crate) fn new(projection: &Projection, aspect: f64) -> Self {
        let near = match *projection {
            Projection::Perspective { near, .. } | Projection::Orthographic { near, .. } => near,
        };
        let near_plane = ([0.0, 0.0, -1.0], -near);
        let planes = match *projection {
            Projection::Perspective { fov_y_radians, .. } => {
                let tan_y = GUARD_BAND * libm::tan(fov_y_radians / 2.0);
                let tan_x = tan_y * aspect;
                [
                    ([1.0, 0.0, -tan_x], 0.0),
                    ([-1.0, 0.0, -tan_x], 0.0),
                    ([0.0, 1.0, -tan_y], 0.0),
                    ([0.0, -1.0, -tan_y], 0.0),
                    near_plane,
                ]
            }
            Projection::Orthographic { height, .. } => {
                let half_y = GUARD_BAND * height / 2.0;
                let half_x = half_y * aspect;
                [
                    ([1.0, 0.0, 0.0], half_x),
                    ([-1.0, 0.0, 0.0], half_x),
                    ([0.0, 1.0, 0.0], half_y),
                    ([0.0, -1.0, 0.0], half_y),
                    near_plane,
                ]
            }
        };
        Self { near, planes }
    }

    /// The part of the segment inside the volume (Liang–Barsky), or `None` when none is.
    pub(crate) fn segment(&self, from: [f64; 3], to: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
        let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
        for &(normal, offset) in &self.planes {
            let start = dot(normal, from) + offset;
            let end = dot(normal, to) + offset;
            if start < 0.0 && end < 0.0 {
                return None;
            }
            if start < 0.0 {
                enter = enter.max(start / (start - end));
            } else if end < 0.0 {
                leave = leave.min(start / (start - end));
            }
        }
        if enter > leave {
            return None;
        }
        let along = |t: f64| add(from, scale(sub(to, from), t));
        let first = if enter > 0.0 { along(enter) } else { from };
        let last = if leave < 1.0 { along(leave) } else { to };
        Some((first, last))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "camera-space distances, not world coordinates: the GPU computes in f32"
)]
fn to_f32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the coordinates and colours compared are exact: pure channels and small integers"
)]
mod tests {
    use super::*;
    use crate::{Camera, Projection};
    use geom::{Point3, Vector3};

    const FAR: f64 = crate::MAX_WORLD_COORDINATE;

    fn view() -> ViewFrame {
        ViewFrame::new(&Camera {
            eye: Point3::new(0.0, -5.0, 0.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: Projection::Perspective {
                fov_y_radians: 1.0,
                near: 0.1,
            },
        })
        .unwrap()
    }

    fn an_object(world: &mut World) -> ObjectId {
        let bounds = crate::bounds::Aabb::of_local([0.0; 3], &[[0.0; 3], [1.0; 3]]).unwrap();
        let mesh = world.add_mesh([0.0; 3], bounds, false);
        world
            .add_object(mesh, Placement::IDENTITY, [1.0; 4])
            .unwrap()
    }

    #[test]
    fn items_are_validated() {
        let mut overlay = Overlay::new();
        assert!(overlay.is_empty());
        let (o, x) = ([0.0; 3], [1.0, 0.0, 0.0]);
        assert_eq!(overlay.line(o, x, 3.0, Tone::Preview), Ok(()));
        for bad in [
            [f64::NAN, 0.0, 0.0],
            [FAR * 2.0, 0.0, 0.0],
            [0.0, f64::INFINITY, 0.0],
        ] {
            assert_eq!(
                overlay.line(o, bad, 3.0, Tone::Snap),
                Err(OverlayError::PointOutOfRange)
            );
            assert_eq!(
                overlay.line(bad, o, 3.0, Tone::Snap),
                Err(OverlayError::PointOutOfRange)
            );
            assert_eq!(
                overlay.marker(bad, SnapKind::Face),
                Err(OverlayError::PointOutOfRange)
            );
            assert_eq!(
                overlay.translucent_box(bad, bad, Tone::Cut),
                Err(OverlayError::PointOutOfRange)
            );
        }
        for width in [0.0, 0.4, MAX_LINE_WIDTH + 1.0, f32::NAN, -1.0] {
            assert_eq!(
                overlay.line(o, x, width, Tone::Snap),
                Err(OverlayError::InvalidWidth),
                "{width}"
            );
        }
        assert_eq!(
            overlay.translucent_box(x, o, Tone::Cut),
            Err(OverlayError::InvalidBox)
        );
        assert_eq!(overlay.translucent_box(o, o, Tone::Cut), Ok(()));
        assert!(!overlay.is_empty());
        overlay.clear();
        assert!(overlay.is_empty());
    }

    #[test]
    fn ghosts_need_a_valid_extra_transform() {
        let mut overlay = Overlay::new();
        let object = an_object(&mut World::new(8));
        let mut bad = Placement::IDENTITY;
        bad.rotation[0][0] = 2.0;
        assert_eq!(
            overlay.ghost(object, bad),
            Err(OverlayError::Placement(PlacementError::NotOrthonormal))
        );
        assert_eq!(overlay.ghost(object, Placement::IDENTITY), Ok(()));
        assert_eq!(overlay.ghost_objects().collect::<Vec<_>>(), [object]);
    }

    #[test]
    fn the_overlay_is_bounded() {
        let mut overlay = Overlay::new();
        for _ in 0..MAX_OVERLAY_ITEMS {
            overlay.marker([0.0; 3], SnapKind::Edge).unwrap();
        }
        assert_eq!(
            overlay.marker([0.0; 3], SnapKind::Edge),
            Err(OverlayError::TooMany)
        );
        // Another kind has its own room.
        assert_eq!(overlay.line([0.0; 3], [1.0; 3], 1.0, Tone::Snap), Ok(()));
    }

    #[test]
    fn a_polyline_is_all_or_nothing() {
        let mut overlay = Overlay::new();
        let points = [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert_eq!(overlay.polyline(&points, 2.0, Tone::Preview), Ok(()));
        assert_eq!(overlay.lines.len(), 2);
        let bad = [[0.0; 3], [1.0, 0.0, 0.0], [FAR * 2.0, 0.0, 0.0]];
        assert_eq!(
            overlay.polyline(&bad, 2.0, Tone::Preview),
            Err(OverlayError::PointOutOfRange)
        );
        assert_eq!(overlay.lines.len(), 2);
        assert_eq!(overlay.polyline(&points[..1], 2.0, Tone::Preview), Ok(()));
        assert_eq!(overlay.lines.len(), 2);
    }

    /// The clip volume of [`view`]'s camera, on a square target.
    fn clip() -> ClipVolume {
        ClipVolume::new(
            &Projection::Perspective {
                fov_y_radians: 1.0,
                near: 0.1,
            },
            1.0,
        )
    }

    #[test]
    fn segments_are_cut_at_the_near_plane() {
        let clip = clip();
        let near = clip.near;
        let front = [0.0, 0.0, -2.0];
        let behind = [0.0, 0.0, 2.0];
        assert_eq!(clip.segment(front, front), Some((front, front)));
        assert_eq!(clip.segment(behind, behind), None);
        // Cut in the middle of the near plane, whichever end is behind.
        let (kept, cut) = clip.segment(front, behind).unwrap();
        assert_eq!(kept, front);
        assert!((cut[2] + near).abs() < 1e-12);
        let (cut, kept) = clip.segment(behind, front).unwrap();
        assert_eq!(kept, front);
        assert!((cut[2] + near).abs() < 1e-12);
        // The cut keeps the line of the segment.
        let (_, cut) = clip.segment([0.0, 0.0, -1.0], [0.2, 0.4, 1.0]).unwrap();
        assert!(
            (cut[0] - 0.09).abs() < 1e-12 && (cut[1] - 0.18).abs() < 1e-12,
            "{cut:?}"
        );
    }

    #[test]
    fn segments_are_cut_at_the_guard_band() {
        let clip = clip();
        let band = GUARD_BAND * libm::tan(0.5);
        // Far beyond the right of the view: cut where it leaves the band.
        let (kept, cut) = clip.segment([0.0, 0.0, -1.0], [1e6, 0.0, -1.0]).unwrap();
        assert_eq!(kept, [0.0, 0.0, -1.0]);
        assert!((cut[0] - band).abs() < 1e-9, "{cut:?}");
        // Entirely beside the band, on either axis, or entirely behind the eye: nothing.
        assert_eq!(clip.segment([10.0, 0.0, -1.0], [20.0, 5.0, -1.0]), None);
        assert_eq!(clip.segment([0.0, -10.0, -1.0], [0.0, -20.0, -2.0]), None);
        // Crossing the whole view: both ends cut.
        let (first, last) = clip.segment([-1e3, 0.0, -1.0], [1e3, 0.0, -1.0]).unwrap();
        assert!((first[0] + band).abs() < 1e-9 && (last[0] - band).abs() < 1e-9);
        // A parallel projection has a band of fixed width.
        let parallel = ClipVolume::new(
            &Projection::Orthographic {
                height: 2.0,
                near: 0.1,
                far: 100.0,
            },
            2.0,
        );
        let (_, cut) = parallel
            .segment([0.0, 0.0, -1.0], [0.0, 1e6, -1.0])
            .unwrap();
        assert!((cut[1] - GUARD_BAND).abs() < 1e-9, "{cut:?}");
        let (_, cut) = parallel
            .segment([0.0, 0.0, -1.0], [1e6, 0.0, -1.0])
            .unwrap();
        assert!((cut[0] - 2.0 * GUARD_BAND).abs() < 1e-9, "{cut:?}");
    }

    /// A rubber band from just in front of the eye to a point 10⁷ m away, crossing the near
    /// plane, reaches the GPU as the same small segment whether the scene sits at the origin or
    /// 10⁷ m from it.
    #[test]
    fn a_rubber_band_far_from_the_origin_is_clipped_like_one_at_the_origin() {
        let records = |offset: f64| {
            let camera = Camera {
                eye: Point3::new(offset, offset - 5.0, offset),
                target: Point3::new(offset, offset, offset),
                up: Vector3::new(0.0, 0.0, 1.0),
                projection: Projection::Perspective {
                    fov_y_radians: 1.0,
                    near: 0.1,
                },
            };
            let mut overlay = Overlay::new();
            overlay
                .line(
                    [offset + 0.5, offset - 5.5, offset],
                    [offset + 1e7, offset + 1e7, offset - 1e6],
                    3.0,
                    Tone::Preview,
                )
                .unwrap();
            let (mut frame, mut list) = (OverlayFrame::default(), DrawList::default());
            let view = ViewFrame::new(&camera).unwrap();
            let clip = ClipVolume::new(&camera.projection, 1.5);
            frame.prepare(
                &overlay,
                &Theme::light(),
                &view,
                &clip,
                &World::new(8),
                &mut list,
            );
            frame.primitives
        };
        let near = records(0.0);
        let far = records(1e7);
        assert_eq!(near.len(), 2 * PRIMITIVE_FLOATS);
        for (a, b) in near.iter().zip(&far) {
            assert!(a.is_finite() && (a - b).abs() <= 1e-3, "{a} {b}");
        }
        // Both ends lie in front of the near plane and within the guard band.
        let tan_y = GUARD_BAND * libm::tan(0.5);
        let (band_x, band_y) = (tan_y * 1.5 + 1e-6, tan_y + 1e-6);
        let ink = &near[PRIMITIVE_FLOATS..];
        for point in
            [&ink[0..3], &ink[4..7]].map(|p| p.iter().map(|&v| f64::from(v)).collect::<Vec<_>>())
        {
            assert!(point[2] <= -0.1 + 1e-6, "{point:?}");
            assert!(point[0].abs() <= band_x * -point[2], "{point:?}");
            assert!(point[1].abs() <= band_y * -point[2], "{point:?}");
        }
    }

    #[test]
    fn a_marker_behind_the_camera_is_not_drawn() {
        let mut overlay = Overlay::new();
        overlay
            .marker([0.0, -10.0, 0.0], SnapKind::Endpoint)
            .unwrap();
        overlay
            .line([0.0, -10.0, 0.0], [0.0, -9.0, 0.0], 3.0, Tone::Snap)
            .unwrap();
        let (mut frame, mut list) = (OverlayFrame::default(), DrawList::default());
        let world = World::new(8);
        frame.prepare(
            &overlay,
            &Theme::light(),
            &view(),
            &clip(),
            &world,
            &mut list,
        );
        assert_eq!(frame.primitive_count(), 0);
        overlay.marker([0.0, 0.0, 0.0], SnapKind::Endpoint).unwrap();
        frame.prepare(
            &overlay,
            &Theme::light(),
            &view(),
            &clip(),
            &world,
            &mut list,
        );
        // A halo and an ink record.
        assert_eq!(frame.primitive_count(), 2);
        assert_eq!(frame.primitive_bytes().len() as u64, 2 * PRIMITIVE_STRIDE);
    }

    #[test]
    fn halos_come_before_inks_in_the_theme_colours() {
        let theme = Theme::dark();
        let mut overlay = Overlay::new();
        overlay
            .line([0.0; 3], [1.0, 0.0, 0.0], 3.0, Tone::Preview)
            .unwrap();
        overlay.marker([0.0; 3], SnapKind::Face).unwrap();
        let (mut frame, mut list) = (OverlayFrame::default(), DrawList::default());
        frame.prepare(
            &overlay,
            &theme,
            &view(),
            &clip(),
            &World::new(8),
            &mut list,
        );
        let records: Vec<_> = frame.primitives.chunks(PRIMITIVE_FLOATS).collect();
        assert_eq!(records.len(), 4);
        let rgb = |record: &[f32]| [record[8], record[9], record[10]];
        // Layers: halo line, halo marker, ink line, ink marker.
        assert_eq!(
            records.iter().map(|r| r[13]).collect::<Vec<_>>(),
            [0.0, 0.0, 1.0, 1.0]
        );
        assert_eq!(rgb(records[0]), theme.halo.linear());
        assert_eq!(rgb(records[1]), theme.halo.linear());
        assert_eq!(rgb(records[2]), theme.preview.linear());
        assert_eq!(rgb(records[3]), theme.snap.linear());
        // Shapes, widths and the halo's extra width.
        assert_eq!((records[2][3], records[2][14]), (0.0, 3.0));
        assert_eq!(
            (records[3][3], records[3][7], records[3][14]),
            (4.0, MARKER_HALF_SIZE, MARKER_STROKE)
        );
        assert_eq!((records[0][12], records[2][12]), (HALO_WIDTH, 0.0));
    }

    #[test]
    fn every_snap_kind_has_its_own_shape() {
        let kinds = [
            SnapKind::Endpoint,
            SnapKind::Midpoint,
            SnapKind::Edge,
            SnapKind::Face,
            SnapKind::Ground,
        ];
        let mut shapes: Vec<f32> = kinds.iter().map(|kind| kind.shape()).collect();
        shapes.sort_by(f32::total_cmp);
        shapes.dedup();
        assert_eq!(shapes, [1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn a_transform_follows_another() {
        let base = Placement {
            rotation: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [1.0, 0.0, 0.0],
        };
        let extra = Placement::from_translation([0.0, 5.0, 0.0]);
        let moved = follow(&base, &extra);
        assert_eq!(moved.rotation, base.rotation);
        assert_eq!(moved.translation, [1.0, 5.0, 0.0]);
        // A turn applies to the translation of the base too: x → y.
        let turn = Placement {
            rotation: base.rotation,
            translation: [0.0; 3],
        };
        assert_eq!(follow(&base, &turn).translation, [0.0, 1.0, 0.0]);
        assert_eq!(moved.validate(), Ok(()));
    }

    #[test]
    fn a_box_is_the_unit_cube_scaled_and_moved_to_camera_space() {
        let mut overlay = Overlay::new();
        overlay
            .translucent_box([-1.0, -1.0, -2.0], [3.0, 1.0, 2.0], Tone::Selection)
            .unwrap();
        let mut list = DrawList::default();
        let mut frame = OverlayFrame::default();
        frame.prepare(
            &overlay,
            &Theme::light(),
            &view(),
            &clip(),
            &World::new(8),
            &mut list,
        );
        assert_eq!(list.boxes, (0, 1));
        let record = &list.ghosts[..INSTANCE_FLOATS];
        // The camera looks along +y from y = −5: world x is camera x, world z is camera y, and
        // the centre (1, 0, 0) is 5 m in front, along −z.
        assert_eq!(&record[..3], &[2.0, 0.0, 0.0]);
        assert_eq!(&record[4..7], &[0.0, 0.0, -1.0]);
        assert_eq!(&record[12..15], &[1.0, 0.0, -5.0]);
        assert_eq!(record[19], BOX_ALPHA);
    }
}
