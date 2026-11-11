//! The GPU side of the renderer: meshes and objects stored on the device, and the opaque pass
//! that draws them into a texture view (VP-04).
//!
//! Each frame, the model-view matrix of every object is computed on the CPU in `f64`, relative to
//! the camera ([`crate::transform`]), and only that small result reaches the GPU in `f32`. Depth
//! is reverse-Z in a `Depth32Float` buffer, compared with `Greater` and cleared to 0
//! (`docs/dev/adr/0020-camera-relative-rendering.md`).

use std::collections::BTreeMap;

use wgpu::util::DeviceExt as _;

use crate::transform::{Matrix, RenderOrigin, ViewFrame, projection_matrix};
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

/// Bytes of the frame and object uniforms: a 4×4 matrix and a vector of four `f32`.
const UNIFORM_SIZE: u32 = 80;

/// Bytes of a vertex position or normal: three `f32`.
const VERTEX_STRIDE: u64 = 12;

/// Bytes of an index.
const INDEX_SIZE: u64 = 4;

/// Identifies a mesh stored in a [`Renderer`]. Ids are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(u64);

/// Identifies an object of a [`Renderer`]: a mesh placed in the world. Ids are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(u64);

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
    /// The renderer holds as many objects as one frame can draw on this device.
    #[error("the renderer holds the {max} objects this device can draw in one frame")]
    TooManyObjects {
        /// The limit for this device.
        max: usize,
    },
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
    /// The target view is not a single-sampled 2D render attachment of the renderer's format and
    /// of the given size.
    #[error("the render target does not match the renderer's format or the given size")]
    TargetMismatch,
}

/// A mesh stored on the GPU.
struct GpuMesh {
    /// The `f64` origin the local `f32` positions are relative to.
    origin: [f64; 3],
    positions: wgpu::Buffer,
    normals: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// A mesh placed in the world, with its colour.
struct Object {
    mesh: MeshId,
    placement: Placement,
    color: [f32; 4],
}

/// The per-object uniform buffer and its bind group, grown as objects are added.
struct ObjectUniforms {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// How many objects the buffer holds.
    capacity: usize,
}

/// The depth buffer, recreated when the size of the target changes.
struct DepthBuffer {
    view: wgpu::TextureView,
    size: [u32; 2],
}

/// Draws triangle meshes placed in an `f64` world, seen from an `f64` camera, into a texture
/// view (VP-04, GEO-07).
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
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    object_layout: wgpu::BindGroupLayout,
    /// Bytes between the uniforms of two objects: the dynamic offset alignment of the device.
    object_stride: u32,
    /// The most objects one frame can draw on this device.
    max_objects: usize,
    /// The largest buffer and the widest texture of the device, which never change.
    max_buffer_size: u64,
    max_texture_side: u32,
    object_uniforms: Option<ObjectUniforms>,
    /// The bytes of the object uniforms, kept between frames to avoid an allocation each frame.
    object_bytes: Vec<u8>,
    depth: Option<DepthBuffer>,
    meshes: BTreeMap<MeshId, GpuMesh>,
    /// Ordered by id, so frames draw in a deterministic order.
    objects: BTreeMap<ObjectId, Object>,
    next_id: u64,
    origin: RenderOrigin,
}

