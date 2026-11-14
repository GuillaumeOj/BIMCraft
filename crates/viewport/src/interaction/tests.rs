use render::{DisplayState, SnapKind};
use scene_wire::SceneDelta;

use super::*;
use crate::announce::INTERVAL;
use crate::event::{Preset, ProjectionMode};
use crate::math::norm;
use crate::test_support::{Call, FakeSink, cubes, element, id};

mod picking;
mod sectioning;

const SIZE: [f64; 2] = [400.0, 300.0];
const NONE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: false,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..NONE
};
const CTRL: Modifiers = Modifiers { ctrl: true, ..NONE };
const META: Modifiers = Modifiers { meta: true, ..NONE };
const ALT: Modifiers = Modifiers { alt: true, ..NONE };

/// The front top corner of cube 1, and the same corner of cube 2, 3 m east.
const CORNER_1: Vec3 = [0.5, -0.5, 1.0];
const CORNER_2: Vec3 = [3.5, -0.5, 1.0];

/// Two unit cubes on the ground, 3 m apart, framed.
fn viewport(ratio: f64) -> Viewport<FakeSink> {
    let mut viewport = Viewport::new(FakeSink::default(), SIZE, ratio);
    viewport
        .apply_delta(&cubes(1, [0.0; 3], &[[0.0, 0.0, 0.5], [3.0, 0.0, 0.5]]))
        .unwrap();
    viewport.command(ViewCommand::ZoomFit);
    viewport.drain_events();
    viewport.sink_mut().take_calls();
    viewport
}

/// The CSS pixel of a world point.
fn px(viewport: &Viewport<FakeSink>, point: Vec3) -> [f64; 2] {
    let view = View::new(&viewport.camera(), viewport.size());
    let [x, y, _] = view.project(sub(point, view.eye)).unwrap();
    [x / viewport.ratio, y / viewport.ratio]
}

fn offset(pos: [f64; 2], dx: f64, dy: f64) -> [f64; 2] {
    [pos[0] + dx, pos[1] + dy]
}

fn click(viewport: &mut Viewport<FakeSink>, pos: [f64; 2], mods: Modifiers) {
    viewport.input(InputEvent::PointerMove { pos, mods });
    viewport.input(InputEvent::PointerDown {
        pos,
        button: Button::Primary,
        mods,
    });
    viewport.input(InputEvent::PointerUp {
        pos,
        button: Button::Primary,
        mods,
    });
}

fn click_on(viewport: &mut Viewport<FakeSink>, point: Vec3, mods: Modifiers) {
    let pos = px(viewport, point);
    click(viewport, pos, mods);
}

fn hover_on(viewport: &mut Viewport<FakeSink>, point: Vec3) {
    let pos = px(viewport, point);
    hover(viewport, pos);
}

fn hover(viewport: &mut Viewport<FakeSink>, pos: [f64; 2]) {
    viewport.input(InputEvent::PointerMove { pos, mods: NONE });
}

fn key(viewport: &mut Viewport<FakeSink>, key: Key) {
    viewport.input(InputEvent::KeyDown { key, mods: NONE });
}

fn type_text(viewport: &mut Viewport<FakeSink>, text: &str) {
    for c in text.chars() {
        key(viewport, Key::Char(c));
    }
}

fn intents(events: &[ViewportEvent]) -> Vec<Intent> {
    events
        .iter()
        .filter_map(|event| match event {
            ViewportEvent::Intent(intent) => Some(intent.clone()),
            _ => None,
        })
        .collect()
}

/// What `events` and the frames of the next announcement window announce, snaps aside.
fn announced(viewport: &mut Viewport<FakeSink>, events: &[ViewportEvent]) -> Vec<AnnouncementKey> {
    for _ in 0..INTERVAL {
        viewport.tick();
    }
    events
        .iter()
        .chain(&viewport.drain_events())
        .filter_map(|event| match event {
            ViewportEvent::Announce(announcement) => Some(announcement.key),
            _ => None,
        })
        .filter(|key| !key.as_str().starts_with("snap."))
        .collect()
}

fn tool_states(events: &[ViewportEvent]) -> Vec<(Tool, ToolStep)> {
    events
        .iter()
        .filter_map(|event| match event {
            ViewportEvent::ToolState(tool, step) => Some((*tool, *step)),
            _ => None,
        })
        .collect()
}

fn announced_now(viewport: &mut Viewport<FakeSink>) -> Vec<AnnouncementKey> {
    let events = viewport.drain_events();
    announced(viewport, &events)
}

/// Lets the announcement window pass.
fn quiet(viewport: &mut Viewport<FakeSink>) {
    for _ in 0..INTERVAL {
        viewport.tick();
    }
    viewport.drain_events();
}

fn translation(intent: &Intent) -> Vec3 {
    match intent {
        Intent::Move { translation, .. } => *translation,
        Intent::PlaceBlock { .. } => panic!("not a move"),
    }
}

fn close(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    norm(sub(a, b)) <= tolerance
}

// Scene.

