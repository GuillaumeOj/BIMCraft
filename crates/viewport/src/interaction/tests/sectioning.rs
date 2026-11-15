//! The section box, the clipping planes and the visibility through the interaction (VP-03).

#![expect(
    clippy::float_cmp,
    reason = "exact values: typed extents and unit normals"
)]

use render::{ClipPlane, CutFaces};

use super::*;
use crate::clipping::Face;
use crate::event::{NameState, SectionBoxState, VisibilityTarget};

/// Two unit cubes 3 m apart, the first a `Wall`, the second a `Slab`, both of discipline `arch`.
fn categorised() -> Viewport<FakeSink> {
    let mut viewport = Viewport::new(FakeSink::default(), SIZE, 1.0);
    let mut delta = cubes(1, [0.0; 3], &[[0.0, 0.0, 0.5], [3.0, 0.0, 0.5]]);
    delta.strings = vec!["Wall".to_owned(), "Slab".to_owned(), "arch".to_owned()];
    delta.elements[0].category = 0;
    delta.elements[1].category = 1;
    for element in &mut delta.elements {
        element.discipline = 2;
    }
    viewport.apply_delta(&delta).unwrap();
    viewport.command(ViewCommand::ZoomFit);
    viewport.drain_events();
    viewport.sink_mut().take_calls();
    viewport
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn view_state(viewport: &Viewport<FakeSink>) -> ViewState {
    viewport.view_state()
}

fn selected(viewport: &mut Viewport<FakeSink>, point: Vec3) -> Vec<ElementId> {
    click_on(viewport, point, NONE);
    viewport.selection().to_vec()
}

fn hidden_states(viewport: &Viewport<FakeSink>) -> Vec<ElementId> {
    viewport
        .sink()
        .states
        .iter()
        .filter(|(_, state)| **state == DisplayState::Hidden)
        .map(|(id, _)| *id)
        .collect()
}

#[test]
fn hiding_a_category_hides_its_elements_and_they_cannot_be_picked() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Hide(VisibilityTarget::Categories(names(&[
        "Wall",
    ]))));
    assert_eq!(hidden_states(&viewport), [id(1)]);
    assert_eq!(selected(&mut viewport, CORNER_1), []);
    assert_eq!(selected(&mut viewport, CORNER_2), [id(2)]);
    let state = view_state(&viewport);
    assert_eq!(
        state.categories,
        [
            NameState {
                name: "Slab".into(),
                count: 1,
                hidden: false,
                isolated: false
            },
            NameState {
                name: "Wall".into(),
                count: 1,
                hidden: true,
                isolated: false
            },
        ]
    );
    assert_eq!(state.disciplines.len(), 1);
    viewport.command(ViewCommand::Show(VisibilityTarget::Categories(names(&[
        "Wall",
    ]))));
    assert!(hidden_states(&viewport).is_empty());
    assert_eq!(selected(&mut viewport, CORNER_1), [id(1)]);
    let keys = announced(&mut viewport, &[]);
    assert!(keys.contains(&AnnouncementKey::SelectionCount), "{keys:?}");
}

#[test]
fn hiding_a_discipline_or_the_selection_works_and_drops_the_selection() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Hide(VisibilityTarget::Disciplines(names(&[
        "arch",
    ]))));
    assert_eq!(hidden_states(&viewport).len(), 2);
    viewport.command(ViewCommand::Show(VisibilityTarget::Disciplines(names(&[
        "arch",
    ]))));
    click_on(&mut viewport, CORNER_1, NONE);
    viewport.drain_events();
    viewport.command(ViewCommand::Hide(VisibilityTarget::Selection));
    let events = viewport.drain_events();
    assert!(
        events.contains(&ViewportEvent::Selection(vec![])),
        "{events:?}"
    );
    assert_eq!(hidden_states(&viewport), [id(1)]);
    assert_eq!(view_state(&viewport).hidden_selection, 1);
    // Hiding an empty selection does nothing.
    viewport.command(ViewCommand::Hide(VisibilityTarget::Selection));
    assert_eq!(hidden_states(&viewport), [id(1)]);
    viewport.command(ViewCommand::Show(VisibilityTarget::Selection));
    assert!(hidden_states(&viewport).is_empty());
    assert_eq!(view_state(&viewport).hidden_selection, 0);
}

