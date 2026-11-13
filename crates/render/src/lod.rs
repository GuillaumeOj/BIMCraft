//! Level of detail and small-feature culling: which version of a mesh an object draws, from its
//! projected size in pixels, with hysteresis (VP-02,
//! `docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`).

use crate::Projection;

/// The settings of the culling and of the level of detail of a [`Renderer`](crate::Renderer).
///
/// The *projected size* of an object is the diameter, in pixels of the target's height, of the
/// sphere that encloses its mesh's local bounding box, around the box's placed centre: turning
/// an object does not change it. An object draws its fine mesh above
/// `coarse_below_px`, its coarse level (the mesh's bounding box, 12 triangles) below, and
/// nothing below `hide_below_px`. To come back up, the size must exceed a threshold times
/// `1 + hysteresis`, so an object near a threshold does not flicker between levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSettings {
    /// Whether objects outside the view are skipped. Turning it off changes no pixel, only the
    /// time a frame takes.
    pub frustum_culling: bool,
    /// The projected size, in pixels, below which an object draws its coarse level; 0 never
    /// draws a coarse level.
    pub coarse_below_px: f64,
    /// The projected size, in pixels, below which an object is not drawn at all; 0 draws every
    /// object, however small.
    pub hide_below_px: f64,
    /// The share a projected size must exceed a threshold by to switch back to more detail,
    /// between 0 and 1.
    pub hysteresis: f64,
}

impl RenderSettings {
    /// The largest threshold, in pixels: far wider than any screen.
    pub const MAX_THRESHOLD_PX: f64 = 1e5;

    /// Checks that the thresholds are finite, between 0 and [`Self::MAX_THRESHOLD_PX`], and that
    /// the hysteresis is between 0 and 1.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let threshold = 0.0..=Self::MAX_THRESHOLD_PX;
        threshold.contains(&self.coarse_below_px)
            && threshold.contains(&self.hide_below_px)
            && (0.0..=1.0).contains(&self.hysteresis)
    }
}

impl Default for RenderSettings {
    /// Culling on; coarse below 24 px, where a 12-triangle box is indistinguishable from most
    /// building elements; hidden below 1 px; 25 % hysteresis, wider than the change in projected
    /// size of one step of a smooth orbit or zoom.
    fn default() -> Self {
        Self {
            frustum_culling: true,
            coarse_below_px: 24.0,
            hide_below_px: 1.0,
            hysteresis: 0.25,
        }
    }
}

/// Which version of its mesh an object draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DetailLevel {
    /// The mesh's bounding box, 12 triangles.
    Coarse,
    /// The mesh itself.
    Fine,
}

/// Whether a mesh of `triangles` triangles gets a coarse level: a box (12 triangles) or less is
/// its own coarse level.
pub(crate) fn has_coarse_level(triangles: usize) -> bool {
    triangles > 12
}

/// What an object shows, less detailed first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Detail {
    Hidden,
    Coarse,
    Fine,
}

/// How the projected size of an object follows from its distance, for one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SizeScale {
    /// Pixels per metre of diameter at a distance of 1 m (perspective), or at any distance
    /// (orthographic).
    pixels_per_metre: f64,
    perspective: bool,
}

impl SizeScale {
    /// The scale of a validated projection on a target `height` pixels high.
    pub(crate) fn new(projection: &Projection, height: u32) -> Self {
        let height = f64::from(height);
        match *projection {
            Projection::Perspective { fov_y_radians, .. } => Self {
                pixels_per_metre: height / (2.0 * libm::tan(fov_y_radians / 2.0)),
                perspective: true,
            },
            Projection::Orthographic {
                height: view_height,
                ..
            } => Self {
                pixels_per_metre: height / view_height,
                perspective: false,
            },
        }
    }

    /// The projected diameter, in pixels, of a sphere of `radius` whose centre is `distance`
    /// from the eye; infinite when the eye is inside the sphere of a perspective view.
    pub(crate) fn pixels(&self, radius: f64, distance: f64) -> f64 {
        let diameter = 2.0 * radius * self.pixels_per_metre;
        if !self.perspective {
            diameter
        } else if distance <= radius {
            f64::INFINITY
        } else {
            diameter / distance
        }
    }
}