#[test]
fn scene_bytes_are_applied_and_bad_ones_change_nothing() {
    let mut viewport = Viewport::new(FakeSink::default(), SIZE, 1.0);
    assert_eq!(viewport.revision(), None);
    let bytes = scene_wire::encode(&cubes(4, [0.0; 3], &[[0.0; 3]]));
    assert_eq!(viewport.apply_scene(&bytes), Ok(4));
    assert_eq!(viewport.revision(), Some(4));
    assert!(matches!(
        viewport.apply_scene(&bytes[..10]),
        Err(SceneError::Decode(_))
    ));
    let stale = SceneDelta {
        revision: 9,
        base: 8,
        ..SceneDelta::default()
    };
    assert_eq!(viewport.apply_delta(&stale), Err(SceneError::BaseMismatch));
    assert_eq!(viewport.revision(), Some(4));
    assert!(
        viewport
            .drain_events()
            .iter()
            .all(|e| !matches!(e, ViewportEvent::Error(_)))
    );
}

#[test]
fn a_scene_the_renderer_refuses_is_reported_and_dropped() {
    let mut viewport = viewport(1.0);
    click_on(&mut viewport, CORNER_1, NONE);
    assert_eq!(viewport.selection(), [id(1)]);
    viewport.drain_events();
    viewport.sink_mut().objects_before_failure = Some(0);
    let mut delta = SceneDelta {
        revision: 2,
        base: 1,
        ..SceneDelta::default()
    };
    delta.elements = vec![element(3, 7, [0.0; 3])];
    assert!(matches!(
        viewport.apply_delta(&delta),
        Err(SceneError::Render(_))
    ));
    let events = viewport.drain_events();
    assert!(events.contains(&ViewportEvent::Error(ErrorKey::SceneRejected)));
    assert!(events.contains(&ViewportEvent::Selection(Vec::new())));
    assert_eq!(viewport.revision(), None);
    assert!(viewport.selection().is_empty());
}

#[test]
fn removed_elements_leave_the_selection_and_the_hover() {
    let mut viewport = viewport(1.0);
    click_on(&mut viewport, CORNER_1, NONE);
    viewport.drain_events();
    let delta = SceneDelta {
        revision: 2,
        base: 1,
        removed_elements: vec![id(1).0],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&delta).unwrap();
    let events = viewport.drain_events();
    assert!(events.contains(&ViewportEvent::Selection(Vec::new())));
    assert!(events.contains(&ViewportEvent::Hover(None)));
    assert!(viewport.selection().is_empty());
}

// Selection.

#[test]
fn clicks_select_add_toggle_and_clear() {
    let mut viewport = viewport(1.0);
    let (one, two) = (
        px(&viewport, [0.0, -0.5, 0.5]),
        px(&viewport, [3.0, -0.5, 0.5]),
    );
    click(&mut viewport, one, NONE);
    let events = viewport.drain_events();
    assert!(events.contains(&ViewportEvent::Selection(vec![id(1)])));
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::SelectionCount]
    );
    assert_eq!(
        viewport.sink().states.get(&id(1)),
        Some(&DisplayState::Selected)
    );
    click(&mut viewport, two, SHIFT);
    assert_eq!(viewport.selection(), [id(1), id(2)]);
    click(&mut viewport, two, SHIFT);
    assert_eq!(
        viewport.selection(),
        [id(1), id(2)],
        "Shift adds, never removes"
    );
    click(&mut viewport, one, CTRL);
    assert_eq!(viewport.selection(), [id(2)]);
    click(&mut viewport, one, META);
    assert_eq!(viewport.selection(), [id(2), id(1)]);
    // Shift or Ctrl on nothing keeps the selection; a plain click on nothing clears it.
    click(&mut viewport, [2.0, 2.0], SHIFT);
    click(&mut viewport, [2.0, 2.0], CTRL);
    assert_eq!(viewport.selection().len(), 2);
    quiet(&mut viewport);
    click(&mut viewport, [2.0, 2.0], NONE);
    assert!(viewport.selection().is_empty());
    assert_eq!(
        announced_now(&mut viewport),
        [AnnouncementKey::SelectionCleared]
    );
    assert!(viewport.sink().states.is_empty());
}

#[test]
fn hover_follows_the_pointer_and_is_not_announced() {
    let mut viewport = viewport(1.0);
    hover_on(&mut viewport, [3.0, -0.5, 0.5]);
    let events = viewport.drain_events();
    assert_eq!(events, [ViewportEvent::Hover(Some(id(2)))]);
    assert_eq!(
        viewport.sink().states.get(&id(2)),
        Some(&DisplayState::Hovered)
    );
    let beside = offset(px(&viewport, [3.0, -0.5, 0.5]), 1.0, 0.0);
    hover(&mut viewport, beside);
    assert!(viewport.drain_events().is_empty());
    viewport.input(InputEvent::PointerLeave);
    assert_eq!(viewport.drain_events(), [ViewportEvent::Hover(None)]);
    assert!(viewport.sink().states.is_empty());
}

#[test]
fn esc_and_commands_clear_or_set_the_selection() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(2), id(9), id(2), id(1)]));
    assert_eq!(viewport.selection(), [id(2), id(1)]);
    viewport.command(ViewCommand::Select(vec![id(2), id(1)]));
    let events = viewport.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, ViewportEvent::Selection(_)))
            .count(),
        1,
        "the same selection again is no change"
    );
    key(&mut viewport, Key::Escape);
    assert!(viewport.selection().is_empty());
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.command(ViewCommand::ClearSelection);
    assert!(viewport.selection().is_empty());
}

