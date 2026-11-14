//! The DOM listeners of the canvas: pointer, wheel and keyboard events become
//! [`InputEvent`]s. Touch contacts are tracked here, because two fingers are one pinch, not two
//! pointers.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{AddEventListenerOptions, Event, HtmlCanvasElement, KeyboardEvent, MouseEvent};

use crate::{Button, InputEvent, Key, Modifiers};

/// A line of a wheel in CSS pixels, for the browsers that report lines.
const LINE_PIXELS: f64 = 16.0;

/// How fast a trackpad pinch (a wheel event with Ctrl held) zooms: the scale per unit of delta.
const PINCH_PER_DELTA: f64 = 0.01;

/// A registered listener, kept to remove it when the viewport is freed.
pub(super) struct Listener {
    name: &'static str,
    closure: Closure<dyn FnMut(Event)>,
}

impl Listener {
    pub(super) fn remove(&self, canvas: &HtmlCanvasElement) {
        canvas
            .remove_event_listener_with_callback(self.name, self.closure.as_ref().unchecked_ref())
            .ok();
    }
}

fn listen(
    canvas: &HtmlCanvasElement,
    name: &'static str,
    handler: impl FnMut(Event) + 'static,
) -> Option<Listener> {
    let closure = Closure::<dyn FnMut(Event)>::new(handler);
    // Not passive: the canvas stops the page scrolling and zooming under the wheel.
    let options = AddEventListenerOptions::new();
    options.set_passive(false);
    canvas
        .add_event_listener_with_callback_and_add_event_listener_options(
            name,
            closure.as_ref().unchecked_ref(),
            &options,
        )
        .ok()?;
    Some(Listener { name, closure })
}

fn modifiers(event: &MouseEvent) -> Modifiers {
    Modifiers {
        shift: event.shift_key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        meta: event.meta_key(),
    }
}

fn key_modifiers(event: &KeyboardEvent) -> Modifiers {
    Modifiers {
        shift: event.shift_key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        meta: event.meta_key(),
    }
}

fn position(canvas: &HtmlCanvasElement, event: &MouseEvent) -> [f64; 2] {
    let rect = canvas.get_bounding_client_rect();
    [
        f64::from(event.client_x()) - rect.left(),
        f64::from(event.client_y()) - rect.top(),
    ]
}

fn button(event: &MouseEvent) -> Option<Button> {
    match event.button() {
        0 => Some(Button::Primary),
        1 => Some(Button::Middle),
        2 => Some(Button::Secondary),
        _ => None,
    }
}

fn key(name: &str) -> Key {
    match name {
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Alt" => Key::Alt,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => Key::Other,
            }
        }
    }
}

/// Whether the canvas takes the key for itself: not Tab (the canvas never traps the focus), not
/// a browser or application shortcut.
fn consumes(key: Key, mods: Modifiers) -> bool {
    key != Key::Other && !mods.toggle()
}

/// The fingers on the canvas.
#[derive(Default)]
struct Touches {
    points: Vec<(i32, [f64; 2])>,
    /// Two fingers went down: the gesture is a pinch until every finger is up.
    pinching: bool,
    last_distance: f64,
}

impl Touches {
    fn set(&mut self, id: i32, at: [f64; 2]) {
        match self.points.iter_mut().find(|(known, _)| *known == id) {
            Some(point) => point.1 = at,
            None => self.points.push((id, at)),
        }
    }

    fn spread(&self) -> Option<(f64, [f64; 2])> {
        let [(_, a), (_, b), ..] = self.points.as_slice() else {
            return None;
        };
        let distance = libm::hypot(a[0] - b[0], a[1] - b[1]);
        Some((
            distance,
            [f64::midpoint(a[0], b[0]), f64::midpoint(a[1], b[1])],
        ))
    }
}

/// Attaches the listeners; `send` receives what they translate. Returns what to remove later.
pub(super) fn attach(canvas: &HtmlCanvasElement, send: &Rc<dyn Fn(InputEvent)>) -> Vec<Listener> {
    let mut listeners = pointer(canvas, send);
    listeners.extend(wheel(canvas, send));
    listeners.extend(keys(canvas, send));
    listeners
}

