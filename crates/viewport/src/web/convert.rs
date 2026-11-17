//! Plain JavaScript values to and from the viewport's types, with the field names of
//! `api-contract.md` §2. Everything that comes from JavaScript is hostile: a bad value is an
//! error for the caller, never a panic.

use js_sys::{Array, Object, Reflect};
use render::{ClipPlane, CutFaces, Rgb, Theme};
use wasm_bindgen::{JsCast, JsError, JsValue};

use crate::{
    Announcement, ElementId, Intent, NameState, Preset, PreviewLabel, ProjectionMode, Tool,
    ToolStep, Value, ViewCommand, ViewState, ViewportEvent, VisibilityTarget,
};

pub(super) type Parsed<T> = Result<T, JsError>;

/// The most ids one `select` command may carry: far above any real selection, so a hostile call
/// cannot make the viewport allocate without bound.
const MAX_IDS: u32 = 1 << 20;

/// The most names one visibility command may carry, and the longest name: the categories and
/// disciplines of a schema are a few dozen short words.
const MAX_NAMES: u32 = 4096;
const MAX_NAME_LEN: usize = 256;

/// The highest value JavaScript holds exactly as a number.
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

fn bad(what: &str) -> JsError {
    JsError::new(what)
}

fn field(object: &JsValue, key: &str) -> Parsed<JsValue> {
    if !object.is_object() {
        return Err(bad("expected an object"));
    }
    Reflect::get(object, &JsValue::from_str(key)).map_err(|_| bad("unreadable field"))
}

fn text(object: &JsValue, key: &str) -> Parsed<String> {
    field(object, key)?
        .as_string()
        .ok_or_else(|| bad("expected a string field"))
}

fn number(value: &JsValue) -> Parsed<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("expected a finite number"))
}

fn boolean(object: &JsValue, key: &str) -> Parsed<bool> {
    field(object, key)?
        .as_bool()
        .ok_or_else(|| bad("expected a boolean field"))
}

/// An id in its canonical form: 32 hexadecimal digits of the 128-bit value, which the scene
/// channel carries as 16 bytes little-endian.
pub(super) fn id_string(id: ElementId) -> String {
    format!("{:032x}", u128::from_le_bytes(id.0))
}

fn parse_id(value: &JsValue) -> Parsed<ElementId> {
    let text = value.as_string().ok_or_else(|| bad("an id is a string"))?;
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("an id has 32 hexadecimal digits"));
    }
    u128::from_str_radix(&text, 16)
        .map(|n| ElementId(n.to_le_bytes()))
        .map_err(|_| bad("an id has 32 hexadecimal digits"))
}

fn parse_ids(value: &JsValue) -> Parsed<Vec<ElementId>> {
    if !Array::is_array(value) {
        return Err(bad("expected an array of ids"));
    }
    let array = Array::from(value);
    if array.length() > MAX_IDS {
        return Err(bad("too many ids"));
    }
    array.iter().map(|id| parse_id(&id)).collect()
}

/// A tool, by its name in the contract.
pub(super) fn parse_tool(value: &JsValue) -> Parsed<Tool> {
    match value.as_string().as_deref() {
        Some("select") => Ok(Tool::Select),
        Some("move") => Ok(Tool::Move),
        Some("place_block") => Ok(Tool::PlaceBlock),
        _ => Err(bad("unknown tool")),
    }
}

const PRESETS: [Preset; 9] = [
    Preset::Top,
    Preset::Front,
    Preset::Back,
    Preset::Left,
    Preset::Right,
    Preset::IsoSe,
    Preset::IsoSw,
    Preset::IsoNe,
    Preset::IsoNw,
];

