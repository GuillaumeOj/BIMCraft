//! The GPU side of the renderer: meshes stored on the device, and the opaque pass that draws the
//! prepared frame into a texture view (VP-02, VP-04).
//!
//! Each frame, [`crate::world`] culls the objects, chooses their levels of detail and computes
//! on the CPU in `f64` the camera-relative model-view of every object drawn
//! ([`crate::transform`]); only that small result reaches the GPU in `f32`, as one instance
//! record per object. Objects that share a mesh and a level are drawn by one instanced draw call
//! (`docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`). Depth is reverse-Z in a
//! `Depth32Float` buffer, compared with `Greater` and cleared to 0
//! (`docs/dev/adr/0020-camera-relative-rendering.md`).
//!
//! After the objects, the same pass tests the boxes of the clusters the world chose with
//! occlusion queries, drawn without writing colour or depth. Their results are copied to a
//! buffer mapped asynchronously, never waited for: a later frame reads them once the GPU has
//! delivered them, which works the same in a browser, where nothing may block.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use wgpu::util::DeviceExt as _;

use crate::bounds::Aabb;
use crate::clip::{CLIP_FLOATS, Clip};
use crate::frustum::Frustum;
use crate::lod::{DetailLevel, RenderSettings, has_coarse_level};
use crate::overlay::{ClipVolume, Overlay, OverlayFrame, PRIMITIVE_ATTRIBUTES, PRIMITIVE_STRIDE};
use crate::theme::{DisplayState, Theme};
use crate::transform::{Matrix, ViewFrame, projection_matrix};
use crate::world::{
    DrawList, FrameStats, INSTANCE_FLOATS, INSTANCE_STRIDE, MeshId, ObjectId, World,
};
use crate::{Camera, CameraError, MeshData, MeshError, Placement, PlacementError};

/// The format of the depth buffer: reverse-Z needs floating-point depth to keep its resolution.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The depth of the far plane in reverse-Z, the value the depth buffer is cleared to.
const FAR_DEPTH: f32 = 0.0;

/// Unit direction towards the light in camera coordinates, `(1, 2, 3) / √14`: from the upper
/// right, behind the viewer, so the three visible faces of a box get three different shades.
const LIGHT_DIRECTION: [f32; 3] = [0.267_261_24, 0.534_522_5, 0.801_783_7];

/// The share of the colour lit by ambient light, so faces turned away from the light stay visible.
const AMBIENT: f32 = 0.25;

/// Bytes of the frame uniform: a 4×4 matrix and two vectors of four `f32`, the light and the
/// size of the target.
const UNIFORM_SIZE: u32 = 320;

/// Where the clipping block starts in the frame uniform, in floats: after the projection, the
/// light and the size of the target.
const CLIP_START: usize = 24;

/// The floats of the frame uniform.
const FRAME_FLOATS: usize = CLIP_START + CLIP_FLOATS;

/// Bytes of a vertex position or normal: three `f32`.
const VERTEX_STRIDE: u64 = 12;

/// Bytes of an index.
const INDEX_SIZE: u64 = 4;

/// Bytes of the result of an occlusion query: a sample count.
const QUERY_RESULT_SIZE: u64 = wgpu::QUERY_SIZE as u64;

/// The most frames whose query results can be on their way back at once. When all are, a frame
/// issues no queries: the results come late, never block.
const MAX_READBACKS: usize = 3;

/// The states of a [`Readback`], shared with the callback of its mapping.
const IDLE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

/// The corners of the unit cube `[-1, 1]³` that the query boxes are drawn from: bit 0 of the
/// index is x, bit 1 y, bit 2 z.
const UNIT_CUBE: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [-1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
];

/// The 12 triangles of the unit cube, two per face.
const UNIT_CUBE_INDICES: [u32; 36] = [
    0, 2, 6, 0, 6, 4, // −x
    1, 3, 7, 1, 7, 5, // +x
    0, 1, 5, 0, 5, 4, // −y
    2, 3, 7, 2, 7, 6, // +y
    0, 1, 3, 0, 3, 2, // −z
    4, 5, 7, 4, 7, 6, // +z
];