#[test]
fn isolating_shows_only_the_target_and_escape_ends_it_before_the_selection() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Categories(names(
        &["Slab"],
    ))));
    assert_eq!(hidden_states(&viewport), [id(1)]);
    let state = view_state(&viewport);
    assert!(state.isolating && state.categories[0].isolated && !state.categories[1].isolated);
    click_on(&mut viewport, CORNER_2, NONE);
    assert_eq!(viewport.selection(), [id(2)]);
    // Esc: first the isolation, then the selection.
    viewport.command(ViewCommand::Cancel);
    assert!(hidden_states(&viewport).is_empty());
    assert_eq!(viewport.selection(), [id(2)]);
    assert!(!view_state(&viewport).isolating);
    viewport.command(ViewCommand::Cancel);
    assert!(viewport.selection().is_empty());
    // Isolating the selection, an empty one first.
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Selection));
    assert!(hidden_states(&viewport).is_empty());
    click_on(&mut viewport, CORNER_1, NONE);
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Selection));
    assert_eq!(hidden_states(&viewport), [id(2)]);
    viewport.command(ViewCommand::EndIsolate);
    viewport.command(ViewCommand::EndIsolate);
    assert!(hidden_states(&viewport).is_empty());
    let keys = announced(&mut viewport, &[]);
    assert!(keys.contains(&AnnouncementKey::VisibilityReset), "{keys:?}");
}

#[test]
fn resetting_the_visibility_shows_everything() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::ResetVisibility);
    viewport.command(ViewCommand::Hide(VisibilityTarget::Categories(names(&[
        "Wall",
    ]))));
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Disciplines(names(
        &["arch"],
    ))));
    assert_eq!(hidden_states(&viewport), [id(1)]);
    viewport.command(ViewCommand::ResetVisibility);
    assert!(hidden_states(&viewport).is_empty());
    let state = view_state(&viewport);
    assert!(!state.isolating && state.categories.iter().all(|c| !c.hidden));
}