impl Renderer {
    /// Creates a renderer that draws with `device` and `queue` into views of textures of
    /// `color_format`.
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
        let uniform_layout = |label, dynamic| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: dynamic,
                        min_binding_size: wgpu::BufferSize::new(u64::from(UNIFORM_SIZE)),
                    },
                    count: None,
                }],
            })
        };
        let frame_layout = uniform_layout("render frame", false);
        let object_layout = uniform_layout("render object", true);
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
        let pipeline = create_pipeline(device, &[&frame_layout, &object_layout], color_format);
        let limits = device.limits();
        let (object_stride, max_objects) = object_slots(&limits);
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            color_format,
            pipeline,
            frame_buffer,
            frame_bind_group,
            object_layout,
            object_stride,
            max_objects,
            max_buffer_size: limits.max_buffer_size,
            max_texture_side: limits.max_texture_dimension_2d,
            object_uniforms: None,
            object_bytes: Vec::new(),
            depth: None,
            meshes: BTreeMap::new(),
            objects: BTreeMap::new(),
            next_id: 0,
            origin: RenderOrigin::Eye,
        })
    }

    /// Validates `mesh` and uploads it to the GPU.
    ///
    /// # Errors
    ///
    /// The [`MeshError`] of [`MeshData::validate`], or [`MeshError::TooLargeForDevice`] when a
    /// buffer of the mesh exceeds the device's largest buffer.
    pub fn add_mesh(&mut self, mesh: &MeshData) -> Result<MeshId, MeshError> {
        mesh.validate()?;
        let index_count = index_count(mesh, self.max_buffer_size)?;
        let buffer = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        let gpu = GpuMesh {
            origin: mesh.origin,
            positions: buffer(
                "render mesh positions",
                bytemuck::cast_slice(&mesh.positions),
                wgpu::BufferUsages::VERTEX,
            ),
            normals: buffer(
                "render mesh normals",
                bytemuck::cast_slice(&mesh.normals),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "render mesh indices",
                bytemuck::cast_slice(&mesh.indices),
                wgpu::BufferUsages::INDEX,
            ),
            index_count,
        };
        let id = MeshId(self.new_id());
        self.meshes.insert(id, gpu);
        Ok(id)
    }

    /// Removes a mesh, and every object that places it.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownMesh`] when the mesh is not in the renderer.
    pub fn remove_mesh(&mut self, id: MeshId) -> Result<(), RenderError> {
        self.meshes
            .remove(&id)
            .ok_or(RenderError::UnknownMesh(id))?;
        self.objects.retain(|_, object| object.mesh != id);
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
        if !self.meshes.contains_key(&mesh) {
            return Err(RenderError::UnknownMesh(mesh));
        }
        placement.validate()?;
        if !color.iter().all(|c| (0.0..=1.0).contains(c)) {
            return Err(RenderError::InvalidColor);
        }
        if self.objects.len() >= self.max_objects {
            return Err(RenderError::TooManyObjects {
                max: self.max_objects,
            });
        }
        let id = ObjectId(self.new_id());
        self.objects.insert(
            id,
            Object {
                mesh,
                placement,
                color,
            },
        );
        Ok(id)
    }

    /// Moves an object.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`], or [`RenderError::Placement`] for an invalid placement.
    pub fn set_placement(&mut self, id: ObjectId, placement: Placement) -> Result<(), RenderError> {
        let object = self
            .objects
            .get_mut(&id)
            .ok_or(RenderError::UnknownObject(id))?;
        placement.validate()?;
        object.placement = placement;
        Ok(())
    }

    /// Removes an object; its mesh stays.
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn remove_object(&mut self, id: ObjectId) -> Result<(), RenderError> {
        self.objects
            .remove(&id)
            .map(drop)
            .ok_or(RenderError::UnknownObject(id))
    }

    /// Draws every object into `target`, seen from `camera`, over the `clear` colour (linear
    /// RGBA), and submits the pass to the queue.
    ///
    /// `target` must view a single-sampled 2D texture of the renderer's device and format, with
    /// one mip level and one layer, `size` pixels wide and high, usable as a render attachment.
    /// All of that is checked but the view's own format, which wgpu does not expose: the view
    /// must keep the texture's format, or wgpu reports a validation error to the device.
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
    ) -> Result<(), RenderError> {
        self.check_target(target, size)?;
        if !clear.iter().all(|c| c.is_finite()) {
            return Err(RenderError::InvalidClearColor);
        }
        let view = ViewFrame::new(camera)?;
        let projection =
            projection_matrix(&camera.projection, f64::from(size[0]) / f64::from(size[1]))?;
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::cast_slice(&frame_uniform(&projection)),
        );
        self.upload_objects(&view);
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
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_objects(&mut pass);
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Measures the coordinates sent to the GPU from the world origin instead of the eye, as a
    /// classic pipeline does: the negative control of `tests/jitter.rs`, which must then see
    /// jitter far from the origin. Not part of the API.
    #[cfg(feature = "jitter-control")]
    #[doc(hidden)]
    pub fn measure_from_world_origin(&mut self) {
        self.origin = RenderOrigin::World;
    }

    fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn check_target(&self, target: &wgpu::TextureView, size: [u32; 2]) -> Result<(), RenderError> {
        if size.contains(&0) {
            return Err(RenderError::ZeroSizedTarget);
        }
        let max = self.max_texture_side;
        if size.iter().any(|&side| side > max) {
            return Err(RenderError::TargetTooLarge { max });
        }
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

    /// Writes the camera-relative model-view and the colour of every object, in id order.
    fn upload_objects(&mut self, view: &ViewFrame) {
        if self.objects.is_empty() {
            return;
        }
        self.reserve_objects();
        let Some(uniforms) = &self.object_uniforms else {
            return;
        };
        let stride = self.object_stride as usize;
        let bytes = &mut self.object_bytes;
        bytes.resize(stride * self.objects.len(), 0);
        for (object, slot) in self.objects.values().zip(bytes.chunks_exact_mut(stride)) {
            let Some(mesh) = self.meshes.get(&object.mesh) else {
                continue;
            };
            let model_view = self.origin.model_view(view, mesh.origin, &object.placement);
            let uniform = object_uniform(&model_view, object.color);
            slot[..UNIFORM_SIZE as usize].copy_from_slice(bytemuck::cast_slice(&uniform));
        }
        self.queue.write_buffer(&uniforms.buffer, 0, bytes);
    }

    /// Grows the object uniform buffer to hold every object, doubling its capacity.
    fn reserve_objects(&mut self) {
        let count = self.objects.len();
        if self
            .object_uniforms
            .as_ref()
            .is_some_and(|uniforms| uniforms.capacity >= count)
        {
            return;
        }
        let capacity = count.next_power_of_two().min(self.max_objects);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render objects"),
            size: capacity as u64 * u64::from(self.object_stride),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render objects"),
            layout: &self.object_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(u64::from(UNIFORM_SIZE)),
                }),
            }],
        });
        self.object_uniforms = Some(ObjectUniforms {
            buffer,
            bind_group,
            capacity,
        });
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

    fn draw_objects(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(uniforms) = &self.object_uniforms else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame_bind_group, &[]);
        for (slot, object) in (0_u64..).zip(self.objects.values()) {
            let Some(mesh) = self.meshes.get(&object.mesh) else {
                continue;
            };
            // `max_objects` keeps every offset within u32.
            let Ok(offset) = u32::try_from(slot * u64::from(self.object_stride)) else {
                return;
            };
            pass.set_bind_group(1, &uniforms.bind_group, &[offset]);
            pass.set_vertex_buffer(0, mesh.positions.slice(..));
            pass.set_vertex_buffer(1, mesh.normals.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
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

/// The bytes between the uniforms of two objects, and the most objects one frame can draw.
fn object_slots(limits: &wgpu::Limits) -> (u32, usize) {
    // WebGPU caps the alignment at 256 bytes.
    let stride = UNIFORM_SIZE.next_multiple_of(limits.min_uniform_buffer_offset_alignment);
    // Dynamic offsets are `u32`, and the whole buffer must fit the device.
    let max_bytes = limits
        .max_buffer_size
        .min(u64::from(u32::MAX) + u64::from(stride));
    let max_objects = usize::try_from(max_bytes / u64::from(stride)).unwrap_or(usize::MAX);
    (stride, max_objects)
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

fn create_pipeline(
    device: &wgpu::Device,
    layouts: &[&wgpu::BindGroupLayout],
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let layouts: Vec<Option<&wgpu::BindGroupLayout>> = layouts.iter().copied().map(Some).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render"),
        bind_group_layouts: &layouts,
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
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render opaque"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(buffer(&position)), Some(buffer(&normal))],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // BIM solids from IFC are not always consistently oriented.
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            // Reverse-Z: nearer is larger.
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        // No MSAA: deterministic pixels for the visual test.
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The frame uniform: the projection, then the light direction with the ambient share.
fn frame_uniform(projection: &Matrix) -> [f32; 20] {
    let mut uniform = [0.0; 20];
    uniform[..16].copy_from_slice(projection.as_flattened());
    uniform[16..19].copy_from_slice(&LIGHT_DIRECTION);
    uniform[19] = AMBIENT;
    uniform
}

/// The object uniform: the model-view, then the colour.
fn object_uniform(model_view: &Matrix, color: [f32; 4]) -> [f32; 20] {
    let mut uniform = [0.0; 20];
    uniform[..16].copy_from_slice(model_view.as_flattened());
    uniform[16..].copy_from_slice(&color);
    uniform
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

    fn mesh(vertices: usize, indices: usize) -> MeshData {
        MeshData {
            origin: [0.0; 3],
            positions: vec![[0.0; 3]; vertices],
            normals: vec![[0.0, 0.0, 1.0]; vertices],
            indices: vec![0; indices],
        }
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
    fn object_uniforms_follow_the_offset_alignment_and_the_buffer_size() {
        let limits = wgpu::Limits::default();
        assert_eq!(object_slots(&limits), (256, 1 << 20));
        let small = wgpu::Limits {
            min_uniform_buffer_offset_alignment: 64,
            max_buffer_size: 1000,
            ..limits.clone()
        };
        assert_eq!(object_slots(&small), (128, 7));
        // Offsets are u32: a huge buffer still holds only the objects u32 offsets reach.
        let huge = wgpu::Limits {
            max_buffer_size: u64::MAX,
            ..limits
        };
        assert_eq!(object_slots(&huge), (256, 1 << 24));
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
    fn uniforms_put_the_matrix_first() {
        let matrix: Matrix = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        let frame = frame_uniform(&matrix);
        assert_eq!(&frame[..16], matrix.as_flattened());
        let [x, y, z] = LIGHT_DIRECTION;
        assert_eq!(&frame[16..], &[x, y, z, AMBIENT]);
        assert!((x * x + y * y + z * z - 1.0).abs() < 1e-6);
        let object = object_uniform(&matrix, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(&object[..16], matrix.as_flattened());
        assert_eq!(&object[16..], &[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(size_of_val(&object), UNIFORM_SIZE as usize);
    }
}
