//! The browser glue of the viewport (spec D2, `api-contract.md` §2): the `wasm-bindgen` exports
//! the UI imports from `ui/src/viewport/pkg/`. It stays thin: it creates the WebGPU surface,
//! attaches the canvas listeners, runs the frame loop only while something changed, and converts
//! values; every decision belongs to [`crate::Viewport`], which native tests cover.
//!
//! Re-entrancy: the event listener of the page may call back into the viewport, so events are
//! queued while the viewport is borrowed and delivered once it is not.

mod convert;
mod gpu;
mod listeners;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use js_sys::Reflect;
use render::{Renderer, Theme};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsError, JsValue};
use web_sys::HtmlCanvasElement;

use self::convert::Parsed;
use self::gpu::Gpu;
use crate::{InputEvent, RendererSink, Viewport};

/// How many frames in a row may fail to get a surface texture before the loop waits for the next
/// change instead of retrying.
const MAX_RETRIES: u8 = 8;

/// Whether the browser offers WebGPU and an adapter for it. A page without it shows its
/// no-WebGPU state.
#[wasm_bindgen(js_name = webgpuAvailable)]
pub async fn webgpu_available() -> bool {
    if !has_gpu() {
        return false;
    }
    gpu::adapter_available().await
}

/// `navigator.gpu` exists; asking wgpu for an instance without it would fail inside the library.
fn has_gpu() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    Reflect::get(&window, &JsValue::from_str("navigator"))
        .and_then(|navigator| Reflect::get(&navigator, &JsValue::from_str("gpu")))
        .is_ok_and(|gpu| !gpu.is_undefined() && !gpu.is_null())
}

/// What a panic leaves in the console: where, never the message, which could carry model
/// content (SEC-03). The browser has no panic hook of its own, and `console_error_panic_hook`
/// would be one more dependency for this line.
fn set_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map_or_else(String::new, |at| format!("{}:{}", at.file(), at.line()));
        web_sys::console::error_1(&JsValue::from_str(&format!("viewport panicked at {place}")));
    }));
}

/// The state that dies with the viewport.
struct Inner {
    viewport: Viewport<RendererSink>,
    gpu: Gpu,
    canvas: HtmlCanvasElement,
    theme: Theme,
    /// A frame did not get a surface texture: draw again.
    retries: u8,
}

/// What the closures and the exported object share.
struct Shared {
    inner: RefCell<Option<Inner>>,
    window: web_sys::Window,
    canvas: HtmlCanvasElement,
    listener: RefCell<Option<js_sys::Function>>,
    outbox: RefCell<Vec<JsValue>>,
    delivering: Cell<bool>,
    frame: RefCell<Option<Closure<dyn FnMut()>>>,
    frame_handle: Cell<Option<i32>>,
    dom: RefCell<Vec<listeners::Listener>>,
    /// Device losses and out-of-memory errors, which wgpu reports from callbacks that cannot
    /// touch the page's objects.
    faults: Arc<Mutex<Vec<&'static str>>>,
}

impl Shared {
    /// Runs `f` on the viewport, then queues its events, asks for a frame and delivers.
    fn run<R>(self: &Rc<Self>, f: impl FnOnce(&mut Inner) -> R) -> Parsed<R> {
        let result = {
            let mut guard = self
                .inner
                .try_borrow_mut()
                .map_err(|_| JsError::new("the viewport is busy"))?;
            let inner = guard
                .as_mut()
                .ok_or_else(|| JsError::new("the viewport was freed"))?;
            let result = f(inner);
            self.collect(inner);
            result
        };
        self.schedule();
        self.deliver();
        Ok(result)
    }

    /// Moves what the viewport and the GPU reported into the outbox.
    fn collect(&self, inner: &mut Inner) {
        let mut outbox = self.outbox.borrow_mut();
        outbox.extend(inner.viewport.drain_events().iter().map(convert::event));
        if let Ok(mut faults) = self.faults.lock() {
            outbox.extend(faults.drain(..).map(convert::error_event));
        }
    }