#[test]
fn a_hidden_element_stays_hidden_when_a_delta_recreates_it_and_forgets_removed_ones() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Hide(VisibilityTarget::Categories(names(&[
        "Wall",
    ]))));
    viewport.command(ViewCommand::Hide(VisibilityTarget::Selection));
    // The wall changes mesh: its object is created anew, hidden again at once.
    let mut delta = SceneDelta {
        revision: 2,
        base: 1,
        strings: vec!["Wall".to_owned(), "arch".to_owned()],
        meshes: vec![crate::test_support::box_mesh(
            8, [0.0; 3], [-0.4; 3], [0.4; 3],
        )],
        elements: vec![element(1, 8, [0.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    delta.elements[0].category = 0;
    delta.elements[0].discipline = 1;
    viewport.apply_delta(&delta).unwrap();
    assert_eq!(hidden_states(&viewport), [id(1)]);
    // New elements follow the rules too.
    let more = SceneDelta {
        revision: 3,
        base: 2,
        strings: vec!["Wall".to_owned(), "arch".to_owned()],
        elements: vec![element(9, 7, [6.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&more).unwrap();
    assert_eq!(hidden_states(&viewport), [id(1), id(9)]);
    // Removing a hidden element leaves nothing to reset.
    let gone = SceneDelta {
        revision: 4,
        base: 3,
        removed_elements: vec![id(9).0],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&gone).unwrap();
    assert_eq!(hidden_states(&viewport), [id(1)]);
}

fn removal(revision: u64, ids: &[ElementId]) -> SceneDelta {
    SceneDelta {
        revision,
        base: revision - 1,
        removed_elements: ids.iter().map(|id| id.0).collect(),
        ..SceneDelta::default()
    }
}

#[test]
fn an_isolation_whose_elements_are_all_gone_ends_and_a_partial_one_stays() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Hide(VisibilityTarget::Categories(names(&[
        "Wall",
    ]))));
    click_on(&mut viewport, CORNER_2, NONE);
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Selection));
    assert!(view_state(&viewport).isolating);
    viewport.drain_events();
    viewport.apply_delta(&removal(2, &[id(2)])).unwrap();
    let state = view_state(&viewport);
    assert!(!state.isolating);
    // The hidden set is kept, and the scene shows again.
    assert_eq!(hidden_states(&viewport), [id(1)]);
    let keys = announced(&mut viewport, &[]);
    assert!(
        keys.contains(&AnnouncementKey::VisibilityHidden),
        "{keys:?}"
    );
    assert!(
        !keys.contains(&AnnouncementKey::VisibilityReset),
        "{keys:?}"
    );
    // Undoing the deletion brings the element back without isolating it again.
    let back = SceneDelta {
        revision: 3,
        base: 2,
        strings: vec!["Slab".to_owned(), "arch".to_owned()],
        elements: vec![element(2, 7, [3.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&back).unwrap();
    assert!(!view_state(&viewport).isolating);

    // Two isolated, one deleted: the isolation stays over the other.
    let mut viewport = categorised();
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Disciplines(names(
        &["arch"],
    ))));
    click_on(&mut viewport, CORNER_1, NONE);
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Selection));
    assert_eq!(hidden_states(&viewport), [id(2)]);
    viewport.apply_delta(&removal(2, &[id(2)])).unwrap();
    assert!(view_state(&viewport).isolating);
    assert!(hidden_states(&viewport).is_empty());
}

#[test]
fn a_category_isolation_whose_elements_are_all_gone_ends() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Categories(names(
        &["Wall"],
    ))));
    assert_eq!(hidden_states(&viewport), [id(2)]);
    viewport.drain_events();
    viewport.apply_delta(&removal(2, &[id(1)])).unwrap();
    assert!(!view_state(&viewport).isolating);
    assert!(hidden_states(&viewport).is_empty());
    let keys = announced(&mut viewport, &[]);
    assert!(keys.contains(&AnnouncementKey::VisibilityReset), "{keys:?}");
}

#[test]
fn a_discipline_isolation_whose_elements_are_all_gone_ends() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::Isolate(VisibilityTarget::Disciplines(names(
        &["arch"],
    ))));
    viewport.apply_delta(&removal(2, &[id(1)])).unwrap();
    assert!(view_state(&viewport).isolating);
    viewport.apply_delta(&removal(3, &[id(2)])).unwrap();
    assert!(!view_state(&viewport).isolating);
}

#[test]
fn the_section_box_fits_the_scene_clips_and_hides_nothing_inside() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::SectionBox(true));
    let calls = viewport.sink_mut().take_calls();
    let clip = calls.iter().rev().find_map(|call| match call {
        Call::Clip(planes, cut) => Some((planes.len(), *cut)),
        _ => None,
    });
    assert_eq!(clip, Some((6, CutFaces::Tinted)));
    let state = view_state(&viewport);
    let SectionBoxState { enabled, min, max } = state.section_box.unwrap();
    assert!(enabled);
    // The scene, 3 m apart in x, with a margin.
    assert!(close(min, [-0.6, -0.6, -0.1], 1e-9) && close(max, [3.6, 0.6, 1.1], 1e-9));
    // Typed extents narrow it: the second cube is outside and cannot be picked.
    viewport.command(ViewCommand::SectionBoxExtents {
        min: [-1.0, -1.0, -1.0],
        max: [1.0, 1.0, 2.0],
    });
    assert_eq!(selected(&mut viewport, CORNER_2), []);
    assert_eq!(selected(&mut viewport, CORNER_1), [id(1)]);
    // Bad extents change nothing.
    viewport.command(ViewCommand::SectionBoxExtents {
        min: [1.0; 3],
        max: [0.0; 3],
    });
    let state = view_state(&viewport);
    assert_eq!(state.section_box.unwrap().max, [1.0, 1.0, 2.0]);
    // Turned off, nothing clips; the extents are kept.
    viewport.command(ViewCommand::SectionBox(false));
    assert_eq!(selected(&mut viewport, CORNER_2), [id(2)]);
    let calls = viewport.sink_mut().take_calls();
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, Call::Clip(planes, _) if planes.is_empty()))
    );
    assert!(!view_state(&viewport).section_box.unwrap().enabled);
}

