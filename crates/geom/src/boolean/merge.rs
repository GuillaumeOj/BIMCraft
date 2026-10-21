//! The merge pass of ADR 0001 after the faces are paired: local operations on the faces of the
//! result, each applied only if the faces it leaves meet the solid's own checks.
//!
//! - Vertices joined by edges no longer than the tolerance collapse onto one of them (step 2),
//!   and faces left back to back cancel (step 1).
//! - Adjacent coplanar faces merge (step 3).
//! - Vertices left between two collinear edges go.
//! - A face narrower than the tolerance is zipped shut: its vertices lie within the tolerance of
//!   its longest edge, so they join the face across that edge instead; one that does not zip
//!   whole may zip in two parts (step 4).
//! - Lumps and voids no thicker than the tolerance on average vanish (steps 1 and 4).
//!
//! Only a collapse moves a vertex's place in a face, by no more than the tolerance, and no
//! operation is applied if a face it touches would then break a rule of the solid's checks it
//! did not break before (bent beyond the tolerance, no wider than it), or if the faces along an
//! edge it touches would no longer pair. So the pass never makes a valid result invalid. An
//! edge no longer than the tolerance or a face narrower than it that no operation removes is
//! reported as [`BooleanError::FeaturesTooClose`].

use std::collections::{BTreeMap, BTreeSet};

use super::BooleanError;
use super::assemble::{Faces, HalfEdges, around_edge};
use super::face::{Pool, Projection, area_and_centre, clean_face, plane};
use super::planar::{canonical, faces_of};
use super::snap::{find, union};
use crate::MERGE_CHAIN_WIDTH;
use crate::brep::{FaceFault, enclosed_volume, face_plane, too_thin};
use crate::tolerance::{WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED};
use crate::vector::{
    Vector, bounds, distance_squared, dot, edges, length, lexicographic, segment_distance_squared,
    sub,
};

/// The plane of a face as the solid's checks find it, or the rule of its own it breaks.
pub(super) fn checked(face: &[Vec<usize>], pool: &Pool) -> Result<(Vector, Vector), FaceFault> {
    let mut points = Vec::new();
    let mut ends = Vec::with_capacity(face.len());
    for ring in face {
        points.extend(ring.iter().map(|&v| pool.get(v)));
        ends.push(points.len());
    }
    face_plane(&points, &ends)
}

/// Whether a ring has an edge within the tolerance, which the solid's checks reject.
fn has_short_edge(ring: &[usize], pool: &Pool) -> bool {
    edges(ring).any(|(u, v)| distance_squared(pool.get(u), pool.get(v)) <= WITHIN_TOLERANCE_SQUARED)
}

/// Whether a face meets every rule of the solid's checks that depends on it alone: rings of
/// three vertices or more, no edge within the tolerance, and [`checked`].
fn holds(face: &[Vec<usize>], pool: &Pool) -> bool {
    face.iter()
        .all(|ring| ring.len() >= 3 && !has_short_edge(ring, pool))
        && checked(face, pool).is_ok()
}

/// Collapses the edges no longer than the tolerance (ADR 0001, merge pass step 2). The vertices
/// joined by such edges collapse together onto one of them, which takes their place in every
/// face around them; this moves those faces' boundaries by no more than the group is wide. The
/// vertex kept is an operand vertex if the group has one, then the smallest by x, then y, then
/// z, so the choice depends only on positions. A collapse is applied only if every face it
/// changes is left without a fault of its own the solid's checks find (bent, degenerate) or
/// vanishes, and the faces along every edge it touches still pair ([`still_paired`]). A group
/// that cannot collapse whole is collapsed edge by edge where that holds, either end staying.
/// Groups and edges are taken in order of position, and tried again until none collapses. A group
/// wider than [`MERGE_CHAIN_WIDTH`] along an axis is [`BooleanError::MergeChainTooWide`]. An edge
/// no collapse removes is left to the other operations, or to [`sliver_remains`].
pub(super) fn collapse_short_edges(faces: &mut Faces, pool: &Pool) -> Result<(), BooleanError> {
    if !faces
        .iter()
        .flatten()
        .any(|ring| has_short_edge(ring, pool))
    {
        return Ok(());
    }
    let mut state = Collapsing::new(faces);
    loop {
        let short = state.short_edges(pool);
        let mut any = false;
        for group in groups(&short) {
            let keep = (group.iter().copied())
                .reduce(|best, v| if kept_before(pool, v, best) { v } else { best })
                .unwrap_or(group[0]);
            let gone: Vec<usize> = group.iter().copied().filter(|&v| v != keep).collect();
            let widened = state.widen(keep, &gone, pool)?;
            if let Some(changed) = collapsed(faces, &state, pool, keep, &gone) {
                state.apply(faces, (keep, &gone), changed, widened);
                any = true;
            }
        }
        if any {
            continue;
        }
        for (u, v) in short {
            if !state.joined(u, v) {
                continue;
            }
            let preferred = if kept_before(pool, u, v) {
                (u, v)
            } else {
                (v, u)
            };
            let widened = state.widen(u, &[v], pool)?;
            // The other end stays if the preferred one cannot.
            let found = [preferred, (preferred.1, preferred.0)]
                .into_iter()
                .find_map(|(keep, gone)| {
                    Some((keep, gone, collapsed(faces, &state, pool, keep, &[gone])?))
                });
            if let Some((keep, gone, changed)) = found {
                state.apply(faces, (keep, &[gone]), changed, widened);
                any = true;
            }
        }
        if !any {
            return Ok(());
        }
    }
}

