//! The section box, the clipping planes and the visibility of the view (VP-03, spec D6): the
//! commands, the face handles of the box, what is hidden or clipped being neither picked nor
//! snapped to, and what the renderer and the host are told. The rules are in `clipping.rs` and
//! `visibility.rs`; this part joins them to the interaction.

use std::collections::{BTreeMap, BTreeSet};

use render::{ClipPlane, SnapKind, Tone};

use super::{Step, Viewport};
use crate::ElementId;
use crate::clipping::{
    Clipping, Face, SHIFT_STEP, SectionBox, drag_coordinate, fitted, nearest_handle, plane_half,
    plane_lines, snap_to_step, valid_extents,
};
use crate::event::{
    AnnouncementKey, NameState, SectionBoxState, Value, ViewCommand, ViewState, VisibilityTarget,
};
use crate::math::{Vec3, add, count, scale, sub};
use crate::pick::{Hit, Pickable};
use crate::sink::{Preview, PreviewLine, SceneSink};
use crate::visibility::{Target, Visibility};

/// How near a pointer must be to a face handle to take it, in CSS pixels.
const HANDLE_REACH: f64 = 12.0;

/// The panels' list of names with their counts and flags.
fn name_states(
    counts: &BTreeMap<&str, usize>,
    hidden: &dyn Fn(&str) -> bool,
    isolated: &dyn Fn(&str) -> bool,
) -> Vec<NameState> {
    counts
        .iter()
        .map(|(&name, &count)| NameState {
            name: name.to_owned(),
            count,
            hidden: hidden(name),
            isolated: isolated(name),
        })
        .collect()
}

/// Whether the box can be handed to the renderer.
fn valid_section(section: &SectionBox) -> bool {
    valid_extents(section.min, section.max)
}

/// What the view holds beyond the camera and the tools.
#[derive(Debug, Default)]
pub(super) struct Sectioning {
    pub(super) visibility: Visibility,
    pub(super) clipping: Clipping,
    /// The elements the visibility hides, as the renderer was told.
    pub(super) hidden: BTreeSet<ElementId>,
    categories: Vec<NameState>,
    disciplines: Vec<NameState>,
    /// The handle under the pointer.
    pub(super) handle: Option<Face>,
    /// The face a clipping plane would be put on: the point and the normal turned to the eye.
    pub(super) face_hit: Option<(Vec3, Vec3)>,
    pub(super) shift: bool,
    /// Counts the changes the UI's panels show; the viewport state is sent when it moves.
    version: u64,
    reported: u64,
}

/// What may be picked or snapped to: not what is hidden, not what a plane clipped away.
pub(super) struct Gate<'a> {
    hidden: &'a BTreeSet<ElementId>,
    planes: Vec<ClipPlane>,
}

impl<'a> Gate<'a> {
    pub(super) fn new(sectioning: &'a Sectioning) -> Self {
        Self {
            hidden: &sectioning.hidden,
            planes: sectioning.clipping.all_planes(),
        }
    }
}

impl Pickable for Gate<'_> {
    fn pickable(&self, element: ElementId, point: Vec3) -> bool {
        !self.hidden.contains(&element) && !crate::clipping::planes_clip(&self.planes, point)
    }
}

