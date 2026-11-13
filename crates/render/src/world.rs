//! The CPU side of the renderer: the meshes and objects it holds, their bounds in a hierarchy,
//! and the preparation of each frame, which culls, chooses levels of detail and writes the
//! instance records (VP-02, `docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`).
//!
//! Nothing here touches the GPU, so every decision is tested without a device.

use std::collections::BTreeMap;

use crate::bounds::Aabb;
use crate::bvh::{Bvh, Leaf};
use crate::frustum::Frustum;
use crate::lod::{Detail, DetailLevel, RenderSettings, SizeScale, choose};
use crate::transform::{Matrix, ViewFrame, camera_relative_model_view};
use crate::vector::{norm, sub};
use crate::{Placement, Projection, RenderError};

/// Identifies a mesh stored in a [`Renderer`](crate::Renderer). Ids are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(u64);

/// Identifies an object of a [`Renderer`](crate::Renderer): a mesh placed in the world. Ids are
/// never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(u64);

/// The `f32` values of an instance record, in this order: the camera-relative model-view matrix,
/// column-major (16), then the linear RGBA colour (4). Later passes append per-object state
/// after the colour; the vertex layout of the pipeline follows this constant.
pub(crate) const INSTANCE_FLOATS: usize = 20;

/// Bytes of an instance record.
pub(crate) const INSTANCE_STRIDE: u64 = INSTANCE_FLOATS as u64 * 4;

/// What a prepared frame draws and skips, counted in objects (and draw calls).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FrameStats {
    /// Every object of the renderer.
    pub objects: usize,
    /// Objects outside the view.
    pub culled_frustum: usize,
    /// Objects in view but smaller than the small-feature threshold.
    pub culled_small: usize,
    /// Objects drawn with their mesh.
    pub fine: usize,
    /// Objects drawn with the coarse level of their mesh.
    pub coarse: usize,
    /// Instanced draw calls: one per mesh and level drawn.
    pub draw_calls: usize,
}

/// A run of instances drawn by one instanced draw call: `count` records from `first`, all
/// placing `mesh` at `level`. Passes after the opaque one can draw the same runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bucket {
    pub(crate) mesh: MeshId,
    pub(crate) level: DetailLevel,
    pub(crate) first: u32,
    pub(crate) count: u32,
}

/// A prepared frame: the instance records, the runs that draw them and the counts. Kept between
/// frames so preparing one allocates nothing once the buffers have grown.
#[derive(Debug, Default)]
pub(crate) struct DrawList {
    pub(crate) buckets: Vec<Bucket>,
    /// [`INSTANCE_FLOATS`] values per drawn object, ordered by bucket.
    pub(crate) instances: Vec<f32>,
    pub(crate) stats: FrameStats,
    /// The coarse instances of the mesh being written, appended after its fine ones.
    coarse: Vec<f32>,
}

impl DrawList {
    /// Bytes of the instance records.
    pub(crate) fn instance_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.instances)
    }
}

/// What the culling needs to know of a mesh.
struct MeshInfo {
    origin: [f64; 3],
    /// The bounds of its vertices, origin included.
    bounds: Aabb,
    /// Whether the mesh has a coarse level: only meshes of more than 12 triangles do.
    has_coarse: bool,
    /// Its objects, in id order: the order they are drawn in.
    objects: Vec<(ObjectId, u32)>,
}

/// An object, in a slot of [`World::slots`].
struct Object {
    mesh: MeshId,
    placement: Placement,
    color: [f32; 4],
    /// The world bounds of its mesh.
    bounds: Aabb,
    leaf: Leaf,
    /// What it showed in the last frame, `None` when it was out of view or is new.
    detail: Option<Detail>,
}

/// The meshes and objects of a renderer, as the CPU sees them.
pub(crate) struct World {
    meshes: BTreeMap<MeshId, MeshInfo>,
    /// Objects by slot; a removed object leaves a free slot.
    slots: Vec<Option<Object>>,
    free: Vec<u32>,
    index: BTreeMap<ObjectId, u32>,
    bvh: Bvh,
    /// The nodes a frustum query has left to classify, with their plane masks.
    pending: Vec<(u32, u8)>,
    /// One bit per slot: whether the object is in view this frame.
    visible: Vec<u64>,
    settings: RenderSettings,
    /// Whether the next frame chooses every level afresh, without hysteresis.
    fresh: bool,
    /// The most objects the renderer can draw in one frame.
    max_objects: usize,
    next_id: u64,
}