/// Why the renderer cannot do what it was asked. Messages never contain coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    /// No GPU adapter is available for the backends tried, so nothing can be drawn.
    #[error(
        "no GPU adapter found for the backends {backends:?} ({reason}); a GPU driver, or a \
         software one such as lavapipe (Vulkan) or WARP (DX12), provides one"
    )]
    NoAdapter {
        /// The backends that were tried.
        backends: wgpu::Backends,
        /// Why the request failed, as the GPU library reports it.
        reason: String,
    },
    /// The adapter refused to create a device.
    #[error("the GPU adapter {adapter} could not create a device ({reason})")]
    NoDevice {
        /// Name of the adapter.
        adapter: String,
        /// Why the request failed, as the GPU library reports it.
        reason: String,
    },
    /// The colour format cannot be rendered to with a floating-point colour on every device.
    #[error("the renderer cannot draw into textures of format {0:?}")]
    UnsupportedFormat(wgpu::TextureFormat),
    /// The mesh id is not, or no longer, in the renderer.
    #[error("the mesh is not in the renderer")]
    UnknownMesh(MeshId),
    /// The object id is not, or no longer, in the renderer.
    #[error("the object is not in the renderer")]
    UnknownObject(ObjectId),
    /// The overlay needs more records than the device's largest buffer holds.
    #[error("the overlay is larger than this device can draw")]
    OverlayTooLarge,
    /// The renderer holds as many objects as one frame can draw on this device.
    #[error("the renderer holds the {max} objects this device can draw in one frame")]
    TooManyObjects {
        /// The limit for this device.
        max: usize,
    },
    /// A threshold of the settings is negative, not finite or too large, or the hysteresis is
    /// not between 0 and 1.
    #[error("the render settings are out of range")]
    InvalidSettings,
    /// The colour of an object is not four finite components between 0 and 1.
    #[error("the colour of the object is not four values between 0 and 1")]
    InvalidColor,
    /// The clear colour is not four finite components.
    #[error("the clear colour is not four finite values")]
    InvalidClearColor,
    /// The placement of an object is invalid.
    #[error(transparent)]
    Placement(#[from] PlacementError),
    /// The camera is invalid, or its projection does not suit the target.
    #[error(transparent)]
    Camera(#[from] CameraError),
    /// The target has no pixels.
    #[error("the render target has no pixels")]
    ZeroSizedTarget,
    /// The target is wider or taller than the device can draw.
    #[error("the render target is larger than the {max} pixels per side this device allows")]
    TargetTooLarge {
        /// The limit for this device.
        max: u32,
    },
    /// The rectangle of a query is not finite or is inverted.
    #[error("the rectangle is not finite or is inverted")]
    InvalidRect,
    /// The target view is not a single-sampled 2D render attachment of the renderer's format and
    /// of the given size.
    #[error("the render target does not match the renderer's format or the given size")]
    TargetMismatch,
}

/// The buffers of one level of a mesh on the GPU.
struct MeshBuffers {
    positions: wgpu::Buffer,
    normals: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// A mesh stored on the GPU: its own triangles, and its coarse level when it has one.
struct GpuMesh {
    fine: MeshBuffers,
    coarse: Option<MeshBuffers>,
}

/// The instance buffer, grown as frames draw more objects.
struct InstanceBuffer {
    buffer: wgpu::Buffer,
    /// How many instance records it holds.
    capacity: u64,
}

/// The query set of the occlusion queries, the buffer it resolves into, and the instance records
/// of the boxes tested, for up to `capacity` queries a frame. Clones share the GPU objects.
#[derive(Clone)]
struct Queries {
    capacity: u32,
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    boxes: wgpu::Buffer,
}

/// A buffer the results of one frame's queries are copied into, then mapped asynchronously.
struct Readback {
    buffer: wgpu::Buffer,
    capacity: u32,
    /// [`IDLE`], [`MAPPING`], [`MAPPED`] or [`FAILED`]: the mapping callback sets the last two.
    state: Arc<AtomicU8>,
    /// The frame that issued the queries, and the node of the cluster of each.
    frame: u64,
    nodes: Vec<u32>,
}

/// The depth buffer, recreated when the size of the target changes.
struct DepthBuffer {
    view: wgpu::TextureView,
    size: [u32; 2],
}

/// Draws triangle meshes placed in an `f64` world, seen from an `f64` camera, into a texture
/// view (VP-02, VP-04, GEO-07).
///
/// Objects placing the same mesh are drawn together by one instanced draw call. Each frame
/// skips the objects outside the view, draws a 12-triangle box for the detailed objects that
/// look small, and skips those smaller than about a pixel; [`RenderSettings`] tunes this, and
/// [`FrameStats`] tells what a frame drew. Clusters of objects that the GPU found hidden behind
/// others in the previous frames are skipped (occlusion culling).
///
/// The host creates the device, the queue and the target (a window surface, a browser canvas or
/// an offscreen texture); the renderer only records and submits its pass. Input is validated
/// before it reaches the GPU; what the GPU alone can refuse, such as running out of memory, goes
/// to the device's error handler, which the host chooses (wgpu's default one panics).
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    /// Draws the query boxes: depth tested, nothing written.
    query_pipeline: wgpu::RenderPipeline,
    /// Draws ghosts: lit, blended, depth tested, nothing written.
    ghost_pipeline: wgpu::RenderPipeline,
    /// Draws translucent boxes: flat, blended, depth tested, nothing written.
    box_pipeline: wgpu::RenderPipeline,
    /// Draws the lines and markers of the overlay: blended, no depth test.
    primitive_pipeline: wgpu::RenderPipeline,
    unit_cube: MeshBuffers,
    queries: Option<Queries>,
    readbacks: Vec<Readback>,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    /// The largest buffer and the widest texture of the device, which never change.
    max_buffer_size: u64,
    max_texture_side: u32,
    instances: Option<InstanceBuffer>,
    /// The instance records of the ghosts and the translucent boxes.
    ghost_instances: Option<InstanceBuffer>,
    /// The records of the lines and markers.
    primitive_instances: Option<InstanceBuffer>,
    depth: Option<DepthBuffer>,
    meshes: BTreeMap<MeshId, GpuMesh>,
    world: World,
    /// The last prepared frame, kept to reuse its allocations.
    draw_list: DrawList,
    theme: Theme,
    /// The planes frames clip with.
    clip: Clip,
    /// The overlay the host last set, drawn by every frame until it is replaced.
    overlay: Overlay,
    overlay_frame: OverlayFrame,
}

impl Renderer {
    /// Creates a renderer that draws with `device` and `queue` into views of textures of
    /// `color_format`, with the default [`RenderSettings`].
    ///
    /// # Errors
    ///
    /// [`RenderError::UnsupportedFormat`] when `color_format` is not a colour format every device
    /// can render to with a floating-point colour (an integer or depth format, for instance).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        if !is_renderable(color_format, device.features()) {
            return Err(RenderError::UnsupportedFormat(color_format));
        }
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(u64::from(UNIFORM_SIZE)),
                },
                count: None,
            }],
        });
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render frame"),
            size: u64::from(UNIFORM_SIZE),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let create = |pass| create_pipeline(device, &shader, &frame_layout, color_format, pass);
        let pipeline = create(Pass::Opaque);
        let query_pipeline = create(Pass::Query);
        let ghost_pipeline = create(Pass::Ghost);
        let box_pipeline = create(Pass::Box);
        let primitive_pipeline =
            create_primitive_pipeline(device, &shader, &frame_layout, color_format);
        let unit_cube = upload_buffers(
            device,
            &UNIT_CUBE,
            &[[0.0, 0.0, 1.0]; 8],
            &UNIT_CUBE_INDICES,
        );
        let limits = device.limits();
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            color_format,
            pipeline,
            query_pipeline,
            ghost_pipeline,
            box_pipeline,
            primitive_pipeline,
            unit_cube,
            queries: None,
            readbacks: Vec::new(),
            frame_buffer,
            frame_bind_group,
            max_buffer_size: limits.max_buffer_size,
            max_texture_side: limits.max_texture_dimension_2d,
            instances: None,
            ghost_instances: None,
            primitive_instances: None,
            depth: None,
            meshes: BTreeMap::new(),
            world: World::new(max_instances(limits.max_buffer_size)),
            draw_list: DrawList::default(),
            theme: Theme::default(),
            clip: Clip::default(),
            overlay: Overlay::default(),
            overlay_frame: OverlayFrame::default(),
        })
    }

    /// The settings of the culling and of the level of detail.
    #[must_use]
    pub fn settings(&self) -> RenderSettings {
        self.world.settings()
    }

    /// Changes the settings of the culling and of the level of detail. Every object takes its
    /// level afresh in the next frame, without hysteresis.
    ///
    /// # Errors
    ///
    /// [`RenderError::InvalidSettings`] when [`RenderSettings::is_valid`] fails.
    pub fn set_settings(&mut self, settings: RenderSettings) -> Result<(), RenderError> {
        self.world.set_settings(settings)
    }

    /// The colours of the viewport.
    #[must_use]
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Changes the colours of the selection, the hover, the ghosts and the overlay from the next
    /// frame on. The background is the host's to use: it is the clear colour it hands to
    /// [`Renderer::render`] ([`Theme::clear_color`]).
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.world.set_tints(theme.tints());
    }

    /// Sets the planes every following frame clips with, and how it shows the cut. An empty
    /// [`Clip`] clips nothing.
    pub fn set_clip(&mut self, clip: Clip) {
        self.clip = clip;
    }

    /// The planes frames clip with.
    #[must_use]
    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    /// Sets how an object is drawn: in its own colour, hovered, selected or as a ghost. Cheap:
    /// only the colour of its instance changes, nothing is uploaded again.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn set_display_state(
        &mut self,
        id: ObjectId,
        state: DisplayState,
    ) -> Result<(), RenderError> {
        self.world.set_state(id, state)
    }

    /// How an object is drawn.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn display_state(&self, id: ObjectId) -> Result<DisplayState, RenderError> {
        self.world.state(id)
    }

    /// Replaces the overlay, the transient geometry that every following frame draws over the
    /// model until it is replaced or cleared. Cheap: the overlay is copied into buffers the
    /// renderer keeps, and a ghost is one more instance of a mesh the renderer already holds.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when a ghost shows an object that is not in the renderer,
    /// or [`RenderError::OverlayTooLarge`] when the overlay, with the objects already in the
    /// ghost state, needs more records than a buffer of the device holds (set the ghost states
    /// first). Either way the previous overlay is kept: nothing is ever drawn in part.
    pub fn set_overlay(&mut self, overlay: &Overlay) -> Result<(), RenderError> {
        if let Some(missing) = overlay
            .ghost_objects()
            .find(|&id| self.world.state(id).is_err())
        {
            return Err(RenderError::UnknownObject(missing));
        }
        let (primitives, instances) = overlay.record_counts();
        let instances = instances + self.world.ghost_state_count() as u64;
        if !overlay_fits(primitives, instances, self.max_buffer_size) {
            return Err(RenderError::OverlayTooLarge);
        }
        self.overlay.clone_from(overlay);
        Ok(())
    }

    /// Removes the overlay: the next frame draws the model alone.
    pub fn clear_overlay(&mut self) {
        self.overlay.clear();
    }

    /// Validates `mesh` and uploads it to the GPU, with its coarse level (its bounding box,
    /// [`MeshData::bounding_box`]) when it has more than 12 triangles.
    ///
    /// # Errors
    ///
    /// The [`MeshError`] of [`MeshData::validate`], or [`MeshError::TooLargeForDevice`] when a
    /// buffer of the mesh exceeds the device's largest buffer.
    pub fn add_mesh(&mut self, mesh: &MeshData) -> Result<MeshId, MeshError> {
        mesh.validate()?;
        let index_count = index_count(mesh, self.max_buffer_size)?;
        // A validated mesh has vertices.
        let bounds = Aabb::of_local(mesh.origin, &mesh.positions).ok_or(MeshError::Empty)?;
        let upload = |mesh: &MeshData| {
            upload_buffers(&self.device, &mesh.positions, &mesh.normals, &mesh.indices)
        };
        let fine = upload(mesh);
        let coarse = has_coarse_level(index_count as usize / 3)
            .then(|| mesh.bounding_box())
            .flatten()
            .map(|coarse| upload(&coarse));
        let id = self.world.add_mesh(mesh.origin, bounds, coarse.is_some());
        self.meshes.insert(id, GpuMesh { fine, coarse });
        Ok(id)
    }

    /// Removes a mesh, and every object that places it.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownMesh`] when the mesh is not in the renderer.
    pub fn remove_mesh(&mut self, id: MeshId) -> Result<(), RenderError> {
        self.world.remove_mesh(id)?;
        self.meshes.remove(&id);
        Ok(())
    }

    /// Places `mesh` in the world with `placement`, drawn in `color` (linear RGBA, 0 to 1).
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownMesh`], [`RenderError::Placement`] for an invalid placement,
    /// [`RenderError::InvalidColor`], or [`RenderError::TooManyObjects`].
    pub fn add_object(
        &mut self,
        mesh: MeshId,
        placement: Placement,
        color: [f32; 4],
    ) -> Result<ObjectId, RenderError> {
        self.world.add_object(mesh, placement, color)
    }

    /// Moves an object.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`], or [`RenderError::Placement`] for an invalid placement.
    pub fn set_placement(&mut self, id: ObjectId, placement: Placement) -> Result<(), RenderError> {
        self.world.set_placement(id, placement)
    }

    /// Removes an object; its mesh stays.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn remove_object(&mut self, id: ObjectId) -> Result<(), RenderError> {
        self.world.remove_object(id)
    }

    /// The level of detail the object was drawn at in the last prepared frame, or `None` when it
    /// was not drawn (outside the view, hidden behind others, smaller than the small-feature
    /// threshold, or no frame prepared since it was added).
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn drawn_level(&self, id: ObjectId) -> Result<Option<DetailLevel>, RenderError> {
        self.world.drawn_level(id)
    }

    /// Calls `found` with every object whose world bounds meet the part of the view seen through
    /// `rect`, `[left, top, right, bottom]` in pixels of a target of `size` (origin at the top
    /// left, y down; it may reach beyond the target). Culling, occlusion and level of detail do
    /// not matter: an object hidden behind another is reported too. It is the broad phase of
    /// picking and snapping under the pointer, tested in `f64` relative to the eye like the
    /// culling, so as exact 10⁷ m from the origin as at it.
    ///
    /// # Errors
    ///
    /// [`RenderError::ZeroSizedTarget`], [`RenderError::InvalidRect`] when `rect` is not finite
    /// or its left is right of its right or its top below its bottom, or
    /// [`RenderError::Camera`] for an invalid camera.
    pub fn objects_in_rect(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        rect: [f64; 4],
        found: impl FnMut(ObjectId),
    ) -> Result<(), RenderError> {
        let frustum = rect_frustum(camera, size, rect)?;
        self.world.objects_in(&frustum, found);
        Ok(())
    }

    /// Prepares a frame seen from `camera` on a target of `size` pixels, without drawing it:
    /// culls the objects, chooses their levels of detail and computes their instance records,
    /// all on the CPU. [`Renderer::render`] does this before drawing; on its own, it measures
    /// or checks those decisions. It uses the occlusion results already delivered but issues no
    /// queries, since it draws nothing.
    ///
    /// # Errors
    ///
    /// [`RenderError::ZeroSizedTarget`], [`RenderError::TargetTooLarge`], or
    /// [`RenderError::Camera`] for an invalid camera or a projection that does not suit `size`.
    pub fn prepare_frame(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
    ) -> Result<FrameStats, RenderError> {
        self.check_size(size)?;
        self.prepare(camera, size, 0).map(|_| self.draw_list.stats)
    }

    /// Draws the objects into `target`, seen from `camera`, over the `clear` colour (linear
    /// RGBA), submits the pass to the queue, and tells what the frame drew.
    ///
    /// The frame skips the clusters that delivered occlusion results found hidden, and tests
    /// others for the next frames. Results arrive when the host's event loop lets wgpu run the
    /// mapping callbacks (a browser does it by itself; natively, any `Device::poll`, which this
    /// method also calls without waiting).
    ///
    /// `target` must view a single-sampled 2D texture of the renderer's device and format, with
    /// one mip level and one layer, `size` pixels wide and high, usable as a render attachment.
    /// All of that is checked but the texture's device and the view's own format, which wgpu
    /// does not expose: the texture must come from the renderer's device and the view must keep
    /// the texture's format, or wgpu reports a validation error to the device.
    ///
    /// # Errors
    ///
    /// [`RenderError::ZeroSizedTarget`], [`RenderError::TargetTooLarge`],
    /// [`RenderError::TargetMismatch`], [`RenderError::InvalidClearColor`], or
    /// [`RenderError::Camera`] for an invalid camera.
    pub fn render(
        &mut self,
        target: &wgpu::TextureView,
        size: [u32; 2],
        camera: &Camera,
        clear: [f64; 4],
    ) -> Result<FrameStats, RenderError> {
        self.check_target(target, size)?;
        if !clear.iter().all(|c| c.is_finite()) {
            return Err(RenderError::InvalidClearColor);
        }
        let budget = self.collect_occlusion();
        let (projection, view) = self.prepare(camera, size, budget)?;
        self.overlay_frame.prepare(
            &self.overlay,
            &self.theme,
            &view,
            &ClipVolume::new(&camera.projection, f64::from(size[0]) / f64::from(size[1])),
            &self.world,
            &mut self.draw_list,
        );
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::cast_slice(&frame_uniform(
                &projection,
                size,
                &self.clip.uniform(&view, self.theme.cut),
            )),
        );
        let instances = self.upload_instances();
        let ghosts = self.upload_ghosts();
        let primitives = self.upload_primitives();
        let queries = self.upload_queries();
        let depth = self.depth_view(size);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render opaque"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(FAR_DEPTH),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: queries.as_ref().map(|queries| &queries.set),
                multiview_mask: None,
            });
            if let Some(instances) = &instances {
                self.draw(&mut pass, instances);
            }
            if let Some(queries) = &queries {
                self.draw_queries(&mut pass, &queries.boxes);
            }
            self.draw_overlay(&mut pass, ghosts.as_ref(), primitives.as_ref());
        }
        let readback = queries.and_then(|queries| self.copy_results(&mut encoder, &queries));
        self.queue.submit([encoder.finish()]);
        if let Some(index) = readback {
            self.map_results(index);
        }
        Ok(self.draw_list.stats)
    }

    /// Reads the query results the GPU has delivered into the world, frees their buffers, and
    /// returns how many queries this frame may issue: none when every readback buffer is still
    /// on its way back, otherwise as many as the settings allow.
    fn collect_occlusion(&mut self) -> usize {
        // Without waiting: runs the callbacks of the mappings that are done (natively; a browser
        // runs them by itself).
        drop(self.device.poll(wgpu::PollType::Poll));
        for readback in &mut self.readbacks {
            match readback.state.load(Ordering::Acquire) {
                MAPPED => {
                    let bytes = readback.nodes.len() as u64 * QUERY_RESULT_SIZE;
                    if let Ok(view) = readback.buffer.get_mapped_range(..bytes) {
                        let (results, _) = view.as_chunks::<8>();
                        for (&node, result) in readback.nodes.iter().zip(results) {
                            let samples = u64::from_ne_bytes(*result);
                            self.world
                                .record_occlusion(readback.frame, node, samples != 0);
                        }
                    }
                    readback.buffer.unmap();
                    readback.state.store(IDLE, Ordering::Release);
                }
                FAILED => readback.state.store(IDLE, Ordering::Release),
                _ => {}
            }
        }
        // The world bounds the queries by the settings; here, only a free buffer matters.
        if self.free_readback().is_some() {
            usize::MAX
        } else {
            0
        }
    }

    /// Writes the boxes of the frame's queries; returns the query resources, or `None` when the
    /// frame tests no cluster.
    fn upload_queries(&mut self) -> Option<Queries> {
        let count = u32::try_from(self.draw_list.queries.len()).ok()?;
        if count == 0 {
            return None;
        }
        let queries = match self.queries.take() {
            Some(queries) if queries.capacity >= count => queries,
            _ => {
                // The budget bounds the count; sizing for it avoids a new set at each change.
                let capacity = self.world.settings().occlusion_queries.max(count);
                Queries {
                    capacity,
                    set: self.device.create_query_set(&wgpu::QuerySetDescriptor {
                        label: Some("render occlusion"),
                        ty: wgpu::QueryType::Occlusion,
                        count: capacity,
                    }),
                    resolve: self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("render occlusion resolve"),
                        size: u64::from(capacity) * QUERY_RESULT_SIZE,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    }),
                    boxes: self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("render occlusion boxes"),
                        size: u64::from(capacity) * INSTANCE_STRIDE,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                }
            }
        };
        self.queue
            .write_buffer(&queries.boxes, 0, self.draw_list.query_bytes());
        self.queries = Some(queries.clone());
        Some(queries)
    }

    /// One occlusion query per box, after every object is drawn.
    fn draw_queries(&self, pass: &mut wgpu::RenderPass<'_>, boxes: &wgpu::Buffer) {
        pass.set_pipeline(&self.query_pipeline);
        // A frame that draws no object has not bound the frame uniform yet.
        pass.set_bind_group(0, &self.frame_bind_group, &[]);
        pass.set_vertex_buffer(0, self.unit_cube.positions.slice(..));
        pass.set_vertex_buffer(1, self.unit_cube.normals.slice(..));
        pass.set_vertex_buffer(2, boxes.slice(..));
        pass.set_index_buffer(self.unit_cube.indices.slice(..), wgpu::IndexFormat::Uint32);
        for index in 0..self.draw_list.queries.len() {
            // The budget keeps the count within u32.
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            pass.begin_occlusion_query(index);
            pass.draw_indexed(0..self.unit_cube.index_count, 0, index..index + 1);
            pass.end_occlusion_query();
        }
    }

    /// The first idle readback buffer, or the index of a new one when fewer than
    /// [`MAX_READBACKS`] exist; `None` when all are on their way back.
    fn free_readback(&self) -> Option<usize> {
        self.readbacks
            .iter()
            .position(|readback| readback.state.load(Ordering::Acquire) == IDLE)
            .or_else(|| (self.readbacks.len() < MAX_READBACKS).then_some(self.readbacks.len()))
    }

    /// Resolves the frame's queries and copies their results into a free readback buffer, made
    /// as large as the query set when it is smaller; returns its index. The frame issued
    /// queries only because [`Renderer::free_readback`] found one.
    fn copy_results(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queries: &Queries,
    ) -> Option<usize> {
        let index = self.free_readback()?;
        if self
            .readbacks
            .get(index)
            .is_none_or(|readback| readback.capacity < queries.capacity)
        {
            let readback = Readback {
                buffer: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("render occlusion readback"),
                    size: u64::from(queries.capacity) * QUERY_RESULT_SIZE,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                capacity: queries.capacity,
                state: Arc::new(AtomicU8::new(IDLE)),
                frame: 0,
                nodes: Vec::new(),
            };
            if index < self.readbacks.len() {
                self.readbacks[index] = readback;
            } else {
                self.readbacks.push(readback);
            }
        }
        let readback = &mut self.readbacks[index];
        // The budget keeps the count within u32.
        let count = u32::try_from(self.draw_list.queries.len()).unwrap_or(u32::MAX);
        encoder.resolve_query_set(&queries.set, 0..count, &queries.resolve, 0);
        encoder.copy_buffer_to_buffer(
            &queries.resolve,
            0,
            &readback.buffer,
            0,
            u64::from(count) * QUERY_RESULT_SIZE,
        );
        readback.frame = self.draw_list.frame;
        // The world clears the list at the next frame: take it rather than copy it.
        core::mem::swap(&mut readback.nodes, &mut self.draw_list.queries);
        Some(index)
    }

    /// Maps a readback buffer once its copy is submitted; its callback marks it mapped.
    fn map_results(&self, index: usize) {
        let readback = &self.readbacks[index];
        let bytes = readback.nodes.len() as u64 * QUERY_RESULT_SIZE;
        readback.state.store(MAPPING, Ordering::Release);
        let state = Arc::clone(&readback.state);
        readback
            .buffer
            .map_async(wgpu::MapMode::Read, ..bytes, move |result| {
                let done = if result.is_ok() { MAPPED } else { FAILED };
                state.store(done, Ordering::Release);
            });
    }

    /// Validates the camera and prepares the draw list with at most `query_budget` occlusion
    /// queries; returns the projection matrix.
    fn prepare(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        query_budget: usize,
    ) -> Result<(Matrix, ViewFrame), RenderError> {
        let view = ViewFrame::new(camera)?;
        let projection =
            projection_matrix(&camera.projection, f64::from(size[0]) / f64::from(size[1]))?;
        self.world.prepare(
            &view,
            &camera.projection,
            self.clip.planes(),
            size,
            query_budget,
            &mut self.draw_list,
        );
        Ok((projection, view))
    }

    fn check_size(&self, size: [u32; 2]) -> Result<(), RenderError> {
        if size.contains(&0) {
            return Err(RenderError::ZeroSizedTarget);
        }
        let max = self.max_texture_side;
        if size.iter().any(|&side| side > max) {
            return Err(RenderError::TargetTooLarge { max });
        }
        Ok(())
    }

    fn check_target(&self, target: &wgpu::TextureView, size: [u32; 2]) -> Result<(), RenderError> {
        self.check_size(size)?;
        let texture = target.texture();
        let matches = texture.format() == self.color_format
            && texture.dimension() == wgpu::TextureDimension::D2
            && texture.sample_count() == 1
            && texture.mip_level_count() == 1
            && texture.depth_or_array_layers() == 1
            && texture
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            && [texture.width(), texture.height()] == size;
        if matches {
            Ok(())
        } else {
            Err(RenderError::TargetMismatch)
        }
    }

    /// Writes the instance records of the prepared frame, and only those, to the instance
    /// buffer, grown by doubling; `None` when the frame draws nothing.
    fn upload_instances(&mut self) -> Option<wgpu::Buffer> {
        upload_records(
            &self.device,
            &self.queue,
            &mut self.instances,
            self.draw_list.instance_bytes(),
            &Records {
                stride: INSTANCE_STRIDE,
                max_buffer_size: self.max_buffer_size,
                label: "render instances",
            },
        )
    }

    /// Writes the instance records of the ghosts and the translucent boxes.
    fn upload_ghosts(&mut self) -> Option<wgpu::Buffer> {
        upload_records(
            &self.device,
            &self.queue,
            &mut self.ghost_instances,
            self.draw_list.ghost_bytes(),
            &Records {
                stride: INSTANCE_STRIDE,
                max_buffer_size: self.max_buffer_size,
                label: "render ghosts",
            },
        )
    }

    /// Writes the records of the lines and markers.
    fn upload_primitives(&mut self) -> Option<wgpu::Buffer> {
        upload_records(
            &self.device,
            &self.queue,
            &mut self.primitive_instances,
            self.overlay_frame.primitive_bytes(),
            &Records {
                stride: PRIMITIVE_STRIDE,
                max_buffer_size: self.max_buffer_size,
                label: "render overlay primitives",
            },
        )
    }

    /// The depth buffer for a target of `size`, created on the first frame and on resizes.
    fn depth_view(&mut self, size: [u32; 2]) -> wgpu::TextureView {
        if let Some(depth) = self.depth.as_ref().filter(|depth| depth.size == size) {
            return depth.view.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render depth"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.depth = Some(DepthBuffer {
            view: view.clone(),
            size,
        });
        view
    }

    /// The overlay pass: ghosts and translucent boxes, depth tested, then lines and markers on
    /// top of everything.
    fn draw_overlay(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        ghosts: Option<&wgpu::Buffer>,
        primitives: Option<&wgpu::Buffer>,
    ) {
        if ghosts.is_some() || primitives.is_some() {
            // A frame that draws no object has not bound the frame uniform yet.
            pass.set_bind_group(0, &self.frame_bind_group, &[]);
        }
        if let Some(ghosts) = ghosts {
            pass.set_pipeline(&self.ghost_pipeline);
            pass.set_vertex_buffer(2, ghosts.slice(..));
            for bucket in &self.draw_list.ghost_buckets {
                if let Some(mesh) = self.meshes.get(&bucket.mesh) {
                    set_mesh(pass, &mesh.fine);
                    pass.draw_indexed(
                        0..mesh.fine.index_count,
                        0,
                        bucket.first..bucket.first + bucket.count,
                    );
                }
            }
            let (first, count) = self.draw_list.boxes;
            if count > 0 {
                pass.set_pipeline(&self.box_pipeline);
                set_mesh(pass, &self.unit_cube);
                pass.draw_indexed(0..self.unit_cube.index_count, 0, first..first + count);
            }
        }
        let count = self.overlay_frame.primitive_count();
        if let (Some(primitives), true) = (primitives, count > 0) {
            pass.set_pipeline(&self.primitive_pipeline);
            pass.set_vertex_buffer(0, primitives.slice(..));
            pass.draw(0..6, 0..count);
        }
    }

    /// One instanced draw call per bucket of the prepared frame.
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, instances: &wgpu::Buffer) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame_bind_group, &[]);
        pass.set_vertex_buffer(2, instances.slice(..));
        for bucket in &self.draw_list.buckets {
            let Some(mesh) = self.meshes.get(&bucket.mesh) else {
                continue;
            };
            let buffers = match bucket.level {
                DetailLevel::Fine => &mesh.fine,
                // The world puts objects in a coarse bucket only for meshes with a coarse level.
                DetailLevel::Coarse => {
                    let Some(coarse) = &mesh.coarse else {
                        continue;
                    };
                    coarse
                }
            };
            set_mesh(pass, buffers);
            pass.draw_indexed(
                0..buffers.index_count,
                0,
                bucket.first..bucket.first + bucket.count,
            );
        }
    }
}

