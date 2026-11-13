//! The reference model of the renderer's performance (VP-02, NFR-02, spec §14): a synthetic
//! 10-storey office building of about 10,000 elements per storey, 100,000 in all, made of a few
//! meshes repeated many times, as real buildings are.
//!
//! The building is computed from storey, grid and bay indices only, with no randomness, so every
//! run builds the same model. It sits at a national-grid offset, as georeferenced projects do,
//! so the renderer works far from the origin (ADR 0020).
//!
//! Each storey has a structural grid of 6 m on a 72 m × 36 m footprint: I-section columns and
//! beams, slab panels, a curtain wall (mullions, transoms, glazing), two rows of cellular offices
//! along the long façades (partitions, corridor walls, doors, desks, chairs, monitors), an open
//! plan between them (desks, chairs, monitors), and a ceiling of 0.6 m tiles with lights and
//! sprinkler heads. Meshes are built in the local frame of their element; elements along the
//! other axis reuse them turned a quarter turn.
//!
//! Shared by the tests and the benches of `render` (`#[path]` includes it), and by the unit tests
//! of the crate, which see the crate itself as `render`.

use geom::{Point3, Vector3};
use render::{Camera, MeshData, Placement, Projection};

/// Where the building's corner lies, in metres: a national-grid offset.
pub const ORIGIN: [f64; 3] = [2_500_000.0, 1_200_000.0, 0.0];

/// Storeys of the full model.
pub const STOREYS: u32 = 10;

/// Floor to floor, in metres.
pub const STOREY_HEIGHT: f64 = 3.6;

/// The length of the footprint, along x, in metres.
pub const LENGTH: f64 = 72.0;

/// The width of the footprint, along y, in metres.
pub const WIDTH: f64 = 36.0;

/// The structural grid, in metres.
const GRID: f64 = 6.0;

/// Depth of the cellular offices from the façade, and width of the corridors, in metres.
const OFFICE_DEPTH: f64 = 6.0;
const CORRIDOR: f64 = 2.0;

/// Width of a cellular office, and of a façade module, in metres.
const OFFICE_WIDTH: f64 = 3.0;
const MODULE: f64 = 1.5;

/// The side of a ceiling tile, in metres.
const TILE: f64 = 0.6;

/// The projection of the reference cameras: 60° vertical field of view.
const PROJECTION: Projection = Projection::Perspective {
    fov_y_radians: core::f64::consts::FRAC_PI_3,
    near: 0.1,
};

/// The size of the reference views, in pixels: full HD.
pub const VIEW: [u32; 2] = [1920, 1080];

fn camera(eye: [f64; 3], target: [f64; 3]) -> Camera {
    let world = |p: [f64; 3]| Point3::new(ORIGIN[0] + p[0], ORIGIN[1] + p[1], ORIGIN[2] + p[2]);
    Camera {
        eye: world(eye),
        target: world(target),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection: PROJECTION,
    }
}

/// The middle of the full building, relative to [`ORIGIN`].
const MIDDLE: [f64; 3] = [LENGTH / 2.0, WIDTH / 2.0, STOREY_HEIGHT * 5.0];

/// One step of an orbit around the whole building, 110 m from its vertical axis and 45 m
/// above the ground, looking at its middle: `step` of `steps` around the full turn.
pub fn orbit(step: u32, steps: u32) -> Camera {
    let angle = core::f64::consts::TAU * f64::from(step) / f64::from(steps);
    let eye = [
        MIDDLE[0] + 110.0 * libm::cos(angle),
        MIDDLE[1] + 110.0 * libm::sin(angle),
        45.0,
    ];
    camera(eye, MIDDLE)
}

/// The whole building from high above a corner.
pub fn aerial() -> Camera {
    camera([-60.0, -80.0, 120.0], MIDDLE)
}

/// Standing in the open plan of the sixth storey, looking along the building.
pub fn inside() -> Camera {
    let z = 5.0 * STOREY_HEIGHT + 1.6;
    camera([4.0, WIDTH / 2.0, z], [LENGTH, WIDTH / 2.0 + 3.0, z])
}

/// The meshes of the model and the objects that place them.
pub struct ReferenceModel {
    pub meshes: Vec<MeshData>,
    /// Index into `meshes`, placement and colour of each element.
    pub objects: Vec<(usize, Placement, [f32; 4])>,
}

/// The kinds of elements, one mesh each.
#[derive(Clone, Copy)]
enum Kind {
    Column,
    Beam,
    Slab,
    Mullion,
    Transom,
    Glazing,
    Partition,
    CorridorWall,
    Door,
    Desk,
    Chair,
    Monitor,
    CeilingTile,
    Light,
    Sprinkler,
}