    /// Calls the page's listener with every queued event, in order, once at a time.
    fn deliver(&self) {
        if self.delivering.replace(true) {
            return;
        }
        loop {
            let events = std::mem::take(&mut *self.outbox.borrow_mut());
            if events.is_empty() {
                break;
            }
            let Some(listener) = self.listener.borrow().clone() else {
                break;
            };
            for event in events {
                // A throwing listener is the page's bug; it must not stop the viewport.
                listener.call1(&JsValue::NULL, &event).ok();
            }
        }
        self.delivering.set(false);
    }

    /// Asks for one frame, unless one is already due.
    fn schedule(&self) {
        if self.frame_handle.get().is_some() || self.inner.borrow().is_none() {
            return;
        }
        if let Some(frame) = self.frame.borrow().as_ref() {
            self.frame_handle.set(
                self.window
                    .request_animation_frame(frame.as_ref().unchecked_ref())
                    .ok(),
            );
        }
    }

    /// One animation frame: ages the viewport, draws when something changed.
    fn frame(self: &Rc<Self>) {
        self.frame_handle.set(None);
        let again = {
            let Ok(mut guard) = self.inner.try_borrow_mut() else {
                return;
            };
            let Some(inner) = guard.as_mut() else {
                return;
            };
            let tick = inner.viewport.tick();
            let mut again = tick.again;
            if tick.draw || inner.retries > 0 {
                match inner.draw() {
                    Ok(true) => inner.retries = 0,
                    Ok(false) => {
                        inner.retries = inner.retries.saturating_add(1);
                        if inner.retries >= MAX_RETRIES {
                            inner.retries = 0;
                        } else {
                            again = true;
                        }
                    }
                    Err(key) => self.outbox.borrow_mut().push(convert::error_event(key)),
                }
            }
            self.collect(inner);
            again
        };
        if again {
            self.schedule();
        }
        self.deliver();
    }

    fn shutdown(&self) {
        for listener in self.dom.borrow_mut().drain(..) {
            listener.remove(&self.canvas);
        }
        if let Some(handle) = self.frame_handle.take() {
            self.window.cancel_animation_frame(handle).ok();
        }
        self.frame.borrow_mut().take();
        self.listener.borrow_mut().take();
        self.outbox.borrow_mut().clear();
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.take();
        }
    }
}

impl Inner {
    /// Draws one frame. `Ok(false)`: the surface gave no texture, try again.
    fn draw(&mut self) -> Result<bool, &'static str> {
        let size = self.viewport.size();
        let camera = self.viewport.camera();
        let clear = self.theme.clear_color();
        let renderer = self.viewport.sink_mut().renderer_mut();
        self.gpu.draw(renderer, size, &camera, clear)
    }
}

/// The 3D viewport on a canvas: the scene, the camera, the tools and the frame loop.
#[wasm_bindgen(js_name = Viewport)]
pub struct WebViewport {
    shared: Rc<Shared>,
}