/// Binds the vertices and indices of a mesh level.
fn set_mesh(pass: &mut wgpu::RenderPass<'_>, buffers: &MeshBuffers) {
    pass.set_vertex_buffer(0, buffers.positions.slice(..));
    pass.set_vertex_buffer(1, buffers.normals.slice(..));
    pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
}

/// What a growing buffer of records is made of.
struct Records {
    /// Bytes of a record.
    stride: u64,
    max_buffer_size: u64,
    label: &'static str,
}

/// Writes `bytes`, whole records, to the buffer in `slot`, which grows by doubling when it is
/// too small; `None` when there is nothing to write.
fn upload_records(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &mut Option<InstanceBuffer>,
    bytes: &[u8],
    records: &Records,
) -> Option<wgpu::Buffer> {
    if bytes.is_empty() {
        return None;
    }
    let count = bytes.len() as u64 / records.stride;
    let buffer = if let Some(held) = slot.as_ref().filter(|held| held.capacity >= count) {
        held.buffer.clone()
    } else {
        // The renderer bounds what it draws so that the records fit the largest buffer, with
        // indices within u32.
        let limit = (records.max_buffer_size / records.stride).min(u64::from(u32::MAX));
        let capacity = count.next_power_of_two().min(limit);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(records.label),
            size: capacity * records.stride,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *slot = Some(InstanceBuffer {
            buffer: buffer.clone(),
            capacity,
        });
        buffer
    };
    queue.write_buffer(&buffer, 0, bytes);
    Some(buffer)
}