/// The groups of vertices the edges join, each in the order its vertices are first met, the
/// groups in the order of their first edge.
fn groups(edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let count = edges.iter().map(|&(u, v)| u.max(v) + 1).max().unwrap_or(0);
    let mut parent: Vec<usize> = (0..count).collect();
    for &(u, v) in edges {
        union(&mut parent, u, v);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_of: BTreeMap<usize, usize> = BTreeMap::new();
    for &(u, v) in edges {
        let g = *group_of.entry(find(&mut parent, u)).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        for x in [u, v] {
            if !groups[g].contains(&x) {
                groups[g].push(x);
            }
        }
    }
    groups
}

/// What [`collapse_short_edges`] keeps track of: the faces around each vertex, the faces along
/// each directed edge, and the box of the vertices each kept vertex stands for.
struct Collapsing {
    owners: BTreeMap<usize, BTreeSet<usize>>,
    along: Along,
    boxes: BTreeMap<usize, (Vector, Vector)>,
}

impl Collapsing {
    fn new(faces: &Faces) -> Self {
        let mut state = Self {
            owners: BTreeMap::new(),
            along: BTreeMap::new(),
            boxes: BTreeMap::new(),
        };
        for (f, face) in faces.iter().enumerate() {
            for &v in face.iter().flatten() {
                state.owners.entry(v).or_default().insert(f);
            }
            own(&mut state.along, face, f);
        }
        state
    }

    /// The edges no longer than the tolerance, each with its smaller vertex first, in order of
    /// position: by their ends, the smaller by x, then y, then z first.
    fn short_edges(&self, pool: &Pool) -> Vec<(usize, usize)> {
        let mut short: Vec<(usize, usize)> = (self.along.keys())
            .map(|&(u, v)| (u.min(v), u.max(v)))
            .filter(|&(u, v)| {
                distance_squared(pool.get(u), pool.get(v)) <= WITHIN_TOLERANCE_SQUARED
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        sort_by_ends(&mut short, |v| pool.get(v));
        short
    }

    /// Whether an edge joins the two vertices.
    fn joined(&self, u: usize, v: usize) -> bool {
        self.along.contains_key(&(u, v)) || self.along.contains_key(&(v, u))
    }

    fn box_of(&self, v: usize, pool: &Pool) -> (Vector, Vector) {
        (self.boxes.get(&v).copied()).unwrap_or((pool.get(v), pool.get(v)))
    }

    /// The box of the vertices `keep` and `gone` stand for, taken together; an error if it is
    /// wider than [`MERGE_CHAIN_WIDTH`] along an axis.
    fn widen(
        &self,
        keep: usize,
        gone: &[usize],
        pool: &Pool,
    ) -> Result<(Vector, Vector), BooleanError> {
        let corners = std::iter::once(keep)
            .chain(gone.iter().copied())
            .flat_map(|v| <[Vector; 2]>::from(self.box_of(v, pool)));
        let (lo, hi) = bounds(corners).unwrap_or_default();
        if (0..3).any(|i| hi[i] - lo[i] > MERGE_CHAIN_WIDTH) {
            return Err(BooleanError::MergeChainTooWide);
        }
        Ok((lo, hi))
    }

    /// Puts `keep` in the place of the vertices `gone`, the faces around them `changed`; `keep`
    /// now stands for the vertices in `widened` ([`Self::widen`]).
    fn apply(
        &mut self,
        faces: &mut Faces,
        (keep, gone): (usize, &[usize]),
        changed: Vec<(usize, Vec<Vec<usize>>)>,
        widened: (Vector, Vector),
    ) {
        self.boxes.insert(keep, widened);
        for (f, face) in changed {
            disown(&mut self.along, &faces[f], f);
            own(&mut self.along, &face, f);
            for &x in faces[f].iter().flatten() {
                if let Some(of) = self.owners.get_mut(&x) {
                    of.remove(&f);
                }
            }
            for &x in face.iter().flatten() {
                self.owners.entry(x).or_default().insert(f);
            }
            faces[f] = face;
        }
        self.along.retain(|_, of| !of.is_empty());
        for v in gone {
            self.owners.remove(v);
            self.boxes.remove(v);
        }
    }
}

/// The faces around the vertices `gone` once `keep` takes their place, if the collapse is
/// allowed: each is left without a fault of its own the solid's checks find, or vanishes, and
/// the edges still pair ([`still_paired`]).
fn collapsed(
    faces: &Faces,
    state: &Collapsing,
    pool: &Pool,
    keep: usize,
    gone: &[usize],
) -> Option<Vec<(usize, Vec<Vec<usize>>)>> {
    let around: BTreeSet<usize> = (gone.iter())
        .filter_map(|v| state.owners.get(v))
        .flatten()
        .copied()
        .collect();
    let changed: Vec<(usize, Vec<Vec<usize>>)> = around
        .into_iter()
        .map(|f| {
            let mut face: Vec<Vec<usize>> = (faces[f].iter())
                .map(|ring| {
                    (ring.iter())
                        .map(|&x| if gone.contains(&x) { keep } else { x })
                        .collect()
                })
                .collect();
            clean_face(&mut face);
            (f, face)
        })
        .collect();
    let no_fault = changed.iter().all(|(f, face)| {
        face.is_empty() || {
            let after = checked(face, pool);
            after.is_ok() || after == checked(&faces[*f], pool)
        }
    });
    (no_fault && still_paired(&state.along, &changed, faces, pool)).then_some(changed)
}

/// The faces running along each directed edge.
type Along = BTreeMap<(usize, usize), Vec<usize>>;

/// Whether vertex `u` stays when an edge from it to `v` collapses: an operand vertex rather than
/// a constructed one, then the smaller by x, then y, then z, then by number.
fn kept_before(pool: &Pool, u: usize, v: usize) -> bool {
    (pool.is_input(v).cmp(&pool.is_input(u)))
        .then(lexicographic(pool.get(u), pool.get(v)))
        .then(u.cmp(&v))
        .is_lt()
}

/// Whether every edge the faces of `changed` have, or have once changed, is still met as often
/// one way as the other, and the faces along it still pair as the solid's checks pair them:
/// around the edge, each side running one way next to one running the other way.
fn still_paired(
    along: &Along,
    changed: &[(usize, Vec<Vec<usize>>)],
    faces: &Faces,
    pool: &Pool,
) -> bool {
    let new: BTreeMap<usize, &Vec<Vec<usize>>> =
        changed.iter().map(|(f, face)| (*f, face)).collect();
    let touched: BTreeSet<(usize, usize)> = (changed.iter())
        .flat_map(|(f, face)| faces[*f].iter().chain(face.iter()))
        .flat_map(|ring| edges(ring))
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    // Only the edges touched can change.
    let mut after: Along = (touched.iter())
        .flat_map(|&(a, b)| [(a, b), (b, a)])
        .filter_map(|edge| Some((edge, along.get(&edge)?.clone())))
        .collect();
    for (f, face) in changed {
        disown(&mut after, &faces[*f], *f);
        own(&mut after, face, *f);
    }
    let of = |edge| after.get(&edge).map_or(&[][..], Vec::as_slice);
    let mut normals: BTreeMap<usize, Vector> = BTreeMap::new();
    let mut normal = |g: usize| {
        *(normals.entry(g))
            .or_insert_with(|| plane(new.get(&g).copied().unwrap_or(&faces[g]), pool).0)
    };
    touched.into_iter().all(|(a, b)| {
        let (forward, backward) = (of((a, b)), of((b, a)));
        if forward.len() != backward.len() {
            return false;
        }
        if forward.len() < 2 {
            return true;
        }
        let sides: Vec<(bool, Vector)> = (forward.iter().map(|&g| (true, g)))
            .chain(backward.iter().map(|&g| (false, g)))
            .map(|(ahead, g)| (ahead, normal(g)))
            .collect();
        let order = around_edge(pool.get(a), pool.get(b), sides.iter().copied());
        (0..order.len()).all(|i| {
            let before = order[(i + order.len() - 1) % order.len()];
            !(sides[order[i]].0 && sides[before].0)
        })
    })
}

/// Removes the faces that coincide with another one turned the other way, vertex for vertex:
/// two faces back to back bound nothing (ADR 0001, merge pass step 1). Each edge of one is met
/// the other way by the other, so the edges around stay paired.
pub(super) fn cancel_back_to_back(faces: &mut Faces) {
    let mut by_form: BTreeMap<Vec<Vec<usize>>, Vec<usize>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        by_form.entry(canonical(face, false)).or_default().push(f);
    }
    let mut gone = BTreeSet::new();
    for (f, face) in faces.iter().enumerate() {
        if face.is_empty() || gone.contains(&f) {
            continue;
        }
        let back = (by_form.get(&canonical(face, true)).into_iter().flatten())
            .copied()
            .find(|&g| g != f && !gone.contains(&g));
        if let Some(g) = back {
            gone.extend([f, g]);
        }
    }
    *faces = std::mem::take(faces)
        .into_iter()
        .enumerate()
        .filter(|(f, _)| !gone.contains(f))
        .map(|(_, face)| face)
        .collect();
}

/// The lumps of the faces, as the root of each face's: faces sharing a vertex are in one.
fn lumps(faces: &Faces) -> Vec<usize> {
    let mut lump: Vec<usize> = (0..faces.len()).collect();
    let mut owner: BTreeMap<usize, usize> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for &v in face.iter().flatten() {
            let g = *owner.entry(v).or_insert(f);
            union(&mut lump, f, g);
        }
    }
    for f in 0..faces.len() {
        lump[f] = find(&mut lump, f);
    }
    lump
}

/// How thick each lump is on average, as the solid's checks read it (`enclosed_volume`): six
/// times its volume, as cones from the centre of its first face, and twice its area. Merging
/// faces or dropping a vertex moves the surface by up to the tolerance, so it can leave a lump
/// just thicker than the tolerance on average thinner than that, which the checks reject; such a
/// change is refused.
struct Thickness {
    lump: Vec<usize>,
    /// By lump: its apex, and six times its volume and twice its area.
    total: BTreeMap<usize, (Vector, (f64, f64))>,
}

impl Thickness {
    fn new(faces: &Faces, pool: &Pool) -> Self {
        let lump = lumps(faces);
        let mut total: BTreeMap<usize, (Vector, (f64, f64))> = BTreeMap::new();
        for (face, root) in faces.iter().zip(&lump) {
            let (apex, sum) = total
                .entry(*root)
                .or_insert_with(|| (area_and_centre(face, pool).1, (0.0, 0.0)));
            let (volume, area) = measure([face], *apex, pool);
            *sum = (sum.0 + volume, sum.1 + area);
        }
        Self { lump, total }
    }

    /// Whether the lump of face `f` may have the faces `gone` replaced by `made`: it must not
    /// become too thin if it was not; if so, its measure is updated.
    fn allows<'a>(
        &mut self,
        f: usize,
        gone: impl IntoIterator<Item = &'a Vec<Vec<usize>>>,
        made: impl IntoIterator<Item = &'a Vec<Vec<usize>>>,
        pool: &Pool,
    ) -> bool {
        let (apex, before) = self.total[&self.lump[f]];
        let lost = measure(gone, apex, pool);
        let new = measure(made, apex, pool);
        let after = (before.0 - lost.0 + new.0, before.1 - lost.1 + new.1);
        if too_thin(after.0, after.1) && !too_thin(before.0, before.1) {
            return false;
        }
        self.total.insert(self.lump[f], (apex, after));
        true
    }
}

