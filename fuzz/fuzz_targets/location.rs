//! Feeds arbitrary layer sets and location lines to the location resolution of the geometry
//! kernel (ARC-30, GEO-13). It must never panic, and what it accepts must hold its invariants: a
//! resolved location line lies in the section, and a flip is undone by a second one.

#![no_main]

use geom::{
    DirectionSense, Face, Layer, LayerId, LayerPlacement, LayerSet, LocationLine, SnapPoint,
    SnapPosition,
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

/// Raw layers (id, thickness, core), and a layer id and an offset for a single-layer set.
type Input = (Vec<(u64, f64, bool)>, u64, f64);

fuzz_target!(|input: Input| {
    let (raw_layers, layer, offset) = input;
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
});