/// Uploads the vertices and indices of a mesh, validated or built by the renderer.
fn upload_buffers(
    device: &wgpu::Device,
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
) -> MeshBuffers {
    let buffer = |label, contents: &[u8], usage| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage,
        })
    };
    MeshBuffers {
        positions: buffer(
            "render mesh positions",
            bytemuck::cast_slice(positions),
            wgpu::BufferUsages::VERTEX,
        ),
        normals: buffer(
            "render mesh normals",
            bytemuck::cast_slice(normals),
            wgpu::BufferUsages::VERTEX,
        ),
        indices: buffer(
            "render mesh indices",
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
        ),
        // Validation bounds the indices of a mesh within u32.
        index_count: u32::try_from(indices.len()).unwrap_or(u32::MAX),
    }
}

/// The number of indices of a validated mesh, when its buffers fit the device's largest buffer of
/// `max` bytes.
fn index_count(mesh: &MeshData, max: u64) -> Result<u32, MeshError> {
    // Validation bounds the counts far below u64::MAX / 12.
    let bytes =
        (mesh.positions.len() as u64 * VERTEX_STRIDE).max(mesh.indices.len() as u64 * INDEX_SIZE);
    match u32::try_from(mesh.indices.len()) {
        Ok(count) if bytes <= max => Ok(count),
        _ => Err(MeshError::TooLargeForDevice { bytes, max }),
    }
}