/// Six times the volume and twice the area of the faces, as cones from `apex`.
fn measure<'a>(
    faces: impl IntoIterator<Item = &'a Vec<Vec<usize>>>,
    apex: Vector,
    pool: &Pool,
) -> (f64, f64) {
    faces.into_iter().fold((0.0, 0.0), |(volume, area), face| {
        let (vector, centre) = area_and_centre(face, pool);
        (
            volume + dot(vector, sub(centre, apex)),
            area + length(vector),
        )
    })
}

/// Merges adjacent faces that lie in the same plane within the tolerance and face the same way,
/// where the merged face passes the solid's checks and its lump stays thick enough
/// ([`Thickness`]).
pub(super) fn merge_coplanar(
    faces: &Faces,
    planes: &[(Vector, Vector)],
    twins: &[usize],
    pool: &Pool,
) -> Faces {
    let half_edges = HalfEdges::new(faces);
    // Whether two groups of faces, taken as one, lie within the tolerance of one face's plane.
    // The merged face must also pass the solid's checks, but that is not enough on its own: a
    // face a few tolerances wide, square to its neighbour, would pass, its width split on
    // either side of the plane fitted to both. Requiring every face's plane would reject a small
    // face tilted by a rounding error next to a large one. Groups are tested whole, not face by
    // face: a face a few tolerances wide may lie within the tolerance of two neighbours that are
    // not coplanar, and would otherwise join them into one bent face.
    let flat_together = |groups: [&[usize]; 2]| {
        groups.into_iter().flatten().any(|&f| {
            let (normal, centre) = planes[f];
            (groups.into_iter().flatten())
                .flat_map(|&g| faces[g].iter().flatten())
                .all(|&v| dot(normal, sub(pool.get(v), centre)).abs() <= WITHIN_TOLERANCE)
        })
    };
    let at = |f: usize| {
        let projection = Projection::along(planes[f].0);
        move |v: usize| projection.apply(pool.get(v))
    };
    let edge = |h: usize| (half_edges.origin[h], half_edges.target(h));
    let mut group: Vec<usize> = (0..faces.len()).collect();
    let mut members: Vec<Vec<usize>> = (0..faces.len()).map(|f| vec![f]).collect();
    // The half-edges between each group and the rest, and the faces a merged group makes.
    let mut boundary: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); faces.len()];
    for h in 0..twins.len() {
        boundary[half_edges.face(h)].insert(h);
    }
    let mut merged: Vec<Option<Faces>> = vec![None; faces.len()];
    let mut thickness = Thickness::new(faces, pool);
    // Groups refused once are refused again along their other shared edges, until one of them
    // grows: keyed by both roots and their sizes.
    let mut refused = BTreeSet::new();
    for (h, &twin) in twins.iter().enumerate() {
        let (f, g) = (half_edges.face(h), half_edges.face(twin));
        if f >= g {
            continue;
        }
        let (rf, rg) = (find(&mut group, f), find(&mut group, g));
        // Faces sharing several edges are tested once: later edges find them merged.
        let key = (rf, rg, members[rf].len(), members[rg].len());
        if rf == rg || dot(planes[f].0, planes[g].0) <= 0.0 || refused.contains(&key) {
            continue;
        }
        if !flat_together([&members[rf], &members[rg]]) {
            refused.insert(key);
            continue;
        }
        let around: BTreeSet<usize> = boundary[rf]
            .iter()
            .chain(&boundary[rg])
            .copied()
            .filter(|&h| ![rf, rg].contains(&find(&mut group, half_edges.face(twins[h]))))
            .collect();
        let outline: BTreeSet<(usize, usize)> = around.iter().map(|&h| edge(h)).collect();
        // Where the solid touches itself along an edge in the middle of the merged face, its
        // pieces from both sides leave a ring running along the edge and back, enclosing
        // nothing: it goes, and the edge is left to the faces that meet there.
        let mut made = faces_of(&outline, at(f));
        made.iter_mut().for_each(clean_face);
        made.retain(|face| !face.is_empty());
        if made.is_empty() || made.iter().any(|face| checked(face, pool).is_err()) {
            refused.insert(key);
            continue;
        }
        let gone = [rf, rg].map(|r| {
            merged[r]
                .as_deref()
                .unwrap_or(std::slice::from_ref(&faces[r]))
        });
        if !thickness.allows(f, gone.into_iter().flatten(), &made, pool) {
            refused.insert(key);
            continue;
        }
        let (root, other) = (rf.min(rg), rf.max(rg));
        group[other] = root;
        let moved = std::mem::take(&mut members[other]);
        members[root].extend(moved);
        boundary[root] = around;
        // Only roots are read from here on; the absorbed group's state is let go.
        boundary[other] = BTreeSet::new();
        merged[root] = Some(made);
    }
    let mut result = Vec::new();
    for (f, face) in faces.iter().enumerate() {
        if find(&mut group, f) != f {
            continue;
        }
        match merged[f].take() {
            Some(made) => result.extend(made),
            None => result.push(face.clone()),
        }
    }
    result
}