impl<S: SceneSink> Viewport<S> {
    /// Runs the commands of the section box, the planes and the visibility; others do nothing.
    pub(super) fn sectioning_command(&mut self, command: ViewCommand) {
        match command {
            ViewCommand::SectionBox(enabled) => self.set_section_enabled(enabled),
            ViewCommand::SectionBoxExtents { min, max } => {
                if valid_extents(min, max) {
                    let enabled = self.sect.clipping.section.is_none_or(|s| s.enabled);
                    self.set_section(Some(SectionBox { enabled, min, max }));
                }
            }
            ViewCommand::SectionBoxFitSelection => {
                let fit = self.cache.extent(Some(&self.selection));
                if let Some(section) = fit.map(|(min, max)| fitted(min, max)).filter(valid_section)
                {
                    self.set_section(Some(section));
                }
            }
            ViewCommand::SectionBoxReset => {
                if let Some((min, max)) = self.cache.extent(None) {
                    let enabled = self.sect.clipping.section.is_some_and(|s| s.enabled);
                    let section = SectionBox {
                        enabled,
                        ..fitted(min, max)
                    };
                    if valid_section(&section) {
                        self.set_section(Some(section));
                    }
                }
            }
            ViewCommand::ClipPlanePick => {
                if self.step == Step::Select
                    && self.sect.clipping.planes.len() < crate::clipping::MAX_USER_PLANES
                {
                    self.step = Step::PickFace;
                    self.sect.face_hit = None;
                    self.refresh_pointer();
                }
            }
            ViewCommand::ClipPlaneAdd { point, normal } => self.add_plane(point, normal),
            ViewCommand::ClipPlaneFlip(index) => {
                if self.sect.clipping.flip(index) {
                    self.clipping_changed();
                    self.say(
                        AnnouncementKey::ClipPlaneFlipped,
                        vec![("index", Value::Number(count(index + 1)))],
                    );
                }
            }
            ViewCommand::ClipPlaneRemove(index) => {
                if self.sect.clipping.remove(index) {
                    self.clipping_changed();
                    let left = self.sect.clipping.planes.len();
                    self.say(
                        AnnouncementKey::ClipPlaneRemoved,
                        vec![("count", Value::Number(count(left)))],
                    );
                }
            }
            ViewCommand::ClipPlanesClear => {
                if !self.sect.clipping.planes.is_empty() {
                    self.sect.clipping.planes.clear();
                    self.clipping_changed();
                    self.say(AnnouncementKey::ClipPlanesCleared, Vec::new());
                }
            }
            ViewCommand::CutFaces(cut) => {
                self.sect.clipping.cut = cut;
                self.clipping_changed();
            }
            ViewCommand::Hide(target) => self.hide(&target),
            ViewCommand::Show(target) => self.show(&target),
            ViewCommand::Isolate(target) => self.isolate(&target),
            ViewCommand::EndIsolate => {
                if self.sect.visibility.end_isolation() {
                    self.visibility_changed(&BTreeSet::new());
                    self.say_visibility();
                }
            }
            ViewCommand::ResetVisibility if !self.sect.visibility.is_clear() => {
                self.sect.visibility = Visibility::default();
                self.visibility_changed(&BTreeSet::new());
                self.say(AnnouncementKey::VisibilityReset, Vec::new());
            }
            _ => {}
        }
    }

    // The section box.

    fn set_section_enabled(&mut self, enabled: bool) {
        let section = match self.sect.clipping.section {
            Some(section) => Some(SectionBox { enabled, ..section }),
            // The first time, the box fits the scene; with nothing to fit there is no box.
            None if enabled => self
                .cache
                .extent(None)
                .map(|(min, max)| fitted(min, max))
                .filter(valid_section),
            None => None,
        };
        self.set_section(section);
    }

    fn set_section(&mut self, section: Option<SectionBox>) {
        self.sect.clipping.section = section;
        // A box that is off has no handles to grab.
        if section.is_none_or(|s| !s.enabled) {
            self.sect.handle = None;
        }
        self.clipping_changed();
    }

    /// The renderer and the panels follow a change of planes.
    fn clipping_changed(&mut self) {
        let planes = self.sect.clipping.all_planes();
        // Refused planes (the viewport never builds any) leave the previous ones in place.
        drop(self.sink.set_clip(&planes, self.sect.clipping.cut));
        self.sect.version += 1;
        self.dirty = true;
    }

    fn add_plane(&mut self, point: Vec3, normal: Vec3) {
        if self.sect.clipping.add(point, normal) {
            self.clipping_changed();
            let planes = self.sect.clipping.planes.len();
            self.say(
                AnnouncementKey::ClipPlaneAdded,
                vec![("count", Value::Number(count(planes)))],
            );
        }
    }

    /// The face under the pointer, for a clipping plane: its point and the normal turned to the
    /// eye, so the plane removes what stands in front of the face.
    pub(super) fn face_under(&mut self, pos: [f64; 2]) -> Option<(Vec3, Vec3)> {
        let hit: Hit = self.pick_hit(pos)?;
        let view = self.view();
        Some((add(view.eye, hit.relative), hit.normal))
    }

    /// A click in the `PickFace` step: a plane on the face, and back to the tool.
    pub(super) fn pick_face_click(&mut self) {
        if let Some((point, normal)) = self.sect.face_hit {
            self.add_plane(point, normal);
        }
        self.sect.face_hit = None;
        self.step = self.first_step();
        self.refresh_pointer();
    }

    // The handles.

