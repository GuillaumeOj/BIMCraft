//! Shared by the GPU tests: a headless device, an offscreen target read back to the CPU, and
//! box meshes.

use geom::Point3;
use render::{MeshData, RenderError};

/// The colour format of the offscreen targets: 8-bit linear RGBA, so pixels compare exactly.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Bytes per pixel of [`FORMAT`].
const PIXEL_BYTES: u32 = 4;

/// The GPU device of the tests, created once per test binary. A missing adapter fails the test,
/// naming it: the tests are never skipped (AGENTS.md §1.2).
pub fn device() -> (wgpu::Device, wgpu::Queue) {
    static DEVICE: std::sync::OnceLock<(wgpu::Device, wgpu::Queue)> = std::sync::OnceLock::new();
    DEVICE
        .get_or_init(|| match pollster::block_on(render::headless_device()) {
            Ok(device) => device,
            Err(error @ RenderError::NoAdapter { .. }) => panic!(
                "the GPU tests need a GPU adapter, software ones included (lavapipe on Linux: \
                 scripts/linux-gpu-deps.sh; WARP on Windows; Metal on macOS): {error}"
            ),
            Err(error) => panic!("the GPU tests could not create a device: {error}"),
        })
        .clone()
}

/// An offscreen colour target of [`FORMAT`] that can be read back.
pub struct Target {
    texture: wgpu::Texture,
    /// The view to render into.
    pub view: wgpu::TextureView,
    /// Width and height, in pixels.
    pub size: [u32; 2],
}

impl Target {
    /// A `width` × `height` target.
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test target"),
            size: extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            size,
        }
    }

    /// The pixels, row by row from the top, four bytes (RGBA) each. Blocks until the GPU is done.
    pub fn read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        let [width, height] = self.size;
        let row_bytes = width * PIXEL_BYTES;
        let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test readback"),
            size: u64::from(padded_row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            extent(self.size),
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = buffer.get_mapped_range(..).unwrap();
        mapped
            .chunks_exact(padded_row as usize)
            .flat_map(|row| &row[..row_bytes as usize])
            .copied()
            .collect()
    }
}

fn extent([width, height]: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

/// Positions, normals and indices of axis-aligned boxes, given as `(min, max)` corners in `f64`.
#[derive(Default)]
pub struct Boxes {
    positions: Vec<Point3>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Boxes {
    /// Adds the box from `min` to `max`: four vertices per face, so each face has its own normal.
    pub fn add(&mut self, min: [f64; 3], max: [f64; 3]) {
        for axis in 0..3 {
            for (side, normal_sign) in [(min, -1.0_f32), (max, 1.0)] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let first = u32::try_from(self.positions.len()).unwrap();
                for (a, b) in [(min, min), (max, min), (max, max), (min, max)] {
                    let mut corner = [0.0; 3];
                    corner[axis] = side[axis];
                    corner[u] = a[u];
                    corner[v] = b[v];
                    self.positions
                        .push(Point3::new(corner[0], corner[1], corner[2]));
                    let mut normal = [0.0; 3];
                    normal[axis] = normal_sign;
                    self.normals.push(normal);
                }
                self.indices
                    .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
            }
        }
    }

    /// The mesh of the boxes, with its origin at the centre of their bounding box.
    pub fn mesh(self) -> MeshData {
        MeshData::from_world(&self.positions, self.normals, self.indices).unwrap()
    }
}