/// Removes the vertices with only two neighbours that lie, within the tolerance, on the segment
/// between them. Such a vertex sits on the edge of two faces only, so dropping it moves the edge
/// by no more than the tolerance; one between two faces left unmerged at a fold stays, or the
/// faces would bend. The test depends only on the vertex and its neighbours, so both faces
/// along the edge agree. Vertices are taken in order of position, so the operands' order does
/// not matter, and one goes only if both its faces still pass the solid's checks without it and
/// its lump stays thick enough ([`Thickness`]).
pub(super) fn remove_collinear(mut faces: Faces, pool: &Pool) -> Faces {
    let mut thickness = Thickness::new(&faces, pool);
    let mut neighbours: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut owners: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for ring in face {
            for (u, v) in edges(ring) {
                neighbours.entry(u).or_default().insert(v);
                neighbours.entry(v).or_default().insert(u);
                owners.entry(u).or_default().insert(f);
            }
        }
    }
    let mut order: Vec<usize> = neighbours.keys().copied().collect();
    order.sort_by(|&v, &w| lexicographic(pool.get(v), pool.get(w)).then(v.cmp(&w)));
    for v in order {
        let mut around = neighbours[&v].iter().copied();
        let (Some(u), Some(w), None) = (around.next(), around.next(), around.next()) else {
            continue;
        };
        // Where u and w are already joined, the edge left would bound four faces.
        if neighbours[&u].contains(&w)
            || segment_distance_squared(pool.get(v), pool.get(u), pool.get(w))
                > WITHIN_TOLERANCE_SQUARED
        {
            continue;
        }
        let without: Vec<(usize, Vec<Vec<usize>>)> = owners[&v]
            .iter()
            .map(|&f| {
                let face = faces[f]
                    .iter()
                    .map(|ring| ring.iter().copied().filter(|&x| x != v).collect())
                    .collect();
                (f, face)
            })
            .collect();
        if !without.iter().all(|(_, face)| holds(face, pool))
            || !thickness.allows(
                without[0].0,
                without.iter().map(|&(f, _)| &faces[f]),
                without.iter().map(|(_, face)| face),
                pool,
            )
        {
            continue;
        }
        for (f, face) in without {
            faces[f] = face;
        }
        for (from, to) in [(u, w), (w, u)] {
            let set = neighbours.entry(from).or_default();
            set.remove(&v);
            set.insert(to);
        }
        neighbours.remove(&v);
    }
    faces
}