#[test]
fn a_press_that_barely_moves_is_a_click() {
    let mut viewport = viewport(2.0);
    let on = px(&viewport, [0.0, -0.5, 0.5]);
    viewport.input(InputEvent::PointerDown {
        pos: on,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: offset(on, 3.0, 0.0),
        mods: NONE,
    });
    viewport.input(InputEvent::PointerUp {
        pos: offset(on, 3.0, 0.0),
        button: Button::Primary,
        mods: NONE,
    });
    assert_eq!(viewport.selection(), [id(1)]);
    assert!(intents(&viewport.drain_events()).is_empty());
}

// Moves.

#[test]
fn dragging_an_element_moves_it_with_snapping() {
    let mut viewport = viewport(1.0);
    let (from, to) = (px(&viewport, CORNER_1), px(&viewport, CORNER_2));
    viewport.input(InputEvent::PointerMove {
        pos: from,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: offset(from, 20.0, 0.0),
        mods: NONE,
    });
    // An unselected element is selected first, and drawn as a ghost at its place.
    assert_eq!(viewport.selection(), [id(1)]);
    assert_eq!(
        viewport.sink().states.get(&id(1)),
        Some(&DisplayState::Ghost)
    );
    viewport.input(InputEvent::PointerMove {
        pos: to,
        mods: NONE,
    });
    let preview = viewport.sink().preview.clone();
    assert_eq!(preview.ghosts.len(), 1);
    assert_eq!(
        preview.markers.first().map(|m| m.1),
        Some(SnapKind::Endpoint)
    );
    assert_eq!(preview.lines.len(), 1);
    viewport.input(InputEvent::PointerUp {
        pos: to,
        button: Button::Primary,
        mods: NONE,
    });
    let events = viewport.drain_events();
    let moves = intents(&events);
    assert_eq!(moves.len(), 1);
    assert!(
        close(translation(&moves[0]), [3.0, 0.0, 0.0], 1e-9),
        "{moves:?}"
    );
    assert!(tool_states(&events).contains(&(Tool::Select, ToolStep::TargetPoint)));
    assert_eq!(
        tool_states(&events).last(),
        Some(&(Tool::Select, ToolStep::Idle))
    );
    // The ghost waits for the scene; the delta that moves the element replaces it.
    assert_eq!(viewport.sink().preview.ghosts.len(), 1);
    let delta = SceneDelta {
        revision: 2,
        base: 1,
        elements: vec![element(1, 7, [3.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&delta).unwrap();
    assert!(viewport.sink().preview.ghosts.is_empty());
    assert_eq!(
        viewport.sink().states.get(&id(1)),
        Some(&DisplayState::Selected)
    );
}

#[test]
fn a_drag_on_nothing_does_nothing_and_shift_drag_pans() {
    let mut viewport = viewport(1.0);
    let camera = viewport.camera();
    let empty = [5.0, 5.0];
    viewport.input(InputEvent::PointerDown {
        pos: empty,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: [60.0, 5.0],
        mods: NONE,
    });
    viewport.input(InputEvent::PointerUp {
        pos: [60.0, 5.0],
        button: Button::Primary,
        mods: NONE,
    });
    assert_eq!(viewport.camera(), camera);
    assert!(intents(&viewport.drain_events()).is_empty());
    let on = px(&viewport, [0.0, -0.5, 0.5]);
    viewport.input(InputEvent::PointerDown {
        pos: on,
        button: Button::Primary,
        mods: SHIFT,
    });
    viewport.input(InputEvent::PointerMove {
        pos: offset(on, 30.0, 0.0),
        mods: SHIFT,
    });
    viewport.input(InputEvent::PointerUp {
        pos: offset(on, 30.0, 0.0),
        button: Button::Primary,
        mods: SHIFT,
    });
    assert_ne!(viewport.camera(), camera);
    assert!(viewport.selection().is_empty());
    // The grabbed point stays under the pointer.
    let grabbed = px(&viewport, [0.0, -0.5, 0.5]);
    assert!((grabbed[0] - on[0] - 30.0).abs() < 1e-6 && (grabbed[1] - on[1]).abs() < 1e-6);
}

#[test]
fn the_move_tool_takes_a_base_a_target_and_typed_distances() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::Move);
    assert_eq!(
        tool_states(&viewport.drain_events()),
        [(Tool::Move, ToolStep::Idle)]
    );
    // Without a selection, clicks do nothing.
    click_on(&mut viewport, CORNER_1, NONE);
    assert!(viewport.selection().is_empty());
    viewport.command(ViewCommand::Select(vec![id(1)]));
    assert_eq!(
        tool_states(&viewport.drain_events()),
        [(Tool::Move, ToolStep::BasePoint)]
    );
    click_on(&mut viewport, CORNER_1, NONE);
    assert_eq!(
        tool_states(&viewport.drain_events()),
        [(Tool::Move, ToolStep::TargetPoint)]
    );
    // Towards the other cube's corner: the direction is +x; 2.5 typed, Enter.
    hover_on(&mut viewport, CORNER_2);
    type_text(&mut viewport, "2.5");
    let label = viewport
        .drain_events()
        .into_iter()
        .filter_map(|event| match event {
            ViewportEvent::PreviewLabel(label) => label,
            _ => None,
        })
        .next_back()
        .unwrap();
    assert_eq!(label.key, LabelKey::MoveDistance);
    assert_eq!(label.typed.as_deref(), Some("2.5"));
    assert_eq!(label.values, [("distance", Value::Number(2.5))]);
    key(&mut viewport, Key::Enter);
    let events = viewport.drain_events();
    let moves = intents(&events);
    assert!(
        close(translation(&moves[0]), [2.5, 0.0, 0.0], 1e-9),
        "{moves:?}"
    );
    assert_eq!(tool_states(&events), [(Tool::Move, ToolStep::BasePoint)]);
    assert!(events.contains(&ViewportEvent::PreviewLabel(None)));
    // A typed distance from the panel works the same; a click on the target too.
    quiet(&mut viewport);
    click_on(&mut viewport, CORNER_1, NONE);
    hover_on(&mut viewport, CORNER_2);
    viewport.command(ViewCommand::TypedDistance(1.0));
    assert!(close(
        translation(&intents(&viewport.drain_events())[0]),
        [1.0, 0.0, 0.0],
        1e-9
    ));
    click_on(&mut viewport, CORNER_1, NONE);
    click_on(&mut viewport, CORNER_2, NONE);
    let events = viewport.drain_events();
    assert!(close(
        translation(&intents(&events)[0]),
        [3.0, 0.0, 0.0],
        1e-9
    ));
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::MoveDistance]
    );
}