#[test]
fn the_box_fits_the_selection_and_resets_to_the_scene() {
    let mut viewport = categorised();
    // Nothing selected, nothing to fit.
    viewport.command(ViewCommand::SectionBoxFitSelection);
    assert_eq!(viewport.sect.clipping.section, None);
    click_on(&mut viewport, CORNER_2, NONE);
    viewport.command(ViewCommand::SectionBoxFitSelection);
    let section = viewport.sect.clipping.section.unwrap();
    assert!(section.enabled && close(section.min, [2.4, -0.6, -0.1], 1e-9));
    viewport.command(ViewCommand::SectionBoxReset);
    let section = viewport.sect.clipping.section.unwrap();
    assert!(section.enabled && close(section.min, [-0.6, -0.6, -0.1], 1e-9));
    // An empty scene has no box to turn on.
    let mut empty = Viewport::new(FakeSink::default(), SIZE, 1.0);
    empty.command(ViewCommand::SectionBox(true));
    empty.command(ViewCommand::SectionBoxReset);
    assert_eq!(empty.sect.clipping.section, None);
}

#[test]
fn a_plane_removes_what_its_normal_points_to_and_is_flipped_and_removed() {
    let mut viewport = categorised();
    // x = 1.5 between the cubes, removing the east side.
    viewport.command(ViewCommand::ClipPlaneAdd {
        point: [1.5, 0.0, 0.0],
        normal: [1.0, 0.0, 0.0],
    });
    assert_eq!(selected(&mut viewport, CORNER_2), []);
    assert_eq!(selected(&mut viewport, CORNER_1), [id(1)]);
    viewport.command(ViewCommand::ClipPlaneFlip(0));
    assert_eq!(
        selected(&mut viewport, CORNER_1),
        [id(1)]
            .map(|_| id(1))
            .to_vec()
            .iter()
            .copied()
            .filter(|_| false)
            .collect::<Vec<_>>()
    );
    assert_eq!(selected(&mut viewport, CORNER_2), [id(2)]);
    let state = view_state(&viewport);
    assert_eq!(state.clip_planes.len(), 1);
    assert_eq!(state.clip_planes[0].normal, [-1.0, 0.0, 0.0]);
    viewport.command(ViewCommand::ClipPlaneFlip(5));
    viewport.command(ViewCommand::ClipPlaneRemove(5));
    viewport.command(ViewCommand::ClipPlaneRemove(0));
    assert!(view_state(&viewport).clip_planes.is_empty());
    viewport.command(ViewCommand::ClipPlanesClear);
    let keys = announced(&mut viewport, &[]);
    assert!(
        keys.contains(&AnnouncementKey::ClipPlaneRemoved),
        "{keys:?}"
    );
}

#[test]
fn planes_are_limited_and_a_plane_without_direction_is_refused() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::ClipPlaneAdd {
        point: [0.0; 3],
        normal: [0.0; 3],
    });
    assert!(viewport.sect.clipping.planes.is_empty());
    for i in 0..7 {
        viewport.command(ViewCommand::ClipPlaneAdd {
            point: [20.0 + f64::from(i), 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        });
    }
    assert_eq!(view_state(&viewport).clip_planes.len(), 6);
    // At the limit, arming the pick does nothing.
    viewport.command(ViewCommand::ClipPlanePick);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    viewport.command(ViewCommand::ClipPlanesClear);
    assert!(view_state(&viewport).clip_planes.is_empty());
    viewport.command(ViewCommand::CutFaces(CutFaces::None));
    assert_eq!(view_state(&viewport).cut_faces, CutFaces::None);
    assert_eq!(viewport.sink().clip.1, CutFaces::None);
}