/// A view command, as `{ kind, … }`.
pub(super) fn parse_command(value: &JsValue) -> Parsed<ViewCommand> {
    let kind = text(value, "kind")?;
    match kind.as_str() {
        "zoom_fit" => Ok(ViewCommand::ZoomFit),
        "zoom_selection" => Ok(ViewCommand::ZoomSelection),
        "view_preset" => {
            let name = text(value, "preset")?;
            PRESETS
                .into_iter()
                .find(|preset| preset.name() == name)
                .map(ViewCommand::ViewPreset)
                .ok_or_else(|| bad("unknown view preset"))
        }
        "projection" => {
            let name = text(value, "projection")?;
            [ProjectionMode::Perspective, ProjectionMode::Parallel]
                .into_iter()
                .find(|mode| mode.name() == name)
                .map(ViewCommand::Projection)
                .ok_or_else(|| bad("unknown projection"))
        }
        "select" => parse_ids(&field(value, "ids")?).map(ViewCommand::Select),
        "clear_selection" => Ok(ViewCommand::ClearSelection),
        "cancel" => Ok(ViewCommand::Cancel),
        "typed_distance" => number(&field(value, "metres")?).map(ViewCommand::TypedDistance),
        "snapping" => boolean(value, "enabled").map(ViewCommand::Snapping),
        _ => parse_sectioning_command(&kind, value),
    }
}

/// A three-number array: a point or a vector, in metres.
fn vec3(value: &JsValue) -> Parsed<[f64; 3]> {
    if !Array::is_array(value) || Array::from(value).length() != 3 {
        return Err(bad("expected three numbers"));
    }
    let array = Array::from(value);
    Ok([
        number(&array.get(0))?,
        number(&array.get(1))?,
        number(&array.get(2))?,
    ])
}

/// A position in a list, from a number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked to be a whole number in 0..=u32::MAX just before"
)]
fn index(object: &JsValue) -> Parsed<usize> {
    let value = number(&field(object, "index")?)?;
    if value < 0.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        return Err(bad("expected a position"));
    }
    Ok(value as usize)
}

fn names(value: &JsValue) -> Parsed<Vec<String>> {
    if !Array::is_array(value) || Array::from(value).length() > MAX_NAMES {
        return Err(bad("expected a list of names"));
    }
    Array::from(value)
        .iter()
        .map(|name| {
            name.as_string()
                .filter(|name| name.len() <= MAX_NAME_LEN)
                .ok_or_else(|| bad("expected a short name"))
        })
        .collect()
}

fn visibility_target(object: &JsValue) -> Parsed<VisibilityTarget> {
    let target = field(object, "target")?;
    match text(&target, "by")?.as_str() {
        "category" => names(&field(&target, "names")?).map(VisibilityTarget::Categories),
        "discipline" => names(&field(&target, "names")?).map(VisibilityTarget::Disciplines),
        "selection" => Ok(VisibilityTarget::Selection),
        _ => Err(bad("unknown visibility target")),
    }
}

/// The section box, clipping plane and visibility commands (VP-03).
fn parse_sectioning_command(kind: &str, value: &JsValue) -> Parsed<ViewCommand> {
    match kind {
        "section_box" => boolean(value, "enabled").map(ViewCommand::SectionBox),
        "section_box_extents" => Ok(ViewCommand::SectionBoxExtents {
            min: vec3(&field(value, "min")?)?,
            max: vec3(&field(value, "max")?)?,
        }),
        "section_box_fit_selection" => Ok(ViewCommand::SectionBoxFitSelection),
        "section_box_reset" => Ok(ViewCommand::SectionBoxReset),
        "clip_plane_pick" => Ok(ViewCommand::ClipPlanePick),
        "clip_plane_add" => Ok(ViewCommand::ClipPlaneAdd {
            point: vec3(&field(value, "point")?)?,
            normal: vec3(&field(value, "normal")?)?,
        }),
        "clip_plane_flip" => index(value).map(ViewCommand::ClipPlaneFlip),
        "clip_plane_remove" => index(value).map(ViewCommand::ClipPlaneRemove),
        "clip_planes_clear" => Ok(ViewCommand::ClipPlanesClear),
        "cut_faces" => match text(value, "style")?.as_str() {
            "tinted" => Ok(ViewCommand::CutFaces(CutFaces::Tinted)),
            "none" => Ok(ViewCommand::CutFaces(CutFaces::None)),
            _ => Err(bad("unknown cut faces style")),
        },
        "hide" => visibility_target(value).map(ViewCommand::Hide),
        "show" => visibility_target(value).map(ViewCommand::Show),
        "isolate" => visibility_target(value).map(ViewCommand::Isolate),
        "end_isolate" => Ok(ViewCommand::EndIsolate),
        "reset_visibility" => Ok(ViewCommand::ResetVisibility),
        _ => Err(bad("unknown view command")),
    }
}