const KINDS: [Kind; 15] = [
    Kind::Column,
    Kind::Beam,
    Kind::Slab,
    Kind::Mullion,
    Kind::Transom,
    Kind::Glazing,
    Kind::Partition,
    Kind::CorridorWall,
    Kind::Door,
    Kind::Desk,
    Kind::Chair,
    Kind::Monitor,
    Kind::CeilingTile,
    Kind::Light,
    Kind::Sprinkler,
];

impl Kind {
    fn color(self) -> [f32; 4] {
        match self {
            Self::Column | Self::Beam => [0.55, 0.55, 0.6, 1.0],
            Self::Slab => [0.7, 0.7, 0.7, 1.0],
            Self::Mullion | Self::Transom => [0.3, 0.3, 0.35, 1.0],
            Self::Glazing => [0.5, 0.7, 0.9, 1.0],
            Self::Partition | Self::CorridorWall => [0.9, 0.88, 0.82, 1.0],
            Self::Door => [0.6, 0.4, 0.2, 1.0],
            Self::Desk => [0.8, 0.7, 0.5, 1.0],
            Self::Chair => [0.2, 0.2, 0.25, 1.0],
            Self::Monitor => [0.05, 0.05, 0.05, 1.0],
            Self::CeilingTile => [0.95, 0.95, 0.95, 1.0],
            Self::Light => [1.0, 1.0, 0.85, 1.0],
            Self::Sprinkler => [0.8, 0.1, 0.1, 1.0],
        }
    }

    /// The mesh, in the element's local frame: x along its length, z up, the origin at its base
    /// or its centre as noted.
    fn mesh(self) -> MeshData {
        let mut mesh = Builder::default();
        match self {
            // HEB 300-like, 3.3 m high, from z = 0.
            Self::Column => mesh.i_section([0.3, 0.3], 0.02, 0.012, [0.0, 3.3], 2),
            // IPE 400-like along x, 6 m, top at z = 0.
            Self::Beam => mesh.i_section([0.18, 0.4], 0.014, 0.009, [-3.0, 3.0], 0),
            Self::Slab => mesh.cuboid([-3.0, -3.0, -0.25], [3.0, 3.0, 0.0]),
            Self::Mullion => mesh.cuboid([-0.03, -0.1, 0.0], [0.03, 0.1, 3.3]),
            Self::Transom => mesh.cuboid([-0.75, -0.1, -0.03], [0.75, 0.1, 0.03]),
            Self::Glazing => mesh.cuboid([-0.72, -0.012, 0.0], [0.72, 0.012, 1.6]),
            Self::Partition => mesh.cuboid([-3.0, -0.05, 0.0], [3.0, 0.05, 2.8]),
            Self::CorridorWall => mesh.cuboid([-1.5, -0.075, 0.0], [1.5, 0.075, 2.8]),
            Self::Door => {
                // Frame (two jambs and a head) and leaf.
                mesh.cuboid([-0.5, -0.08, 0.0], [-0.45, 0.08, 2.1]);
                mesh.cuboid([0.45, -0.08, 0.0], [0.5, 0.08, 2.1]);
                mesh.cuboid([-0.5, -0.08, 2.05], [0.5, 0.08, 2.1]);
                mesh.cuboid([-0.45, -0.02, 0.0], [0.45, 0.02, 2.05]);
            }
            Self::Desk => {
                mesh.cuboid([-0.8, -0.4, 0.7], [0.8, 0.4, 0.73]);
                mesh.cuboid([-0.78, -0.38, 0.0], [-0.74, 0.38, 0.7]);
                mesh.cuboid([0.74, -0.38, 0.0], [0.78, 0.38, 0.7]);
            }
            Self::Chair => {
                mesh.cuboid([-0.25, -0.25, 0.42], [0.25, 0.25, 0.47]);
                mesh.cuboid([-0.25, 0.2, 0.47], [0.25, 0.25, 1.0]);
                mesh.cuboid([-0.03, -0.03, 0.0], [0.03, 0.03, 0.42]);
                mesh.cuboid([-0.3, -0.3, 0.0], [0.3, 0.3, 0.03]);
            }
            Self::Monitor => {
                mesh.cuboid([-0.3, -0.02, 0.1], [0.3, 0.02, 0.45]);
                mesh.cuboid([-0.02, -0.02, 0.0], [0.02, 0.02, 0.1]);
                mesh.cuboid([-0.1, -0.08, 0.0], [0.1, 0.08, 0.01]);
            }
            Self::CeilingTile => mesh.cuboid([-0.295, -0.295, -0.015], [0.295, 0.295, 0.0]),
            Self::Light => mesh.cuboid([-0.6, -0.15, -0.08], [0.6, 0.15, 0.0]),
            // An octagonal head, 4 cm across, under the ceiling.
            Self::Sprinkler => mesh.octagonal_prism(0.02, [-0.05, 0.0]),
        }
        mesh.finish()
    }
}