/// Zips shut the faces narrower than the tolerance, as the solid's checks find them, where they
/// can be, and tells whether it zipped any. A face whose other vertices lie within the
/// tolerance of its longest edge, in order along it, is removed, and the face across that edge
/// runs through those vertices instead. No vertex moves, and the surface moves by no more than
/// the tolerance. Every edge the faces had or have must still be met once each way, or the
/// sliver stays. Slivers are taken in order of position, so the operands' order does not
/// matter.
pub(super) fn remove_slivers(faces: &mut Faces, pool: &Pool) -> bool {
    let mut owners: Along = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        own(&mut owners, face, f);
    }
    let sliver = |face: &[Vec<usize>]| checked(face, pool) == Err(FaceFault::Degenerate);
    let mut by_position: Vec<(Vec<Vector>, usize)> = (0..faces.len())
        .filter(|&f| faces[f].len() == 1 && sliver(&faces[f]))
        .map(|f| {
            let mut points: Vec<Vector> = faces[f][0].iter().map(|&v| pool.get(v)).collect();
            points.sort_by(|&p, &q| lexicographic(p, q));
            (points, f)
        })
        .collect();
    by_position.sort_by(|(one, f), (other, g)| {
        (one.iter().zip(other))
            .map(|(&p, &q)| lexicographic(p, q))
            .find(|order| order.is_ne())
            .unwrap_or_else(|| one.len().cmp(&other.len()))
            .then(f.cmp(g))
    });
    // A zip never makes a sliver (the face it changes must hold), so the list is made once; a
    // sliver that took another in is no longer one, and is skipped. Zipping one can let an
    // earlier one zip, so the list is walked again until none does.
    let mut slivers: Vec<usize> = by_position.into_iter().map(|(_, f)| f).collect();
    let mut zipped_any = false;
    loop {
        while let Some((k, f, (g, face))) = slivers.iter().enumerate().find_map(|(k, &f)| {
            let zipped = sliver(&faces[f]).then(|| zip(f, faces, &owners, pool))??;
            Some((k, f, zipped))
        }) {
            slivers.remove(k);
            apply_zip(faces, &mut owners, f, (g, face));
            zipped_any = true;
        }
        // A sliver no zip takes whole may zip in two parts, split along a diagonal.
        let Some((k, split)) = slivers.iter().enumerate().find_map(|(k, &f)| {
            Some((
                k,
                sliver(&faces[f]).then(|| zip_in_two(f, faces, &owners, pool))??,
            ))
        }) else {
            return zipped_any;
        };
        slivers.remove(k);
        (*faces, owners) = split;
        zipped_any = true;
    }
}