fn hex(object: &JsValue, key: &str) -> Parsed<Rgb> {
    Rgb::from_hex(&text(object, key)?).map_err(|_| bad("a colour is #RRGGBB"))
}

/// The theme colours of the contract; the halo under overlay lines, which the tokens do not
/// carry, follows the background: white over a light one, near black over a dark one.
pub(super) fn parse_theme(value: &JsValue) -> Parsed<Theme> {
    let background = hex(value, "background")?;
    let [r, g, b] = background.linear();
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.2;
    let halo = if light {
        Theme::light().halo
    } else {
        Theme::dark().halo
    };
    Ok(Theme {
        background,
        selection: hex(value, "selection")?,
        hover: hex(value, "hover")?,
        snap: hex(value, "snap")?,
        preview: hex(value, "preview")?,
        cut: hex(value, "cut")?,
        halo,
    })
}

/// The `theme` of the options of `Viewport.create`.
pub(super) fn parse_options(value: &JsValue) -> Parsed<Theme> {
    parse_theme(&field(value, "theme")?)
}

/// A finite size in CSS pixels.
pub(super) fn parse_size(width: f64, height: f64, ratio: f64) -> Parsed<([f64; 2], f64)> {
    let ok = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_SAFE;
    if ok(width) && ok(height) && ok(ratio) {
        Ok(([width, height], ratio))
    } else {
        Err(bad("a size and a pixel ratio are positive finite numbers"))
    }
}

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    let object = Object::new();
    for (key, value) in entries {
        // Setting a field on a fresh plain object cannot fail.
        Reflect::set(&object, &JsValue::from_str(key), &value).ok();
    }
    object.into()
}

fn num(v: f64) -> JsValue {
    JsValue::from_f64(v)
}

fn vec3_js(v: [f64; 3]) -> JsValue {
    Array::of3(&num(v[0]), &num(v[1]), &num(v[2])).into()
}

fn ids(ids: &[ElementId]) -> JsValue {
    ids.iter()
        .map(|id| JsValue::from_str(&id_string(*id)))
        .collect::<Array>()
        .into()
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "select",
        Tool::Move => "move",
        Tool::PlaceBlock => "place_block",
    }
}

fn step_name(step: ToolStep) -> &'static str {
    match step {
        ToolStep::Idle => "idle",
        ToolStep::BasePoint => "base_point",
        ToolStep::TargetPoint => "target_point",
        ToolStep::FirstCorner => "first_corner",
        ToolStep::SecondCorner => "second_corner",
        ToolStep::Height => "height",
        ToolStep::PickFace => "pick_face",
    }
}

fn values(values: &[(&'static str, Value)]) -> JsValue {
    object(
        values
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    Value::Number(n) => num(*n),
                    Value::Text(t) => JsValue::from_str(t),
                };
                (*key, value)
            })
            .collect(),
    )
}

fn announcement(announcement: &Announcement) -> JsValue {
    object(vec![
        ("kind", "announce".into()),
        ("key", announcement.key.as_str().into()),
        ("values", values(&announcement.values)),
    ])
}

fn label(label: &PreviewLabel) -> JsValue {
    object(vec![
        (
            "at",
            Array::of2(&num(label.at[0]), &num(label.at[1])).into(),
        ),
        ("key", label.key.as_str().into()),
        ("values", values(&label.values)),
        (
            "typed",
            label
                .typed
                .as_deref()
                .map_or(JsValue::NULL, JsValue::from_str),
        ),
    ])
}

