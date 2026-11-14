//! The CPU side of the renderer: the meshes and objects it holds, their bounds in a hierarchy,
//! and the preparation of each frame, which culls (outside the view, hidden behind others, too
//! small), chooses levels of detail, and writes the instance records and the occlusion queries
//! (VP-02, `docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`).
//!
//! Nothing here touches the GPU, so every decision is tested without a device.

use std::collections::BTreeMap;

use crate::bounds::Aabb;
use crate::bvh::{Bvh, Leaf, Overlap};
use crate::frustum::Frustum;
use crate::lod::{Detail, DetailLevel, RenderSettings, SizeScale, choose};
use crate::occlusion::{CLUSTER_LEAVES, Occlusion, beyond_near_plane};
use crate::theme::{DisplayState, Tints};
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
    /// Objects in view whose cluster the previous frames found hidden behind others.
    pub culled_occluded: usize,
    /// Objects in view but smaller than the small-feature threshold.
    pub culled_small: usize,
    /// Objects drawn with their mesh.
    pub fine: usize,
    /// Objects drawn with the coarse level of their mesh.
    pub coarse: usize,
    /// Objects in the [`DisplayState::Ghost`] state that are drawn, translucent, in the overlay
    /// pass; they are counted neither in `fine` nor in `coarse`.
    pub ghosts: usize,
    /// Instanced draw calls: one per mesh and level drawn.
    pub draw_calls: usize,
    /// Clusters whose visibility the frame tests on the GPU, at most
    /// [`RenderSettings::occlusion_queries`].
    pub occlusion_queries: usize,
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
    /// The instance records of the ghosts, [`INSTANCE_FLOATS`] values each, in the order of
    /// their buckets: those of the objects in the ghost state first, then the transient ones.
    pub(crate) ghosts: Vec<f32>,
    /// The runs of [`DrawList::ghosts`], always at the fine level.
    pub(crate) ghost_buckets: Vec<Bucket>,
    /// The instance records of the translucent boxes: the run of [`DrawList::ghosts`] after the
    /// ghosts, as `(first, count)`.
    pub(crate) boxes: (u32, u32),
    pub(crate) stats: FrameStats,
    /// The nodes of the clusters to test after drawing, the instance records of their boxes, and
    /// the number of the frame that tests them.
    pub(crate) queries: Vec<u32>,
    pub(crate) query_instances: Vec<f32>,
    pub(crate) frame: u64,
    /// The coarse instances of the mesh being written, appended after its fine ones.
    coarse: Vec<f32>,
}