/// The faces once face `sliver` is split along a diagonal into two parts, each then zipped like a
/// whole sliver ([`zip`]), with their owners; `None` if no diagonal lets both parts zip. The
/// diagonals are tried in order of their ends' positions, so the operands' order does not
/// matter. Like a zipped sliver, both parts are left empty.
fn zip_in_two(sliver: usize, faces: &Faces, owners: &Along, pool: &Pool) -> Option<(Faces, Along)> {
    let ring = &faces[sliver][0];
    let count = ring.len();
    let mut diagonals: Vec<(usize, usize)> = (0..count)
        .flat_map(|from| (from + 2..count).map(move |to| (from, to)))
        .filter(|&(from, to)| !(from == 0 && to == count - 1))
        .collect();
    sort_by_ends(&mut diagonals, |k| pool.get(ring[k]));
    diagonals.into_iter().find_map(|(from, to)| {
        let mut trial = faces.clone();
        let mut owned = owners.clone();
        disown(&mut owned, &trial[sliver], sliver);
        let half = ring[from..=to].to_vec();
        let rest = ring[to..].iter().chain(&ring[..=from]).copied().collect();
        let other = trial.len();
        trial[sliver] = vec![half];
        trial.push(vec![rest]);
        own(&mut owned, &trial[sliver], sliver);
        own(&mut owned, &trial[other], other);
        for part in [sliver, other] {
            let zipped = zip(part, &trial, &owned, pool)?;
            apply_zip(&mut trial, &mut owned, part, zipped);
        }
        Some((trial, owned))
    })
}

/// Removes face `sliver` and gives the face across it what it becomes ([`zip`]), keeping the
/// owners of the edges up to date.
fn apply_zip(
    faces: &mut Faces,
    owners: &mut Along,
    sliver: usize,
    (across, face): (usize, Vec<Vec<usize>>),
) {
    disown(owners, &std::mem::take(&mut faces[sliver]), sliver);
    disown(owners, &std::mem::replace(&mut faces[across], face), across);
    own(owners, &faces[across], across);
}

/// Sorts pairs of items by the positions of their two ends, the smaller end by x, then y, then
/// z first, then by the items themselves: an order that depends only on positions, so the
/// operands' order does not matter.
fn sort_by_ends(pairs: &mut [(usize, usize)], at: impl Fn(usize) -> Vector) {
    let ends = |(one, other): (usize, usize)| {
        let (p, q) = (at(one), at(other));
        if lexicographic(p, q).is_le() {
            (p, q)
        } else {
            (q, p)
        }
    };
    pairs.sort_by(|&one, &other| {
        let ((first, last), (first_other, last_other)) = (ends(one), ends(other));
        (lexicographic(first, first_other).then(lexicographic(last, last_other)))
            .then(one.cmp(&other))
    });
}

/// Records face `f` as an owner of each of its directed edges.
fn own(owners: &mut Along, face: &[Vec<usize>], f: usize) {
    for edge in face.iter().flat_map(|ring| edges(ring)) {
        owners.entry(edge).or_default().push(f);
    }
}

/// Forgets face `f` as an owner of each of the directed edges it had.
fn disown(owners: &mut Along, face: &[Vec<usize>], f: usize) {
    for edge in face.iter().flat_map(|ring| edges(ring)) {
        if let Some(of) = owners.get_mut(&edge) {
            of.retain(|&g| g != f);
        }
    }
}

