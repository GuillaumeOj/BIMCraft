//! The WebGPU surface of the canvas and the frame it presents.
//!
//! The renderer expects an sRGB colour target, which a browser canvas cannot be (its formats are
//! the plain `bgra8unorm` and `rgba8unorm`). So the frame is drawn into a texture of the sRGB
//! twin of the canvas format and copied to the canvas texture: the two formats differ only in
//! the transfer function, which WebGPU allows a copy between.

use std::sync::{Arc, Mutex};

use render::{Camera, RenderError};
use wasm_bindgen::{JsError, JsValue};
use web_sys::HtmlCanvasElement;

type Faults = Arc<Mutex<Vec<&'static str>>>;

thread_local! {
    // One instance for the page: the availability check and every viewport share it, so the
    // browser never sees a WebGPU instance made only to be abandoned.
    static INSTANCE: wgpu::Instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
}

fn instance() -> wgpu::Instance {
    INSTANCE.with(Clone::clone)
}

/// Whether the browser gives an adapter at all.
pub(super) async fn adapter_available() -> bool {
    instance()
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .is_ok()
}

/// The device, the surface of the canvas and the target the renderer draws into.
pub(super) struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The sRGB twin of the canvas format: what the renderer draws in.
    render_format: wgpu::TextureFormat,
    target: Option<wgpu::Texture>,
}

impl Gpu {
    /// Creates the device and configures the canvas for a `size` target. Device losses and
    /// out-of-memory errors go to `faults`; other GPU errors are logged by their label only.
    pub(super) async fn new(
        canvas: &HtmlCanvasElement,
        size: [u32; 2],
        faults: &Faults,
    ) -> Result<Self, JsError> {
        let instance = instance();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| JsError::new(&error.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..wgpu::RequestAdapterOptions::default()
            })
            .await
            .map_err(|error| JsError::new(&format!("no WebGPU adapter: {error}")))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("viewport"),
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(|error| JsError::new(&format!("no WebGPU device: {error}")))?;
        install_handlers(&device, faults);
        let mut config = surface
            .get_default_config(&adapter, size[0], size[1])
            .ok_or_else(|| JsError::new("the canvas cannot be drawn with this adapter"))?;
        config.usage |= wgpu::TextureUsages::COPY_DST;
        surface.configure(&device, &config);
        let render_format = config.format.add_srgb_suffix();
        Ok(Self {
            device,
            queue,
            surface,
            config,
            render_format,
            target: None,
        })
    }

    pub(super) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(super) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The format the renderer is created for.
    pub(super) fn render_format(&self) -> wgpu::TextureFormat {
        self.render_format
    }

    /// The offscreen target, recreated when the canvas size changes.
    fn target(&mut self, size: [u32; 2]) -> wgpu::Texture {
        if let Some(texture) = self
            .target
            .as_ref()
            .filter(|texture| [texture.width(), texture.height()] == size)
        {
            return texture.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.render_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.target = Some(texture.clone());
        texture
    }

    /// Draws one frame and presents it. `Ok(false)`: the canvas gave no texture this time.
    pub(super) fn draw(
        &mut self,
        renderer: &mut render::Renderer,
        size: [u32; 2],
        camera: &Camera,
        clear: [f64; 4],
    ) -> Result<bool, &'static str> {
        if [self.config.width, self.config.height] != size {
            self.config.width = size[0];
            self.config.height = size[1];
            self.surface.configure(&self.device, &self.config);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            _ => return Ok(false),
        };
        let target = self.target(size);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        renderer
            .render(&view, size, camera, clear)
            .map_err(|error| match error {
                RenderError::TargetTooLarge { .. } => "out_of_memory",
                _ => "scene_rejected",
            })?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("viewport present"),
            });
        encoder.copy_texture_to_texture(
            target.as_image_copy(),
            frame.texture.as_image_copy(),
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }
}

fn install_handlers(device: &wgpu::Device, faults: &Faults) {
    let lost = faults.clone();
    device.set_device_lost_callback(move |reason, _| {
        // Destroying the device is the page freeing the viewport: not a loss.
        if let (false, Ok(mut faults)) = (reason == wgpu::DeviceLostReason::Destroyed, lost.lock())
        {
            faults.push("device_lost");
        }
    });
    let errors = faults.clone();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| match error {
        wgpu::Error::OutOfMemory { .. } => {
            if let Ok(mut faults) = errors.lock() {
                faults.push("out_of_memory");
            }
        }
        // wgpu's default handler panics, which would take the page's module down; a validation
        // error is a bug to read in the console, not model content.
        other => web_sys::console::warn_1(&JsValue::from_str(&other.to_string())),
    }));
}
