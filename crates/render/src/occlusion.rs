//! Occlusion culling, CPU side: which clusters of objects the previous frames found hidden, which
//! of them to skip, and which to test again (VP-02,
//! `docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`).
//!
//! A *cluster* is a compact node of the object hierarchy with at most [`CLUSTER_LEAVES`] objects
//! below it ([`crate::bvh::Bvh::clusters`]). Each frame the GPU tests the bounding boxes of
//! some clusters against the depth of the frame just drawn, with occlusion queries; the results
//! come back asynchronously, a frame or more later, and decide which clusters later frames skip.
//! The rule is conservative: a node is skipped only when a query issued while it was in view,
//! after the last edit of the hierarchy and at most [`MAX_AGE`] frames ago, found it hidden.
//!
//! When both children of a node above the clusters are found hidden, the next frames test the
//! node instead of them, and so on up the tree (the "pull-up" of coherent hierarchical culling,
//! Mattausch et al., 2008): a hidden region costs one query, however many clusters it holds.
//!
//! Nothing here touches the GPU, so every decision is tested without a device: the tests feed
//! [`Occlusion::record`] the results a GPU would give.

use crate::Projection;
use crate::bounds::Aabb;
use crate::bvh::Step;
use crate::transform::{Matrix, ViewFrame, to_gpu};
use crate::vector::{apply, dot, norm, sub};

/// The most objects in a cluster. Larger clusters need fewer queries but are hidden less often,
/// since one visible object makes its whole cluster visible. With the compactness rule of the
/// hierarchy, 128 splits the reference model into 5,780 clusters.
pub(crate) const CLUSTER_LEAVES: u32 = 128;

/// The most frames a query result may be old and still skip a cluster: it bounds how long an
/// object revealed by a moving camera can stay missing when the queries cannot keep up.
pub(crate) const MAX_AGE: u64 = 4;

/// How much a query box is enlarged, relative to its radius plus its distance: far more than the
/// rounding of the `f32` coordinates sent to the GPU, so the box always covers its objects.
const QUERY_MARGIN: f64 = 1e-5;

/// What is known of a node of the hierarchy, whatever subtree it holds today. Frames are
/// numbered from 1, so 0 means never.
#[derive(Clone, Copy, Debug, Default)]
struct NodeState {
    /// The frame of the last query issued for it.
    queried: u64,
    /// The frame of the query whose result is known, and whether it found the cluster hidden.
    tested: u64,
    hidden: bool,
    /// The first and the last frame of its current run of frames in view.
    in_view_since: u64,
    last_in_view: u64,
}

impl NodeState {
    /// Notes the node in view in `frame`; a break since the last frame in view starts a new run.
    fn mark_in_view(&mut self, frame: u64) {
        if self.last_in_view + 1 != frame {
            self.in_view_since = frame;
        }
        self.last_in_view = frame;
    }
}

/// A node that can be tested this frame: a cluster, or a node above them.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    node: u32,
    bounds: Aabb,
    /// Whether the frame skips it: its result needs refreshing first.
    skipped: bool,
    queried: u64,
}

/// The occlusion state of the clusters, kept between frames.
#[derive(Debug, Default)]
pub(crate) struct Occlusion {
    /// By node index of the hierarchy.
    states: Vec<NodeState>,
    /// The number of the current frame.
    frame: u64,
    /// The first frame whose queries are trusted: an edit of the hierarchy or of the settings can
    /// reveal what earlier queries found hidden.
    trusted_from: u64,
    candidates: Vec<Candidate>,
}

impl Occlusion {
    /// Starts a frame over a hierarchy whose node indices are below `nodes`; returns its number.
    pub(crate) fn begin_frame(&mut self, nodes: usize) -> u64 {
        self.frame += 1;
        self.candidates.clear();
        if self.states.len() < nodes {
            self.states.resize(nodes, NodeState::default());
        }
        self.frame
    }

    /// Distrusts every result of the queries issued so far: called when an object is added,
    /// moved or removed, or the settings change.
    pub(crate) fn invalidate(&mut self) {
        self.trusted_from = self.frame + 1;
    }