/// The most objects one frame can draw: as many instance records as the device's largest
/// buffer holds, with instance indices within `u32`.
fn max_instances(max_buffer_size: u64) -> usize {
    let count = (max_buffer_size / INSTANCE_STRIDE).min(u64::from(u32::MAX));
    usize::try_from(count).unwrap_or(usize::MAX)
}

/// The frustum seen through `rect`, in pixels of a target of `size`; see
/// [`Renderer::objects_in_rect`].
fn rect_frustum(camera: &Camera, size: [u32; 2], rect: [f64; 4]) -> Result<Frustum, RenderError> {
    if size.contains(&0) {
        return Err(RenderError::ZeroSizedTarget);
    }
    let [left, top, right, bottom] = rect;
    if !rect.iter().all(|v| v.is_finite()) || left > right || top > bottom {
        return Err(RenderError::InvalidRect);
    }
    camera.validate()?;
    let view = ViewFrame::new(camera)?;
    let [width, height] = size.map(f64::from);
    let ndc_x = |x: f64| 2.0 * x / width - 1.0;
    let ndc_y = |y: f64| 1.0 - 2.0 * y / height;
    Ok(Frustum::window(
        &view,
        &camera.projection,
        width / height,
        [ndc_x(left), ndc_y(bottom), ndc_x(right), ndc_y(top)],
    ))
}