/// The face across the longest edge of `sliver` and what it becomes once the sliver is zipped
/// onto it; `None` if the sliver cannot be zipped.
fn zip(
    sliver: usize,
    faces: &Faces,
    owners: &Along,
    pool: &Pool,
) -> Option<(usize, Vec<Vec<usize>>)> {
    let ring = &faces[sliver][0];
    let count = ring.len();
    let length = |i: usize| distance_squared(pool.get(ring[i]), pool.get(ring[(i + 1) % count]));
    let longest = (0..count).fold(0, |best, i| if length(i) > length(best) { i } else { best });
    let (a, b) = (ring[longest], ring[(longest + 1) % count]);
    // From b round to a, the other way along the edge.
    let chain: Vec<usize> = (2..count).map(|k| ring[(longest + k) % count]).collect();
    let (start, along) = (pool.get(a), sub(pool.get(b), pool.get(a)));
    let fraction = |v: usize| dot(sub(pool.get(v), start), along) / dot(along, along);
    let in_order = std::iter::once(1.0)
        .chain(chain.iter().map(|&v| fraction(v)))
        .chain(std::iter::once(0.0))
        .collect::<Vec<f64>>()
        .windows(2)
        .all(|pair| pair[0] > pair[1]);
    let near = chain.iter().all(|&v| {
        segment_distance_squared(pool.get(v), start, pool.get(b)) <= WITHIN_TOLERANCE_SQUARED
    });
    if !in_order || !near {
        return None;
    }
    let across = (owners.get(&(b, a))?.first().copied()).filter(|&g| g != sliver)?;
    let mut face = faces[across].clone();
    let (r, at) = face.iter().enumerate().find_map(|(r, ring)| {
        let at = (1..=ring.len()).find(|&j| ring[j - 1] == b && ring[j % ring.len()] == a)?;
        Some((r, at))
    })?;
    face[r].splice(at..at, chain);
    // Where the face across also runs along the sliver, back to back with it, the two cancel.
    clean_face(&mut face);
    let mut seen = BTreeSet::new();
    let once = face.iter().flatten().all(|&v| seen.insert(v));
    let removed: BTreeSet<(usize, usize)> = (faces[sliver].iter().chain(&faces[across]))
        .flat_map(|ring| edges(ring))
        .collect();
    let added: BTreeSet<(usize, usize)> = face.iter().flat_map(|ring| edges(ring)).collect();
    // Every edge the faces had or have is then still met once each way, by the faces around.
    let sides = |edge: (usize, usize)| {
        let kept = owners.get(&edge).map_or(0, |of| {
            of.iter().filter(|&&g| g != sliver && g != across).count()
        });
        kept + usize::from(added.contains(&edge))
    };
    let paired = removed.union(&added).all(|&(u, v)| {
        let (forward, backward) = (sides((u, v)), sides((v, u)));
        forward <= 1 && forward == backward
    });
    (once && paired && holds(&face, pool)).then_some((across, face))
}

/// Whether a face no operation could remove is narrower than the tolerance, or has an edge no
/// longer than it: features of the operands within the tolerance of each other that the merge
/// pass cannot resolve without bending a face or breaking the solid apart.
pub(super) fn sliver_remains(faces: &Faces, pool: &Pool) -> Result<(), BooleanError> {
    if faces.iter().any(|face| {
        face.iter().any(|ring| has_short_edge(ring, pool))
            || checked(face, pool) == Err(FaceFault::Degenerate)
    }) {
        return Err(BooleanError::FeaturesTooClose);
    }
    Ok(())
}

