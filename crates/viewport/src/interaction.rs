//! The viewport: input in, renderer calls and events out. Tools are state machines driven
//! event by event (`design/ux-notes.md` flows 1–4).

use std::collections::{BTreeMap, BTreeSet};

use render::{Camera, DisplayState, SnapKind, Theme, Tone};
use scene_wire::SceneDelta;

use crate::ElementId;
use crate::announce::Announcer;
use crate::cache::{SceneCache, SceneError, Sphere};
use crate::camera::CameraController;
use crate::clipping::{Face, SectionBox};
use crate::event::{
    Announcement, AnnouncementKey, ErrorKey, Intent, LabelKey, PreviewLabel, Tool, ToolStep, Value,
    ViewCommand, ViewState, ViewportEvent,
};
use crate::input::{Button, InputEvent, Key, Modifiers};
use crate::math::{Vec3, add, distance_2d, dot, norm, normalize, scale, sub};
use crate::pick::{Hit, pick};
use crate::sink::{Preview, PreviewLine, SceneSink, SinkError};
use crate::snap::{self, DEFAULT_TOLERANCE, Query, Snap};
use crate::typed::Typed;
use crate::view::{Ray, View};

mod sectioning;
use sectioning::{Gate, Sectioning};

/// How far the pointer moves, in CSS pixels, before a press becomes a drag.
const DRAG_THRESHOLD: f64 = 4.0;

/// The orbit of a pointer drag, in radians per CSS pixel.
const ORBIT_RATE: f64 = 0.005;

/// The orbit of an arrow key: 15°.
const ORBIT_STEP: f64 = core::f64::consts::PI / 12.0;

/// The pan of Shift+arrow, as a share of the view's height.
const PAN_STEP: f64 = 0.1;

/// The zoom of `+` and `-`.
const ZOOM_STEP: f64 = 1.25;

/// The zoom of one CSS pixel of wheel scroll, as a power of e.
const WHEEL_RATE: f64 = 0.0015;

/// The smallest footprint side and height of a block, in metres.
const MIN_BLOCK_SIDE: f64 = 1e-3;

/// The height of a block when Enter is pressed with nothing typed (flow 3).
const DEFAULT_BLOCK_HEIGHT: f64 = 3.0;

/// The longest typed distance, in metres: the bound of `elements.move`.
const MAX_TYPED_DISTANCE: f64 = 1e6;

/// The sides and height a block may have, in metres.
const BLOCK_SIDES: core::ops::RangeInclusive<f64> = MIN_BLOCK_SIDE..=MAX_TYPED_DISTANCE;

/// Frames a move's ghosts wait for the scene to show the moved elements, then vanish (the move
/// may have been refused).
pub(crate) const PENDING_FRAMES: u32 = 120;

/// What [`Viewport::tick`] tells the frame loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tick {
    /// Something changed: draw a frame.
    pub draw: bool,
    /// Call `tick` again next frame even if nothing else happens: a preview waits for the
    /// scene, or an announcement for its turn.
    pub again: bool,
}

/// Where the tool stands.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Select,
    /// The Move tool without a selection.
    MoveIdle,
    MoveBase,
    /// `direct` when dragging a selected element with the Select tool.
    MoveTarget {
        base: Vec3,
        direct: bool,
    },
    FirstCorner,
    SecondCorner {
        first: Vec3,
        width: Option<f64>,
    },
    Height {
        first: Vec3,
        second: Vec3,
        /// Whether the width was typed, so Esc goes back to the depth step and keeps it.
        width_typed: bool,
    },
    /// A clipping plane waits for the face the user clicks.
    PickFace,
}

/// What the pointer is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    None,
    /// A button is down and has not moved beyond the drag threshold.
    Pressed {
        start: [f64; 2],
        mods: Modifiers,
    },
    Orbit {
        last: [f64; 2],
        pivot: Vec3,
    },
    Pan {
        last: [f64; 2],
        depth: f64,
    },
    /// A direct move of the selection.
    Drag,
    /// A face handle of the section box, `start` being the box before the drag.
    Handle {
        face: Face,
        start: SectionBox,
    },
}

/// A move sent to the core, whose ghosts stay until the scene shows it.
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    ids: Vec<ElementId>,
    translation: Vec3,
    frames: u32,
}

/// The interaction of the 3D viewport: it holds the scene, the camera, the selection and the
/// tools; takes pointer, wheel and keyboard input and view commands; drives a [`SceneSink`]
/// (the renderer); and reports [`ViewportEvent`]s. Nothing here touches a platform, so all of
/// it is tested natively.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags: Alt held, snapping on, ghosts refused, a frame due"
)]
pub struct Viewport<S: SceneSink> {
    sink: S,
    cache: SceneCache,
    camera: CameraController,
    /// The target in device pixels, and device pixels per CSS pixel.
    size: [u32; 2],
    ratio: f64,
    tool: Tool,
    step: Step,
    selection: Vec<ElementId>,
    hover: Option<ElementId>,
    gesture: Gesture,
    /// The pointer, in device pixels, while over the canvas.
    cursor: Option<[f64; 2]>,
    snap: Option<Snap>,
    alt: bool,
    snapping: bool,
    typed: Typed,
    pending: Option<Pending>,
    /// The device refused the ghosts of this move: the preview goes on without them.
    ghosts_dropped: bool,
    states: BTreeMap<ElementId, DisplayState>,
    preview: Preview,
    label: Option<PreviewLabel>,
    reported: (Tool, ToolStep, ViewState),
    announcer: Announcer,
    /// The section box, the clipping planes and the visibility.
    sect: Sectioning,
    events: Vec<ViewportEvent>,
    dirty: bool,
}