#[test]
fn picking_a_face_puts_a_plane_on_it_facing_the_eye() {
    let mut viewport = categorised();
    viewport.command(ViewCommand::ClipPlanePick);
    assert_eq!(viewport.tool_step(), ToolStep::PickFace);
    hover_on(&mut viewport, [0.0, -0.5, 0.5]);
    assert!(
        viewport
            .sink()
            .preview
            .markers
            .iter()
            .any(|(_, kind)| *kind == SnapKind::Face)
    );
    // Over nothing, a click adds no plane and leaves the pick.
    click(&mut viewport, [2.0, 2.0], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    assert!(viewport.sect.clipping.planes.is_empty());
    viewport.command(ViewCommand::ClipPlanePick);
    click_on(&mut viewport, [0.0, -0.5, 0.5], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    let ClipPlane { point, normal } = viewport.sect.clipping.planes[0];
    // The front face of the first cube, its normal turned to the eye in front of it.
    assert!(close(point, [0.0, -0.5, 0.5], 0.05), "{point:?}");
    assert!(normal[1] < -0.99, "{normal:?}");
    // The pick is one-shot, and Esc cancels it.
    viewport.command(ViewCommand::ClipPlanePick);
    assert_eq!(viewport.tool_step(), ToolStep::PickFace);
    viewport.command(ViewCommand::Cancel);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    assert_eq!(viewport.sect.clipping.planes.len(), 1);
}

fn section_on(viewport: &mut Viewport<FakeSink>) {
    viewport.command(ViewCommand::SectionBox(true));
    viewport.drain_events();
}

/// The handle of the upper x face: its pixel.
fn x_max_handle(viewport: &Viewport<FakeSink>) -> [f64; 2] {
    let section = viewport.sect.clipping.section.unwrap();
    px(
        viewport,
        section.face_centre(Face {
            axis: 0,
            upper: true,
        }),
    )
}

#[test]
fn a_handle_drags_its_face_in_steps_of_100_mm_with_shift() {
    let mut viewport = categorised();
    section_on(&mut viewport);
    let from = x_max_handle(&viewport);
    viewport.input(InputEvent::PointerMove {
        pos: from,
        mods: NONE,
    });
    assert_eq!(
        viewport.sect.handle,
        Some(Face {
            axis: 0,
            upper: true
        })
    );
    // The handle takes the pointer from the elements: no hover.
    viewport.input(InputEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        mods: SHIFT,
    });
    let to = px(&viewport, [2.0, 0.0, 0.5]);
    viewport.input(InputEvent::PointerMove {
        pos: to,
        mods: SHIFT,
    });
    let x = viewport.sect.clipping.section.unwrap().max[0];
    assert!(
        (x - 2.0).abs() < 0.11 && ((x * 10.0).round() - x * 10.0).abs() < 1e-9,
        "{x}"
    );
    viewport.input(InputEvent::PointerUp {
        pos: to,
        button: Button::Primary,
        mods: SHIFT,
    });
    let keys = announced(&mut viewport, &[]);
    assert!(
        keys.contains(&AnnouncementKey::SectionBoxFaceMoved),
        "{keys:?}"
    );
    let state = view_state(&viewport);
    assert!((state.section_box.unwrap().max[0] - x).abs() < 1e-12);
    // The second cube is now outside the box.
    assert_eq!(selected(&mut viewport, CORNER_2), []);
}

#[test]
fn a_handle_drag_cut_short_puts_the_box_back() {
    for cut in ["leave", "escape"] {
        let mut viewport = categorised();
        section_on(&mut viewport);
        let before = viewport.sect.clipping.section.unwrap();
        let from = x_max_handle(&viewport);
        viewport.input(InputEvent::PointerMove {
            pos: from,
            mods: NONE,
        });
        viewport.input(InputEvent::PointerDown {
            pos: from,
            button: Button::Primary,
            mods: NONE,
        });
        let to = px(&viewport, [1.0, 0.0, 0.5]);
        viewport.input(InputEvent::PointerMove {
            pos: to,
            mods: SHIFT,
        });
        assert_ne!(viewport.sect.clipping.section.unwrap(), before);
        if cut == "leave" {
            viewport.input(InputEvent::PointerLeave);
        } else {
            key(&mut viewport, Key::Escape);
        }
        assert_eq!(viewport.sect.clipping.section.unwrap(), before, "{cut}");
        viewport.input(InputEvent::PointerUp {
            pos: to,
            button: Button::Primary,
            mods: NONE,
        });
        assert_eq!(viewport.sect.clipping.section.unwrap(), before, "{cut}");
    }
}

#[test]
fn the_box_and_the_planes_are_drawn_as_overlay_lines() {
    let mut viewport = categorised();
    section_on(&mut viewport);
    // Twelve edges of the box, a handle on each face.
    assert_eq!(viewport.sink().preview.lines.len(), 12);
    assert_eq!(viewport.sink().preview.markers.len(), 6);
    viewport.command(ViewCommand::ClipPlaneAdd {
        point: [1.5, 0.0, 0.0],
        normal: [1.0, 0.0, 0.0],
    });
    assert_eq!(viewport.sink().preview.lines.len(), 12 + 4 * 8 + 3);
    viewport.command(ViewCommand::SectionBox(false));
    viewport.command(ViewCommand::ClipPlanesClear);
    assert!(viewport.sink().preview.is_empty());
}

#[test]
fn clipped_points_are_not_snapped_to() {
    let mut viewport = categorised();
    viewport.set_tool(Tool::Move);
    viewport.command(ViewCommand::ClipPlaneAdd {
        point: [1.5, 0.0, 0.0],
        normal: [1.0, 0.0, 0.0],
    });
    // The corner of the clipped cube is no endpoint any more.
    hover_on(&mut viewport, CORNER_2);
    assert!(
        viewport
            .snap
            .is_none_or(|snap| snap.kind != Some(SnapKind::Endpoint))
    );
    hover_on(&mut viewport, CORNER_1);
    assert_eq!(
        viewport.snap.and_then(|snap| snap.kind),
        Some(SnapKind::Endpoint)
    );
}

#[test]
fn arming_a_pick_does_not_discard_a_tool_in_progress() {
    let mut viewport = categorised();
    viewport.set_tool(Tool::PlaceBlock);
    click_on(&mut viewport, [0.0, -0.5, 1.0], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::SecondCorner);
    viewport.command(ViewCommand::ClipPlanePick);
    assert_eq!(viewport.tool_step(), ToolStep::SecondCorner);
}

#[test]
fn a_box_turned_off_has_no_handle_to_grab() {
    let mut viewport = categorised();
    section_on(&mut viewport);
    let from = x_max_handle(&viewport);
    viewport.input(InputEvent::PointerMove {
        pos: from,
        mods: NONE,
    });
    assert!(viewport.sect.handle.is_some());
    viewport.command(ViewCommand::SectionBox(false));
    assert_eq!(viewport.sect.handle, None);
    let before = viewport.sect.clipping.section;
    viewport.input(InputEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: offset(from, -60.0, 0.0),
        mods: NONE,
    });
    assert_eq!(viewport.sect.clipping.section, before);
}

#[test]
fn a_delta_that_changes_nothing_the_panels_show_sends_no_view_state() {
    let mut viewport = categorised();
    viewport.drain_events();
    let delta = SceneDelta {
        revision: 2,
        base: 1,
        strings: vec!["Wall".to_owned(), "arch".to_owned()],
        elements: vec![element(1, 7, [0.0, 0.5, 0.5])],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&delta).unwrap();
    let sent = viewport
        .drain_events()
        .iter()
        .any(|event| matches!(event, ViewportEvent::ViewState(_)));
    assert!(!sent);
}