#[test]
fn bad_typed_values_are_ignored() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    hover_on(&mut viewport, CORNER_2);
    viewport.drain_events();
    for bad in [0.0, -1.0, f64::NAN, 2e6] {
        viewport.command(ViewCommand::TypedDistance(bad));
    }
    // A lone point is no number; Backspace edits.
    type_text(&mut viewport, ".");
    key(&mut viewport, Key::Enter);
    key(&mut viewport, Key::Backspace);
    key(&mut viewport, Key::Backspace);
    // Letters are not typed values.
    key(&mut viewport, Key::Char('x'));
    assert!(intents(&viewport.drain_events()).is_empty());
}

#[test]
fn esc_steps_back_one_step_at_a_time() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    quiet(&mut viewport);
    key(&mut viewport, Key::Escape);
    let events = viewport.drain_events();
    assert_eq!(tool_states(&events), [(Tool::Move, ToolStep::BasePoint)]);
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::MoveCancelled]
    );
    key(&mut viewport, Key::Escape);
    assert_eq!(
        tool_states(&viewport.drain_events()),
        [(Tool::Select, ToolStep::Idle)]
    );
    assert_eq!(viewport.selection(), [id(1)]);
    viewport.command(ViewCommand::Cancel);
    assert!(viewport.selection().is_empty());
}

#[test]
fn a_waiting_preview_vanishes_after_its_frames_or_on_esc() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    click_on(&mut viewport, CORNER_2, NONE);
    assert_eq!(viewport.sink().preview.ghosts.len(), 1);
    for _ in 1..PENDING_FRAMES {
        assert!(viewport.tick().again);
    }
    assert_eq!(viewport.sink().preview.ghosts.len(), 1);
    let tick = viewport.tick();
    assert!(tick.draw);
    assert!(viewport.sink().preview.ghosts.is_empty());
    assert!(!viewport.tick().again);
    // Esc drops it at once, before the selection.
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    click_on(&mut viewport, CORNER_2, NONE);
    viewport.set_tool(Tool::Select);
    key(&mut viewport, Key::Escape);
    assert!(viewport.sink().preview.ghosts.is_empty());
    assert_eq!(viewport.selection(), [id(1)]);
}

#[test]
fn ghosts_the_device_cannot_hold_are_dropped_quietly() {
    let mut viewport = viewport(1.0);
    viewport.sink_mut().max_ghosts = Some(0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    hover_on(&mut viewport, CORNER_2);
    let preview = viewport.sink().preview.clone();
    assert!(preview.ghosts.is_empty());
    assert_eq!(preview.lines.len(), 1, "the rubber band stays");
    assert_ne!(
        viewport.sink().states.get(&id(1)),
        Some(&DisplayState::Ghost)
    );
    click_on(&mut viewport, CORNER_2, NONE);
    let events = viewport.drain_events();
    assert_eq!(intents(&events).len(), 1);
    assert!(events.iter().all(|e| !matches!(e, ViewportEvent::Error(_))));
}

#[test]
fn alt_suspends_snapping_until_released() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    viewport.drain_events();
    let near_corner = offset(px(&viewport, CORNER_1), -3.0, 3.0);
    viewport.input(InputEvent::PointerMove {
        pos: near_corner,
        mods: ALT,
    });
    let snapping = |events: &[ViewportEvent]| {
        events.iter().find_map(|e| match e {
            ViewportEvent::ViewState(state) => Some(state.snapping),
            _ => None,
        })
    };
    assert_eq!(snapping(&viewport.drain_events()), Some(false));
    assert!(viewport.sink().preview.markers.is_empty());
    viewport.input(InputEvent::KeyUp {
        key: Key::Alt,
        mods: NONE,
    });
    assert_eq!(snapping(&viewport.drain_events()), Some(true));
    assert_eq!(
        viewport.sink().preview.markers.first().map(|m| m.1),
        Some(SnapKind::Endpoint)
    );
    viewport.input(InputEvent::KeyDown {
        key: Key::Alt,
        mods: ALT,
    });
    assert_eq!(snapping(&viewport.drain_events()), Some(false));
    viewport.input(InputEvent::Blur);
    assert_eq!(snapping(&viewport.drain_events()), Some(true));
    viewport.command(ViewCommand::Snapping(false));
    assert_eq!(snapping(&viewport.drain_events()), Some(false));
    assert!(viewport.sink().preview.markers.is_empty());
}