/// The triangles of a mesh being built, in local `f32` coordinates around the origin 0.
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Builder {
    fn next(&self) -> u32 {
        u32::try_from(self.positions.len()).unwrap()
    }

    /// An axis-aligned box, four vertices and a normal per face.
    fn cuboid(&mut self, min: [f32; 3], max: [f32; 3]) {
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (side, sign) in [(min, -1.0), (max, 1.0)] {
                let first = self.next();
                for (a, b) in [(min, min), (max, min), (max, max), (min, max)] {
                    let mut corner = [0.0; 3];
                    corner[axis] = side[axis];
                    corner[u] = a[u];
                    corner[v] = b[v];
                    self.positions.push(corner);
                    let mut normal = [0.0; 3];
                    normal[axis] = sign;
                    self.normals.push(normal);
                }
                self.indices
                    .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
            }
        }
    }

    /// An I-section of `width` × `depth`, with flanges and web of the given thicknesses,
    /// extruded from `extent[0]` to `extent[1]` along `axis` (0: x, the depth along z; 2: z, the
    /// depth along y). Three boxes: 36 triangles.
    fn i_section(
        &mut self,
        [width, depth]: [f32; 2],
        flange: f32,
        web: f32,
        [from, to]: [f32; 2],
        axis: usize,
    ) {
        let (half_width, half_web) = (width / 2.0, web / 2.0);
        // In section coordinates (across, up), the depth going from −depth to 0.
        let parts = [
            ([-half_width, -depth], [half_width, -depth + flange]),
            ([-half_web, -depth + flange], [half_web, -flange]),
            ([-half_width, -flange], [half_width, 0.0]),
        ];
        for ([across_min, up_min], [across_max, up_max]) in parts {
            if axis == 0 {
                self.cuboid([from, across_min, up_min], [to, across_max, up_max]);
            } else {
                let shift = depth / 2.0;
                self.cuboid(
                    [across_min, up_min + shift, from],
                    [across_max, up_max + shift, to],
                );
            }
        }
    }

    /// A vertical octagonal prism of `radius` between two heights: 28 triangles.
    fn octagonal_prism(&mut self, radius: f32, [bottom, top]: [f32; 2]) {
        let corners: Vec<[f32; 2]> = (0..8_u8)
            .map(|k| {
                let angle = core::f32::consts::FRAC_PI_4 * f32::from(k);
                [radius * libm::cosf(angle), radius * libm::sinf(angle)]
            })
            .collect();
        for (z, sign) in [(bottom, -1.0), (top, 1.0)] {
            let first = self.next();
            for &[x, y] in &corners {
                self.positions.push([x, y, z]);
                self.normals.push([0.0, 0.0, sign]);
            }
            for k in 1..7 {
                self.indices.extend([first, first + k, first + k + 1]);
            }
        }
        for k in 0..8_u8 {
            let ([x0, y0], [x1, y1]) = (corners[usize::from(k)], corners[usize::from((k + 1) % 8)]);
            let middle = core::f32::consts::FRAC_PI_4 * (f32::from(k) + 0.5);
            let normal = [libm::cosf(middle), libm::sinf(middle), 0.0];
            let first = self.next();
            for corner in [
                [x0, y0, bottom],
                [x1, y1, bottom],
                [x1, y1, top],
                [x0, y0, top],
            ] {
                self.positions.push(corner);
                self.normals.push(normal);
            }
            self.indices
                .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
        }
    }

    fn finish(self) -> MeshData {
        let mesh = MeshData {
            origin: [0.0; 3],
            positions: self.positions,
            normals: self.normals,
            indices: self.indices,
        };
        mesh.validate().unwrap();
        mesh
    }
}

/// A quarter turn about z: x becomes y.
const QUARTER_TURN: [[f64; 3]; 3] = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];

/// The model, with the first `storeys` storeys.
pub fn building(storeys: u32) -> ReferenceModel {
    let mut objects = Vec::new();
    for storey in 0..storeys {
        let floor = f64::from(storey) * STOREY_HEIGHT;
        let mut put = |kind: Kind, at: [f64; 3], turned: bool| {
            let translation = core::array::from_fn(|axis| {
                ORIGIN[axis] + at[axis] + if axis == 2 { floor } else { 0.0 }
            });
            let placement = if turned {
                Placement {
                    rotation: QUARTER_TURN,
                    translation,
                }
            } else {
                Placement::from_translation(translation)
            };
            objects.push((kind as usize, placement, kind.color()));
        };
        structure(&mut put);
        facade(&mut put);
        offices(&mut put);
        open_plan(&mut put);
        ceiling(&mut put);
    }
    ReferenceModel {
        meshes: KINDS.map(Kind::mesh).into(),
        objects,
    }
}

