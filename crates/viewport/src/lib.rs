//! The interaction of the `Fabrilo` 3D viewport (VP-01, spec D2 and D5).
//!
//! Everything that happens at pointer speed runs here, in Rust: the [`Viewport`] holds the scene
//! as the core sends it (deltas of the binary scene channel, `scene-wire`), a camera controller
//! (orbit, pan, zoom to the pointer, framing, presets, projections), picking and snapping on the
//! CPU, and the tools (select, move, place a block) as state machines. Input arrives as
//! platform-independent [`InputEvent`]s and [`ViewCommand`]s; what the user sees goes to a
//! [`SceneSink`] (the renderer, through [`RendererSink`]); what the host needs comes out as
//! [`ViewportEvent`]s, among them the finished actions ([`Intent`]) that the UI sends to the core
//! as commands (UI-01). Only the browser glue, added separately, touches a platform; so all of
//! this is tested natively and deterministically: announcements are throttled by frames, not by
//! the clock.
//!
//! Every mapping between the screen and the world works in `f64` relative to the eye
//! (`docs/dev/adr/0020-camera-relative-rendering.md`): picking and snapping 10⁷ m from the origin
//! give the results they give at it.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and computes trigonometry with `libm` (ENG-04).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod announce;
mod cache;
mod camera;
mod clipping;
mod event;
mod input;
mod interaction;
mod math;
mod pick;
mod sink;
mod snap;
#[cfg(test)]
mod test_support;
mod typed;
mod view;
mod visibility;
#[cfg(target_arch = "wasm32")]
mod web;

pub use cache::SceneError;
pub use event::{
    Announcement, AnnouncementKey, ElementId, ErrorKey, Intent, LabelKey, NameState, Preset,
    PreviewLabel, ProjectionMode, SectionBoxState, Tool, ToolStep, Value, ViewCommand, ViewState,
    ViewportEvent, VisibilityTarget,
};
pub use input::{Button, InputEvent, Key, Modifiers};
pub use interaction::{Tick, Viewport};
pub use render::{ClipPlane, CutFaces, MAX_CLIP_PLANES};
pub use sink::{Preview, PreviewLine, RendererSink, SceneSink, SinkError};
pub use snap::DEFAULT_TOLERANCE;

/// The configuration of the property tests: fixed seed, no persistence, so runs are
/// deterministic (AGENTS.md §5).
#[cfg(test)]
fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(71),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}