    /// The handle under `pos`, when the box is on.
    pub(super) fn handle_at(&self, pos: [f64; 2]) -> Option<Face> {
        let section = self.sect.clipping.section.filter(|s| s.enabled)?;
        let view = self.view();
        let projected: Vec<_> = Face::ALL
            .iter()
            .map(|&face| {
                let at = view
                    .project(sub(section.face_centre(face), view.eye))
                    .map(|[x, y, _]| [x, y]);
                (face, at)
            })
            .collect();
        nearest_handle(&projected, pos, HANDLE_REACH * self.ratio)
    }

    /// The pointer dragged a handle to `pos`.
    pub(super) fn drag_handle(&mut self, face: Face, pos: [f64; 2]) {
        let Some(mut section) = self.sect.clipping.section else {
            return;
        };
        let view = self.view();
        let Some(mut value) = drag_coordinate(&section, face, &view.ray(pos), view.eye) else {
            return;
        };
        if self.sect.shift {
            value = snap_to_step(value, SHIFT_STEP);
        } else if let Some(snap) = self.snap_at(pos).filter(|snap| {
            // Only the points of the model: a face or the ground under the pointer says nothing
            // about where along the axis it points.
            matches!(
                snap.kind,
                Some(SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Edge)
            )
        }) {
            value = snap.point[face.axis];
        }
        section.move_face(face, value);
        if Some(section) != self.sect.clipping.section {
            self.set_section(Some(section));
        }
    }

    /// The handle was released: say where the face is.
    pub(super) fn drop_handle(&mut self, face: Face) {
        if let Some(section) = self.sect.clipping.section {
            self.say(
                AnnouncementKey::SectionBoxFaceMoved,
                vec![
                    ("face", Value::Text(face.name())),
                    ("value", Value::Number(section.coordinate(face))),
                ],
            );
        }
    }

    /// A drag of a handle cut short puts the box back.
    pub(super) fn restore_handle(&mut self, start: SectionBox) {
        self.set_section(Some(start));
    }

    // Visibility.

    fn resolve(&self, target: &VisibilityTarget) -> Option<Target> {
        match target {
            VisibilityTarget::Categories(names) => Some(Target::Categories(names.clone())),
            VisibilityTarget::Disciplines(names) => Some(Target::Disciplines(names.clone())),
            VisibilityTarget::Selection => {
                (!self.selection.is_empty()).then(|| Target::Elements(self.selection.clone()))
            }
        }
    }

    fn hide(&mut self, target: &VisibilityTarget) {
        if let Some(target) = self.resolve(target) {
            let before = self.sect.hidden.len();
            self.sect.visibility.hide(&target);
            self.visibility_changed(&BTreeSet::new());
            if self.sect.hidden.len() != before {
                self.say_visibility();
            }
        }
    }

    fn show(&mut self, target: &VisibilityTarget) {
        // Hidden elements cannot be selected: showing the selection shows what "hide selection"
        // hid.
        if *target == VisibilityTarget::Selection {
            self.sect.visibility.show_hidden_elements();
        } else if let Some(target) = self.resolve(target) {
            self.sect.visibility.show(&target);
        }
        let before = self.sect.hidden.len();
        self.visibility_changed(&BTreeSet::new());
        if self.sect.hidden.len() != before {
            self.say_visibility();
        }
    }

    fn isolate(&mut self, target: &VisibilityTarget) {
        if let Some(target) = self.resolve(target) {
            self.sect.visibility.isolate(&target);
            self.visibility_changed(&BTreeSet::new());
            let shown = self.cache.elements().count() - self.sect.hidden.len();
            self.say(
                AnnouncementKey::VisibilityIsolated,
                vec![("count", Value::Number(count(shown)))],
            );
        }
    }

    /// After a change that leaves something hidden, or nothing.
    pub(super) fn say_visibility(&mut self) {
        if self.sect.hidden.is_empty() {
            self.say(AnnouncementKey::VisibilityReset, Vec::new());
        } else {
            let hidden = self.sect.hidden.len();
            self.say(
                AnnouncementKey::VisibilityHidden,
                vec![("count", Value::Number(count(hidden)))],
            );
        }
    }