fn intent(intent: &Intent) -> JsValue {
    match intent {
        Intent::Move {
            ids: moved,
            translation,
        } => object(vec![
            ("kind", "move".into()),
            ("ids", ids(moved)),
            ("translation", vec3_js(*translation)),
        ]),
        Intent::PlaceBlock {
            location,
            rotation,
            size,
        } => object(vec![
            ("kind", "place_block".into()),
            ("location", vec3_js(*location)),
            ("rotation", num(*rotation)),
            ("size", vec3_js(*size)),
        ]),
    }
}

fn name_states(states: &[NameState]) -> JsValue {
    states
        .iter()
        .map(|state| {
            object(vec![
                ("name", state.name.as_str().into()),
                ("count", num(crate::math::count(state.count))),
                ("hidden", state.hidden.into()),
                ("isolated", state.isolated.into()),
            ])
        })
        .collect::<Array>()
        .into()
}

fn clip_plane(plane: &ClipPlane) -> JsValue {
    object(vec![
        ("point", vec3_js(plane.point)),
        ("normal", vec3_js(plane.normal)),
    ])
}

fn view_state(state: &ViewState) -> JsValue {
    object(vec![
        ("projection", state.projection.name().into()),
        ("snapping", state.snapping.into()),
        ("tool", tool_name(state.tool).into()),
        ("categories", name_states(&state.categories)),
        ("disciplines", name_states(&state.disciplines)),
        ("isolating", state.isolating.into()),
        (
            "hiddenSelection",
            num(crate::math::count(state.hidden_selection)),
        ),
        (
            "sectionBox",
            state.section_box.map_or(JsValue::NULL, |section| {
                object(vec![
                    ("enabled", section.enabled.into()),
                    ("min", vec3_js(section.min)),
                    ("max", vec3_js(section.max)),
                ])
            }),
        ),
        (
            "clipPlanes",
            state
                .clip_planes
                .iter()
                .map(clip_plane)
                .collect::<Array>()
                .into(),
        ),
        (
            "cutFaces",
            match state.cut_faces {
                CutFaces::None => "none",
                _ => "tinted",
            }
            .into(),
        ),
    ])
}

/// An event of the contract, as a plain object with a `kind`.
pub(super) fn event(event: &ViewportEvent) -> JsValue {
    match event {
        ViewportEvent::Selection(selected) => {
            object(vec![("kind", "selection".into()), ("ids", ids(selected))])
        }
        ViewportEvent::Hover(hovered) => object(vec![
            ("kind", "hover".into()),
            (
                "id",
                hovered.map_or(JsValue::NULL, |id| JsValue::from_str(&id_string(id))),
            ),
        ]),
        ViewportEvent::ToolState(tool, step) => object(vec![
            ("kind", "tool_state".into()),
            ("tool", tool_name(*tool).into()),
            ("step", step_name(*step).into()),
        ]),
        ViewportEvent::Announce(said) => announcement(said),
        ViewportEvent::Intent(done) => {
            object(vec![("kind", "intent".into()), ("intent", intent(done))])
        }
        ViewportEvent::ViewState(state) => object(vec![
            ("kind", "view_state".into()),
            ("state", view_state(state)),
        ]),
        ViewportEvent::PreviewLabel(shown) => object(vec![
            ("kind", "preview_label".into()),
            ("label", shown.as_ref().map_or(JsValue::NULL, label)),
        ]),
        ViewportEvent::Error(key) => error_event(match key {
            crate::ErrorKey::SceneRejected => "scene_rejected",
        }),
    }
}

/// An `error` event.
pub(super) fn error_event(key: &str) -> JsValue {
    object(vec![("kind", "error".into()), ("key", key.into())])
}

/// Whether `value` is a function, for `onEvent`.
pub(super) fn function(value: JsValue) -> Parsed<js_sys::Function> {
    value
        .dyn_into::<js_sys::Function>()
        .map_err(|_| bad("expected a function"))
}
