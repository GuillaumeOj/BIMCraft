//! Axis-aligned bounding boxes in `f64` world coordinates: the bounds the culling works on
//! (VP-02).

use crate::Placement;
use crate::vector::{add, apply, norm, scale, sub};

/// An axis-aligned box, `min` ≤ `max` on every axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Aabb {
    pub(crate) min: [f64; 3],
    pub(crate) max: [f64; 3],
}

impl Aabb {
    /// The bounds of local `f32` positions, relative to `origin`; `None` without positions.
    pub(crate) fn of_local(origin: [f64; 3], positions: &[[f32; 3]]) -> Option<Self> {
        let (first, rest) = positions.split_first()?;
        let mut min = *first;
        let mut max = *first;
        for position in rest {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        Some(Self {
            min: add(origin, min.map(f64::from)),
            max: add(origin, max.map(f64::from)),
        })
    }

    /// The bounds of this box once placed: the box of its eight placed corners. A rotation turns
    /// the box, so the result encloses it but may be larger.
    pub(crate) fn placed(&self, placement: &Placement) -> Self {
        // The centre and the half-extents placed separately: |R|·e bounds the turned box exactly.
        let centre = add(
            apply(&placement.rotation, self.centre()),
            placement.translation,
        );
        let half = self.half_extents();
        let extent: [f64; 3] = core::array::from_fn(|row| {
            (0..3)
                .map(|column| placement.rotation[row][column].abs() * half[column])
                .sum()
        });
        Self {
            min: sub(centre, extent),
            max: add(centre, extent),
        }
    }

    /// The smallest box enclosing both.
    pub(crate) fn union(&self, other: &Self) -> Self {
        Self {
            min: core::array::from_fn(|axis| self.min[axis].min(other.min[axis])),
            max: core::array::from_fn(|axis| self.max[axis].max(other.max[axis])),
        }
    }

    /// Whether `other` lies inside this box.
    #[cfg(test)]
    pub(crate) fn contains(&self, other: &Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.min[axis] && other.max[axis] <= self.max[axis])
    }

    /// Whether the two boxes share a point.
    #[cfg(test)]
    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.max[axis] && other.min[axis] <= self.max[axis])
    }

    /// The area of the surface of the box: the cost the hierarchy minimises.
    pub(crate) fn surface_area(&self) -> f64 {
        let [x, y, z] = sub(self.max, self.min);
        2.0 * (x * y + y * z + z * x)
    }

    pub(crate) fn centre(&self) -> [f64; 3] {
        // Halving first cannot overflow.
        core::array::from_fn(|axis| self.min[axis] / 2.0 + self.max[axis] / 2.0)
    }

    pub(crate) fn half_extents(&self) -> [f64; 3] {
        scale(sub(self.max, self.min), 0.5)
    }

    /// The radius of the sphere around the centre that encloses the box.
    pub(crate) fn radius(&self) -> f64 {
        norm(self.half_extents())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: Aabb = Aabb {
        min: [-1.0, -2.0, -3.0],
        max: [1.0, 2.0, 3.0],
    };

    #[test]
    fn local_bounds_add_the_origin() {
        let bounds = Aabb::of_local([10.0, 20.0, 30.0], &[[1.0, -2.0, 0.5], [-1.0, 3.0, 0.0]]);
        assert_eq!(
            bounds,
            Some(Aabb {
                min: [9.0, 18.0, 30.0],
                max: [11.0, 23.0, 30.5],
            })
        );
        assert_eq!(Aabb::of_local([0.0; 3], &[]), None);
    }

    #[test]
    fn a_translation_moves_the_box() {
        let moved = UNIT.placed(&Placement::from_translation([5.0, 0.0, -5.0]));
        assert_eq!(
            moved,
            Aabb {
                min: [4.0, -2.0, -8.0],
                max: [6.0, 2.0, -2.0],
            }
        );
    }

    #[test]
    fn a_quarter_turn_swaps_the_extents_and_a_reflection_keeps_them() {
        let quarter = Placement {
            rotation: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
        };
        assert_eq!(
            UNIT.placed(&quarter),
            Aabb {
                min: [-2.0, -1.0, -3.0],
                max: [2.0, 1.0, 3.0],
            }
        );
        let mirror = Placement {
            rotation: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
        };
        assert_eq!(UNIT.placed(&mirror), UNIT);
    }

    #[test]
    fn an_eighth_turn_encloses_the_turned_corners() {
        let (s, c) = (libm::sin(0.5), libm::cos(0.5));
        let turn = Placement {
            rotation: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            translation: [3.0, 4.0, 0.0],
        };
        let placed = UNIT.placed(&turn);
        for corner in 0..8 {
            let local = core::array::from_fn(|axis| {
                if corner >> axis & 1 == 0 {
                    UNIT.min[axis]
                } else {
                    UNIT.max[axis]
                }
            });
            let world = add(apply(&turn.rotation, local), turn.translation);
            let point = Aabb {
                min: world,
                max: world,
            };
            assert!(placed.contains(&point), "corner {corner}");
        }
    }

    #[test]
    fn unions_areas_and_radii() {
        let other = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [4.0, 1.0, 1.0],
        };
        let both = UNIT.union(&other);
        assert_eq!(
            both,
            Aabb {
                min: [-1.0, -2.0, -3.0],
                max: [4.0, 2.0, 3.0],
            }
        );
        assert!(both.contains(&UNIT) && both.contains(&other));
        assert!(UNIT.overlaps(&other));
        let apart = Aabb {
            min: [2.0, 0.0, 0.0],
            max: [3.0, 1.0, 1.0],
        };
        assert!(!UNIT.overlaps(&apart));
        assert!((UNIT.surface_area() - 2.0 * (2.0 * 4.0 + 4.0 * 6.0 + 6.0 * 2.0)).abs() < 1e-12);
        assert!((UNIT.radius() - 14.0_f64.sqrt()).abs() < 1e-12);
        assert!(UNIT.centre().iter().all(|&v| v == 0.0));
    }
}