    /// Recomputes which elements are hidden, tells the renderer about the changes (and about
    /// `recreated` elements, whose objects forgot), drops hidden ones from the selection and the
    /// hover, and refreshes the panels' lists of categories and disciplines.
    pub(super) fn visibility_changed(&mut self, recreated: &BTreeSet<ElementId>) {
        let isolation_ended = self
            .sect
            .visibility
            .retain_elements(|id| self.cache.contains(id));
        let mut hidden = BTreeSet::new();
        let mut categories: BTreeMap<&str, usize> = BTreeMap::new();
        let mut disciplines: BTreeMap<&str, usize> = BTreeMap::new();
        for (id, element) in self.cache.elements() {
            let (category, discipline) = self.cache.names_of(element);
            if !self.sect.visibility.visible(id, category, discipline) {
                hidden.insert(id);
            }
            *categories.entry(category).or_default() += 1;
            *disciplines.entry(discipline).or_default() += 1;
        }
        let vis = &self.sect.visibility;
        let categories = name_states(&categories, &|n| vis.category_hidden(n), &|n| {
            vis.category_isolated(n)
        });
        let disciplines = name_states(&disciplines, &|n| vis.discipline_hidden(n), &|n| {
            vis.discipline_isolated(n)
        });
        // Nothing the panels show changed: a delta that moves a wall sends no view state.
        let changed = hidden != self.sect.hidden
            || categories != self.sect.categories
            || disciplines != self.sect.disciplines;
        let old = core::mem::replace(&mut self.sect.hidden, hidden);
        for &id in self
            .sect
            .hidden
            .iter()
            .filter(|id| !old.contains(id) || recreated.contains(id))
        {
            drop(
                self.sink
                    .set_display_state(id, render::DisplayState::Hidden),
            );
        }
        for &id in old.iter().filter(|id| !self.sect.hidden.contains(id)) {
            // An element the delta removed has no object to reset.
            drop(
                self.sink
                    .set_display_state(id, render::DisplayState::Normal),
            );
        }
        self.sect.categories = categories;
        self.sect.disciplines = disciplines;
        let selection: Vec<ElementId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| !self.sect.hidden.contains(id))
            .collect();
        self.set_selection(selection);
        if self.hover.is_some_and(|id| self.sect.hidden.contains(&id)) {
            self.set_hover(None);
        }
        // A hidden element is hidden for the renderer whatever else it was: not tracked.
        self.states.retain(|id, _| !self.sect.hidden.contains(id));
        if changed {
            self.sect.version += 1;
        }
        if isolation_ended {
            self.say(AnnouncementKey::VisibilityReset, Vec::new());
        }
        self.dirty = true;
    }

    // What the host and the renderer see.

    /// The whole view state, for the panels.
    pub(super) fn view_state(&self) -> ViewState {
        ViewState {
            projection: self.camera.mode(),
            snapping: self.snapping && !self.alt,
            tool: self.tool,
            categories: self.sect.categories.clone(),
            disciplines: self.sect.disciplines.clone(),
            isolating: self.sect.visibility.is_isolating(),
            hidden_selection: self.sect.visibility.hidden_element_count(),
            section_box: self.sect.clipping.section.map(|s| SectionBoxState {
                enabled: s.enabled,
                min: s.min,
                max: s.max,
            }),
            clip_planes: self.sect.clipping.planes.clone(),
            cut_faces: self.sect.clipping.cut,
        }
    }

    /// Whether the view state differs from the one the host holds.
    pub(super) fn view_state_changed(&self) -> bool {
        let held = &self.reported.2;
        held.projection != self.camera.mode()
            || held.snapping != (self.snapping && !self.alt)
            || held.tool != self.tool
            || self.sect.version != self.sect.reported
    }

    /// The host now holds the view state.
    pub(super) fn view_state_sent(&mut self) {
        self.sect.reported = self.sect.version;
    }

    /// The lines and markers of the box, its handles and the planes.
    pub(super) fn sectioning_preview(&self, preview: &mut Preview) {
        if let Some(section) = self.sect.clipping.section.filter(|s| s.enabled) {
            for (from, to) in section.edges() {
                preview.lines.push(PreviewLine {
                    from,
                    to,
                    tone: Tone::Selection,
                });
            }
            for face in Face::ALL {
                preview
                    .markers
                    .push((section.face_centre(face), SnapKind::Endpoint));
            }
        }
        let half = plane_half(self.scene().map_or(0.0, |s| s.radius));
        for plane in &self.sect.clipping.planes {
            for (from, to) in plane_lines(plane, half) {
                preview.lines.push(PreviewLine {
                    from,
                    to,
                    tone: Tone::Selection,
                });
            }
        }
        // The face a plane would be put on.
        if self.step == Step::PickFace
            && let Some((point, normal)) = self.sect.face_hit
        {
            preview.markers.push((point, SnapKind::Face));
            preview.lines.push(PreviewLine {
                from: point,
                to: add(point, scale(normal, half * 0.25)),
                tone: Tone::Hover,
            });
        }
    }
}