/// The values `start + step · k` up to `end`, inclusive within a millimetre.
fn steps(start: f64, end: f64, step: f64) -> impl Iterator<Item = f64> {
    (0_u32..)
        .map(move |k| start + f64::from(k) * step)
        .take_while(move |&v| v <= end + 1e-3)
}

/// Columns on the grid, beams between them, a slab panel per bay.
fn structure(put: &mut impl FnMut(Kind, [f64; 3], bool)) {
    for x in steps(0.0, LENGTH, GRID) {
        for y in steps(0.0, WIDTH, GRID) {
            put(Kind::Column, [x, y, 0.0], false);
        }
    }
    for y in steps(0.0, WIDTH, GRID) {
        for x in steps(GRID / 2.0, LENGTH, GRID) {
            put(Kind::Beam, [x, y, STOREY_HEIGHT], false);
        }
    }
    for x in steps(0.0, LENGTH, GRID) {
        for y in steps(GRID / 2.0, WIDTH, GRID) {
            put(Kind::Beam, [x, y, STOREY_HEIGHT], true);
        }
    }
    for x in steps(GRID / 2.0, LENGTH, GRID) {
        for y in steps(GRID / 2.0, WIDTH, GRID) {
            put(Kind::Slab, [x, y, STOREY_HEIGHT], false);
        }
    }
}

/// A mullion per 1.5 m module around the perimeter, two transoms and two glazing panels per
/// module.
fn facade(put: &mut impl FnMut(Kind, [f64; 3], bool)) {
    let mut module = |at: [f64; 2], turned: bool| {
        let [x, y] = at;
        let (dx, dy) = if turned {
            (0.0, MODULE / 2.0)
        } else {
            (MODULE / 2.0, 0.0)
        };
        put(Kind::Mullion, [x, y, 0.0], turned);
        for z in [0.1, 1.8] {
            put(Kind::Transom, [x + dx, y + dy, z], turned);
            put(Kind::Glazing, [x + dx, y + dy, z + 0.03], turned);
        }
    };
    for x in steps(0.0, LENGTH - MODULE, MODULE) {
        module([x, 0.0], false);
        module([x, WIDTH], false);
    }
    for y in steps(0.0, WIDTH - MODULE, MODULE) {
        module([0.0, y], true);
        module([LENGTH, y], true);
    }
}

/// Two rows of 3 m offices along the long façades: partitions, corridor walls with a door per
/// office, two desks with chairs and monitors per office.
fn offices(put: &mut impl FnMut(Kind, [f64; 3], bool)) {
    for (wall_y, sign) in [(OFFICE_DEPTH, 1.0), (WIDTH - OFFICE_DEPTH, -1.0)] {
        for x in steps(0.0, LENGTH, OFFICE_WIDTH) {
            put(
                Kind::Partition,
                [x, wall_y - sign * OFFICE_DEPTH / 2.0, 0.0],
                true,
            );
        }
        for x in steps(OFFICE_WIDTH / 2.0, LENGTH, OFFICE_WIDTH) {
            put(Kind::CorridorWall, [x, wall_y, 0.0], false);
            put(Kind::Door, [x, wall_y, 0.0], false);
            for depth in [1.5, 3.5] {
                let y = wall_y - sign * depth;
                put(Kind::Desk, [x, y, 0.0], false);
                put(Kind::Chair, [x, y + sign * 0.7, 0.0], false);
                put(Kind::Monitor, [x, y - sign * 0.2, 0.73], false);
            }
        }
    }
}

/// Desks with chairs on a 1.8 m × 1.6 m grid between the corridors.
fn open_plan(put: &mut impl FnMut(Kind, [f64; 3], bool)) {
    let (first, last) = (
        OFFICE_DEPTH + CORRIDOR + 0.8,
        WIDTH - OFFICE_DEPTH - CORRIDOR - 0.8,
    );
    for x in steps(1.0, LENGTH - 1.0, 1.8) {
        for y in steps(first, last, 1.6) {
            put(Kind::Desk, [x, y, 0.0], false);
            put(Kind::Chair, [x, y - 0.7, 0.0], false);
        }
    }
}

/// Ceiling tiles on a 0.6 m grid at 3 m, and a light and a sprinkler head every 3.6 m.
fn ceiling(put: &mut impl FnMut(Kind, [f64; 3], bool)) {
    const HEIGHT: f64 = 3.0;
    for x in steps(TILE / 2.0, LENGTH, TILE) {
        for y in steps(TILE / 2.0, WIDTH, TILE) {
            put(Kind::CeilingTile, [x, y, HEIGHT], false);
        }
    }
    for x in steps(1.8, LENGTH, 3.6) {
        for y in steps(1.8, WIDTH, 3.6) {
            put(Kind::Light, [x, y, HEIGHT - 0.015], false);
            put(Kind::Sprinkler, [x + 0.9, y + 0.9, HEIGHT - 0.015], false);
        }
    }
}