#[wasm_bindgen(js_class = Viewport)]
impl WebViewport {
    /// Attaches to `canvas`: owns its pointer, wheel and key listeners and its frame loop.
    /// Rejects when the browser has no WebGPU adapter.
    pub async fn create(canvas: JsValue, options: JsValue) -> Result<WebViewport, JsError> {
        set_panic_hook();
        let canvas = canvas
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsError::new("expected a canvas element"))?;
        let theme = convert::parse_options(&options)?;
        if !has_gpu() {
            return Err(JsError::new("WebGPU is not available"));
        }
        let window = web_sys::window().ok_or_else(|| JsError::new("no window"))?;
        let ratio = window.device_pixel_ratio();
        let css = [
            f64::from(canvas.client_width().max(1)),
            f64::from(canvas.client_height().max(1)),
        ];
        let faults = Arc::new(Mutex::new(Vec::new()));
        let size = [device_pixels(css[0], ratio), device_pixels(css[1], ratio)];
        canvas.set_width(size[0]);
        canvas.set_height(size[1]);
        let gpu = Gpu::new(&canvas, size, &faults).await?;
        let renderer = Renderer::new(gpu.device(), gpu.queue(), gpu.render_format())
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut viewport = Viewport::new(RendererSink::new(renderer), css, ratio);
        viewport.set_theme(theme);
        let shared = Rc::new(Shared {
            inner: RefCell::new(Some(Inner {
                viewport,
                gpu,
                canvas: canvas.clone(),
                theme,
                retries: 0,
            })),
            window,
            canvas: canvas.clone(),
            listener: RefCell::new(None),
            outbox: RefCell::new(Vec::new()),
            delivering: Cell::new(false),
            frame: RefCell::new(None),
            frame_handle: Cell::new(None),
            dom: RefCell::new(Vec::new()),
            faults,
        });
        let weak: Weak<Shared> = Rc::downgrade(&shared);
        let frame_owner = weak.clone();
        *shared.frame.borrow_mut() = Some(Closure::new(move || {
            if let Some(shared) = frame_owner.upgrade() {
                shared.frame();
            }
        }));
        let send: Rc<dyn Fn(InputEvent)> = Rc::new(move |event| {
            if let Some(shared) = weak.upgrade() {
                // The page freed the viewport, or it is mid-call: the event is stale.
                shared.run(|inner| inner.viewport.input(event)).ok();
            }
        });
        *shared.dom.borrow_mut() = listeners::attach(&canvas, &send);
        shared.schedule();
        Ok(WebViewport { shared })
    }

    /// Applies one scene delta (bytes from the `scene` command); returns the revision now held.
    #[wasm_bindgen(js_name = applyScene)]
    #[expect(
        clippy::cast_precision_loss,
        reason = "revisions stay below 2^53 (api-contract §1)"
    )]
    pub fn apply_scene(&self, bytes: &[u8]) -> Result<f64, JsError> {
        self.shared
            .run(|inner| inner.viewport.apply_scene(bytes))?
            .map(|revision| revision as f64)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// The revision held, or `null` before the first delta.
    #[expect(
        clippy::cast_precision_loss,
        reason = "revisions stay below 2^53 (api-contract §1)"
    )]
    pub fn revision(&self) -> Result<Option<f64>, JsError> {
        self.shared
            .run(|inner| inner.viewport.revision().map(|revision| revision as f64))
    }

    /// The canvas is now `width` × `height` CSS pixels at `device_pixel_ratio`.
    pub fn resize(&self, width: f64, height: f64, device_pixel_ratio: f64) -> Result<(), JsError> {
        let (size, ratio) = convert::parse_size(width, height, device_pixel_ratio)?;
        self.shared.run(|inner| {
            inner.viewport.resize(size, ratio);
            let [w, h] = inner.viewport.size();
            inner.canvas.set_width(w);
            inner.canvas.set_height(h);
        })
    }

    /// Changes the colours, from the CSS tokens.
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&self, theme: &JsValue) -> Result<(), JsError> {
        let theme = convert::parse_theme(theme)?;
        self.shared.run(|inner| {
            inner.theme = theme;
            inner.viewport.set_theme(theme);
        })
    }

    /// Runs a view command.
    pub fn command(&self, command: &JsValue) -> Result<(), JsError> {
        let command = convert::parse_command(command)?;
        self.shared.run(|inner| inner.viewport.command(command))
    }

    /// Selects the tool the pointer drives in the canvas.
    #[wasm_bindgen(js_name = setTool)]
    pub fn set_tool(&self, tool: &JsValue) -> Result<(), JsError> {
        let tool = convert::parse_tool(tool)?;
        self.shared.run(|inner| inner.viewport.set_tool(tool))
    }

    /// Sets the one listener of every event; a second call replaces the first.
    #[wasm_bindgen(js_name = onEvent)]
    pub fn on_event(&self, listener: JsValue) -> Result<(), JsError> {
        let listener = convert::function(listener)?;
        *self.shared.listener.borrow_mut() = Some(listener);
        self.shared.deliver();
        Ok(())
    }
}

/// `wasm-bindgen` generates `free()` for the class; dropping the object is what detaches the
/// listeners, stops the loop and frees the GPU resources.
impl Drop for WebViewport {
    fn drop(&mut self) {
        self.shared.shutdown();
    }
}

/// CSS pixels to whole device pixels, at least one.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 1..=u32::MAX and rounded just before"
)]
fn device_pixels(css: f64, ratio: f64) -> u32 {
    let pixels = (css * ratio).round().clamp(1.0, f64::from(u32::MAX));
    pixels as u32
}