// Blocks.

#[test]
fn a_block_is_placed_with_two_corners_and_a_height() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    assert_eq!(
        tool_states(&viewport.drain_events()),
        [(Tool::PlaceBlock, ToolStep::FirstCorner)]
    );
    let (a, b) = ([-3.0, -3.0, 0.0], [-1.0, -2.0, 0.0]);
    click_on(&mut viewport, a, NONE);
    hover_on(&mut viewport, b);
    assert_eq!(viewport.sink().preview.lines.len(), 4);
    let label = viewport.drain_events().into_iter().find_map(|e| match e {
        ViewportEvent::PreviewLabel(Some(label)) => Some(label),
        _ => None,
    });
    assert_eq!(label.map(|l| l.key), Some(LabelKey::PlaceSize));
    click_on(&mut viewport, b, NONE);
    let events = viewport.drain_events();
    assert_eq!(tool_states(&events), [(Tool::PlaceBlock, ToolStep::Height)]);
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::PlaceSize]
    );
    // Enter at once: the default height.
    quiet(&mut viewport);
    key(&mut viewport, Key::Enter);
    let events = viewport.drain_events();
    let Intent::PlaceBlock {
        location,
        rotation,
        size,
    } = intents(&events)[0]
    else {
        panic!("{events:?}");
    };
    assert!(close(location, [-2.0, -2.5, 0.0], 1e-6), "{location:?}");
    assert!(close(size, [2.0, 1.0, 3.0], 1e-6), "{size:?}");
    assert!(rotation.abs() < f64::EPSILON);
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::PlaceHeight]
    );
    assert_eq!(
        tool_states(&events),
        [(Tool::PlaceBlock, ToolStep::FirstCorner)]
    );
}

#[test]
fn the_label_of_a_typed_block_goes_from_size_to_depth_to_height() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    click_on(&mut viewport, [-3.0, -3.0, 0.0], NONE);
    hover_on(&mut viewport, [-4.0, -2.0, 0.0]);
    let last_label = |viewport: &mut Viewport<FakeSink>| {
        viewport
            .drain_events()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                ViewportEvent::PreviewLabel(label) => Some(label),
                _ => None,
            })
            .and_then(|label| label.map(|label| label.key))
    };
    assert_eq!(last_label(&mut viewport), Some(LabelKey::PlaceSize));
    type_text(&mut viewport, "4");
    key(&mut viewport, Key::Enter);
    assert_eq!(last_label(&mut viewport), Some(LabelKey::PlaceDepth));
    type_text(&mut viewport, "2");
    key(&mut viewport, Key::Enter);
    assert_eq!(last_label(&mut viewport), Some(LabelKey::PlaceHeight));
}

#[test]
fn the_depth_follows_the_pointer_once_the_width_is_confirmed() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    click_on(&mut viewport, [-3.0, -3.0, 0.0], NONE);
    hover_on(&mut viewport, [-1.0, -2.0, 0.0]);
    type_text(&mut viewport, "4");
    key(&mut viewport, Key::Enter);
    let label = |viewport: &mut Viewport<FakeSink>| {
        viewport
            .drain_events()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                ViewportEvent::PreviewLabel(Some(label)) => Some(label),
                _ => None,
            })
            .unwrap()
    };
    let size = |label: &PreviewLabel| {
        label
            .values
            .iter()
            .map(|(_, value)| match value {
                Value::Number(n) => *n,
                Value::Text(_) => f64::NAN,
            })
            .collect::<Vec<_>>()
    };
    hover_on(&mut viewport, [-1.0, -1.0, 0.0]);
    let first = label(&mut viewport);
    hover_on(&mut viewport, [-1.0, 0.0, 0.0]);
    let second = label(&mut viewport);
    assert_eq!(
        (first.key, second.key),
        (LabelKey::PlaceDepth, LabelKey::PlaceDepth)
    );
    // The width stays at the confirmed 4 m; the depth follows the pointer.
    let (a, b) = (size(&first), size(&second));
    assert!((a[0] - 4.0).abs() < 1e-9 && (b[0] - 4.0).abs() < 1e-9);
    assert!(b[1] > a[1], "{a:?} {b:?}");
}

