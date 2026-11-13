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

use std::collections::BTreeMap;

use wgpu::util::DeviceExt as _;

use crate::bounds::Aabb;
use crate::lod::{DetailLevel, RenderSettings, has_coarse_level};
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

/// Bytes of the frame uniform: a 4×4 matrix and a vector of four `f32`.
const UNIFORM_SIZE: u32 = 80;

/// Bytes of a vertex position or normal: three `f32`.
const VERTEX_STRIDE: u64 = 12;

/// Bytes of an index.
const INDEX_SIZE: u64 = 4;

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
/// [`FrameStats`] tells what a frame drew.
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
    /// The largest buffer and the widest texture of the device, which never change.
    max_buffer_size: u64,
    max_texture_side: u32,
    instances: Option<InstanceBuffer>,
    depth: Option<DepthBuffer>,
    meshes: BTreeMap<MeshId, GpuMesh>,
    world: World,
    /// The last prepared frame, kept to reuse its allocations.
    draw_list: DrawList,
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
        let pipeline = create_pipeline(device, &frame_layout, color_format);
        let limits = device.limits();
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            color_format,
            pipeline,
            frame_buffer,
            frame_bind_group,
            max_buffer_size: limits.max_buffer_size,
            max_texture_side: limits.max_texture_dimension_2d,
            instances: None,
            depth: None,
            meshes: BTreeMap::new(),
            world: World::new(max_instances(limits.max_buffer_size)),
            draw_list: DrawList::default(),
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
        let fine = self.upload(mesh, index_count);
        let coarse = has_coarse_level(mesh.indices.len() / 3)
            .then(|| mesh.bounding_box())
            .flatten()
            .map(|coarse| self.upload(&coarse, 36));
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
    /// was not drawn (outside the view, smaller than the small-feature threshold, or no frame
    /// prepared since it was added).
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownObject`] when the object is not in the renderer.
    pub fn drawn_level(&self, id: ObjectId) -> Result<Option<DetailLevel>, RenderError> {
        self.world.drawn_level(id)
    }

    /// Prepares a frame seen from `camera` on a target of `size` pixels, without drawing it:
    /// culls the objects, chooses their levels of detail and computes their instance records,
    /// all on the CPU. [`Renderer::render`] does this before drawing; on its own, it measures
    /// or checks those decisions.
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
        self.prepare(camera, size).map(|_| self.draw_list.stats)
    }

    /// Draws the objects into `target`, seen from `camera`, over the `clear` colour (linear
    /// RGBA), submits the pass to the queue, and tells what the frame drew.
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
        let projection = self.prepare(camera, size)?;
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::cast_slice(&frame_uniform(&projection)),
        );
        let instances = self.upload_instances();
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
            if let Some(instances) = &instances {
                self.draw(&mut pass, instances);
            }
        }
        self.queue.submit([encoder.finish()]);
        Ok(self.draw_list.stats)
    }

    /// Validates the camera and prepares the draw list; returns the projection matrix.
    fn prepare(&mut self, camera: &Camera, size: [u32; 2]) -> Result<Matrix, RenderError> {
        let view = ViewFrame::new(camera)?;
        let projection =
            projection_matrix(&camera.projection, f64::from(size[0]) / f64::from(size[1]))?;
        self.world
            .prepare(&view, &camera.projection, size, &mut self.draw_list);
        Ok(projection)
    }

    fn upload(&self, mesh: &MeshData, index_count: u32) -> MeshBuffers {
        let buffer = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        MeshBuffers {
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
        }
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
        let bytes = self.draw_list.instance_bytes();
        if bytes.is_empty() {
            return None;
        }
        let count = bytes.len() as u64 / INSTANCE_STRIDE;
        let buffer = if let Some(instances) = self
            .instances
            .as_ref()
            .filter(|buffer| buffer.capacity >= count)
        {
            instances.buffer.clone()
        } else {
            // The world holds at most `max_instances` objects, which fit the largest buffer.
            let capacity = count
                .next_power_of_two()
                .min(max_instances(self.max_buffer_size) as u64);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render instances"),
                size: capacity * INSTANCE_STRIDE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instances = Some(InstanceBuffer {
                buffer: buffer.clone(),
                capacity,
            });
            buffer
        };
        self.queue
            .write_buffer(&buffer, 0, self.draw_list.instance_bytes());
        Some(buffer)
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
            pass.set_vertex_buffer(0, buffers.positions.slice(..));
            pass.set_vertex_buffer(1, buffers.normals.slice(..));
            pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..buffers.index_count,
                0,
                bucket.first..bucket.first + bucket.count,
            );
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

/// The most objects one frame can draw: as many instance records as the device's largest
/// buffer holds, with instance indices within `u32`.
fn max_instances(max_buffer_size: u64) -> usize {
    let count = (max_buffer_size / INSTANCE_STRIDE).min(u64::from(u32::MAX));
    usize::try_from(count).unwrap_or(usize::MAX)
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
    frame_layout: &wgpu::BindGroupLayout,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
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
        label: Some("render opaque"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
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
        let frame = frame_uniform(&matrix);
        assert_eq!(&frame[..16], matrix.as_flattened());
        let [x, y, z] = LIGHT_DIRECTION;
        assert_eq!(&frame[16..], &[x, y, z, AMBIENT]);
        assert!((x * x + y * y + z * z - 1.0).abs() < 1e-6);
        assert_eq!(size_of_val(&frame), UNIFORM_SIZE as usize);
    }
}
