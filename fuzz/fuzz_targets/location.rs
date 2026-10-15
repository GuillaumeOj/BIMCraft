//! Feeds arbitrary layer sets, location lines, IFC cardinal point codes and quadrilateral
//! profiles to the location resolution of the geometry kernel (ARC-30, STR-14, GEO-13). It must
//! never panic, and what it accepts must hold its invariants: a resolved location line lies in
//! the section, a flip is undone by a second one, and a grid point lies in the profile's bounds.
//! The shear centre is left out: one mesh and solve per input would starve the fuzzer.

#![no_main]

use geom::{
    CardinalPoint, DirectionSense, Edge, Face, Layer, LayerId, LayerPlacement, LayerSet,
    LocationLine, LocationPoint, Point2, Profile, SnapPoint, SnapPosition,
};
use libfuzzer_sys::fuzz_target;

const POSITIONS: [SnapPosition; 3] = [
    SnapPosition::ExteriorFace,
    SnapPosition::Centre,
    SnapPosition::InteriorFace,
];

/// One of every kind of location line, picked by `kind`, naming `layer`.
fn line(kind: u8, layer: u64, offset: f64) -> LocationLine {
    let layer = LayerId(layer);
    let face = if kind.is_multiple_of(2) {
        Face::Exterior
    } else {
        Face::Interior
    };
    match kind % 4 {
        0 => LocationLine::CoreCentre,
        1 => LocationLine::WallFace(face),
        2 => LocationLine::Snap(SnapPoint {
            layer,
            position: POSITIONS[usize::from(kind / 4) % POSITIONS.len()],
        }),
        _ => LocationLine::Custom {
            layer,
            face,
            offset,
        },
    }
}

fn layers(raw: Vec<(u64, f64, bool)>) {
    let layers = raw
        .into_iter()
        .map(|(id, thickness, core)| Layer {
            id: LayerId(id),
            thickness,
            core,
        })
        .collect();
    let Ok(set) = LayerSet::new(layers) else {
        return;
    };
    assert_eq!(set.snap_points().len(), 2 * set.layers().len() + 1);
    for kind in 0..16 {
        let layer = set.layers()[usize::from(kind) % set.layers().len()].id;
        let resolved = set.resolve_or_core_centre(&line(kind, layer.0, set.total() / 3.0));
        assert!((0.0..=set.total()).contains(&resolved.distance));
        for sense in [DirectionSense::Positive, DirectionSense::Negative] {
            let placement = LayerPlacement::new(resolved.distance, sense);
            assert_eq!(placement.flipped().flipped(), placement);
        }
    }
}

fn quadrilateral(coordinates: [f64; 8]) {
    let points: Vec<Point2> = coordinates
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| Point2::new(x, y))
        .collect();
    let edges = (0..points.len())
        .map(|i| Edge::Line {
            start: points[i],
            end: points[(i + 1) % points.len()],
        })
        .collect();
    let Ok(profile) = Profile::new(edges, vec![]) else {
        return;
    };
    let [min_x, min_y, max_x, max_y] = profile.bounds();
    for code in 1..=9 {
        let cardinal = CardinalPoint::from_ifc_code(code).expect("1 to 9 are grid points");
        let point = LocationPoint::at(cardinal.point)
            .point_on(&profile, None)
            .expect("grid points always resolve");
        assert!((min_x..=max_x).contains(&point.x) && (min_y..=max_y).contains(&point.y));
    }
}

/// Raw layers (id, thickness, core), an IFC cardinal point code, a layer id and an offset for a
/// single-layer set, and the corners of a quadrilateral.
type Input = (Vec<(u64, f64, bool)>, u8, u64, f64, [f64; 8]);

fuzz_target!(|input: Input| {
    let (raw_layers, code, layer, offset, coordinates) = input;
    if let Ok(read) = CardinalPoint::from_ifc_code(code) {
        assert!(!read.exact || read.point.ifc_code() == code);
    } else {
        assert!(code == 0 || code > 19);
    }
    // A single layer as thick as the offset: reading any IFC offset back must resolve.
    let lone = Layer {
        id: LayerId(layer),
        thickness: offset,
        core: true,
    };
    if let Ok(set) = LayerSet::new(vec![lone])
        && let Ok(line) = set.line_at(offset / 2.0)
    {
        assert!(set.resolve(&line).is_ok());
    }
    layers(raw_layers);
    quadrilateral(coordinates);
});