#[test]
fn escape_goes_back_one_step_at_a_time_while_placing_a_block() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    let first = [-3.0, -3.0, 0.0];
    click_on(&mut viewport, first, NONE);
    hover_on(&mut viewport, [-1.0, -2.0, 0.0]);
    type_text(&mut viewport, "4");
    key(&mut viewport, Key::Enter);
    type_text(&mut viewport, "2");
    // Esc empties the typed depth, then returns to the width, then to the first corner.
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::SecondCorner);
    assert!(viewport.typed.text().is_none());
    key(&mut viewport, Key::Escape);
    assert!(matches!(
        viewport.step,
        Step::SecondCorner { width: None, .. }
    ));
    let label = viewport
        .drain_events()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            ViewportEvent::PreviewLabel(Some(label)) => Some(label.key),
            _ => None,
        });
    assert_eq!(label, Some(LabelKey::PlaceSize));
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::FirstCorner);
    // From the height step: the typed value first, then the second corner.
    click_on(&mut viewport, first, NONE);
    hover_on(&mut viewport, [-1.0, -2.0, 0.0]);
    click_on(&mut viewport, [-1.0, -2.0, 0.0], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::Height);
    type_text(&mut viewport, "2");
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::Height);
    key(&mut viewport, Key::Escape);
    assert!(matches!(
        viewport.step,
        Step::SecondCorner { width: None, .. }
    ));
    // At the first corner, Esc leaves the tool.
    key(&mut viewport, Key::Escape);
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    assert_eq!(viewport.tool, Tool::Select);
}

#[test]
fn a_block_can_be_typed() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    let a = [-3.0, -3.0, 0.0];
    click_on(&mut viewport, a, NONE);
    // The pointer towards −x and +y: the typed sides follow it.
    hover_on(&mut viewport, [-4.0, -2.0, 0.0]);
    type_text(&mut viewport, "4");
    key(&mut viewport, Key::Enter);
    type_text(&mut viewport, "2");
    key(&mut viewport, Key::Enter);
    assert_eq!(
        tool_states(&viewport.drain_events()).last(),
        Some(&(Tool::PlaceBlock, ToolStep::Height))
    );
    type_text(&mut viewport, "2.5");
    assert_eq!(viewport.sink().preview.boxes.len(), 1);
    key(&mut viewport, Key::Enter);
    let Intent::PlaceBlock { location, size, .. } = intents(&viewport.drain_events())[0] else {
        panic!("not a block");
    };
    assert!(close(location, [-5.0, -2.0, 0.0], 1e-6), "{location:?}");
    assert!(close(size, [4.0, 2.0, 2.5], 1e-9), "{size:?}");
}

#[test]
fn the_height_follows_the_pointer() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    let (a, b) = ([-3.0, -3.0, 0.0], [-1.0, -2.0, 0.0]);
    click_on(&mut viewport, a, NONE);
    click_on(&mut viewport, b, NONE);
    click_on(&mut viewport, [-1.0, -2.0, 1.5], NONE);
    let Intent::PlaceBlock { size, .. } = intents(&viewport.drain_events())[0] else {
        panic!("not a block");
    };
    assert!((size[2] - 1.5).abs() < 1e-6, "{size:?}");
}

#[test]
fn a_flat_block_is_refused_and_esc_steps_back() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    let a = px(&viewport, [-3.0, -3.0, 0.0]);
    click(&mut viewport, a, NONE);
    click(&mut viewport, a, NONE);
    assert_eq!(viewport.tool_step(), ToolStep::SecondCorner);
    click_on(&mut viewport, [-1.0, -2.0, 0.0], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::Height);
    type_text(&mut viewport, "0");
    key(&mut viewport, Key::Enter);
    assert!(intents(&viewport.drain_events()).is_empty());
    // The refused value is still typed: Esc empties it first, then steps back.
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::Height);
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::SecondCorner);
    key(&mut viewport, Key::Escape);
    assert_eq!(viewport.tool_step(), ToolStep::FirstCorner);
    key(&mut viewport, Key::Escape);
    assert_eq!(
        (viewport.tool, viewport.tool_step()),
        (Tool::Select, ToolStep::Idle)
    );
}

// Camera.