/// Whether `primitives` line and marker records and `instances` ghost and box records each fit
/// a buffer of `max_buffer_size` bytes, with indices within `u32`.
fn overlay_fits(primitives: u64, instances: u64, max_buffer_size: u64) -> bool {
    let fits = |count: u64, stride: u64| {
        u32::try_from(count).is_ok() && count.saturating_mul(stride) <= max_buffer_size
    };
    fits(primitives, PRIMITIVE_STRIDE) && fits(instances, INSTANCE_STRIDE)
}

/// Whether every device can render a floating-point colour into textures of `format`.
fn is_renderable(format: wgpu::TextureFormat, features: wgpu::Features) -> bool {
    format.has_color_aspect()
        && matches!(
            format.sample_type(None, Some(features)),
            Some(wgpu::TextureSampleType::Float { .. })
        )
        && format
            .guaranteed_format_features(features)
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
}

/// What a pipeline draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// The objects: depth tested and written, colour written.
    Opaque,
    /// The boxes of the occlusion queries: depth tested with `GreaterEqual`, so a box at the
    /// depth of what it holds still counts as visible; nothing written.
    Query,
    /// Ghosts: lit like the objects, blended over them, depth tested, nothing written.
    Ghost,
    /// Translucent boxes: flat, blended, depth tested, nothing written.
    Box,
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    frame_layout: &wgpu::BindGroupLayout,
    color_format: wgpu::TextureFormat,
    pass: Pass,
) -> wgpu::RenderPipeline {
    let query = pass == Pass::Query;
    let blended = matches!(pass, Pass::Ghost | Pass::Box);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render"),
        bind_group_layouts: &[Some(frame_layout)],
        immediate_size: 0,
    });
    let attribute = |location| {
        [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: location,
        }]
    };
    let (position, normal) = (attribute(0), attribute(1));
    let buffer = |attributes| wgpu::VertexBufferLayout {
        array_stride: VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes,
    };
    let instance = instance_attributes();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match pass {
            Pass::Opaque => "render opaque",
            Pass::Query => "render query",
            Pass::Ghost => "render ghosts",
            Pass::Box => "render boxes",
        }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[
                Some(buffer(&position)),
                Some(buffer(&normal)),
                Some(wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_STRIDE,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance,
                }),
            ],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // BIM solids from IFC are not always consistently oriented.
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(pass == Pass::Opaque),
            // Reverse-Z: nearer is larger. Previews at the depth of a surface, such as the ghost
            // of a move that has not moved yet, must show over it.
            depth_compare: Some(if query || blended {
                wgpu::CompareFunction::GreaterEqual
            } else {
                wgpu::CompareFunction::Greater
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        // No MSAA: deterministic pixels for the visual test.
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if pass == Pass::Box {
                "fs_flat"
            } else {
                "fs_main"
            }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: blended.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: if query {
                    wgpu::ColorWrites::empty()
                } else {
                    wgpu::ColorWrites::ALL
                },
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// A length in pixels as `f32`.
#[expect(
    clippy::cast_precision_loss,
    reason = "sizes are at most the device's texture limit, far below 2²⁴"
)]
fn pixels(length: u32) -> f32 {
    length as f32
}

/// The pipeline of the lines and markers of the overlay: one instance per primitive, drawn as a
/// quad of six vertices, blended, over everything (no depth test, nothing written).
fn create_primitive_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    frame_layout: &wgpu::BindGroupLayout,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render overlay"),
        bind_group_layouts: &[Some(frame_layout)],
        immediate_size: 0,
    });
    let attributes: [wgpu::VertexAttribute; PRIMITIVE_ATTRIBUTES] =
        core::array::from_fn(|index| wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            // At most 4 · 16 bytes: no truncation.
            offset: index as u64 * 16,
            shader_location: u32::try_from(index).unwrap_or(0),
        });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render overlay primitives"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_primitive"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: PRIMITIVE_STRIDE,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        // The pass has a depth attachment, so the pipeline declares it; nothing is tested or
        // written, the overlay is drawn over the model whatever is in front.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_primitive"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The frame uniform: the projection, then the light direction with the ambient share, then the
/// size of the target in pixels, then the clipping block ([`Clip::uniform`]).
fn frame_uniform(
    projection: &Matrix,
    size: [u32; 2],
    clip: &[f32; CLIP_FLOATS],
) -> [f32; FRAME_FLOATS] {
    let mut uniform = [0.0; FRAME_FLOATS];
    uniform[CLIP_START..].copy_from_slice(clip);
    uniform[..16].copy_from_slice(projection.as_flattened());
    uniform[16..19].copy_from_slice(&LIGHT_DIRECTION);
    uniform[19] = AMBIENT;
    uniform[20] = pixels(size[0]);
    uniform[21] = pixels(size[1]);
    uniform
}

