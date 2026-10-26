//! Where a point lies with respect to a solid, read as ADR 0001 reads every distance: within the
//! tolerance of the boundary, or clearly inside or outside it.

use core::f64::consts::PI;

use super::Solid;
use crate::Point3;
use crate::tolerance::{ROUNDING, WITHIN_TOLERANCE};
use crate::vector::{
    Vector, add, centroid, coords, dot, edges, fan_solid_angle, largest_axis, length, newell,
    scale, segment_distance_squared, sub,
};

/// Where a point lies with respect to a solid; see [`Solid::locate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Containment {
    /// Farther than the tolerance from the boundary, inside the solid.
    Inside,
    /// Within the tolerance of a face of the solid.
    Boundary,
    /// Farther than the tolerance from the boundary, outside the solid (in a void counts).
    Outside,
}

/// One face of a solid, ready for locating points: the points of its rings projected onto its
/// plane, which goes through the centre of its vertices along its unit normal.
struct Flat {
    rings: Vec<Vec<Vector>>,
    normal: Vector,
    centre: Vector,
}

impl Solid {
    /// Where a point lies with respect to this solid.
    ///
    /// [`Containment::Boundary`] if the point lies within [`crate::LINEAR_TOLERANCE`] of a face
    /// (up to the 1 nm the kernel allows for rounding everywhere, ADR 0001), as
    /// [`Solid::boundary_distance`] measures it. Otherwise the winding number of the faces
    /// around the point decides: about 1 inside, about 0 outside or in a void, and far from
    /// ambiguous once the point is farther than the tolerance from every face.
    #[must_use]
    pub fn locate(&self, point: Point3) -> Containment {
        let p = coords(point);
        let mut winding = 0.0;
        for face in self.flats() {
            // The flat face lies in its plane, so only a point within the tolerance of the plane
            // can be within the tolerance of the face.
            let off = face.off(p).abs();
            if off <= WITHIN_TOLERANCE && face.distance(p) <= WITHIN_TOLERANCE {
                return Containment::Boundary;
            }
            // Seen from its own plane a face subtends no angle, and the arc tangent of the solid
            // angle would turn on the sign of a zero.
            if off > ROUNDING {
                winding += face.solid_angle(p);
            }
        }
        if winding > 2.0 * PI {
            Containment::Inside
        } else {
            Containment::Outside
        }
    }

    /// The distance from a point to the nearest face of this solid, in metres.
    ///
    /// A face is planar only within the tolerance, so each is measured flattened onto its
    /// plane, through the centre of its vertices; that moves it by at most the tolerance, and
    /// only where a vertex merge bent it. Flat, the fan of triangles a face is measured with for
    /// [`Solid::locate`] lies on one side of a point off its plane, whatever the face's shape.
    #[must_use]
    pub fn boundary_distance(&self, point: Point3) -> f64 {
        let p = coords(point);
        self.flats()
            .map(|face| face.distance(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Every face flattened onto its plane.
    fn flats(&self) -> impl Iterator<Item = Flat> + '_ {
        self.faces.iter().map(|face| {
            let rings: Vec<Vec<Vector>> = face
                .rings
                .iter()
                .map(|&ring| {
                    self.ring_vertices(ring)
                        .map(|v| coords(self.vertices[v]))
                        .collect()
                })
                .collect();
            let area = rings
                .iter()
                .fold([0.0; 3], |sum, ring| add(sum, newell(ring)));
            let normal = scale(area, 1.0 / length(area));
            let centre = centroid(rings.iter().flatten());
            let flatten = |p: Vector| sub(p, scale(normal, dot(normal, sub(p, centre))));
            let rings = rings
                .into_iter()
                .map(|ring| ring.into_iter().map(flatten).collect())
                .collect();
            Flat {
                rings,
                normal,
                centre,
            }
        })
    }
}

impl Flat {
    /// The signed distance from the face's plane to `p`.
    fn off(&self, p: Vector) -> f64 {
        dot(self.normal, sub(p, self.centre))
    }

    /// The distance from `p` to the face: to its plane over the region its rings enclose,
    /// otherwise to its nearest edge.
    fn distance(&self, p: Vector) -> f64 {
        let edge = (self.rings.iter())
            .flat_map(|ring| edges(ring))
            .map(|(a, b)| segment_distance_squared(p, a, b))
            .fold(f64::INFINITY, f64::min)
            .sqrt();
        if self.encloses(p) {
            edge.min(self.off(p).abs())
        } else {
            edge
        }
    }

    /// Whether `p`, projected along the face's largest normal axis, falls inside its rings (an
    /// odd number of crossings with a ray along the first other axis). Near an edge the answer
    /// may go either way; the distance to that edge is then about the distance to the plane.
    fn encloses(&self, p: Vector) -> bool {
        let k = largest_axis(self.normal);
        let (u, w) = ((k + 1) % 3, (k + 2) % 3);
        let crossings = self
            .rings
            .iter()
            .flat_map(|ring| edges(ring))
            .filter(|(a, b)| {
                (a[w] > p[w]) != (b[w] > p[w])
                    && p[u] < a[u] + (p[w] - a[w]) / (b[w] - a[w]) * (b[u] - a[u])
            });
        crossings.count() % 2 == 1
    }

    /// The signed solid angle the face subtends at `p`: positive where its outer side faces
    /// away from `p`.
    fn solid_angle(&self, p: Vector) -> f64 {
        (self.rings.iter())
            .map(|ring| fan_solid_angle(p, ring.iter().copied()))
            .sum()
    }
}