impl World {
    /// An empty world that holds at most `max_objects` objects.
    pub(crate) fn new(max_objects: usize) -> Self {
        Self {
            meshes: BTreeMap::new(),
            slots: Vec::new(),
            free: Vec::new(),
            index: BTreeMap::new(),
            bvh: Bvh::new(),
            pending: Vec::new(),
            visible: Vec::new(),
            settings: RenderSettings::default(),
            fresh: false,
            max_objects,
            next_id: 0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }

    pub(crate) fn settings(&self) -> RenderSettings {
        self.settings
    }

    /// Changes the settings, once validated; every object takes its level afresh in the next
    /// frame.
    pub(crate) fn set_settings(&mut self, settings: RenderSettings) -> Result<(), RenderError> {
        if !settings.is_valid() {
            return Err(RenderError::InvalidSettings);
        }
        self.settings = settings;
        self.fresh = true;
        Ok(())
    }

    /// Adds a mesh whose vertices span `bounds` around `origin`, with a coarse level or not.
    pub(crate) fn add_mesh(&mut self, origin: [f64; 3], bounds: Aabb, has_coarse: bool) -> MeshId {
        let id = MeshId(self.new_id());
        self.meshes.insert(
            id,
            MeshInfo {
                origin,
                bounds,
                has_coarse,
                objects: Vec::new(),
            },
        );
        id
    }

    /// Removes a mesh and its objects.
    pub(crate) fn remove_mesh(&mut self, id: MeshId) -> Result<(), RenderError> {
        let mesh = self
            .meshes
            .remove(&id)
            .ok_or(RenderError::UnknownMesh(id))?;
        for (object, slot) in mesh.objects {
            self.index.remove(&object);
            self.release(slot);
        }
        Ok(())
    }

    /// Places a mesh, after checking the mesh, the placement, the colour (linear RGBA, 0 to 1)
    /// and the number of objects, in that order.
    pub(crate) fn add_object(
        &mut self,
        mesh: MeshId,
        placement: Placement,
        color: [f32; 4],
    ) -> Result<ObjectId, RenderError> {
        let info = self
            .meshes
            .get_mut(&mesh)
            .ok_or(RenderError::UnknownMesh(mesh))?;
        placement.validate()?;
        if !color.iter().all(|c| (0.0..=1.0).contains(c)) {
            return Err(RenderError::InvalidColor);
        }
        if self.index.len() >= self.max_objects {
            return Err(RenderError::TooManyObjects {
                max: self.max_objects,
            });
        }
        let id = ObjectId(self.next_id);
        self.next_id += 1;
        let bounds = info.bounds.placed(&placement);
        let slots = &mut self.slots;
        let slot = self.free.pop().unwrap_or_else(|| {
            slots.push(None);
            // The renderer bounds the objects far below u32::MAX.
            u32::try_from(slots.len() - 1).unwrap_or(u32::MAX)
        });
        let leaf = self.bvh.insert(bounds, slot);
        self.slots[slot as usize] = Some(Object {
            mesh,
            placement,
            color,
            bounds,
            leaf,
            detail: None,
        });
        self.index.insert(id, slot);
        // Ids grow, so pushing keeps the list in id order.
        info.objects.push((id, slot));
        Ok(id)
    }

    /// Moves an object, after checking the object, then the placement.
    pub(crate) fn set_placement(
        &mut self,
        id: ObjectId,
        placement: Placement,
    ) -> Result<(), RenderError> {
        let object = self
            .index
            .get(&id)
            .and_then(|&slot| self.slots.get_mut(slot as usize)?.as_mut())
            .ok_or(RenderError::UnknownObject(id))?;
        placement.validate()?;
        object.placement = placement;
        // Every object's mesh is in the world: removing a mesh removes its objects.
        if let Some(mesh) = self.meshes.get(&object.mesh) {
            let bounds = mesh.bounds.placed(&placement);
            if object.bounds != bounds {
                object.bounds = bounds;
                self.bvh.update(object.leaf, bounds);
            }
        }
        Ok(())
    }

    /// Removes an object.
    pub(crate) fn remove_object(&mut self, id: ObjectId) -> Result<(), RenderError> {
        let slot = self
            .index
            .remove(&id)
            .ok_or(RenderError::UnknownObject(id))?;
        let mesh = self.slots[slot as usize].as_ref().map(|object| object.mesh);
        if let Some(info) = mesh.and_then(|mesh| self.meshes.get_mut(&mesh))
            && let Ok(position) = info
                .objects
                .binary_search_by_key(&id, |&(object, _)| object)
        {
            info.objects.remove(position);
        }
        self.release(slot);
        Ok(())
    }

    /// The level the object was drawn at in the last prepared frame; `None` when it was not drawn.
    pub(crate) fn drawn_level(&self, id: ObjectId) -> Result<Option<DetailLevel>, RenderError> {
        let object = self
            .index
            .get(&id)
            .and_then(|&slot| self.slots.get(slot as usize)?.as_ref())
            .ok_or(RenderError::UnknownObject(id))?;
        let has_coarse = self
            .meshes
            .get(&object.mesh)
            .is_some_and(|mesh| mesh.has_coarse);
        Ok(object.detail.and_then(|detail| level(detail, has_coarse)))
    }

    /// Culls the objects, chooses their levels and writes the instance records of the frame
    /// seen through `view` and a validated `projection`, on a target of `size` pixels.
    pub(crate) fn prepare(
        &mut self,
        view: &ViewFrame,
        projection: &Projection,
        size: [u32; 2],
        list: &mut DrawList,
    ) {
        self.find_visible(view, projection, size);
        let scale = SizeScale::new(projection, size[1]);
        list.buckets.clear();
        list.instances.clear();
        list.stats = FrameStats {
            objects: self.len(),
            ..FrameStats::default()
        };
        let settings = self.settings;
        let fresh = core::mem::take(&mut self.fresh);
        for (&mesh_id, mesh) in &self.meshes {
            let radius = mesh.bounds.radius();
            let fine_start = list.instances.len();
            list.coarse.clear();
            for &(_, slot) in &mesh.objects {
                let Some(object) = self.slots[slot as usize].as_mut() else {
                    continue;
                };
                if self.visible[slot as usize / 64] >> (slot % 64) & 1 == 0 {
                    object.detail = None;
                    list.stats.culled_frustum += 1;
                    continue;
                }
                let distance = norm(sub(object.bounds.centre(), view.eye));
                let previous = if fresh { None } else { object.detail };
                let detail = choose(scale.pixels(radius, distance), previous, &settings);
                object.detail = Some(detail);
                let target = match level(detail, mesh.has_coarse) {
                    None => {
                        list.stats.culled_small += 1;
                        continue;
                    }
                    Some(DetailLevel::Fine) => {
                        list.stats.fine += 1;
                        &mut list.instances
                    }
                    Some(DetailLevel::Coarse) => {
                        list.stats.coarse += 1;
                        &mut list.coarse
                    }
                };
                let model_view = camera_relative_model_view(view, mesh.origin, &object.placement);
                push_instance(target, &model_view, object.color);
            }
            let fine_end = list.instances.len();
            push_bucket(list, mesh_id, DetailLevel::Fine, fine_start, fine_end);
            list.instances.extend_from_slice(&list.coarse);
            let coarse_end = list.instances.len();
            push_bucket(list, mesh_id, DetailLevel::Coarse, fine_end, coarse_end);
        }
        list.stats.draw_calls = list.buckets.len();
    }

    /// Sets the bit of every object in view, or of every object without frustum culling.
    fn find_visible(&mut self, view: &ViewFrame, projection: &Projection, size: [u32; 2]) {
        self.visible.clear();
        self.visible.resize(self.slots.len().div_ceil(64), 0);
        if !self.settings.frustum_culling {
            // Free slots are marked too: the frame skips them anyway.
            self.visible.fill(!0);
            return;
        }
        let visible = &mut self.visible;
        let frustum = Frustum::new(view, projection, f64::from(size[0]) / f64::from(size[1]));
        self.bvh.query(
            &mut self.pending,
            frustum.all_planes(),
            |bounds, mask| frustum.classify(bounds, mask),
            |slot| visible[slot as usize / 64] |= 1 << (slot % 64),
        );
    }

    fn release(&mut self, slot: u32) {
        if let Some(object) = self.slots.get_mut(slot as usize).and_then(Option::take) {
            self.bvh.remove(object.leaf);
            self.free.push(slot);
        }
    }

    fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// The level an object of `detail` draws, depending on whether its mesh has a coarse level.
fn level(detail: Detail, has_coarse: bool) -> Option<DetailLevel> {
    match detail {
        Detail::Hidden => None,
        Detail::Coarse if has_coarse => Some(DetailLevel::Coarse),
        Detail::Coarse | Detail::Fine => Some(DetailLevel::Fine),
    }
}

/// Appends one instance record.
fn push_instance(target: &mut Vec<f32>, model_view: &Matrix, color: [f32; 4]) {
    target.extend_from_slice(model_view.as_flattened());
    target.extend_from_slice(&color);
}

/// Records the run of instances between two offsets in floats, unless it is empty.
fn push_bucket(list: &mut DrawList, mesh: MeshId, level: DetailLevel, start: usize, end: usize) {
    if end == start {
        return;
    }
    // The renderer bounds the objects so that instance indices fit u32.
    let index = |floats: usize| u32::try_from(floats / INSTANCE_FLOATS).unwrap_or(u32::MAX);
    list.buckets.push(Bucket {
        mesh,
        level,
        first: index(start),
        count: index(end) - index(start),
    });
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use geom::{Point3, Vector3};

    use super::*;
    use crate::Camera;

    const WHITE: [f32; 4] = [1.0; 4];

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: FRAC_PI_2,
        near: 0.1,
    };

    /// 100 px high: with 90°, a 1 m sphere (radius 0.5 m) at distance d is 50/d px.
    const SIZE: [u32; 2] = [100, 100];

    /// Corners of a 1 m cube centred on its origin: `triangles` decides whether it has a coarse
    /// level.
    fn cube(world: &mut World, origin: [f64; 3], triangles: usize) -> MeshId {
        let corners: Vec<[f32; 3]> = (0..8)
            .map(|c| core::array::from_fn(|axis| if c >> axis & 1 == 0 { -0.5 } else { 0.5 }))
            .collect();
        let bounds = Aabb::of_local(origin, &corners).unwrap();
        world.add_mesh(origin, bounds, triangles > 12)
    }

    /// A camera at `eye` looking along +y.
    fn view_from(eye: [f64; 3]) -> ViewFrame {
        let camera = Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(eye[0], eye[1] + 1.0, eye[2]),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: PERSPECTIVE,
        };
        ViewFrame::new(&camera).unwrap()
    }