/// Removes the lumps, and the voids, no thicker than the tolerance on average: the solid's checks
/// would reject them, and the result is regularised, so they vanish (ADR 0001, merge pass
/// steps 1 and 4). Lumps share no vertex by now, so faces sharing one belong to the same lump,
/// and removing one leaves the others as they were.
pub(super) fn drop_thin_lumps(faces: Faces, pool: &Pool) -> Faces {
    let lump = lumps(&faces);
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (f, &root) in lump.iter().enumerate() {
        members.entry(root).or_default().push(f);
    }
    let thin: BTreeSet<usize> = members
        .values()
        .filter(|members| {
            let planes: Vec<(Vector, Vector)> = members
                .iter()
                .map(|&f| area_and_centre(&faces[f], pool))
                .collect();
            enclosed_volume(&planes).1
        })
        .flatten()
        .copied()
        .collect();
    faces
        .into_iter()
        .enumerate()
        .filter(|(f, _)| !thin.contains(f))
        .map(|(_, face)| face)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::assemble::assemble;
    use super::*;

    /// The corners of a tetrahedron with three faces on the coordinate planes, then the points
    /// of slivers along its edges: `extra` lie within a micrometre of an edge, beyond the
    /// tetrahedron's faces.
    fn tetrahedron(extra: &[Vector]) -> Pool {
        let mut points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        points.extend_from_slice(extra);
        Pool::of(points)
    }

    #[test]
    fn slivers_are_zipped_onto_the_faces_across_their_longest_edges() {
        // Vertex 4 sits 0.5 µm off the edge from 0 to 1, vertex 5 off the edge from 0 to 2;
        // each sliver bends back over both faces along its edge, so neither merges with them.
        let mut pool = tetrahedron(&[[0.5, -0.4e-6, 0.3e-6], [0.4e-6, 0.5, -0.3e-6]]);
        let faces = vec![
            vec![vec![0, 2, 1, 4]],
            vec![vec![0, 1, 3]],
            vec![vec![0, 3, 2, 5]],
            vec![vec![1, 2, 3]],
            vec![vec![0, 4, 1]],
            vec![vec![0, 5, 2]],
        ];
        // Zipped shut, the slivers leave their vertices between collinear edges, which go.
        let solid = assemble(faces, &mut pool).unwrap().unwrap();
        assert_eq!((solid.vertices().len(), solid.face_count()), (4, 4));
        assert!((solid.volume() - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_sliver_zigzagging_along_its_edge_zips_in_two() {
        // A sliver along the edge from 0 to 1 whose other vertices run back the wrong way: 4
        // near 1, then 5 near 0. No face takes them in order, but split along its diagonal from
        // 0 to 5, each part zips onto the face across its own longest edge.
        let mut pool = tetrahedron(&[[0.9, -0.4e-6, 0.3e-6], [0.1, -0.4e-6, 0.3e-6]]);
        let faces = vec![
            vec![vec![0, 2, 1, 5, 4]],
            vec![vec![0, 1, 3]],
            vec![vec![0, 3, 2]],
            vec![vec![1, 2, 3]],
            vec![vec![0, 4, 5, 1]],
        ];
        let solid = assemble(faces, &mut pool).unwrap().unwrap();
        assert_eq!((solid.vertices().len(), solid.face_count()), (4, 4));
        assert!((solid.volume() - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn only_vertices_between_their_neighbours_go() {
        // Two faces back to back; vertex 3 is the middle of the edge from 1 to 2, while 0, 1
        // and 2 are corners with two neighbours each.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 1.0, 0.0],
        ]);
        let faces = vec![vec![vec![0, 1, 3, 2]], vec![vec![2, 3, 1, 0]]];
        assert_eq!(
            remove_collinear(faces, &pool),
            [vec![vec![0, 1, 2]], vec![vec![2, 1, 0]]]
        );
    }

    #[test]
    fn a_vertex_stays_between_vertices_already_joined() {
        // Vertex 2 lies 0.5 µm off the edge from 0 to 1 that two other faces already share:
        // without it, that edge would bound four faces.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [1.0, 0.5e-6, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
        ]);
        let faces = vec![
            vec![vec![0, 2, 1, 3]],
            vec![vec![1, 2, 0, 4]],
            vec![vec![0, 1, 5]],
            vec![vec![1, 0, 6]],
        ];
        assert_eq!(remove_collinear(faces.clone(), &pool), faces);
    }

    #[test]
    fn a_vertex_stays_if_a_face_would_not_hold_without_it() {
        // Two triangles back to back, vertex 2 just off the segment from 0 to 1: without it,
        // neither would have three vertices.
        let pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.5e-6, 0.0]]);
        let faces = vec![vec![vec![0, 1, 2]], vec![vec![0, 2, 1]]];
        assert_eq!(remove_collinear(faces.clone(), &pool), faces);
    }

    #[test]
    fn a_chain_of_short_edges_wider_than_ten_tolerances_is_refused() {
        // Two faces back to back whose rings run through thirteen vertices 0.9 µm apart: the
        // edges between them collapse as one group 10.8 µm wide.
        let mut points: Vec<Vector> = (0..13).map(|i| [f64::from(i) * 0.9e-6, 0.0, 0.0]).collect();
        points.push([0.0, 1.0, 0.0]);
        let pool = Pool::of(points);
        let ring: Vec<usize> = (0..14).collect();
        let back: Vec<usize> = ring.iter().rev().copied().collect();
        let mut faces = vec![vec![ring], vec![back]];
        assert_eq!(
            collapse_short_edges(&mut faces, &pool),
            Err(BooleanError::MergeChainTooWide)
        );
    }

    #[test]
    fn a_collapse_must_leave_each_edge_met_both_ways() {
        // Two faces back to back along the edge from 0 to 1; a third running from 0 to 1 too
        // would leave it met twice one way and once the other.
        let pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let faces = vec![vec![vec![0, 1, 2]], vec![vec![2, 1, 0]], Vec::new()];
        let mut along = BTreeMap::new();
        for (f, face) in faces.iter().enumerate() {
            own(&mut along, face, f);
        }
        let changed = [(2, vec![vec![0, 1, 2]])];
        assert!(!still_paired(&along, &changed, &faces, &pool));
        assert!(still_paired(&along, &[(2, Vec::new())], &faces, &pool));
    }

    #[test]
    fn faces_back_to_back_cancel() {
        // The first two faces run round the same vertices opposite ways; the third shares only
        // an edge with them, and empty faces match nothing.
        let mut faces = vec![
            vec![vec![0, 1, 2]],
            Vec::new(),
            vec![vec![1, 0, 2]],
            vec![vec![0, 1, 3]],
            Vec::new(),
        ];
        cancel_back_to_back(&mut faces);
        assert_eq!(faces, [Vec::new(), vec![vec![0, 1, 3]], Vec::new()]);
    }
}