fn pointer(canvas: &HtmlCanvasElement, send: &Rc<dyn Fn(InputEvent)>) -> Vec<Listener> {
    let touches = Rc::new(RefCell::new(Touches::default()));
    let mut listeners = Vec::new();

    let (c, s, t) = (canvas.clone(), send.clone(), touches.clone());
    listeners.extend(listen(canvas, "pointerdown", move |event| {
        let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() else {
            return;
        };
        let at = position(&c, &event);
        // Keeps the drag when the pointer leaves the canvas; a refusal only loses that.
        c.set_pointer_capture(event.pointer_id()).ok();
        if event.pointer_type() == "touch" {
            let mut touches = t.borrow_mut();
            touches.set(event.pointer_id(), at);
            if let Some((distance, _)) = touches.spread() {
                if !touches.pinching {
                    touches.pinching = true;
                    touches.last_distance = distance;
                    // The first finger's gesture is abandoned, never committed: the pinch
                    // takes over.
                    drop(touches);
                    s(InputEvent::PointerLeave);
                }
                return;
            }
        }
        if let Some(button) = button(&event) {
            s(InputEvent::PointerDown {
                pos: at,
                button,
                mods: modifiers(&event),
            });
        }
    }));

    let (c, s, t) = (canvas.clone(), send.clone(), touches.clone());
    listeners.extend(listen(canvas, "pointermove", move |event| {
        let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() else {
            return;
        };
        let at = position(&c, &event);
        if event.pointer_type() == "touch" {
            let mut touches = t.borrow_mut();
            touches.set(event.pointer_id(), at);
            if touches.pinching {
                if let Some((distance, centre)) = touches.spread() {
                    let last = touches.last_distance;
                    if last > 0.0 && distance > 0.0 {
                        touches.last_distance = distance;
                        drop(touches);
                        s(InputEvent::Pinch {
                            pos: centre,
                            scale: distance / last,
                        });
                    }
                }
                return;
            }
        }
        s(InputEvent::PointerMove {
            pos: at,
            mods: modifiers(&event),
        });
    }));

    for name in ["pointerup", "pointercancel"] {
        let (c, s, t) = (canvas.clone(), send.clone(), touches.clone());
        listeners.extend(listen(canvas, name, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() else {
                return;
            };
            if event.pointer_type() == "touch" {
                let mut touches = t.borrow_mut();
                touches.points.retain(|(id, _)| *id != event.pointer_id());
                if touches.pinching {
                    touches.pinching = !touches.points.is_empty();
                    return;
                }
            }
            // The browser took the pointer away (its button is meaningless): end the gesture
            // without committing it.
            if name == "pointercancel" {
                s(InputEvent::PointerLeave);
                return;
            }
            if let Some(button) = button(&event) {
                s(InputEvent::PointerUp {
                    pos: position(&c, &event),
                    button,
                    mods: modifiers(&event),
                });
            }
        }));
    }

    let s = send.clone();
    listeners.extend(listen(canvas, "pointerleave", move |_| {
        s(InputEvent::PointerLeave);
    }));

    // The context menu would open under a right-button orbit or pan.
    listeners.extend(listen(canvas, "contextmenu", |event| {
        event.prevent_default();
    }));
    listeners
}

fn wheel(canvas: &HtmlCanvasElement, send: &Rc<dyn Fn(InputEvent)>) -> Vec<Listener> {
    let (c, s) = (canvas.clone(), send.clone());
    listen(canvas, "wheel", move |event| {
        let Ok(event) = event.dyn_into::<web_sys::WheelEvent>() else {
            return;
        };
        event.prevent_default();
        let pos = position(&c, &event);
        if event.ctrl_key() {
            // A trackpad pinch reaches the page as a wheel event with Ctrl held.
            let scale = libm::exp(-event.delta_y() * PINCH_PER_DELTA);
            s(InputEvent::Pinch { pos, scale });
            return;
        }
        let unit = if event.delta_mode() == web_sys::WheelEvent::DOM_DELTA_PIXEL {
            1.0
        } else {
            LINE_PIXELS
        };
        let delta = [event.delta_x() * unit, event.delta_y() * unit];
        // A two-finger scroll has a horizontal part or fractional pixels; a wheel notch does not.
        let pan = event.delta_mode() == web_sys::WheelEvent::DOM_DELTA_PIXEL
            && (delta[0] != 0.0 || delta[1].fract() != 0.0);
        s(InputEvent::Wheel {
            pos,
            delta,
            pan,
            mods: modifiers(&event),
        });
    })
    .into_iter()
    .collect()
}

fn keys(canvas: &HtmlCanvasElement, send: &Rc<dyn Fn(InputEvent)>) -> Vec<Listener> {
    let mut listeners = Vec::new();
    for (name, down) in [("keydown", true), ("keyup", false)] {
        let s = send.clone();
        listeners.extend(listen(canvas, name, move |event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let (key, mods) = (key(&event.key()), key_modifiers(&event));
            if consumes(key, mods) {
                event.prevent_default();
            }
            s(if down {
                InputEvent::KeyDown { key, mods }
            } else {
                InputEvent::KeyUp { key, mods }
            });
        }));
    }

    let s = send.clone();
    listeners.extend(listen(canvas, "blur", move |_| s(InputEvent::Blur)));
    listeners
}