impl<S: SceneSink> Viewport<S> {
    /// A viewport drawing into `sink`, on a canvas of `size` CSS pixels at `ratio` device pixels
    /// per CSS pixel.
    pub fn new(sink: S, size: [f64; 2], ratio: f64) -> Self {
        let mut viewport = Self {
            sink,
            cache: SceneCache::default(),
            camera: CameraController::default(),
            size: [1, 1],
            ratio: 1.0,
            tool: Tool::Select,
            step: Step::Select,
            selection: Vec::new(),
            hover: None,
            gesture: Gesture::None,
            cursor: None,
            snap: None,
            alt: false,
            snapping: true,
            typed: Typed::default(),
            pending: None,
            ghosts_dropped: false,
            states: BTreeMap::new(),
            preview: Preview::default(),
            label: None,
            reported: (Tool::Select, ToolStep::Idle, ViewState::default()),
            announcer: Announcer::default(),
            sect: Sectioning::default(),
            events: Vec::new(),
            dirty: true,
        };
        viewport.reported.2 = viewport.view_state();
        viewport.resize(size, ratio);
        viewport
    }

    /// The renderer, to draw a frame.
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// The renderer.
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// The camera to draw the next frame with.
    #[must_use]
    pub fn camera(&self) -> Camera {
        self.camera.camera(self.scene())
    }

    /// The target, in device pixels.
    #[must_use]
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The selected elements, in the order they were selected.
    #[must_use]
    pub fn selection(&self) -> &[ElementId] {
        &self.selection
    }

    /// The revision of the scene held, or `None` before the first delta (and after a refusal
    /// that dropped the scene): the `since` of the next `scene` request.
    #[must_use]
    pub fn revision(&self) -> Option<u64> {
        self.cache.revision()
    }

    /// The events since the last call.
    pub fn drain_events(&mut self) -> Vec<ViewportEvent> {
        core::mem::take(&mut self.events)
    }

    /// Applies the bytes of one scene delta; returns the revision now held.
    ///
    /// # Errors
    ///
    /// [`SceneError`]: a delta that does not decode or does not fit what is held changes
    /// nothing; one the renderer refuses drops the scene, and an
    /// [`ErrorKey::SceneRejected`] event follows.
    pub fn apply_scene(&mut self, bytes: &[u8]) -> Result<u64, SceneError> {
        let delta = scene_wire::decode(bytes)?;
        self.apply_delta(&delta)
    }

    /// Applies one decoded scene delta; see [`Viewport::apply_scene`].
    ///
    /// # Errors
    ///
    /// As [`Viewport::apply_scene`].
    pub fn apply_delta(&mut self, delta: &SceneDelta) -> Result<u64, SceneError> {
        let result = self.cache.apply(delta, &mut self.sink);
        let mut recreated = BTreeSet::new();
        match &result {
            Ok(applied) => {
                recreated.clone_from(&applied.recreated);
                if self.pending.as_ref().is_some_and(|pending| {
                    pending.ids.iter().any(|id| applied.touched.contains(id))
                }) {
                    self.pending = None;
                }
                // The states of the elements the delta re-created are gone with their objects;
                // a moved object keeps its state, which the next sync may still have to reset.
                for id in &applied.recreated {
                    self.states.remove(id);
                }
            }
            Err(SceneError::Render(_)) => {
                self.events
                    .push(ViewportEvent::Error(ErrorKey::SceneRejected));
                self.states.clear();
                self.pending = None;
                // The overlay may still show lines and markers: clear it in the renderer too.
                drop(self.sink.set_preview(&Preview::default()));
                self.preview = Preview::default();
            }
            Err(_) => return result.map(|_| delta.revision),
        }
        let before = self.selection.len();
        self.selection.retain(|&id| self.cache.contains(id));
        if self.selection.len() != before {
            self.selection_changed();
        }
        // Whatever the delta added or recreated is hidden or shown by the rules at once.
        self.visibility_changed(&recreated);
        if self.hover.is_some_and(|id| !self.cache.contains(id)) {
            self.hover = None;
            self.events.push(ViewportEvent::Hover(None));
        }
        self.refresh();
        self.dirty = true;
        result.map(|_| delta.revision)
    }