impl DrawList {
    /// Bytes of the instance records.
    pub(crate) fn instance_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.instances)
    }

    /// Bytes of the instance records of the ghosts and the boxes.
    pub(crate) fn ghost_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.ghosts)
    }

    /// Bytes of the instance records of the occlusion query boxes.
    pub(crate) fn query_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.query_instances)
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
    id: ObjectId,
    mesh: MeshId,
    placement: Placement,
    color: [f32; 4],
    state: DisplayState,
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
    /// How many objects are in the [`DisplayState::Ghost`] state.
    ghosts: usize,
    bvh: Bvh,
    /// The nodes a frustum query has left to classify, with their plane masks.
    pending: Vec<(u32, u8)>,
    /// One bit per slot: whether the object is in view this frame.
    visible: Vec<u64>,
    /// One bit per slot: whether the object's cluster is skipped as hidden this frame.
    occluded: Vec<u64>,
    occlusion: Occlusion,
    /// The nodes the cluster walk has left to visit.
    clusters: Vec<u32>,
    settings: RenderSettings,
    /// The colours of the display states.
    tints: Tints,
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
            ghosts: 0,
            bvh: Bvh::new(),
            pending: Vec::new(),
            visible: Vec::new(),
            occluded: Vec::new(),
            occlusion: Occlusion::default(),
            clusters: Vec::new(),
            settings: RenderSettings::default(),
            tints: Tints::default(),
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
        self.occlusion.invalidate();
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
        self.occlusion.invalidate();
        self.slots[slot as usize] = Some(Object {
            id,
            mesh,
            placement,
            color,
            state: DisplayState::Normal,
            bounds,
            leaf,
            detail: None,
        });
        self.index.insert(id, slot);
        // Ids grow, so pushing keeps the list in id order.
        info.objects.push((id, slot));
        Ok(id)
    }

    /// Sets the colours of the display states, from the next frame on.
    pub(crate) fn set_tints(&mut self, tints: Tints) {
        self.tints = tints;
    }

    /// Sets how an object is drawn. Cheap: nothing but the colour of its instance changes, and
    /// an object leaving or entering the ghost state moves to the other pass.
    pub(crate) fn set_state(
        &mut self,
        id: ObjectId,
        state: DisplayState,
    ) -> Result<(), RenderError> {
        let object = self.object_mut(id)?;
        let was_ghost = object.state == DisplayState::Ghost;
        object.state = state;
        match (was_ghost, state == DisplayState::Ghost) {
            (false, true) => self.ghosts += 1,
            (true, false) => self.ghosts -= 1,
            _ => {}
        }
        Ok(())
    }

    /// How an object is drawn.
    pub(crate) fn state(&self, id: ObjectId) -> Result<DisplayState, RenderError> {
        self.object(id).map(|object| object.state)
    }

    /// How many objects are in the [`DisplayState::Ghost`] state.
    pub(crate) fn ghost_state_count(&self) -> usize {
        self.ghosts
    }

    /// What a ghost of an object needs: its mesh, the origin of the mesh and its placement.
    pub(crate) fn ghost_source(&self, id: ObjectId) -> Option<(MeshId, [f64; 3], Placement)> {
        let object = self.object(id).ok()?;
        let mesh = self.meshes.get(&object.mesh)?;
        Some((object.mesh, mesh.origin, object.placement))
    }

    fn object(&self, id: ObjectId) -> Result<&Object, RenderError> {
        self.index
            .get(&id)
            .and_then(|&slot| self.slots.get(slot as usize)?.as_ref())
            .ok_or(RenderError::UnknownObject(id))
    }

    fn object_mut(&mut self, id: ObjectId) -> Result<&mut Object, RenderError> {
        self.index
            .get(&id)
            .and_then(|&slot| self.slots.get_mut(slot as usize)?.as_mut())
            .ok_or(RenderError::UnknownObject(id))
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
        if object.placement != placement {
            // A turn can keep the bounds yet change what the object hides.
            self.occlusion.invalidate();
        }
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
        let object = self.object(id)?;
        let has_coarse = self
            .meshes
            .get(&object.mesh)
            .is_some_and(|mesh| mesh.has_coarse);
        Ok(object.detail.and_then(|detail| level(detail, has_coarse)))
    }

    /// Records the result of the occlusion query that the frame numbered `frame` issued for the
    /// cluster at `node`: whether any sample of its box passed the depth test.
    pub(crate) fn record_occlusion(&mut self, frame: u64, node: u32, visible: bool) {
        self.occlusion.record(frame, node, visible);
    }

    /// Culls the objects, chooses their levels and writes the instance records of the frame
    /// seen through `view` and a validated `projection`, on a target of `size` pixels, with at
    /// most `query_budget` occlusion queries.
    pub(crate) fn prepare(
        &mut self,
        view: &ViewFrame,
        projection: &Projection,
        size: [u32; 2],
        query_budget: usize,
        list: &mut DrawList,
    ) {
        let frustum = Frustum::new(view, projection, f64::from(size[0]) / f64::from(size[1]));
        self.find_visible(&frustum);
        self.find_occluded(&frustum, view, projection, query_budget, list);
        let scale = SizeScale::new(projection, size[1]);
        list.buckets.clear();
        list.instances.clear();
        list.ghosts.clear();
        list.ghost_buckets.clear();
        list.boxes = (0, 0);
        list.stats = FrameStats {
            objects: self.len(),
            occlusion_queries: list.queries.len(),
            ..FrameStats::default()
        };
        let settings = self.settings;
        let fresh = core::mem::take(&mut self.fresh);
        for (&mesh_id, mesh) in &self.meshes {
            let radius = mesh.bounds.radius();
            let fine_start = list.instances.len();
            let ghost_start = list.ghosts.len();
            list.coarse.clear();
            for &(_, slot) in &mesh.objects {
                let Some(object) = self.slots[slot as usize].as_mut() else {
                    continue;
                };
                if !has_bit(&self.visible, slot) {
                    object.detail = None;
                    list.stats.culled_frustum += 1;
                    continue;
                }
                if has_bit(&self.occluded, slot) {
                    object.detail = None;
                    list.stats.culled_occluded += 1;
                    continue;
                }
                let distance = norm(sub(object.bounds.centre(), view.eye));
                let previous = if fresh { None } else { object.detail };
                let detail = choose(scale.pixels(radius, distance), previous, &settings);
                object.detail = Some(detail);
                let shown = level(detail, mesh.has_coarse);
                if shown.is_some() && object.state == DisplayState::Ghost {
                    list.stats.ghosts += 1;
                    let model_view =
                        camera_relative_model_view(view, mesh.origin, &object.placement);
                    let color = self.tints.apply(object.state, object.color);
                    push_instance(&mut list.ghosts, &model_view, color);
                    continue;
                }
                let target = match shown {
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
                let color = self.tints.apply(object.state, object.color);
                push_instance(target, &model_view, color);
            }
            push_bucket(
                list,
                mesh_id,
                DetailLevel::Fine,
                ghost_start,
                list.ghosts.len(),
                true,
            );
            let fine_end = list.instances.len();
            push_bucket(
                list,
                mesh_id,
                DetailLevel::Fine,
                fine_start,
                fine_end,
                false,
            );
            list.instances.extend_from_slice(&list.coarse);
            let coarse_end = list.instances.len();
            push_bucket(
                list,
                mesh_id,
                DetailLevel::Coarse,
                fine_end,
                coarse_end,
                false,
            );
        }
        list.stats.draw_calls = list.buckets.len();
    }

    /// Calls `found` with every object whose world bounds meet `frustum`, whatever the culling
    /// settings, occlusion and level of detail: what a pick may hit.
    pub(crate) fn objects_in(&mut self, frustum: &Frustum, mut found: impl FnMut(ObjectId)) {
        let slots = &self.slots;
        self.bvh.query(
            &mut self.pending,
            frustum.all_planes(),
            |bounds, mask| frustum.classify(bounds, mask),
            |slot| {
                if let Some(object) = slots.get(slot as usize).and_then(Option::as_ref) {
                    found(object.id);
                }
            },
        );
    }

    /// Sets the bit of every object in view, or of every object without frustum culling.
    fn find_visible(&mut self, frustum: &Frustum) {
        clear_bits(&mut self.visible, self.slots.len());
        if !self.settings.frustum_culling {
            // Free slots are marked too: the frame skips them anyway.
            self.visible.fill(!0);
            return;
        }
        let visible = &mut self.visible;
        self.bvh.query(
            &mut self.pending,
            frustum.all_planes(),
            |bounds, mask| frustum.classify(bounds, mask),
            |slot| set_bit(visible, slot),
        );
    }

    /// Sets the bit of every object whose cluster the previous frames found hidden, and chooses
    /// the clusters in view to test after drawing, when occlusion culling is on.
    fn find_occluded(
        &mut self,
        frustum: &Frustum,
        view: &ViewFrame,
        projection: &Projection,
        query_budget: usize,
        list: &mut DrawList,
    ) {
        clear_bits(&mut self.occluded, self.slots.len());
        list.queries.clear();
        list.query_instances.clear();
        if !self.settings.occlusion_culling {
            return;
        }
        list.frame = self.occlusion.begin_frame(self.bvh.node_capacity());
        let occlusion = &mut self.occlusion;
        let occluded = &mut self.occluded;
        self.bvh.clusters(
            &mut self.clusters,
            CLUSTER_LEAVES,
            |node, bounds, children| {
                let in_view = !matches!(
                    frustum.classify(bounds, frustum.all_planes()),
                    Overlap::Outside
                );
                let query = in_view && beyond_near_plane(view, projection, bounds);
                occlusion.visit(node, bounds, children, in_view, query)
            },
            |slot| set_bit(occluded, slot),
        );
        let budget = query_budget.min(self.settings.occlusion_queries as usize);
        self.occlusion
            .select(budget, view, &mut list.queries, &mut list.query_instances);
    }

    fn release(&mut self, slot: u32) {
        if let Some(object) = self.slots.get_mut(slot as usize).and_then(Option::take) {
            if object.state == DisplayState::Ghost {
                self.ghosts -= 1;
            }
            self.occlusion.invalidate();
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

/// Makes `bits` one cleared bit per slot, for `slots` slots.
fn clear_bits(bits: &mut Vec<u64>, slots: usize) {
    bits.clear();
    bits.resize(slots.div_ceil(64), 0);
}

fn set_bit(bits: &mut [u64], slot: u32) {
    bits[slot as usize / 64] |= 1 << (slot % 64);
}

fn has_bit(bits: &[u64], slot: u32) -> bool {
    bits[slot as usize / 64] >> (slot % 64) & 1 == 1
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
fn push_bucket(
    list: &mut DrawList,
    mesh: MeshId,
    level: DetailLevel,
    start: usize,
    end: usize,
    ghost: bool,
) {
    if end == start {
        return;
    }
    // The renderer bounds the objects so that instance indices fit u32.
    let index = |floats: usize| u32::try_from(floats / INSTANCE_FLOATS).unwrap_or(u32::MAX);
    let bucket = Bucket {
        mesh,
        level,
        first: index(start),
        count: index(end) - index(start),
    };
    if ghost {
        list.ghost_buckets.push(bucket);
    } else {
        list.buckets.push(bucket);
    }
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use geom::{Point3, Vector3};

    use super::*;
    use crate::Camera;
    use crate::occlusion::MAX_AGE;

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
        world.prepare(&view_from(eye), &PERSPECTIVE, SIZE, 0, &mut list);
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
    fn display_states_tint_instances_and_ghosts_leave_the_opaque_pass() {
        let mut world = World::new(16);
        let mesh = cube(&mut world, [0.0; 3], 12);
        let objects = row(&mut world, mesh, 3);
        let theme = crate::Theme::light();
        world.set_tints(theme.tints());
        assert_eq!(world.state(objects[0]), Ok(DisplayState::Normal));
        world.set_state(objects[0], DisplayState::Selected).unwrap();
        assert_eq!(world.ghost_state_count(), 0);
        world.set_state(objects[1], DisplayState::Ghost).unwrap();
        assert_eq!(world.state(objects[1]), Ok(DisplayState::Ghost));
        assert_eq!(world.ghost_state_count(), 1);
        // Setting the same state again counts once.
        world.set_state(objects[1], DisplayState::Ghost).unwrap();
        assert_eq!(world.ghost_state_count(), 1);

        let list = prepare(&mut world, [0.0; 3]);
        // Two objects in the opaque pass, the selected one tinted; one ghost, translucent.
        assert_eq!((list.stats.fine, list.stats.ghosts), (2, 1));
        assert_eq!(list.instances.len(), 2 * INSTANCE_FLOATS);
        assert_eq!(list.ghosts.len(), INSTANCE_FLOATS);
        let tints = theme.tints();
        assert_eq!(
            list.instances[16..20],
            tints.apply(DisplayState::Selected, WHITE)
        );
        assert_eq!(list.instances[36..40], WHITE);
        assert_eq!(list.ghosts[16..20], tints.apply(DisplayState::Ghost, WHITE));
        assert_eq!(list.ghost_buckets.len(), 1);
        assert_eq!(
            (list.ghost_buckets[0].mesh, list.ghost_buckets[0].count),
            (mesh, 1)
        );
        // Ghosts are not opaque draw calls.
        assert_eq!(list.buckets.len(), 1);
        assert_eq!(list.buckets[0].count, 2);

        // A ghost out of view or removed is not drawn; back to normal, it joins the opaque pass.
        world.set_state(objects[1], DisplayState::Normal).unwrap();
        assert_eq!(world.ghost_state_count(), 0);
        let list = prepare(&mut world, [0.0; 3]);
        assert_eq!((list.stats.fine, list.stats.ghosts), (3, 0));
        assert!(list.ghost_buckets.is_empty());
        // A removed ghost is no longer counted.
        world.set_state(objects[2], DisplayState::Ghost).unwrap();
        world.remove_object(objects[2]).unwrap();
        assert_eq!(world.ghost_state_count(), 0);
        assert_eq!(
            world.set_state(objects[2], DisplayState::Ghost),
            Err(RenderError::UnknownObject(objects[2]))
        );
        assert_eq!(
            world.state(objects[2]),
            Err(RenderError::UnknownObject(objects[2]))
        );
        assert!(world.ghost_source(objects[2]).is_none());
        let (found, origin, placement) = world.ghost_source(objects[0]).unwrap();
        assert_eq!((found, origin), (mesh, [0.0; 3]));
        assert!(
            placement
                .translation
                .iter()
                .zip([-3.0, 10.0, 0.0])
                .all(|(a, b)| (a - b).abs() < 1e-12)
        );
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

    /// A camera at `eye` looking along `direction`, z up.
    fn view_towards(eye: [f64; 3], direction: [f64; 3]) -> ViewFrame {
        let camera = Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(
                eye[0] + direction[0],
                eye[1] + direction[1],
                eye[2] + direction[2],
            ),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: PERSPECTIVE,
        };
        ViewFrame::new(&camera).unwrap()
    }

    /// Prepares a frame with up to `budget` occlusion queries.
    fn prepare_queried(world: &mut World, view: &ViewFrame, budget: usize) -> DrawList {
        let mut list = DrawList::default();
        world.prepare(view, &PERSPECTIVE, SIZE, budget, &mut list);
        assert_eq!(list.stats.occlusion_queries, list.queries.len());
        assert_eq!(
            list.query_instances.len(),
            list.queries.len() * INSTANCE_FLOATS
        );
        list
    }

    /// Answers the queries of a frame as a GPU would behind a wall that fills the view at
    /// `depth` metres: a box is visible when its nearest point is not behind the wall.
    fn answer_behind_wall(world: &mut World, list: &DrawList, depth: f64) {
        for (record, &node) in list
            .query_instances
            .as_chunks::<INSTANCE_FLOATS>()
            .0
            .iter()
            .zip(&list.queries)
        {
            // Column 3 holds the centre relative to the eye; row 2 of each column is the
            // backward axis, the opposite of depth.
            let centre = -f64::from(record[14]);
            let reach: f64 = (0..3)
                .map(|column| f64::from(record[column * 4 + 2]).abs())
                .sum();
            world.record_occlusion(list.frame, node, centre - reach <= depth);
        }
    }

    /// A wall 12 m wide and high, 5 m ahead of the origin across the +y view, which it fills,
    /// and 300 cubes 30 m ahead behind it; the wall's id.
    fn walled_world() -> (World, ObjectId) {
        let mut world = World::new(usize::MAX);
        let wall_bounds = Aabb {
            min: [-6.0, -0.1, -6.0],
            max: [6.0, 0.1, 6.0],
        };
        let wall_mesh = world.add_mesh([0.0; 3], wall_bounds, false);
        let wall = world
            .add_object(
                wall_mesh,
                Placement::from_translation([0.0, 5.0, 0.0]),
                WHITE,
            )
            .unwrap();
        let mesh = cube(&mut world, [0.0; 3], 12);
        for i in 0..20_u32 {
            for j in 0..15_u32 {
                let at = [f64::from(i) * 2.0 - 19.0, 30.0, f64::from(j) * 2.0 - 14.0];
                world
                    .add_object(mesh, Placement::from_translation(at), WHITE)
                    .unwrap();
            }
        }
        (world, wall)
    }

    #[test]
    fn clusters_found_hidden_are_skipped_until_something_may_reveal_them() {
        let (mut world, wall) = walled_world();
        let ahead = view_from([0.0; 3]);
        // The first frame draws everything and tests every cluster: the cubes need three.
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (301, 0));
        let clusters = list.queries.len();
        assert!(clusters >= 3, "{clusters}");
        answer_behind_wall(&mut world, &list, 5.0);
        // Then only the wall is drawn, and the hidden clusters are tested again, several at
        // once through the nodes above them.
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (1, 300));
        assert_eq!(list.instances.len(), INSTANCE_FLOATS);
        assert_eq!(world.drawn_level(wall), Ok(Some(DetailLevel::Fine)));
        assert!(list.queries.len() < clusters, "{}", list.queries.len());
        // A budget of one query a frame goes to a hidden cluster; none keeps the last results.
        assert_eq!(prepare_queried(&mut world, &ahead, 1).queries.len(), 1);
        let list = prepare_queried(&mut world, &ahead, 0);
        assert_eq!((list.stats.culled_occluded, list.queries.len()), (300, 0));
        // Results expire after MAX_AGE frames without a new query.
        for _ in 0..MAX_AGE {
            prepare_queried(&mut world, &ahead, 0);
        }
        assert_eq!(
            prepare_queried(&mut world, &ahead, 0).stats.culled_occluded,
            0
        );
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!(list.queries.len(), clusters);
        answer_behind_wall(&mut world, &list, 5.0);
        assert_eq!(
            prepare_queried(&mut world, &ahead, 1024)
                .stats
                .culled_occluded,
            300
        );
        // Turning the wall half a turn keeps its bounds but may change what it hides: the next
        // frame draws the cubes too.
        let turned = Placement {
            rotation: [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0, 5.0, 0.0],
        };
        world.set_placement(wall, turned).unwrap();
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (301, 0));
        answer_behind_wall(&mut world, &list, 5.0);
        assert_eq!(
            prepare_queried(&mut world, &ahead, 1024)
                .stats
                .culled_occluded,
            300
        );
        // Placing it where it already is changes nothing.
        world.set_placement(wall, turned).unwrap();
        assert_eq!(
            prepare_queried(&mut world, &ahead, 1024)
                .stats
                .culled_occluded,
            300
        );
        // Moving the wall away may reveal the cubes: the next frame draws them all.
        world
            .set_placement(wall, Placement::from_translation([0.0, 50.0, 0.0]))
            .unwrap();
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (301, 0));
    }

    #[test]
    fn clusters_newly_in_view_are_drawn_the_same_frame() {
        let (mut world, _) = walled_world();
        let ahead = view_from([0.0; 3]);
        let list = prepare_queried(&mut world, &ahead, 1024);
        answer_behind_wall(&mut world, &list, 5.0);
        assert_eq!(
            prepare_queried(&mut world, &ahead, 1024)
                .stats
                .culled_occluded,
            300
        );
        // Looking the other way, then back: the cubes come back into view, and are drawn at once
        // although their clusters were hidden two frames before.
        let behind = prepare_queried(&mut world, &view_towards([0.0; 3], [0.0, -1.0, 0.0]), 1024);
        assert_eq!(
            (behind.stats.culled_frustum, behind.queries.len()),
            (301, 0)
        );
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (301, 0));
        // New objects are drawn the frame they are added, even inside a hidden cluster.
        answer_behind_wall(&mut world, &list, 5.0);
        assert_eq!(
            prepare_queried(&mut world, &ahead, 1024)
                .stats
                .culled_occluded,
            300
        );
        let mesh = cube(&mut world, [0.0; 3], 12);
        world
            .add_object(mesh, Placement::from_translation([0.0, 31.0, 0.0]), WHITE)
            .unwrap();
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!((list.stats.fine, list.stats.culled_occluded), (302, 0));
    }

    #[test]
    fn without_occlusion_culling_nothing_is_queried_or_skipped() {
        let (mut world, _) = walled_world();
        let ahead = view_from([0.0; 3]);
        let list = prepare_queried(&mut world, &ahead, 1024);
        answer_behind_wall(&mut world, &list, 5.0);
        let off = RenderSettings {
            occlusion_culling: false,
            ..RenderSettings::default()
        };
        world.set_settings(off).unwrap();
        let list = prepare_queried(&mut world, &ahead, 1024);
        assert_eq!(
            (
                list.stats.fine,
                list.stats.culled_occluded,
                list.queries.len()
            ),
            (301, 0, 0)
        );
        // The settings bound the queries of a frame, whatever the renderer allows.
        world
            .set_settings(RenderSettings {
                occlusion_queries: 2,
                ..RenderSettings::default()
            })
            .unwrap();
        assert_eq!(prepare_queried(&mut world, &ahead, 1024).queries.len(), 2);
        // A camera inside the wall's box tests nothing across the near plane.
        let inside = view_from([0.0, 5.0, 0.0]);
        let list = prepare_queried(&mut world, &inside, 1024);
        assert!(list.queries.len() <= 2);
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
        world.prepare(
            &view,
            &camera.projection,
            crate::reference::VIEW,
            0,
            &mut list,
        );
        let stats = list.stats;
        assert_eq!(
            stats.culled_frustum
                + stats.culled_occluded
                + stats.culled_small
                + stats.fine
                + stats.coarse,
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

    /// The clusters of the reference model, and the queries of its first frames with the default
    /// budget. Pinned: a change in them is a change in what occlusion culling tests.
    #[test]
    fn reference_clusters_and_queries() {
        use crate::reference::{aerial, inside};
        let mut world = reference_world(crate::reference::STOREYS);
        let mut clusters = 0;
        world.bvh.clusters(
            &mut Vec::new(),
            CLUSTER_LEAVES,
            |_, _, children| {
                if children.is_some() {
                    return crate::bvh::Step::Descend;
                }
                clusters += 1;
                crate::bvh::Step::Leave
            },
            |_| {},
        );
        let mut queries = |camera: &Camera| {
            let mut list = DrawList::default();
            let view = ViewFrame::new(camera).unwrap();
            world.prepare(
                &view,
                &camera.projection,
                crate::reference::VIEW,
                usize::MAX,
                &mut list,
            );
            list.queries.len()
        };
        // More clusters are in view than the default budget tests in one frame: the queries
        // rotate between them, hidden ones first.
        assert_eq!(
            (clusters, queries(&aerial()), queries(&inside())),
            (5_780, 1_024, 1_024)
        );
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