#[test]
fn pointer_and_keys_move_the_camera() {
    let mut viewport = viewport(1.0);
    let start = viewport.camera();
    // Right-drag orbits around the point grabbed, which keeps its pixel.
    let grabbed = px(&viewport, [0.0, -0.5, 0.5]);
    viewport.input(InputEvent::PointerDown {
        pos: grabbed,
        button: Button::Secondary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: offset(grabbed, 40.0, 10.0),
        mods: NONE,
    });
    viewport.input(InputEvent::PointerUp {
        pos: offset(grabbed, 40.0, 10.0),
        button: Button::Secondary,
        mods: NONE,
    });
    assert_ne!(viewport.camera(), start);
    let after = px(&viewport, [0.0, -0.5, 0.5]);
    assert!((after[0] - grabbed[0]).abs() < 1e-6 && (after[1] - grabbed[1]).abs() < 1e-6);
    // The wheel zooms towards the point under the pointer.
    let distance = viewport.camera.distance();
    viewport.input(InputEvent::Wheel {
        pos: after,
        delta: [0.0, -100.0],
        pan: false,
        mods: NONE,
    });
    assert!(viewport.camera.distance() < distance);
    let zoomed = px(&viewport, [0.0, -0.5, 0.5]);
    assert!((zoomed[0] - after[0]).abs() < 1e-6 && (zoomed[1] - after[1]).abs() < 1e-6);
    viewport.input(InputEvent::Pinch {
        pos: [10.0, 10.0],
        scale: 0.5,
    });
    viewport.input(InputEvent::Pinch {
        pos: [10.0, 10.0],
        scale: f64::NAN,
    });
    viewport.input(InputEvent::Wheel {
        pos: [10.0, 10.0],
        delta: [5.0, 5.0],
        pan: true,
        mods: NONE,
    });
    // Middle-drag pans.
    let before = viewport.camera();
    viewport.input(InputEvent::PointerDown {
        pos: [10.0, 10.0],
        button: Button::Middle,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: [30.0, 10.0],
        mods: NONE,
    });
    assert_ne!(viewport.camera(), before);
    // Keys: arrows orbit, Shift+arrows pan, + and - zoom, F frames.
    for k in [
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
        Key::ArrowDown,
    ] {
        let before = viewport.camera();
        key(&mut viewport, k);
        assert_ne!(viewport.camera(), before, "{k:?}");
        viewport.input(InputEvent::KeyDown {
            key: k,
            mods: SHIFT,
        });
    }
    let distance = viewport.camera.distance();
    key(&mut viewport, Key::Char('+'));
    assert!(viewport.camera.distance() < distance);
    key(&mut viewport, Key::Char('-'));
    assert!((viewport.camera.distance() - distance).abs() < 1e-9 * distance);
    key(&mut viewport, Key::Char('f'));
    assert!(viewport.tick().draw);
    key(&mut viewport, Key::Other);
    assert!(!viewport.tick().draw);
}

#[test]
fn zoom_to_selection_needs_a_selection() {
    let mut viewport = viewport(1.0);
    let camera = viewport.camera();
    viewport.input(InputEvent::KeyDown {
        key: Key::Char('F'),
        mods: SHIFT,
    });
    assert_eq!(viewport.camera(), camera);
    viewport.command(ViewCommand::Select(vec![id(2)]));
    viewport.command(ViewCommand::ZoomSelection);
    assert!((viewport.camera.target()[0] - 3.0).abs() < 1e-9);
    // An empty scene frames nothing.
    let mut empty = Viewport::new(FakeSink::default(), SIZE, 1.0);
    let camera = empty.camera();
    empty.command(ViewCommand::ZoomFit);
    assert_eq!(empty.camera(), camera);
}

#[test]
fn presets_and_projections_are_announced_and_mirrored() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::ViewPreset(Preset::Top));
    let events = viewport.drain_events();
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::ViewPreset]
    );
    quiet(&mut viewport);
    viewport.command(ViewCommand::Projection(ProjectionMode::Parallel));
    let events = viewport.drain_events();
    assert_eq!(
        announced(&mut viewport, &events),
        [AnnouncementKey::ViewProjection]
    );
    let state = events
        .iter()
        .find_map(|event| match event {
            ViewportEvent::ViewState(state) => Some(state),
            _ => None,
        })
        .expect("a view state");
    assert_eq!(
        (state.projection, state.snapping, state.tool),
        (ProjectionMode::Parallel, true, Tool::Select)
    );
}

#[test]
fn frames_are_drawn_only_when_something_changed() {
    let mut viewport = viewport(1.0);
    viewport.tick();
    assert_eq!(
        viewport.tick(),
        Tick {
            draw: false,
            again: false
        }
    );
    viewport.resize([800.0, 600.0], 2.0);
    assert_eq!(viewport.size(), [1600, 1200]);
    assert!(viewport.tick().draw);
    viewport.set_theme(Theme::dark());
    assert_eq!(viewport.sink_mut().take_calls(), [Call::Theme]);
    assert!(viewport.tick().draw);
    viewport.resize([f64::NAN, 1e9], f64::NAN);
    assert_eq!(viewport.size(), [1, u32::from(u16::MAX)]);
}

// Regressions.

#[test]
fn a_refused_typed_height_places_nothing() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    click_on(&mut viewport, [-3.0, -3.0, 0.0], NONE);
    click_on(&mut viewport, [-1.0, -2.0, 0.0], NONE);
    assert_eq!(viewport.tool_step(), ToolStep::Height);
    for bad in [0.0, -1.0, f64::NAN, 2e6] {
        viewport.command(ViewCommand::TypedDistance(bad));
    }
    assert!(intents(&viewport.drain_events()).is_empty());
    assert_eq!(viewport.tool_step(), ToolStep::Height);
}

#[test]
fn the_current_projection_again_keeps_the_view() {
    let mut viewport = viewport(1.0);
    key(&mut viewport, Key::Char('+'));
    let camera = viewport.camera();
    viewport.command(ViewCommand::Projection(ProjectionMode::Perspective));
    assert_eq!(viewport.camera(), camera);
    viewport.command(ViewCommand::Projection(ProjectionMode::Parallel));
    key(&mut viewport, Key::Char('+'));
    let camera = viewport.camera();
    viewport.command(ViewCommand::Projection(ProjectionMode::Parallel));
    assert_eq!(viewport.camera(), camera);
}