    /// Notes a node of the walk down to the clusters ([`crate::bvh::Bvh::clusters`]) and tells
    /// what the frame does with it. `children` are its children when it lies above the clusters;
    /// `query` holds when its box, enlarged, lies wholly beyond the near plane, where a query can
    /// test it.
    ///
    /// - Out of view, it is left alone: its objects are outside the view.
    /// - Found hidden by a trusted result, it is skipped and queried again.
    /// - Above the clusters, when trusted results found both its children hidden, and no newer
    ///   one found it visible, it is skipped and queried in their place.
    /// - Otherwise a node above the clusters is descended, and a cluster is drawn and queried.
    pub(crate) fn visit(
        &mut self,
        node: u32,
        bounds: &Aabb,
        children: Option<[u32; 2]>,
        in_view: bool,
        query: bool,
    ) -> Step {
        let frame = self.frame;
        if !in_view {
            return Step::Leave;
        }
        let Some(state) = self.states.get_mut(node as usize) else {
            return Step::Descend;
        };
        state.mark_in_view(frame);
        let state = *state;
        let result = self.trusted_result(&state);
        let grouped = || {
            children.is_some_and(|children| {
                children.iter().all(|&child| {
                    self.states.get(child as usize).is_some_and(|child| {
                        // In view until the last frame, found hidden after the node was last
                        // found visible.
                        child.last_in_view + 1 == frame
                            && self.trusted_result(child) == Some(true)
                            && (result != Some(false) || child.tested > state.tested)
                    })
                })
            })
        };
        let skipped = query && (result == Some(true) || grouped());
        if skipped {
            // Its children stay in view while it hides them, so their results stay trusted.
            for child in children.into_iter().flatten() {
                if let Some(child) = self.states.get_mut(child as usize) {
                    child.mark_in_view(frame);
                }
            }
        } else if children.is_some() {
            return Step::Descend;
        }
        if query {
            self.candidates.push(Candidate {
                node,
                bounds: *bounds,
                skipped,
                queried: state.queried,
            });
        }
        if skipped { Step::Skip } else { Step::Leave }
    }

    /// Whether the last result found the node hidden, when that result can be trusted: issued
    /// while the node was in view, after the last edit, and at most [`MAX_AGE`] frames ago.
    fn trusted_result(&self, state: &NodeState) -> Option<bool> {
        (state.tested >= state.in_view_since.max(self.trusted_from)
            && self.frame - state.tested <= MAX_AGE)
            .then_some(state.hidden)
    }

    /// Chooses at most `budget` of the candidates of the frame, those it skips first (to find
    /// soon what they would reveal), then those queried longest ago. Writes their nodes to
    /// `nodes` and, to `records`, one instance record each (`INSTANCE_FLOATS` values, colour
    /// unused) that maps the unit cube `[-1, 1]³` onto the cluster's box, slightly enlarged.
    pub(crate) fn select(
        &mut self,
        budget: usize,
        view: &ViewFrame,
        nodes: &mut Vec<u32>,
        records: &mut Vec<f32>,
    ) {
        nodes.clear();
        records.clear();
        if self.candidates.len() > budget {
            self.candidates
                .select_nth_unstable_by_key(budget, |c| (!c.skipped, c.queried, c.node));
            self.candidates.truncate(budget);
        }
        for candidate in &self.candidates {
            if let Some(state) = self.states.get_mut(candidate.node as usize) {
                state.queried = self.frame;
            }
            nodes.push(candidate.node);
            records.extend_from_slice(query_model_view(view, &candidate.bounds).as_flattened());
            records.extend_from_slice(&[0.0; 4]);
        }
    }

    /// Records the result of the query issued in `frame` for the cluster at `node`: whether any
    /// sample of its box passed the depth test. A result older than the one known is ignored.
    pub(crate) fn record(&mut self, frame: u64, node: u32, visible: bool) {
        if let Some(state) = self.states.get_mut(node as usize)
            && frame >= state.tested
            && frame <= self.frame
        {
            state.tested = frame;
            state.hidden = !visible;
        }
    }
}

/// Whether the box lies wholly beyond the near plane once enlarged, so that its faces are not
/// clipped there: a query on a box across the near plane could miss what the box holds.
pub(crate) fn beyond_near_plane(view: &ViewFrame, projection: &Projection, bounds: &Aabb) -> bool {
    let near = match *projection {
        Projection::Perspective { near, .. } | Projection::Orthographic { near, .. } => near,
    };
    let (centre, half) = enlarged(view, bounds);
    // The third row of the view rotation points backwards: depth is minus its dot product.
    let backward = view.rotation[2];
    let depth = -dot(backward, centre);
    let reach = dot(backward.map(f64::abs), half);
    depth - reach > near
}