    /// The canvas is now `size` CSS pixels at `ratio` device pixels per CSS pixel.
    pub fn resize(&mut self, size: [f64; 2], ratio: f64) {
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        };
        self.ratio = ratio;
        self.size = size.map(|side| device_pixels(side * ratio));
        self.dirty = true;
    }

    /// Changes the colours.
    pub fn set_theme(&mut self, theme: Theme) {
        self.sink.set_theme(theme);
        self.dirty = true;
    }

    /// Selects the tool the pointer drives; the current step is dropped.
    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.typed.clear();
        self.gesture = Gesture::None;
        self.step = self.first_step();
        self.refresh_pointer();
        self.refresh();
    }

    /// Runs a view command.
    pub fn command(&mut self, command: ViewCommand) {
        match command {
            ViewCommand::ZoomFit => self.zoom_fit(None),
            ViewCommand::ZoomSelection => {
                if !self.selection.is_empty() {
                    let ids = self.selection.clone();
                    self.zoom_fit(Some(&ids));
                }
            }
            ViewCommand::ViewPreset(preset) => {
                self.camera.preset(preset, self.scene());
                self.zoom_fit(None);
                self.say(
                    AnnouncementKey::ViewPreset,
                    vec![("name", Value::Text(preset.name()))],
                );
            }
            ViewCommand::Projection(mode) => {
                self.camera.set_mode(mode, self.scene());
                self.dirty = true;
                self.say(
                    AnnouncementKey::ViewProjection,
                    vec![("projection", Value::Text(mode.name()))],
                );
            }
            ViewCommand::Select(ids) => {
                let mut selection = Vec::new();
                for id in ids {
                    if self.cache.contains(id) && !selection.contains(&id) {
                        selection.push(id);
                    }
                }
                self.set_selection(selection);
            }
            ViewCommand::ClearSelection => self.set_selection(Vec::new()),
            ViewCommand::Cancel => self.cancel(),
            ViewCommand::TypedDistance(metres) => self.confirm(Some(metres)),
            ViewCommand::Snapping(enabled) => {
                self.snapping = enabled;
                self.refresh_pointer();
            }
            other => self.sectioning_command(other),
        }
        self.refresh();
    }

    /// Handles one input event.
    pub fn input(&mut self, event: InputEvent) {
        match event {
            InputEvent::PointerDown { pos, button, mods } => {
                self.set_alt(mods.alt);
                self.sect.shift = mods.shift;
                self.pointer_down(self.device(pos), button, mods);
            }
            InputEvent::PointerMove { pos, mods } => {
                self.set_alt(mods.alt);
                self.sect.shift = mods.shift;
                self.pointer_move(self.device(pos));
            }
            InputEvent::PointerUp { pos, button, mods } => {
                self.set_alt(mods.alt);
                self.pointer_up(self.device(pos), button, mods);
            }
            InputEvent::PointerLeave => {
                // The glue also sends it when a second finger lands or the browser cancels the
                // pointer: the gesture ends without committing anything.
                self.abandon_gesture();
                self.cursor = None;
                self.snap = None;
                self.set_hover(None);
            }
            InputEvent::Wheel {
                pos,
                delta,
                pan,
                mods,
            } => {
                self.set_alt(mods.alt);
                let pos = self.device(pos);
                if pan {
                    let depth = self.camera.distance();
                    let size = self.view().pixel_size(depth) * self.ratio;
                    self.camera
                        .pan(delta[0] * size, -delta[1] * size, self.scene());
                } else {
                    self.zoom_at(pos, libm::exp(delta[1] * WHEEL_RATE));
                }
                self.dirty = true;
            }
            InputEvent::Pinch { pos, scale } => {
                if scale.is_finite() && scale > 0.0 {
                    self.zoom_at(self.device(pos), 1.0 / scale);
                    self.dirty = true;
                }
            }
            InputEvent::KeyDown { key, mods } => {
                self.set_alt(mods.alt || key == Key::Alt);
                self.key(key, mods);
            }
            InputEvent::KeyUp { key, mods } => self.set_alt(mods.alt && key != Key::Alt),
            InputEvent::Blur => {
                self.set_alt(false);
                self.abandon_gesture();
            }
        }
        self.refresh();
    }

    /// Ends the gesture in progress (a press, an orbit, a pan or a drag) without committing it: a drag cut short (the focus left, a
    /// second finger landed) is cancelled, not left following the pointer with no button held.
    fn abandon_gesture(&mut self) {
        if self.gesture == Gesture::Drag && matches!(self.step, Step::MoveTarget { .. }) {
            self.cancel();
        }
        if let Gesture::Handle { start, .. } = self.gesture {
            self.restore_handle(start);
        }
        self.gesture = Gesture::None;
    }

    /// One frame passed: sends due announcements and ages a waiting preview.
    pub fn tick(&mut self) -> Tick {
        let said = self.announcer.tick();
        self.announce(said);
        if let Some(pending) = &mut self.pending {
            pending.frames = pending.frames.saturating_sub(1);
            if pending.frames == 0 {
                self.pending = None;
                self.refresh();
            }
        }
        Tick {
            draw: core::mem::take(&mut self.dirty),
            again: self.pending.is_some() || self.announcer.is_waiting(),
        }
    }

    fn device(&self, pos: [f64; 2]) -> [f64; 2] {
        pos.map(|v| v * self.ratio)
    }

    fn scene(&self) -> Option<Sphere> {
        self.cache.sphere(None)
    }

    fn view(&self) -> View {
        View::new(&self.camera(), self.size)
    }

    fn aspect(&self) -> f64 {
        f64::from(self.size[0]) / f64::from(self.size[1])
    }

    fn first_step(&self) -> Step {
        match self.tool {
            Tool::Select => Step::Select,
            Tool::Move if self.selection.is_empty() => Step::MoveIdle,
            Tool::Move => Step::MoveBase,
            Tool::PlaceBlock => Step::FirstCorner,
        }
    }

    fn say(&mut self, key: AnnouncementKey, values: Vec<(&'static str, Value)>) {
        let said = self.announcer.say(Announcement { key, values });
        self.announce(said);
    }

    fn announce(&mut self, announcement: Option<Announcement>) {
        self.events
            .extend(announcement.map(ViewportEvent::Announce));
    }

    fn set_alt(&mut self, alt: bool) {
        if alt != self.alt {
            self.alt = alt;
            self.refresh_pointer();
        }
    }

    // Selection.

    fn set_selection(&mut self, selection: Vec<ElementId>) {
        if selection != self.selection {
            self.selection = selection;
            self.selection_changed();
        }
    }

    fn selection_changed(&mut self) {
        self.events
            .push(ViewportEvent::Selection(self.selection.clone()));
        if self.selection.is_empty() {
            self.say(AnnouncementKey::SelectionCleared, Vec::new());
        } else {
            let count = crate::math::count(self.selection.len());
            self.say(
                AnnouncementKey::SelectionCount,
                vec![("count", Value::Number(count))],
            );
        }
        match (self.step, self.selection.is_empty()) {
            (Step::MoveIdle, false) => self.step = Step::MoveBase,
            (Step::MoveBase | Step::MoveTarget { .. }, true) => self.step = self.first_step(),
            _ => {}
        }
    }

    fn click_select(&mut self, hit: Option<ElementId>, mods: Modifiers) {
        let mut selection = self.selection.clone();
        match hit {
            Some(id) if mods.toggle() => {
                if let Some(at) = selection.iter().position(|&s| s == id) {
                    selection.remove(at);
                } else {
                    selection.push(id);
                }
            }
            Some(id) if mods.shift => {
                if !selection.contains(&id) {
                    selection.push(id);
                }
            }
            Some(id) => selection = vec![id],
            None if mods.shift || mods.toggle() => {}
            None => selection.clear(),
        }
        self.set_selection(selection);
    }

    fn set_hover(&mut self, hover: Option<ElementId>) {
        if hover != self.hover {
            self.hover = hover;
            self.events.push(ViewportEvent::Hover(hover));
        }
    }

    // Pointer.

    /// What is under `pos`, among what is shown and not clipped away.
    fn pick_hit(&mut self, pos: [f64; 2]) -> Option<Hit> {
        let camera = self.camera();
        let gate = Gate::new(&self.sect);
        pick(&self.cache, &mut self.sink, &camera, self.size, pos, &gate)
    }

    fn pick_at(&mut self, pos: [f64; 2]) -> Option<(ElementId, Vec3, f64)> {
        let hit = self.pick_hit(pos)?;
        let view = self.view();
        Some((
            hit.element,
            add(view.eye, hit.relative),
            view.depth(hit.relative),
        ))
    }

    fn snap_at(&mut self, pos: [f64; 2]) -> Option<Snap> {
        let camera = self.camera();
        let moving: &[ElementId] = match self.step {
            Step::MoveTarget { .. } => &self.selection,
            _ => &[],
        };
        let gate = Gate::new(&self.sect);
        let query = Query {
            cache: &self.cache,
            camera: &camera,
            size: self.size,
            pos,
            tolerance: DEFAULT_TOLERANCE * self.ratio,
            enabled: self.snapping && !self.alt,
            skip: &|id| moving.contains(&id),
            pickable: &gate,
        };
        snap::snap(&query, &mut self.sink)
    }

    fn pointer_down(&mut self, pos: [f64; 2], button: Button, mods: Modifiers) {
        self.cursor = Some(pos);
        self.gesture = match button {
            Button::Secondary => {
                let pivot = self
                    .pick_at(pos)
                    .map(|(_, point, _)| point)
                    .or_else(|| self.scene().map(|s| s.centre))
                    .unwrap_or_else(|| self.camera.target());
                Gesture::Orbit { last: pos, pivot }
            }
            Button::Middle => Gesture::Pan {
                last: pos,
                depth: self.pan_depth(pos),
            },
            Button::Primary => match self.handle_at(pos).zip(self.sect.clipping.section) {
                Some((face, start)) if self.step == Step::Select => Gesture::Handle { face, start },
                _ => Gesture::Pressed { start: pos, mods },
            },
        };
    }

    fn pan_depth(&mut self, pos: [f64; 2]) -> f64 {
        self.pick_at(pos)
            .map_or(self.camera.distance(), |(_, _, depth)| depth)
    }

    fn pointer_move(&mut self, pos: [f64; 2]) {
        self.cursor = Some(pos);
        match self.gesture {
            Gesture::Orbit { last, pivot } => {
                let [dx, dy] = [
                    (pos[0] - last[0]) / self.ratio,
                    (pos[1] - last[1]) / self.ratio,
                ];
                self.camera
                    .orbit(-dx * ORBIT_RATE, -dy * ORBIT_RATE, pivot, self.scene());
                self.gesture = Gesture::Orbit { last: pos, pivot };
                self.dirty = true;
            }
            Gesture::Pan { last, depth } => {
                let size = self.view().pixel_size(depth);
                self.camera.pan(
                    -(pos[0] - last[0]) * size,
                    (pos[1] - last[1]) * size,
                    self.scene(),
                );
                self.gesture = Gesture::Pan { last: pos, depth };
                self.dirty = true;
            }
            Gesture::Pressed { start, mods, .. }
                if distance_2d(pos, start) > DRAG_THRESHOLD * self.ratio =>
            {
                self.start_drag(start, pos, mods);
            }
            Gesture::Handle { face, .. } => {
                self.drag_handle(face, pos);
                self.dirty = true;
            }
            Gesture::Pressed { .. } | Gesture::None | Gesture::Drag => self.refresh_pointer(),
        }
    }

    /// A primary press became a drag: Shift pans; on an element with the Select tool, it moves
    /// the selection; otherwise nothing.
    fn start_drag(&mut self, start: [f64; 2], pos: [f64; 2], mods: Modifiers) {
        if mods.shift {
            let depth = self.pan_depth(start);
            self.gesture = Gesture::Pan { last: start, depth };
            self.pointer_move(pos);
            return;
        }
        if self.step != Step::Select {
            self.gesture = Gesture::None;
            return;
        }
        let Some((id, point, _)) = self.pick_at(start) else {
            self.gesture = Gesture::None;
            return;
        };
        if !self.selection.contains(&id) {
            self.set_selection(vec![id]);
        }
        let base = self.snap_at(start).map_or(point, |snap| snap.point);
        self.set_hover(None);
        self.begin_move(base, true);
        self.gesture = Gesture::Drag;
        self.refresh_pointer();
    }

    fn pointer_up(&mut self, pos: [f64; 2], button: Button, mods: Modifiers) {
        self.cursor = Some(pos);
        let gesture = core::mem::replace(&mut self.gesture, Gesture::None);
        match gesture {
            Gesture::Pressed { .. } if button == Button::Primary => self.click(pos, mods),
            Gesture::Drag => {
                self.refresh_pointer();
                self.commit_move(self.move_target());
            }
            Gesture::Handle { face, .. } => {
                self.drop_handle(face);
                self.refresh_pointer();
            }
            _ => {}
        }
    }

    fn click(&mut self, pos: [f64; 2], mods: Modifiers) {
        self.refresh_pointer();
        match self.step {
            Step::Select => {
                let hit = self.pick_at(pos).map(|(id, _, _)| id);
                self.click_select(hit, mods);
            }
            Step::PickFace => self.pick_face_click(),
            Step::MoveIdle => {}
            Step::MoveBase => {
                if let Some(snap) = self.snap {
                    self.begin_move(snap.point, false);
                }
            }
            Step::MoveTarget { .. } => {
                if let Some(target) = self.move_target() {
                    self.commit_move(Some(target));
                }
            }
            Step::FirstCorner => {
                if let Some(snap) = self.snap {
                    self.step = Step::SecondCorner {
                        first: snap.point,
                        width: None,
                    };
                    self.typed.clear();
                }
            }
            Step::SecondCorner { first, width } => {
                if let Some(second) = self.second_corner() {
                    // A width being typed when the corner is clicked counts as typed.
                    let width_typed = width.is_some() || self.typed.value().is_some();
                    self.confirm_footprint(first, second, width_typed);
                }
            }
            Step::Height { first, second, .. } => {
                if let Some(height) = self.pointer_height(first, second) {
                    self.commit_block(first, second, height);
                }
            }
        }
    }

    /// Recomputes what follows the pointer: the hover with the Select tool, the snap otherwise.
    fn refresh_pointer(&mut self) {
        let Some(pos) = self.cursor else {
            return;
        };
        if self.step == Step::PickFace {
            self.snap = None;
            self.set_hover(None);
            self.sect.face_hit = self.face_under(pos);
        } else if self.step == Step::Select {
            self.snap = None;
            if matches!(self.gesture, Gesture::None | Gesture::Pressed { .. }) {
                // A face handle of the section box takes the pointer from the elements behind.
                self.sect.handle = self.handle_at(pos);
                let hover = if self.sect.handle.is_some() {
                    None
                } else {
                    self.pick_at(pos).map(|(id, _, _)| id)
                };
                self.set_hover(hover);
            }
        } else {
            self.set_hover(None);
            self.snap = self.snap_at(pos);
            let said = self.announcer.snap(self.snap.and_then(|snap| snap.kind));
            self.announce(said);
        }
    }

    fn zoom_at(&mut self, pos: [f64; 2], factor: f64) {
        let anchor = self
            .pick_at(pos)
            .map_or_else(|| self.on_target_plane(pos), |(_, point, _)| point);
        self.camera.zoom(factor, anchor, self.scene());
    }

    /// The point under `pos` at the depth of the target (the parallel view stands further back
    /// than the distance).
    fn on_target_plane(&self, pos: [f64; 2]) -> Vec3 {
        let view = self.view();
        let ray = view.ray(pos);
        let forward = self.camera.forward();
        let along = dot(ray.direction, forward);
        let depth = view.depth(sub(self.camera.target(), view.eye));
        let t = (depth - dot(ray.origin, forward)) / along;
        add(view.eye, ray.at(if t.is_finite() { t } else { 0.0 }))
    }

    fn zoom_fit(&mut self, ids: Option<&[ElementId]>) {
        if let Some(sphere) = self.cache.sphere(ids) {
            self.camera.fit(sphere, self.aspect());
            self.dirty = true;
        }
    }

    // Keyboard.

    fn key(&mut self, key: Key, mods: Modifiers) {
        let scene = self.scene();
        let pivot = scene.map_or_else(|| self.camera.target(), |s| s.centre);
        let pan =
            self.view().pixel_size(self.camera.distance()) * f64::from(self.size[1]) * PAN_STEP;
        match (key, mods.shift) {
            (Key::ArrowLeft, false) => self.camera.orbit(ORBIT_STEP, 0.0, pivot, scene),
            (Key::ArrowRight, false) => self.camera.orbit(-ORBIT_STEP, 0.0, pivot, scene),
            (Key::ArrowUp, false) => self.camera.orbit(0.0, -ORBIT_STEP, pivot, scene),
            (Key::ArrowDown, false) => self.camera.orbit(0.0, ORBIT_STEP, pivot, scene),
            (Key::ArrowLeft, true) => self.camera.pan(-pan, 0.0, scene),
            (Key::ArrowRight, true) => self.camera.pan(pan, 0.0, scene),
            (Key::ArrowUp, true) => self.camera.pan(0.0, pan, scene),
            (Key::ArrowDown, true) => self.camera.pan(0.0, -pan, scene),
            (Key::Char('+' | '='), _) => self.zoom_centre(1.0 / ZOOM_STEP),
            (Key::Char('-' | '_'), _) => self.zoom_centre(ZOOM_STEP),
            (Key::Char('f' | 'F'), true) => self.command(ViewCommand::ZoomSelection),
            (Key::Char('f' | 'F'), false) => self.zoom_fit(None),
            (Key::Char(c), _) => {
                if self.takes_values() && self.typed.push(c) {
                    self.refresh_pointer();
                }
                return;
            }
            (Key::Backspace, _) => {
                if self.typed.pop() {
                    self.refresh_pointer();
                }
                return;
            }
            (Key::Enter, _) => {
                let value = self.typed.value();
                if self.typed.text().is_none() || value.is_some() {
                    self.confirm(value);
                }
                return;
            }
            (Key::Escape, _) => {
                self.cancel();
                return;
            }
            (Key::Alt | Key::Other, _) => return,
        }
        self.dirty = true;
    }

    fn zoom_centre(&mut self, factor: f64) {
        let target = self.camera.target();
        self.camera.zoom(factor, target, self.scene());
    }

    fn takes_values(&self) -> bool {
        matches!(
            self.step,
            Step::MoveTarget { .. } | Step::SecondCorner { .. } | Step::Height { .. }
        )
    }

    /// Enter with the typed value (`None`: nothing typed), or a typed distance.
    fn confirm(&mut self, value: Option<f64>) {
        let given = value.is_some();
        let value = value.filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_TYPED_DISTANCE);
        match self.step {
            Step::MoveTarget { base, .. } => {
                let direction = self.snap.and_then(|snap| normalize(sub(snap.point, base)));
                if let (Some(v), Some(direction)) = (value, direction) {
                    self.commit_move(Some(add(base, scale(direction, v))));
                }
            }
            Step::SecondCorner { first, width } => {
                let Some(v) = value else { return };
                let pointer = self.on_plane(first).unwrap_or(first);
                let sign = |delta: f64| if delta < 0.0 { -1.0 } else { 1.0 };
                match width {
                    None => {
                        self.step = Step::SecondCorner {
                            first,
                            width: Some(sign(pointer[0] - first[0]) * v),
                        };
                        self.typed.clear();
                    }
                    Some(width) => {
                        let second = [
                            first[0] + width,
                            first[1] + sign(pointer[1] - first[1]) * v,
                            first[2],
                        ];
                        self.confirm_footprint(first, second, true);
                    }
                }
            }
            Step::Height { first, second, .. } => {
                // The default only when nothing was given: a refused value places nothing.
                let height = if given {
                    value
                } else {
                    Some(DEFAULT_BLOCK_HEIGHT)
                };
                if let Some(height) = height {
                    self.commit_block(first, second, height);
                }
            }
            _ => {}
        }
    }

    /// Esc: one tool step back, else the waiting preview, else the selection.
    fn cancel(&mut self) {
        // While a tool takes values, Esc first empties what is being typed, then goes back one
        // step: a move keeps its base point.
        if self.takes_values() && self.typed.text().is_some() {
            self.typed.clear();
            self.refresh_pointer();
            return;
        }
        self.typed.clear();
        match self.step {
            Step::MoveTarget { direct, .. } => {
                self.gesture = Gesture::None;
                self.step = if direct { Step::Select } else { Step::MoveBase };
                self.say(AnnouncementKey::MoveCancelled, Vec::new());
            }
            // Back to the step just left, with what it had before: a typed width is kept.
            Step::Height {
                first,
                second,
                width_typed,
            } => {
                self.step = Step::SecondCorner {
                    first,
                    width: width_typed.then(|| second[0] - first[0]),
                };
            }
            // The depth step goes back to the width; the width step, to the first corner.
            Step::SecondCorner {
                first,
                width: Some(_),
            } => self.step = Step::SecondCorner { first, width: None },
            Step::SecondCorner { width: None, .. } => self.step = Step::FirstCorner,
            Step::MoveIdle | Step::MoveBase | Step::FirstCorner => {
                self.tool = Tool::Select;
                self.step = Step::Select;
            }
            Step::PickFace => {
                self.sect.face_hit = None;
                self.step = self.first_step();
            }
            Step::Select => {
                if let Gesture::Handle { start, .. } = self.gesture {
                    self.restore_handle(start);
                    self.gesture = Gesture::None;
                } else if self.pending.is_some() {
                    self.pending = None;
                } else if self.sect.visibility.end_isolation() {
                    self.visibility_changed(&BTreeSet::new());
                    self.say_visibility();
                } else {
                    self.set_selection(Vec::new());
                }
            }
        }
        self.refresh_pointer();
    }

    // Moves.

    fn begin_move(&mut self, base: Vec3, direct: bool) {
        self.pending = None;
        self.ghosts_dropped = false;
        self.typed.clear();
        self.step = Step::MoveTarget { base, direct };
    }

    /// The target of the move in progress: the typed distance along the pointer's direction,
    /// else the snapped pointer.
    fn move_target(&self) -> Option<Vec3> {
        let Step::MoveTarget { base, .. } = self.step else {
            return None;
        };
        let pointer = self.snap?.point;
        match self.typed.value() {
            Some(v) => {
                normalize(sub(pointer, base)).map(|direction| add(base, scale(direction, v)))
            }
            None => Some(pointer),
        }
    }

    /// Sends the move to `target`, if it moves anything. A direct move (a drag) ends either way;
    /// the Move tool waits for another target.
    fn commit_move(&mut self, target: Option<Vec3>) {
        let Step::MoveTarget { base, direct } = self.step else {
            return;
        };
        let translation = target.map_or([0.0; 3], |target| sub(target, base));
        let distance = norm(translation);
        if distance.is_nan() || distance <= 1e-9 || self.selection.is_empty() {
            if direct {
                self.step = Step::Select;
            }
            return;
        }
        let ids = self.selection.clone();
        self.events.push(ViewportEvent::Intent(Intent::Move {
            ids: ids.clone(),
            translation,
        }));
        self.say(
            AnnouncementKey::MoveDistance,
            vec![("distance", Value::Number(distance))],
        );
        self.pending = Some(Pending {
            ids,
            translation,
            frames: PENDING_FRAMES,
        });
        self.typed.clear();
        self.step = if direct { Step::Select } else { Step::MoveBase };
    }

    // Blocks.

    /// Where the pointer's ray meets the horizontal plane through `first`, snapped when it
    /// snaps.
    fn on_plane(&self, first: Vec3) -> Option<Vec3> {
        // The ground is z = 0: above or below it, its point is not under the pointer.
        if let Some(snap) = self
            .snap
            .filter(|snap| snap.kind.is_some_and(|kind| kind != SnapKind::Ground))
        {
            return Some([snap.point[0], snap.point[1], first[2]]);
        }
        let view = self.view();
        let ray = view.ray(self.cursor?);
        let height = first[2] - view.eye[2] - ray.origin[2];
        let t = height / ray.direction[2];
        (t > 0.0 && t.is_finite()).then(|| {
            let point = add(view.eye, ray.at(t));
            [point[0], point[1], first[2]]
        })
    }

    /// The opposite corner from the pointer and the typed width, if any.
    fn second_corner(&self) -> Option<Vec3> {
        let Step::SecondCorner { first, width } = self.step else {
            return None;
        };
        let pointer = self.on_plane(first)?;
        let typed = self.typed.value();
        let sign = |delta: f64| if delta < 0.0 { -1.0 } else { 1.0 };
        Some(match width {
            None => match typed {
                Some(v) => [
                    first[0] + sign(pointer[0] - first[0]) * v,
                    pointer[1],
                    first[2],
                ],
                None => pointer,
            },
            Some(width) => {
                let y = typed.map_or(pointer[1], |v| first[1] + sign(pointer[1] - first[1]) * v);
                [first[0] + width, y, first[2]]
            }
        })
    }

    fn confirm_footprint(&mut self, first: Vec3, second: Vec3, width_typed: bool) {
        let [width, depth] = footprint(first, second);
        // Clicked or typed, a side obeys the limits of a typed one, as the height does.
        if !BLOCK_SIDES.contains(&width) || !BLOCK_SIDES.contains(&depth) {
            return;
        }
        self.typed.clear();
        self.step = Step::Height {
            first,
            second,
            width_typed,
        };
        self.say(
            AnnouncementKey::PlaceSize,
            vec![
                ("width", Value::Number(width)),
                ("depth", Value::Number(depth)),
            ],
        );
        self.refresh_pointer();
    }

    /// The height under the pointer: where its ray passes nearest the vertical line through the
    /// opposite corner, or the typed value.
    fn pointer_height(&self, first: Vec3, second: Vec3) -> Option<f64> {
        if let Some(v) = self.typed.value() {
            return Some(v);
        }
        let view = self.view();
        let ray = view.ray(self.cursor?);
        let foot = sub(second, view.eye);
        let height = vertical_nearest(&ray, foot)?;
        Some((height - (first[2] - second[2])).max(0.0))
    }

    fn commit_block(&mut self, first: Vec3, second: Vec3, height: f64) {
        if !BLOCK_SIDES.contains(&height) {
            return;
        }
        let [width, depth] = footprint(first, second);
        let size = [width, depth, height];
        let location = [
            f64::midpoint(first[0], second[0]),
            f64::midpoint(first[1], second[1]),
            first[2],
        ];
        self.events.push(ViewportEvent::Intent(Intent::PlaceBlock {
            location,
            rotation: 0.0,
            size,
        }));
        self.say(
            AnnouncementKey::PlaceHeight,
            vec![("height", Value::Number(height))],
        );
        self.typed.clear();
        self.step = Step::FirstCorner;
        self.refresh_pointer();
    }

    // What the renderer and the host see.

    /// Brings the display states, the preview, the label and the reported states in line with
    /// the interaction.
    fn refresh(&mut self) {
        self.sync_states();
        let preview = self.build_preview();
        if preview != self.preview {
            match self.sink.set_preview(&preview) {
                Ok(()) => self.preview = preview,
                Err(SinkError::Render(render::RenderError::OverlayTooLarge))
                    if !self.ghosts_dropped =>
                {
                    // Too many ghosts for this device: the preview goes on without them, and
                    // nothing is reported as broken.
                    self.ghosts_dropped = true;
                    self.refresh();
                    return;
                }
                Err(_) => {
                    let empty = Preview::default();
                    if self.sink.set_preview(&empty).is_ok() {
                        self.preview = empty;
                    }
                }
            }
            self.dirty = true;
        }
        let label = self.build_label();
        if label != self.label {
            self.label.clone_from(&label);
            self.events.push(ViewportEvent::PreviewLabel(label));
        }
        let step = self.tool_step();
        if (self.tool, step) != (self.reported.0, self.reported.1) {
            self.events.push(ViewportEvent::ToolState(self.tool, step));
        }
        // The state holds lists: it is built only when something it shows changed.
        if self.view_state_changed() {
            let state = self.view_state();
            self.events.push(ViewportEvent::ViewState(state.clone()));
            self.reported.2 = state;
            self.view_state_sent();
        }
        self.reported.0 = self.tool;
        self.reported.1 = step;
    }

    fn tool_step(&self) -> ToolStep {
        match self.step {
            Step::Select | Step::MoveIdle => ToolStep::Idle,
            Step::MoveBase => ToolStep::BasePoint,
            Step::MoveTarget { .. } => ToolStep::TargetPoint,
            Step::FirstCorner => ToolStep::FirstCorner,
            Step::SecondCorner { .. } => ToolStep::SecondCorner,
            Step::Height { .. } => ToolStep::Height,
            Step::PickFace => ToolStep::PickFace,
        }
    }

    /// The elements drawn as ghosts at their place, with the translation of their preview.
    fn moving(&self) -> Option<(Vec<ElementId>, Vec3)> {
        if self.ghosts_dropped {
            return None;
        }
        if let Step::MoveTarget { base, .. } = self.step {
            let target = self.move_target().unwrap_or(base);
            return Some((self.selection.clone(), sub(target, base)));
        }
        self.pending
            .as_ref()
            .map(|pending| (pending.ids.clone(), pending.translation))
    }

    fn sync_states(&mut self) {
        let mut wanted = BTreeMap::new();
        if let Some(id) = self.hover.filter(|_| self.step == Step::Select) {
            wanted.insert(id, DisplayState::Hovered);
        }
        for &id in &self.selection {
            wanted.insert(id, DisplayState::Selected);
        }
        if let Some((ids, _)) = self.moving() {
            // A hidden element stays hidden, whatever move is waiting for the scene.
            for id in ids.into_iter().filter(|id| !self.sect.hidden.contains(id)) {
                wanted.insert(id, DisplayState::Ghost);
            }
        }
        let mut changed = false;
        for (&id, &state) in &wanted {
            if self.states.get(&id) != Some(&state) {
                drop(self.sink.set_display_state(id, state));
                changed = true;
            }
        }
        for &id in self.states.keys() {
            if !wanted.contains_key(&id) && self.cache.contains(id) {
                drop(self.sink.set_display_state(id, DisplayState::Normal));
                changed = true;
            }
        }
        self.states = wanted;
        self.dirty |= changed;
    }

    fn build_preview(&self) -> Preview {
        let mut preview = Preview::default();
        if let Some((ids, translation)) = self.moving() {
            preview.ghosts = ids.into_iter().map(|id| (id, translation)).collect();
        }
        match self.step {
            Step::MoveTarget { base, .. } => {
                if let Some(target) = self.move_target() {
                    preview.lines.push(PreviewLine {
                        from: base,
                        to: target,
                        tone: Tone::Preview,
                    });
                }
            }
            Step::SecondCorner { first, .. } => {
                if let Some(second) = self.second_corner() {
                    let corners = [
                        first,
                        [second[0], first[1], first[2]],
                        second,
                        [first[0], second[1], first[2]],
                    ];
                    for (i, &from) in corners.iter().enumerate() {
                        preview.lines.push(PreviewLine {
                            from,
                            to: corners[(i + 1) % 4],
                            tone: Tone::Preview,
                        });
                    }
                }
            }
            Step::Height { first, second, .. } => {
                if let Some(height) = self.pointer_height(first, second) {
                    let min = [first[0].min(second[0]), first[1].min(second[1]), first[2]];
                    let max = [
                        first[0].max(second[0]),
                        first[1].max(second[1]),
                        first[2] + height,
                    ];
                    preview.boxes.push((min, max));
                }
            }
            _ => {}
        }
        self.sectioning_preview(&mut preview);
        // The height follows the pointer without snapping; the Select tool does not snap (the
        // snap of a direct move that just ended is stale).
        if let Some(Snap {
            kind: Some(kind),
            point,
        }) = self.snap
            && !matches!(self.step, Step::Height { .. } | Step::Select)
        {
            preview.markers.push((point, kind));
        }
        preview
    }

    fn build_label(&self) -> Option<PreviewLabel> {
        let typed = self.typed.text().map(str::to_owned);
        let view = self.view();
        let css = |[x, y]: [f64; 2]| [x / self.ratio, y / self.ratio];
        let cursor = self.cursor.map(css);
        match self.step {
            Step::MoveTarget { base, .. } => {
                let target = self.move_target()?;
                let middle = scale(add(base, target), 0.5);
                let at = view
                    .project(sub(middle, view.eye))
                    .map(|[x, y, _]| css([x, y]))
                    .or(cursor)?;
                Some(PreviewLabel {
                    at,
                    key: LabelKey::MoveDistance,
                    values: vec![("distance", Value::Number(norm(sub(target, base))))],
                    typed,
                })
            }
            Step::SecondCorner {
                first,
                width: fixed,
            } => {
                let [width, depth] = footprint(first, self.second_corner()?);
                Some(PreviewLabel {
                    at: cursor?,
                    // Once the width is confirmed, the depth is what is being set.
                    key: if fixed.is_some() {
                        LabelKey::PlaceDepth
                    } else {
                        LabelKey::PlaceSize
                    },
                    values: vec![
                        ("width", Value::Number(width)),
                        ("depth", Value::Number(depth)),
                    ],
                    typed,
                })
            }
            Step::Height { first, second, .. } => Some(PreviewLabel {
                at: cursor?,
                key: LabelKey::PlaceHeight,
                values: vec![("height", Value::Number(self.pointer_height(first, second)?))],
                typed,
            }),
            _ => None,
        }
    }
}

/// The width and depth of the footprint between two opposite corners.
fn footprint(first: Vec3, second: Vec3) -> [f64; 2] {
    [(second[0] - first[0]).abs(), (second[1] - first[1]).abs()]
}

/// A side in device pixels, between 1 and 65 535 (NaN gives 1).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 1..=65535 first, so the conversion is exact"
)]
fn device_pixels(side: f64) -> u32 {
    let clamped = side.round().clamp(1.0, f64::from(u16::MAX));
    if clamped.is_nan() { 1 } else { clamped as u32 }
}

/// The height above `foot` (relative to the eye) of the point of the vertical line through it
/// nearest to the ray, or `None` when the ray is vertical.
fn vertical_nearest(ray: &Ray, foot: Vec3) -> Option<f64> {
    let up = [0.0, 0.0, 1.0];
    let from = sub(foot, ray.origin);
    let along = dot(ray.direction, up);
    let denominator = 1.0 - along * along;
    if denominator <= 1e-12 {
        return None;
    }
    Some((along * dot(ray.direction, from) - dot(up, from)) / denominator)
}

#[cfg(test)]
mod tests;
