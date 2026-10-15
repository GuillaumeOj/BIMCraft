//! Properties of location lines (ARC-30, GEO-13, ENG-02): resolved locations stay within the
//! section, flips are involutions, reordering keeps a choice on its layer, and the IFC mapping
//! round-trips.

use geom::{
    DirectionSense, Face, LINEAR_TOLERANCE, Layer, LayerId, LayerPlacement, LayerSet, LocationLine,
    SnapPoint, SnapPosition,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// Layers 1 mm to 0.5 m thick, ids 0 to n − 1, some of them core.
fn layers() -> impl Strategy<Value = Vec<Layer>> {
    prop::collection::vec((1e-3..0.5f64, any::<bool>()), 1..12).prop_map(|raw| {
        raw.into_iter()
            .zip(0..)
            .map(|((thickness, core), id)| Layer {
                id: LayerId(id),
                thickness,
                core,
            })
            .collect()
    })
}

fn position() -> impl Strategy<Value = SnapPosition> {
    prop_oneof![
        Just(SnapPosition::ExteriorFace),
        Just(SnapPosition::Centre),
        Just(SnapPosition::InteriorFace),
    ]
}

fn face() -> impl Strategy<Value = Face> {
    prop_oneof![Just(Face::Exterior), Just(Face::Interior)]
}

/// Any location line naming a layer id from 0 to 15, so some ids are missing from the set.
fn line() -> impl Strategy<Value = LocationLine> {
    prop_oneof![
        Just(LocationLine::CoreCentre),
        face().prop_map(LocationLine::WallFace),
        (0..16u64, position()).prop_map(|(id, position)| LocationLine::Snap(SnapPoint {
            layer: LayerId(id),
            position,
        })),
        (0..16u64, face(), -1.0..1.0f64).prop_map(|(id, face, offset)| LocationLine::Custom {
            layer: LayerId(id),
            face,
            offset,
        }),
    ]
}

fn sense() -> impl Strategy<Value = DirectionSense> {
    prop_oneof![
        Just(DirectionSense::Positive),
        Just(DirectionSense::Negative)
    ]
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1_000,
        rng_seed: RngSeed::Fixed(10),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn resolved_location_lies_within_the_section(layers in layers(), line in line()) {
        let set = LayerSet::new(layers).expect("valid layers");
        let resolved = set.resolve_or_core_centre(&line);
        prop_assert!((0.0..=set.total()).contains(&resolved.distance));
        prop_assert_eq!(resolved.fallback.is_some(), set.resolve(&line).is_err());
        for snap in set.snap_points() {
            prop_assert!((0.0..=set.total()).contains(&snap.distance));
        }
    }

    #[test]
    fn flipping_twice_gives_back_the_original(distance in 0.0..10.0f64, sense in sense()) {
        let placement = LayerPlacement::new(distance, sense);
        prop_assert_eq!(placement.flipped().flipped(), placement);
        prop_assert_eq!(placement.flipped().distance().to_bits(), placement.distance().to_bits());
    }

    #[test]
    fn reordering_keeps_a_face_choice_on_the_same_layer_face(
        (layers, order) in layers().prop_flat_map(|layers| {
            let order = Just((0..layers.len()).collect::<Vec<_>>()).prop_shuffle();
            (Just(layers), order)
        }),
        pick in any::<prop::sample::Index>(),
        interior in any::<bool>(),
    ) {
        let reordered: Vec<Layer> = order.iter().map(|&i| layers[i]).collect();
        let set = LayerSet::new(reordered.clone()).expect("valid layers");
        let chosen = layers[pick.index(layers.len())].id;
        let position = if interior { SnapPosition::InteriorFace } else { SnapPosition::ExteriorFace };
        let line = LocationLine::Snap(SnapPoint { layer: chosen, position });
        // The face of the chosen layer, wherever reordering put it.
        let at = reordered.iter().position(|layer| layer.id == chosen).expect("still present");
        let end = if interior { at + 1 } else { at };
        let expected = reordered[..end].iter().fold(0.0, |sum, layer| sum + layer.thickness);
        prop_assert_eq!(set.resolve(&line).expect("still present").to_bits(), expected.to_bits());
    }

    #[test]
    fn ifc_layer_offsets_round_trip(layers in layers(), line in line(), sense in sense()) {
        let set = LayerSet::new(layers).expect("valid layers");
        let distance = set.resolve_or_core_centre(&line).distance;
        let placement = LayerPlacement::new(distance, sense);
        // Exported as OffsetFromReferenceLine and DirectionSense, read back exactly.
        prop_assert_eq!(placement.distance().to_bits(), distance.to_bits());
        let back = set.line_at(placement.distance()).expect("inside the section");
        let again = set.resolve(&back).expect("resolves");
        prop_assert!((again - distance).abs() <= LINEAR_TOLERANCE);
        if matches!(back, LocationLine::Custom { .. }) {
            prop_assert_eq!(again.to_bits(), distance.to_bits());
        }
    }
}