    fn prepare(world: &mut World, eye: [f64; 3]) -> DrawList {
        let mut list = DrawList::default();
        world.prepare(&view_from(eye), &PERSPECTIVE, SIZE, &mut list);
        list
    }

    /// A row of `count` cubes along x, 2 m apart, 10 m ahead of the origin.
    fn row(world: &mut World, mesh: MeshId, count: u32) -> Vec<ObjectId> {
        (0..count)
            .map(|i| {
                let at = [f64::from(i) * 2.0 - f64::from(count), 10.0, 0.0];
                world
                    .add_object(mesh, Placement::from_translation(at), WHITE)
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn objects_sharing_a_mesh_are_one_bucket_in_id_order() {
        let mut world = World::new(usize::MAX);
        let first = cube(&mut world, [0.0; 3], 12);
        let second = cube(&mut world, [0.0; 3], 12);
        let a = row(&mut world, first, 3);
        let b = row(&mut world, second, 2);
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!(
            list.buckets,
            [
                Bucket {
                    mesh: first,
                    level: DetailLevel::Fine,
                    first: 0,
                    count: 3
                },
                Bucket {
                    mesh: second,
                    level: DetailLevel::Fine,
                    first: 3,
                    count: 2
                },
            ]
        );
        assert_eq!(list.instances.len(), 5 * INSTANCE_FLOATS);
        assert_eq!(list.instance_bytes().len() as u64, 5 * INSTANCE_STRIDE);
        assert_eq!(list.stats.draw_calls, 2);
        assert_eq!(list.stats.fine, 5);
        // The record of the first object: its model-view, then its colour.
        let view = view_from([0.0; 3]);
        let placement = Placement::from_translation([-3.0, 10.0, 0.0]);
        let model_view = camera_relative_model_view(&view, [0.0; 3], &placement);
        assert_eq!(&list.instances[..16], model_view.as_flattened());
        assert_eq!(&list.instances[16..20], &WHITE);
        for id in a.iter().chain(&b) {
            assert_eq!(world.drawn_level(*id), Ok(Some(DetailLevel::Fine)));
        }
    }

    #[test]
    fn objects_behind_or_beside_the_camera_are_culled_far_from_the_origin() {
        let far = [1e7, -1e7, 50.0];
        let mut world = World::new(usize::MAX);
        let mesh = cube(&mut world, far, 12);
        // 20 cubes from x = −20 to 18 m at 10 m ahead: the 90° view spans ±10 m there.
        let ahead = row(&mut world, mesh, 20);
        let behind = world
            .add_object(mesh, Placement::from_translation([0.0, -10.0, 0.0]), WHITE)
            .unwrap();
        let list = prepare(&mut world, far);
        // x from −20 to 18: the cubes at −10 (reaching −9.5) to 10 (from 9.5)... the ones whose
        // box reaches into |x| ≤ 10 at y = 10, i.e. centres −10..=10: 11 cubes.
        assert_eq!(list.stats.fine, 11);
        assert_eq!(list.stats.culled_frustum, 10);
        assert_eq!(world.drawn_level(behind), Ok(None));
        assert_eq!(world.drawn_level(ahead[0]), Ok(None));
        assert_eq!(world.drawn_level(ahead[10]), Ok(Some(DetailLevel::Fine)));
        // Without culling, everything in front of the camera is drawn, as is what is behind it:
        // the GPU clips it.
        world
            .set_settings(RenderSettings {
                frustum_culling: false,
                ..RenderSettings::default()
            })
            .unwrap();
        let list = prepare(&mut world, far);
        assert_eq!((list.stats.fine, list.stats.culled_frustum), (21, 0));
    }

    #[test]
    fn levels_switch_with_distance_and_hysteresis() {
        use DetailLevel::{Coarse, Fine};
        let mut world = World::new(usize::MAX);
        let detailed = cube(&mut world, [0.0; 3], 100);
        let simple = cube(&mut world, [0.0; 3], 12);
        let near = world
            .add_object(
                detailed,
                Placement::from_translation([0.0, 0.0, 0.0]),
                WHITE,
            )
            .unwrap();
        let plain = world
            .add_object(simple, Placement::from_translation([0.0, 0.0, 0.0]), WHITE)
            .unwrap();
        // The cubes' spheres have a radius of √3/2 m: √3·50/d px. 24 px at d ≈ 3.61 m, 30 px
        // (24 × 1.25) at d ≈ 2.89 m, 1 px at d ≈ 86.6 m, 1.25 px at d ≈ 69.3 m.
        let level_at = |world: &mut World, distance: f64| {
            prepare(world, [0.0, -distance, 0.0]);
            (
                world.drawn_level(near).unwrap(),
                world.drawn_level(plain).unwrap(),
            )
        };
        assert_eq!(level_at(&mut world, 3.5), (Some(Fine), Some(Fine)));
        assert_eq!(level_at(&mut world, 3.7), (Some(Coarse), Some(Fine)));
        // Back inside the band: stays coarse.
        assert_eq!(level_at(&mut world, 3.0), (Some(Coarse), Some(Fine)));
        assert_eq!(level_at(&mut world, 2.8), (Some(Fine), Some(Fine)));
        assert_eq!(level_at(&mut world, 3.0), (Some(Fine), Some(Fine)));
        // Far away: hidden below 1 px, back only above 1.25 px.
        assert_eq!(level_at(&mut world, 90.0), (None, None));
        assert_eq!(level_at(&mut world, 80.0), (None, None));
        assert_eq!(level_at(&mut world, 60.0), (Some(Coarse), Some(Fine)));
        let list = prepare(&mut world, [0.0, -60.0, 0.0]);
        assert_eq!((list.stats.fine, list.stats.coarse), (1, 1));
        assert_eq!(list.stats.draw_calls, 2);
        assert_eq!(list.buckets[0].level, Coarse);
        assert_eq!(list.buckets[1].level, Fine);
        let list = prepare(&mut world, [0.0, -90.0, 0.0]);
        assert_eq!((list.stats.culled_small, list.stats.draw_calls), (2, 0));
        // Changing the settings forgets the hysteresis: at 80 m the objects are 1.08 px.
        world.set_settings(RenderSettings::default()).unwrap();
        assert_eq!(level_at(&mut world, 80.0), (Some(Coarse), Some(Fine)));
    }

    #[test]
    fn an_object_leaving_the_view_takes_its_level_afresh_on_return() {
        let mut world = World::new(usize::MAX);
        let mesh = cube(&mut world, [0.0; 3], 100);
        let object = world.add_object(mesh, Placement::IDENTITY, WHITE).unwrap();
        prepare(&mut world, [0.0, -3.7, 0.0]);
        assert_eq!(world.drawn_level(object), Ok(Some(DetailLevel::Coarse)));
        // Looking away from it, then back from inside the band.
        prepare(&mut world, [0.0, 3.7, 0.0]);
        assert_eq!(world.drawn_level(object), Ok(None));
        prepare(&mut world, [0.0, -3.0, 0.0]);
        assert_eq!(world.drawn_level(object), Ok(Some(DetailLevel::Fine)));
    }

    #[test]
    fn moved_and_removed_objects_are_culled_where_they_are() {
        let mut world = World::new(usize::MAX);
        let mesh = cube(&mut world, [0.0; 3], 12);
        let objects = row(&mut world, mesh, 4);
        assert_eq!(prepare(&mut world, [0.0; 3]).stats.fine, 4);
        // Moving the first cube behind the camera culls it.
        let behind = Placement::from_translation([0.0, -5.0, 0.0]);
        world.set_placement(objects[0], behind).unwrap();
        assert_eq!(prepare(&mut world, [0.0; 3]).stats.fine, 3);
        // The same placement again changes nothing.
        world.set_placement(objects[0], behind).unwrap();
        world.remove_object(objects[1]).unwrap();
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!(
            (
                list.stats.objects,
                list.stats.fine,
                list.stats.culled_frustum
            ),
            (3, 2, 1)
        );
        assert_eq!(
            world.remove_object(objects[1]),
            Err(RenderError::UnknownObject(objects[1]))
        );
        assert_eq!(
            world.drawn_level(objects[1]),
            Err(RenderError::UnknownObject(objects[1]))
        );
        assert_eq!(
            world.set_placement(objects[1], Placement::IDENTITY),
            Err(RenderError::UnknownObject(objects[1]))
        );
        // A freed slot is reused by the next object, which draws in id order, last.
        let again = world
            .add_object(mesh, Placement::from_translation([0.0, 5.0, 1.0]), WHITE)
            .unwrap();
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!(list.stats.fine, 3);
        assert_eq!(world.slots.len(), 4);
        let view = view_from([0.0; 3]);
        let model_view = camera_relative_model_view(
            &view,
            [0.0; 3],
            &Placement::from_translation([0.0, 5.0, 1.0]),
        );
        assert_eq!(
            &list.instances[2 * INSTANCE_FLOATS..][..16],
            model_view.as_flattened()
        );
        assert!(world.drawn_level(again).is_ok());
        // Removing the mesh removes its objects.
        world.remove_mesh(mesh).unwrap();
        assert_eq!(world.len(), 0);
        assert_eq!(world.remove_mesh(mesh), Err(RenderError::UnknownMesh(mesh)));
        assert_eq!(
            world.add_object(mesh, Placement::IDENTITY, WHITE),
            Err(RenderError::UnknownMesh(mesh))
        );
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!((list.stats, list.buckets.len()), (FrameStats::default(), 0));
    }

    #[test]
    fn only_meshes_with_a_coarse_level_draw_one() {
        let mut world = World::new(usize::MAX);
        for triangles in [12, 13] {
            let mesh = cube(&mut world, [0.0; 3], triangles);
            world
                .add_object(mesh, Placement::from_translation([0.0, 10.0, 0.0]), WHITE)
                .unwrap();
        }
        // 8.7 px: coarse, but only the mesh of 13 triangles has a coarse level.
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!((list.stats.fine, list.stats.coarse), (1, 1));
    }

    #[test]
    fn objects_are_checked_mesh_first_and_counted() {
        let mut world = World::new(1);
        let mesh = cube(&mut world, [0.0; 3], 12);
        let bad = Placement::from_translation([f64::NAN; 3]);
        world.remove_mesh(mesh).unwrap();
        assert_eq!(
            world.add_object(mesh, bad, [2.0; 4]),
            Err(RenderError::UnknownMesh(mesh))
        );
        let mesh = cube(&mut world, [0.0; 3], 12);
        assert!(matches!(
            world.add_object(mesh, bad, [2.0; 4]),
            Err(RenderError::Placement(_))
        ));
        assert_eq!(
            world.add_object(mesh, Placement::IDENTITY, [2.0; 4]),
            Err(RenderError::InvalidColor)
        );
        let object = world.add_object(mesh, Placement::IDENTITY, WHITE).unwrap();
        assert_eq!(
            world.add_object(mesh, Placement::IDENTITY, WHITE),
            Err(RenderError::TooManyObjects { max: 1 })
        );
        assert!(matches!(
            world.set_placement(object, bad),
            Err(RenderError::Placement(_))
        ));
        let bad_settings = RenderSettings {
            hysteresis: 2.0,
            ..RenderSettings::default()
        };
        assert_eq!(
            world.set_settings(bad_settings),
            Err(RenderError::InvalidSettings)
        );
    }

    /// The reference model in a world; the ids of its meshes, in the model's order.
    fn reference_world(storeys: u32) -> World {
        let model = crate::reference::building(storeys);
        let mut world = World::new(usize::MAX);
        let meshes: Vec<MeshId> = model
            .meshes
            .iter()
            .map(|mesh| {
                let bounds = Aabb::of_local(mesh.origin, &mesh.positions).unwrap();
                world.add_mesh(mesh.origin, bounds, mesh.indices.len() > 36)
            })
            .collect();
        for (mesh, placement, color) in model.objects {
            world.add_object(meshes[mesh], placement, color).unwrap();
        }
        world
    }

    fn reference_frame(world: &mut World, camera: &Camera) -> FrameStats {
        let mut list = DrawList::default();
        let view = ViewFrame::new(camera).unwrap();
        world.prepare(&view, &camera.projection, crate::reference::VIEW, &mut list);
        let stats = list.stats;
        assert_eq!(
            stats.culled_frustum + stats.culled_small + stats.fine + stats.coarse,
            stats.objects
        );
        assert_eq!(
            list.instances.len(),
            (stats.fine + stats.coarse) * INSTANCE_FLOATS
        );
        stats
    }

    #[test]
    fn the_reference_model_has_100_000_elements_and_15_meshes() {
        let model = crate::reference::building(crate::reference::STOREYS);
        assert_eq!(model.objects.len(), 100_150);
        assert_eq!(model.meshes.len(), 15);
        // About 10,000 per storey.
        assert_eq!(crate::reference::building(1).objects.len(), 10_015);
    }

    /// Culling and levels of the reference model from fixed cameras. The counts are pinned:
    /// a change in them is a change in what the renderer draws.
    #[test]
    fn reference_frames_draw_what_they_should() {
        use crate::reference::{aerial, inside, orbit};
        let mut world = reference_world(crate::reference::STOREYS);
        let counts = |stats: FrameStats| {
            (
                stats.culled_frustum,
                stats.culled_small,
                stats.fine,
                stats.coarse,
                stats.draw_calls,
            )
        };
        // From high above, everything is in view; the 2,000 sprinkler heads (4 cm) are under a
        // pixel, the far I-sections, doors, desks, chairs and monitors are boxes.
        assert_eq!(
            counts(reference_frame(&mut world, &aerial())),
            (0, 2_000, 84_520, 13_630, 14)
        );
        // Inside, a third of the building is behind the camera.
        assert_eq!(
            counts(reference_frame(&mut world, &inside())),
            (29_520, 404, 65_692, 4_534, 18)
        );
        // Around the orbit, the whole building stays in view, and no frame needs more than two
        // draw calls per mesh.
        for step in 0..36 {
            let stats = reference_frame(&mut world, &orbit(step, 36));
            assert_eq!(
                (stats.culled_frustum, stats.culled_small),
                (0, 2_000),
                "{step}"
            );
            assert!(stats.draw_calls <= 2 * 15);
        }
    }
}