/// The centre relative to the eye and the half-extents of the box, enlarged by [`QUERY_MARGIN`].
fn enlarged(view: &ViewFrame, bounds: &Aabb) -> ([f64; 3], [f64; 3]) {
    let centre = sub(bounds.centre(), view.eye);
    let half = bounds.half_extents();
    let margin = QUERY_MARGIN * (norm(half) + norm(centre));
    (centre, half.map(|h| h + margin))
}

/// The camera-relative model-view that maps the unit cube `[-1, 1]³` onto the enlarged box,
/// computed in `f64` like every model-view (ADR 0020).
fn query_model_view(view: &ViewFrame, bounds: &Aabb) -> Matrix {
    let (centre, half) = enlarged(view, bounds);
    let scaled = view
        .rotation
        .map(|row| core::array::from_fn(|axis| row[axis] * half[axis]));
    to_gpu(&scaled, apply(&view.rotation, centre))
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use geom::{Point3, Vector3};

    use super::*;
    use crate::Camera;

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: FRAC_PI_2,
        near: 0.5,
    };

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

    fn unit_at(centre: [f64; 3]) -> Aabb {
        Aabb {
            min: centre.map(|v| v - 1.0),
            max: centre.map(|v| v + 1.0),
        }
    }

    const BOX: Aabb = Aabb {
        min: [-1.0, 9.0, -1.0],
        max: [1.0, 11.0, 1.0],
    };

    /// One frame with one cluster at node 0, in view and testable; whether it is skipped, and
    /// the queries of the frame.
    fn frame(occlusion: &mut Occlusion, budget: usize) -> (bool, Vec<u32>) {
        occlusion.begin_frame(1);
        let skipped = occlusion.visit(0, &BOX, None, true, true) == Step::Skip;
        let (mut nodes, mut records) = (Vec::new(), Vec::new());
        occlusion.select(budget, &view_from([0.0; 3]), &mut nodes, &mut records);
        assert_eq!(records.len(), nodes.len() * 20);
        (skipped, nodes)
    }

    #[test]
    fn a_cluster_is_skipped_only_once_a_query_found_it_hidden() {
        let mut occlusion = Occlusion::default();
        // Untested: drawn, and queried.
        let (skipped, queries) = frame(&mut occlusion, 8);
        assert!(!skipped);
        assert_eq!(queries, [0]);
        // The result of frame 1 arrives during frame 2: hidden, skipped from frame 3.
        assert!(!frame(&mut occlusion, 8).0);
        occlusion.record(1, 0, false);
        let (skipped, queries) = frame(&mut occlusion, 8);
        assert!(skipped);
        // Still queried, to find out when it shows again.
        assert_eq!(queries.len(), 1);
        // A visible result brings it back.
        occlusion.record(3, 0, true);
        assert!(!frame(&mut occlusion, 8).0);
        // An older result arriving late changes nothing.
        occlusion.record(2, 0, false);
        assert!(!frame(&mut occlusion, 8).0);
        // A result from a frame not yet prepared, or for an unknown node, is ignored.
        occlusion.record(99, 0, false);
        occlusion.record(5, 7, false);
        assert!(!frame(&mut occlusion, 8).0);
    }

    #[test]
    fn results_expire_and_edits_distrust_them() {
        let mut occlusion = Occlusion::default();
        frame(&mut occlusion, 0);
        occlusion.record(1, 0, false);
        // Skipped while the result is at most MAX_AGE frames old, with no new query (budget 0).
        for _ in 2..=1 + MAX_AGE {
            assert!(frame(&mut occlusion, 0).0);
        }
        assert!(!frame(&mut occlusion, 0).0);
        // A fresh result skips again, until an edit.
        let now = occlusion.frame;
        occlusion.record(now, 0, false);
        assert!(frame(&mut occlusion, 0).0);
        occlusion.invalidate();
        assert!(!frame(&mut occlusion, 0).0);
        // Results of queries issued before the edit stay distrusted; the next ones count.
        occlusion.record(now, 0, false);
        assert!(!frame(&mut occlusion, 0).0);
        let now = occlusion.frame;
        occlusion.record(now, 0, false);
        assert!(frame(&mut occlusion, 0).0);
    }

    #[test]
    fn a_cluster_newly_in_view_is_drawn_until_tested_again() {
        let mut occlusion = Occlusion::default();
        frame(&mut occlusion, 8);
        occlusion.record(1, 0, false);
        assert!(frame(&mut occlusion, 8).0);
        // Out of view for a frame: neither skipped nor queried.
        occlusion.begin_frame(1);
        assert_eq!(occlusion.visit(0, &BOX, None, false, true), Step::Leave);
        let (mut queries, mut records) = (Vec::new(), Vec::new());
        occlusion.select(8, &view_from([0.0; 3]), &mut queries, &mut records);
        assert!(queries.is_empty());
        // Back in view: drawn, although its last result is recent, until a new query answers.
        assert!(!frame(&mut occlusion, 8).0);
        let back = occlusion.frame;
        assert!(!frame(&mut occlusion, 8).0);
        occlusion.record(back, 0, false);
        assert!(frame(&mut occlusion, 8).0);
        // Across the near plane: drawn and not queried.
        occlusion.begin_frame(1);
        assert_eq!(occlusion.visit(0, &BOX, None, true, false), Step::Leave);
        occlusion.select(8, &view_from([0.0; 3]), &mut queries, &mut records);
        assert!(queries.is_empty() && records.is_empty());
    }

    #[test]
    fn the_budget_goes_to_skipped_clusters_then_to_the_oldest_queries() {
        let mut occlusion = Occlusion::default();
        let view = view_from([0.0; 3]);
        let (mut queries, mut records) = (Vec::new(), Vec::new());
        let mut round = |occlusion: &mut Occlusion, budget| {
            occlusion.begin_frame(6);
            let skipped: Vec<bool> = (0..6)
                .map(|node| occlusion.visit(node, &BOX, None, true, true) == Step::Skip)
                .collect();
            occlusion.select(budget, &view, &mut queries, &mut records);
            let mut nodes = queries.clone();
            nodes.sort_unstable();
            (skipped, nodes)
        };
        // Six untested clusters, three queries a frame: two frames test them all.
        assert_eq!(round(&mut occlusion, 3).1, [0, 1, 2]);
        assert_eq!(round(&mut occlusion, 3).1, [3, 4, 5]);
        // Nodes 4 and 5 are hidden: they come first, then the oldest queries.
        occlusion.record(2, 4, false);
        occlusion.record(2, 5, false);
        let (skipped, nodes) = round(&mut occlusion, 3);
        assert_eq!(skipped, [false, false, false, false, true, true]);
        assert_eq!(nodes, [0, 4, 5]);
        assert_eq!(round(&mut occlusion, 3).1, [1, 4, 5]);
        // A budget wide enough tests every cluster in view.
        assert_eq!(round(&mut occlusion, 100).1, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn hidden_siblings_are_tested_together_through_their_parent() {
        use Step::{Descend, Leave, Skip};
        let mut occlusion = Occlusion::default();
        let view = view_from([0.0; 3]);
        let (mut queries, mut records) = (Vec::new(), Vec::new());
        // Node 0 holds the clusters 1 and 2. Each frame walks it, then its clusters if asked.
        let mut walk = |occlusion: &mut Occlusion| {
            occlusion.begin_frame(3);
            let mut steps = vec![occlusion.visit(0, &BOX, Some([1, 2]), true, true)];
            if steps[0] == Descend {
                for node in [1, 2] {
                    steps.push(occlusion.visit(node, &BOX, None, true, true));
                }
            }
            occlusion.select(8, &view, &mut queries, &mut records);
            (steps, queries.clone())
        };
        assert_eq!(
            walk(&mut occlusion),
            (vec![Descend, Leave, Leave], vec![1, 2])
        );
        // Both clusters hidden: the parent is skipped and queried in their place, while its
        // own result is on its way.
        occlusion.record(1, 1, false);
        occlusion.record(1, 2, false);
        assert_eq!(walk(&mut occlusion), (vec![Skip], vec![0]));
        assert_eq!(walk(&mut occlusion), (vec![Skip], vec![0]));
        // Then on its own result.
        occlusion.record(2, 0, false);
        assert_eq!(walk(&mut occlusion), (vec![Skip], vec![0]));
        // Found visible: the walk goes down again. The clusters' own results, still trusted,
        // skip them until they expire; then they are drawn and tested again.
        occlusion.record(4, 0, true);
        assert_eq!(
            walk(&mut occlusion),
            (vec![Descend, Skip, Skip], vec![1, 2])
        );
        assert_eq!(
            walk(&mut occlusion),
            (vec![Descend, Leave, Leave], vec![1, 2])
        );
        // Only one hidden: the parent is not grouped.
        occlusion.record(6, 1, false);
        occlusion.record(6, 2, true);
        assert_eq!(
            walk(&mut occlusion),
            (vec![Descend, Skip, Leave], vec![1, 2])
        );
    }

    #[test]
    fn a_hidden_parent_does_not_carry_its_children_across_a_break_in_view() {
        use Step::{Descend, Leave, Skip};
        let mut occlusion = Occlusion::default();
        let view = view_from([0.0; 3]);
        let (mut queries, mut records) = (Vec::new(), Vec::new());
        // Cluster 1 found hidden, then out of view for a frame while its parent 0 is in view
        // and found hidden on its own.
        occlusion.begin_frame(3);
        occlusion.visit(1, &BOX, None, true, true);
        occlusion.select(8, &view, &mut queries, &mut records);
        occlusion.record(1, 1, false);
        occlusion.begin_frame(3);
        assert_eq!(occlusion.visit(0, &BOX, Some([1, 2]), true, true), Descend);
        assert_eq!(occlusion.visit(1, &BOX, None, false, true), Leave);
        occlusion.record(2, 0, false);
        // The parent is skipped: its children count as in view from this frame, not since
        // before the break.
        occlusion.begin_frame(3);
        assert_eq!(occlusion.visit(0, &BOX, Some([1, 2]), true, true), Skip);
        // Found visible: the walk goes down, and cluster 1, back in view after the break, is
        // drawn until a new query answers.
        occlusion.record(3, 0, true);
        occlusion.begin_frame(3);
        assert_eq!(occlusion.visit(0, &BOX, Some([1, 2]), true, true), Descend);
        assert_eq!(occlusion.visit(1, &BOX, None, true, true), Leave);
    }

    #[test]
    fn query_boxes_enclose_their_cluster_relative_to_the_eye() {
        // Far from the origin, the unit cube maps onto the box around the eye, slightly enlarged.
        let eye = [1e7, -1e7, 50.0];
        let view = view_from(eye);
        let bounds = unit_at([1e7 + 3.0, -1e7 + 10.0, 50.0]);
        let matrix = query_model_view(&view, &bounds);
        // A camera along +y: x right, z up, y forward (−z in camera coordinates).
        let corner = |m: &Matrix, v: [f32; 3]| -> [f32; 3] {
            core::array::from_fn(|row| (0..3).map(|k| m[k][row] * v[k]).sum::<f32>() + m[3][row])
        };
        let [x, y, z] = corner(&matrix, [1.0, 1.0, 1.0]);
        // The half-extent of the enlarged box.
        let half = 1.0 + QUERY_MARGIN * (3.0_f64.sqrt() + 109.0_f64.sqrt());
        let [x, y, z] = [x, y, z].map(f64::from);
        assert!((x - (3.0 + half)).abs() < 1e-5, "{x}");
        assert!((y - half).abs() < 1e-5, "{y}");
        assert!((z + 10.0 + half).abs() < 1e-5, "{z}");
        assert!(x > 4.0 && y > 1.0 && z < -11.0);
    }

    #[test]
    fn boxes_across_the_near_plane_cannot_be_queried() {
        let view = view_from([0.0; 3]);
        // The near plane is 0.5 m ahead.
        assert!(beyond_near_plane(
            &view,
            &PERSPECTIVE,
            &unit_at([0.0, 1.6, 0.0])
        ));
        assert!(!beyond_near_plane(
            &view,
            &PERSPECTIVE,
            &unit_at([0.0, 1.5, 0.0])
        ));
        assert!(!beyond_near_plane(&view, &PERSPECTIVE, &unit_at([0.0; 3])));
        assert!(!beyond_near_plane(
            &view,
            &PERSPECTIVE,
            &unit_at([0.0, -5.0, 0.0])
        ));
        let parallel = Projection::Orthographic {
            height: 10.0,
            near: 2.0,
            far: 100.0,
        };
        assert!(beyond_near_plane(
            &view,
            &parallel,
            &unit_at([30.0, 3.1, 0.0])
        ));
        assert!(!beyond_near_plane(
            &view,
            &parallel,
            &unit_at([30.0, 2.9, 0.0])
        ));
    }
}