/// The attributes of an instance record ([`INSTANCE_FLOATS`]): the four columns of the
/// model-view at locations 2 to 5, then the colour at location 6, each four `f32`.
fn instance_attributes() -> [wgpu::VertexAttribute; INSTANCE_FLOATS / 4] {
    core::array::from_fn(|index| wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        // At most 4 · 16 bytes: no truncation.
        offset: index as u64 * 16,
        shader_location: u32::try_from(index).unwrap_or(0) + 2,
    })
}

/// Requests a device without a surface, for tests and tools. The backends and the power
/// preference can be chosen with the `WGPU_BACKEND` and `WGPU_POWER_PREF` environment variables.
/// Async, it never blocks (ARCH-02).
///
/// # Errors
///
/// [`RenderError::NoAdapter`] when no GPU adapter is available, [`RenderError::NoDevice`] when
/// the adapter cannot create a device with the default limits.
pub async fn headless_device() -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
    let descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    let backends = descriptor.backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::from_env().unwrap_or_default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| RenderError::NoAdapter {
            backends,
            reason: error.to_string(),
        })?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("render headless"),
            ..wgpu::DeviceDescriptor::default()
        })
        .await
        .map_err(|error| RenderError::NoDevice {
            adapter: adapter.get_info().name,
            reason: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Projection;

    fn mesh(vertices: usize, indices: usize) -> MeshData {
        MeshData {
            origin: [0.0; 3],
            positions: vec![[0.0; 3]; vertices],
            normals: vec![[0.0, 0.0, 1.0]; vertices],
            indices: vec![0; indices],
        }
    }

    /// Unit cubes at x = −2, 0 and 2 (and one 5 m behind the middle one), 5 m in front of a
    /// camera looking along +y, all offset by `offset`; returns the objects a rectangle of a
    /// 100 × 100 target meets, as their positions in that list.
    fn objects_in(offset: f64, rect: [f64; 4], projection: Projection) -> Vec<usize> {
        let mut world = World::new(16);
        let bounds = Aabb::of_local([0.0; 3], &[[-0.5; 3], [0.5; 3]]).unwrap();
        let mesh = world.add_mesh([0.0; 3], bounds, false);
        let objects: Vec<ObjectId> = [
            [-2.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 5.0, 0.0],
        ]
        .into_iter()
        .map(|at| {
            let at = at.map(|v: f64| v + offset);
            world
                .add_object(mesh, Placement::from_translation(at), [1.0; 4])
                .unwrap()
        })
        .collect();
        let camera = Camera {
            eye: geom::Point3::new(offset, offset - 5.0, offset),
            target: geom::Point3::new(offset, offset, offset),
            up: geom::Vector3::new(0.0, 0.0, 1.0),
            projection,
        };
        let frustum = rect_frustum(&camera, [100, 100], rect).unwrap();
        let mut found = Vec::new();
        world.objects_in(&frustum, |id| {
            found.push(objects.iter().position(|&o| o == id).unwrap());
        });
        found.sort_unstable();
        found
    }

    #[test]
    fn a_rectangle_finds_the_objects_behind_it_whatever_the_distance_to_the_origin() {
        let perspective = Projection::Perspective {
            fov_y_radians: core::f64::consts::FRAC_PI_2,
            near: 0.1,
        };
        // 90° over 100 px: at 5 m, a pixel is 0.1 m; the cubes span x ∈ [−2.5, −1.5], [−0.5,
        // 0.5] and [1.5, 2.5], so pixels 25–35, 45–55 and 65–75; the far one hides behind the
        // middle one.
        let parallel = Projection::Orthographic {
            height: 10.0,
            near: 0.1,
            far: 100.0,
        };
        for offset in [0.0, 1e6, 1e7] {
            assert_eq!(
                objects_in(offset, [49.0, 49.0, 51.0, 51.0], perspective),
                [1, 3]
            );
            assert_eq!(
                objects_in(offset, [29.0, 49.0, 31.0, 51.0], perspective),
                [0]
            );
            assert_eq!(
                objects_in(offset, [38.0, 49.0, 42.0, 51.0], perspective),
                Vec::<usize>::new()
            );
            assert_eq!(
                objects_in(offset, [20.0, 49.0, 80.0, 51.0], perspective),
                [0, 1, 2, 3]
            );
            // Above the cubes, and off the target entirely.
            assert_eq!(
                objects_in(offset, [45.0, 10.0, 55.0, 20.0], perspective),
                Vec::<usize>::new()
            );
            assert_eq!(
                objects_in(offset, [-90.0, 49.0, -80.0, 51.0], perspective),
                Vec::<usize>::new()
            );
            // In parallel, the far cube is the same size as the near ones.
            assert_eq!(objects_in(offset, [69.0, 49.0, 71.0, 51.0], parallel), [2]);
            assert_eq!(
                objects_in(offset, [49.0, 49.0, 51.0, 51.0], parallel),
                [1, 3]
            );
        }
    }

    /// The pixel where the GPU draws the world point `q`: `P · V · (q − eye)`, then the viewport
    /// transform, from the top left.
    fn drawn_at(camera: &Camera, size: [u32; 2], q: [f64; 3]) -> [f64; 2] {
        let view = ViewFrame::new(camera).unwrap();
        let [width, height] = size.map(f64::from);
        let projection = projection_matrix(&camera.projection, width / height).unwrap();
        let p = crate::vector::apply(&view.rotation, crate::vector::sub(q, view.eye));
        let clip: [f64; 4] = core::array::from_fn(|row| {
            (0..3)
                .map(|k| f64::from(projection[k][row]) * p[k])
                .sum::<f64>()
                + f64::from(projection[3][row])
        });
        let [x, y] = [clip[0] / clip[3], clip[1] / clip[3]];
        [
            f64::midpoint(x, 1.0) * width,
            f64::midpoint(1.0, -y) * height,
        ]
    }

    #[test]
    fn a_rectangle_finds_what_is_drawn_under_it_in_any_view() {
        use crate::bounds::Aabb;
        use crate::bvh::Overlap;

        let size = [400, 300];
        let target = [5.0, 3.0, 1.5];
        // A Top view looking down −z with north up, as the viewport builds it (its forward is
        // not exactly vertical), an isometric view from the south-east, and one from below.
        let views = [
            ([5.0, 3.0 - 1e-12, 40.0], [0.0, 1.0, 0.0]),
            ([25.0, -17.0, 18.0], [0.0, 0.0, 1.0]),
            ([-10.0, 20.0, -12.0], [0.0, 0.0, 1.0]),
        ];
        let projections = [
            Projection::Perspective {
                fov_y_radians: 0.87,
                near: 0.04,
            },
            Projection::Orthographic {
                height: 14.0,
                near: 0.001,
                far: 100.0,
            },
        ];
        for (eye, up) in views {
            for projection in projections {
                let camera = Camera {
                    eye: geom::Point3::new(eye[0], eye[1], eye[2]),
                    target: geom::Point3::new(target[0], target[1], target[2]),
                    up: geom::Vector3::new(up[0], up[1], up[2]),
                    projection,
                };
                for q in [target, [2.0, 1.0, 0.0], [8.0, 6.0, 3.0], [1.0, 7.0, 6.0]] {
                    let [x, y] = drawn_at(&camera, size, q);
                    let speck = Aabb {
                        min: q.map(|v| v - 1e-3),
                        max: q.map(|v| v + 1e-3),
                    };
                    let found = |dx: f64| {
                        let rect = [x + dx - 1.0, y - 1.0, x + dx + 1.0, y + 1.0];
                        let frustum = rect_frustum(&camera, size, rect).unwrap();
                        !matches!(
                            frustum.classify(&speck, frustum.all_planes()),
                            Overlap::Outside
                        )
                    };
                    assert!(found(0.0), "{eye:?} {projection:?} {q:?} at {x} {y}");
                    assert!(
                        !found(20.0) && !found(-20.0),
                        "{eye:?} {projection:?} {q:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rectangle_must_be_finite_and_upright() {
        let camera = Camera {
            eye: geom::Point3::new(0.0, -5.0, 0.0),
            target: geom::Point3::new(0.0, 0.0, 0.0),
            up: geom::Vector3::new(0.0, 0.0, 1.0),
            projection: Projection::Perspective {
                fov_y_radians: 1.0,
                near: 0.1,
            },
        };
        for rect in [
            [f64::NAN, 0.0, 1.0, 1.0],
            [0.0, 0.0, f64::INFINITY, 1.0],
            [2.0, 0.0, 1.0, 1.0],
            [0.0, 2.0, 1.0, 1.0],
        ] {
            assert!(matches!(
                rect_frustum(&camera, [10, 10], rect),
                Err(RenderError::InvalidRect)
            ));
        }
        assert!(matches!(
            rect_frustum(&camera, [0, 10], [0.0; 4]),
            Err(RenderError::ZeroSizedTarget)
        ));
        let mut bad = camera;
        bad.target = bad.eye;
        assert!(matches!(
            rect_frustum(&bad, [10, 10], [0.0; 4]),
            Err(RenderError::Camera(CameraError::EyeAtTarget))
        ));
    }

    /// On a device with a tiny largest buffer, one record too many is refused, never truncated.
    #[test]
    fn an_overlay_must_fit_the_largest_buffer() {
        let lines = 2 * PRIMITIVE_STRIDE;
        assert!(overlay_fits(2, 0, lines));
        assert!(!overlay_fits(3, 0, lines));
        let ghosts = 3 * INSTANCE_STRIDE;
        assert!(overlay_fits(0, 3, ghosts));
        assert!(!overlay_fits(0, 4, ghosts));
        assert!(overlay_fits(0, 0, 0));
        // Indices stay within u32 whatever the buffer.
        assert!(!overlay_fits(u64::from(u32::MAX) + 1, 0, u64::MAX));
        assert!(!overlay_fits(0, u64::from(u32::MAX) + 1, u64::MAX));
    }

    #[test]
    fn meshes_must_fit_the_largest_buffer() {
        assert_eq!(index_count(&mesh(3, 6), 36), Ok(6));
        assert_eq!(
            index_count(&mesh(4, 6), 36),
            Err(MeshError::TooLargeForDevice { bytes: 48, max: 36 })
        );
        assert_eq!(
            index_count(&mesh(3, 12), 36),
            Err(MeshError::TooLargeForDevice { bytes: 48, max: 36 })
        );
    }

    #[test]
    fn the_instance_buffer_bounds_the_objects() {
        // WebGPU's default largest buffer, 256 MiB, holds 3 355 443 records of 80 bytes.
        assert_eq!(max_instances(256 << 20), 3_355_443);
        assert_eq!(max_instances(799), 9);
        // Instance indices are u32.
        assert_eq!(max_instances(u64::MAX), u32::MAX as usize);
    }

    #[test]
    fn instance_attributes_cover_the_record() {
        let attributes = instance_attributes();
        assert_eq!(attributes.len() as u64 * 16, INSTANCE_STRIDE);
        for (index, attribute) in attributes.iter().enumerate() {
            assert_eq!(attribute.offset, index as u64 * 16);
            assert_eq!(attribute.shader_location as usize, index + 2);
        }
    }

    #[test]
    fn only_colour_formats_with_float_output_are_renderable() {
        let features = wgpu::Features::empty();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba16Float,
        ] {
            assert!(is_renderable(format, features), "{format:?}");
        }
        for format in [
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureFormat::R32Uint,
            wgpu::TextureFormat::Rgba8Snorm,
        ] {
            assert!(!is_renderable(format, features), "{format:?}");
        }
    }

    #[test]
    fn the_frame_uniform_puts_the_projection_first() {
        let matrix: Matrix = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        let clip = [0.5; CLIP_FLOATS];
        let frame = frame_uniform(&matrix, [640, 480], &clip);
        assert_eq!(&frame[CLIP_START..], &clip);
        assert_eq!(&frame[..16], matrix.as_flattened());
        let [x, y, z] = LIGHT_DIRECTION;
        assert_eq!(&frame[16..22], &[x, y, z, AMBIENT, 640.0, 480.0]);
        assert!((x * x + y * y + z * z - 1.0).abs() < 1e-6);
        assert_eq!(size_of_val(&frame), UNIFORM_SIZE as usize);
    }

    /// The offsets of the WGSL `Frame` struct, from the shader as the GPU compiles it, against
    /// the floats [`frame_uniform`] and [`Clip::uniform`] write (native: naga comes with wgpu-core).
    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn the_frame_uniform_matches_the_shader_layout() {
        use crate::clip::{CLIP_PARAMS, CUT_COLOR, MAX_CLIP_PLANES};
        use wgpu::naga;

        let module = naga::front::wgsl::parse_str(include_str!("shader.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).unwrap();
        let (handle, frame) = module
            .types
            .iter()
            .find(|(_, ty)| ty.name.as_deref() == Some("Frame"))
            .unwrap();
        let naga::TypeInner::Struct { members, span } = &frame.inner else {
            panic!("Frame is a struct");
        };
        let offsets: Vec<(&str, u32)> = members
            .iter()
            .map(|member| (member.name.as_deref().unwrap(), member.offset / 4))
            .collect();
        let floats = |at: usize| u32::try_from(at).unwrap();
        assert_eq!(
            offsets,
            [
                ("projection", 0),
                ("light", 16),
                ("viewport", 20),
                ("planes", floats(CLIP_START)),
                ("clip", floats(CLIP_START + CLIP_PARAMS)),
                ("cut_color", floats(CLIP_START + CUT_COLOR)),
            ]
        );
        assert_eq!(*span, UNIFORM_SIZE);
        assert_eq!(layouter[handle].size, UNIFORM_SIZE);
        assert_eq!(FRAME_FLOATS * 4, UNIFORM_SIZE as usize);
        // The planes are an array of MAX_CLIP_PLANES vectors of four.
        let planes = module.types[members[3].ty].inner.clone();
        assert!(matches!(
            planes,
            naga::TypeInner::Array { size: naga::ArraySize::Constant(n), stride: 16, .. }
                if n.get() == u32::try_from(MAX_CLIP_PLANES).unwrap()
        ));
    }
}