/// The detail an object of projected `size` shows, given what it showed in the previous frame
/// (`None` for an object that was not in view). Less detail is taken as soon as the size falls
/// under a threshold; more detail only once it exceeds the threshold by the hysteresis.
pub(crate) fn choose(size: f64, previous: Option<Detail>, settings: &RenderSettings) -> Detail {
    let classify = |stretch: f64| {
        if size < settings.hide_below_px * stretch {
            Detail::Hidden
        } else if size < settings.coarse_below_px * stretch {
            Detail::Coarse
        } else {
            Detail::Fine
        }
    };
    let down = classify(1.0);
    let Some(previous) = previous else {
        return down;
    };
    let up = classify(1.0 + settings.hysteresis);
    if down < previous {
        down
    } else if up > previous {
        up
    } else {
        previous
    }
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use super::*;

    const SETTINGS: RenderSettings = RenderSettings {
        frustum_culling: true,
        coarse_below_px: 20.0,
        hide_below_px: 2.0,
        hysteresis: 0.5,
    };

    #[test]
    fn the_defaults_are_valid_and_out_of_range_settings_are_not() {
        assert!(RenderSettings::default().is_valid());
        assert!(SETTINGS.is_valid());
        let off = RenderSettings {
            frustum_culling: false,
            coarse_below_px: 0.0,
            hide_below_px: 0.0,
            hysteresis: 0.0,
        };
        assert!(off.is_valid());
        for bad in [
            RenderSettings {
                coarse_below_px: -1.0,
                ..SETTINGS
            },
            RenderSettings {
                coarse_below_px: f64::NAN,
                ..SETTINGS
            },
            RenderSettings {
                hide_below_px: 2e5,
                ..SETTINGS
            },
            RenderSettings {
                hide_below_px: f64::INFINITY,
                ..SETTINGS
            },
            RenderSettings {
                hysteresis: 1.5,
                ..SETTINGS
            },
            RenderSettings {
                hysteresis: -0.1,
                ..SETTINGS
            },
        ] {
            assert!(!bad.is_valid(), "{bad:?}");
        }
    }

    #[test]
    fn projected_sizes_follow_the_projection() {
        // 90°: at 1 m the view is 2 m high, so 100 px over 2 m is 50 px per metre.
        let perspective = SizeScale::new(
            &Projection::Perspective {
                fov_y_radians: FRAC_PI_2,
                near: 0.1,
            },
            100,
        );
        assert!((perspective.pixels(1.0, 10.0) - 10.0).abs() < 1e-9);
        assert!((perspective.pixels(1.0, 20.0) - 5.0).abs() < 1e-9);
        assert!(perspective.pixels(1.0, 0.5).is_infinite());
        let orthographic = SizeScale::new(
            &Projection::Orthographic {
                height: 10.0,
                near: 0.1,
                far: 100.0,
            },
            100,
        );
        assert!((orthographic.pixels(1.0, 10.0) - 20.0).abs() < 1e-12);
        assert!((orthographic.pixels(1.0, 0.0) - 20.0).abs() < 1e-12);
    }

    #[test]
    fn a_new_object_takes_its_level_without_hysteresis() {
        for (size, detail) in [
            (f64::INFINITY, Detail::Fine),
            (20.0, Detail::Fine),
            (19.9, Detail::Coarse),
            (2.0, Detail::Coarse),
            (1.9, Detail::Hidden),
            (0.0, Detail::Hidden),
        ] {
            assert_eq!(choose(size, None, &SETTINGS), detail, "{size}");
        }
    }

    #[test]
    fn less_detail_comes_at_the_threshold_and_more_only_past_the_hysteresis() {
        use Detail::{Coarse, Fine, Hidden};
        // Shrinking: fine down to 20 px, coarse down to 2 px, then hidden.
        let mut detail = Fine;
        let mut seen = Vec::new();
        for size in [40.0, 20.0, 19.0, 10.0, 2.0, 1.9, 1.0] {
            detail = choose(size, Some(detail), &SETTINGS);
            seen.push(detail);
        }
        assert_eq!(seen, [Fine, Fine, Coarse, Coarse, Coarse, Hidden, Hidden]);
        // Growing back: hidden until 3 px (2 × 1.5), coarse until 30 px (20 × 1.5).
        seen.clear();
        for size in [2.0, 2.9, 3.0, 20.0, 29.9, 30.0] {
            detail = choose(size, Some(detail), &SETTINGS);
            seen.push(detail);
        }
        assert_eq!(seen, [Hidden, Hidden, Coarse, Coarse, Coarse, Fine]);
        // A jump across both thresholds goes straight to the level of the size.
        assert_eq!(choose(100.0, Some(Hidden), &SETTINGS), Fine);
        assert_eq!(choose(1.0, Some(Fine), &SETTINGS), Hidden);
    }

    #[test]
    fn a_size_oscillating_inside_the_band_never_switches() {
        for start in [Detail::Fine, Detail::Coarse] {
            let mut detail = start;
            for step in 0..100 {
                let size = if step % 2 == 0 { 20.5 } else { 29.5 };
                detail = choose(size, Some(detail), &SETTINGS);
                assert_eq!(detail, start);
            }
        }
    }

    #[test]
    fn zero_thresholds_keep_every_object_fine() {
        let off = RenderSettings {
            coarse_below_px: 0.0,
            hide_below_px: 0.0,
            ..SETTINGS
        };
        assert_eq!(choose(0.0, Some(Detail::Fine), &off), Detail::Fine);
        assert_eq!(choose(1e-9, None, &off), Detail::Fine);
    }
}