#[test]
fn a_parallel_zoom_over_nothing_keeps_the_target_depth() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Projection(ProjectionMode::Parallel));
    let (target, forward) = (viewport.camera.target(), viewport.camera.forward());
    viewport.input(InputEvent::Wheel {
        pos: [5.0, 5.0],
        delta: [0.0, -100.0],
        pan: false,
        mods: NONE,
    });
    assert!(norm(sub(viewport.camera.target(), target)) > 1e-3);
    assert!(dot(sub(viewport.camera.target(), target), forward).abs() < 1e-9);
}

#[test]
fn a_moved_element_no_longer_shown_as_a_ghost_is_drawn_normally() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    click_on(&mut viewport, CORNER_2, NONE);
    viewport.set_tool(Tool::Select);
    // The selection goes while the move waits for the scene: the ghost stays.
    viewport.command(ViewCommand::ClearSelection);
    assert_eq!(
        viewport.sink().states.get(&id(1)),
        Some(&DisplayState::Ghost)
    );
    let delta = SceneDelta {
        revision: 2,
        base: 1,
        elements: vec![element(1, 7, [3.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&delta).unwrap();
    assert_eq!(viewport.sink().states.get(&id(1)), None);
}

#[test]
fn a_refused_scene_clears_the_preview_and_the_waiting_move() {
    let mut viewport = viewport(1.0);
    viewport.command(ViewCommand::Select(vec![id(1)]));
    viewport.set_tool(Tool::Move);
    click_on(&mut viewport, CORNER_1, NONE);
    click_on(&mut viewport, CORNER_2, NONE);
    viewport.set_tool(Tool::Select);
    viewport.command(ViewCommand::ClearSelection);
    assert!(!viewport.sink().preview.is_empty());
    viewport.sink_mut().objects_before_failure = Some(0);
    let delta = SceneDelta {
        revision: 2,
        base: 1,
        elements: vec![element(3, 7, [0.0; 3])],
        ..SceneDelta::default()
    };
    assert!(viewport.apply_delta(&delta).is_err());
    assert!(viewport.sink().preview.is_empty());
    assert_eq!(viewport.pending, None);
}

#[test]
fn a_drag_cut_by_a_blur_is_cancelled_and_leaves_no_marker() {
    let mut viewport = viewport(1.0);
    let (from, to) = (px(&viewport, CORNER_1), px(&viewport, CORNER_2));
    let press = |viewport: &mut Viewport<FakeSink>| {
        viewport.input(InputEvent::PointerMove {
            pos: from,
            mods: NONE,
        });
        viewport.input(InputEvent::PointerDown {
            pos: from,
            button: Button::Primary,
            mods: NONE,
        });
        viewport.input(InputEvent::PointerMove {
            pos: to,
            mods: NONE,
        });
    };
    press(&mut viewport);
    assert_eq!(viewport.tool_step(), ToolStep::TargetPoint);
    viewport.input(InputEvent::Blur);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    click(&mut viewport, offset(to, 1.0, 0.0), NONE);
    assert!(intents(&viewport.drain_events()).is_empty());
    // A drag that ends leaves no snap marker behind in the Select tool.
    press(&mut viewport);
    viewport.input(InputEvent::PointerUp {
        pos: to,
        button: Button::Primary,
        mods: NONE,
    });
    assert_eq!(intents(&viewport.drain_events()).len(), 1);
    assert!(viewport.sink().preview.markers.is_empty());
}

#[test]
fn a_pointer_leave_mid_drag_cancels_without_an_intent() {
    let mut viewport = viewport(1.0);
    let (from, to) = (px(&viewport, CORNER_1), px(&viewport, CORNER_2));
    viewport.input(InputEvent::PointerMove {
        pos: from,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerMove {
        pos: to,
        mods: NONE,
    });
    assert_eq!(viewport.tool_step(), ToolStep::TargetPoint);
    viewport.input(InputEvent::PointerLeave);
    assert_eq!(viewport.tool_step(), ToolStep::Idle);
    viewport.input(InputEvent::PointerUp {
        pos: to,
        button: Button::Primary,
        mods: NONE,
    });
    assert!(intents(&viewport.drain_events()).is_empty());
}

#[test]
fn a_pointer_leave_after_a_press_makes_the_release_no_click() {
    let mut viewport = viewport(1.0);
    let at = px(&viewport, CORNER_1);
    viewport.input(InputEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        mods: NONE,
    });
    viewport.input(InputEvent::PointerLeave);
    viewport.input(InputEvent::PointerUp {
        pos: at,
        button: Button::Primary,
        mods: NONE,
    });
    assert!(viewport.selection().is_empty());
}

#[test]
fn a_raised_footprint_follows_the_pointer_over_nothing() {
    let mut viewport = viewport(1.0);
    viewport.set_tool(Tool::PlaceBlock);
    click_on(&mut viewport, [0.0, 0.0, 1.0], NONE);
    let ToolStep::SecondCorner = viewport.tool_step() else {
        panic!("no first corner");
    };
    let corner = [-2.0, -3.0, 1.0];
    hover_on(&mut viewport, corner);
    let lines = &viewport.sink().preview.lines;
    assert_eq!(lines.len(), 4);
    assert!(close(lines[2].from, corner, 1e-6), "{lines:?}");
}
