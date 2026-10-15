//! Location lines against the worked example of the design (Penpot board 3.3).

use super::*;
use crate::MODEL_EXTENT;

const MM: f64 = 1e-3;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

fn layer(id: u64, thickness_mm: f64, core: bool) -> Layer {
    Layer {
        id: LayerId(id),
        thickness: thickness_mm * MM,
        core,
    }
}

/// Render 10, EPS 140, concrete core 200, plaster 13: 363 mm.
fn etics() -> LayerSet {
    LayerSet::new(vec![
        layer(1, 10.0, false),
        layer(2, 140.0, false),
        layer(3, 200.0, true),
        layer(4, 13.0, false),
    ])
    .expect("valid layers")
}

fn snap(id: u64, position: SnapPosition) -> LocationLine {
    LocationLine::Snap(SnapPoint {
        layer: LayerId(id),
        position,
    })
}

fn custom(id: u64, face: Face, offset_mm: f64) -> LocationLine {
    LocationLine::Custom {
        layer: LayerId(id),
        face,
        offset: offset_mm * MM,
    }
}

#[test]
fn snap_points_of_the_design_example() {
    let set = etics();
    assert!(close(set.total(), 363.0 * MM));
    let snaps = set.snap_points();
    let distances: Vec<f64> = snaps.iter().map(|s| s.distance / MM).collect();
    let expected = [0.0, 5.0, 10.0, 80.0, 150.0, 250.0, 350.0, 356.5, 363.0];
    assert_eq!(distances.len(), expected.len());
    for (got, want) in distances.iter().zip(expected) {
        assert!((got - want).abs() < 1e-9, "{got} != {want}");
    }
    assert_eq!(
        snaps[5].name("Structural core"),
        "Layer 3 — Structural core · centre"
    );
    assert_eq!(
        snaps[4].name("Structural core"),
        "Layer 3 — Structural core · exterior face"
    );
    assert_eq!(snaps[8].name("Finish"), "Layer 4 — Finish · interior face");
    assert_eq!(
        snaps[8].point,
        SnapPoint {
            layer: LayerId(4),
            position: SnapPosition::InteriorFace
        }
    );
}

#[test]
fn the_default_is_the_core_centre_at_250_mm() {
    let set = etics();
    assert_eq!(LocationLine::default(), LocationLine::CoreCentre);
    assert!(close(
        set.resolve(&LocationLine::default()).expect("resolves"),
        250.0 * MM
    ));
    assert!(close(set.core_centre(), 250.0 * MM));
}

#[test]
fn core_centre_spans_several_core_layers_or_the_whole_section() {
    let split_core = LayerSet::new(vec![
        layer(1, 100.0, true),
        layer(2, 50.0, false),
        layer(3, 100.0, true),
        layer(4, 50.0, false),
    ])
    .expect("valid layers");
    assert!(close(split_core.core_centre(), 125.0 * MM));
    let no_core =
        LayerSet::new(vec![layer(1, 100.0, false), layer(2, 60.0, false)]).expect("valid layers");
    assert!(close(no_core.core_centre(), 80.0 * MM));
}

#[test]
fn faces_centres_and_wall_faces_resolve() {
    let set = etics();
    let cases = [
        (LocationLine::WallFace(Face::Exterior), 0.0),
        (LocationLine::WallFace(Face::Interior), 363.0),
        (snap(2, SnapPosition::ExteriorFace), 10.0),
        (snap(1, SnapPosition::InteriorFace), 10.0),
        (snap(2, SnapPosition::Centre), 80.0),
        (snap(3, SnapPosition::InteriorFace), 350.0),
        (snap(4, SnapPosition::InteriorFace), 363.0),
    ];
    for (line, want) in cases {
        assert!(
            close(set.resolve(&line).expect("resolves"), want * MM),
            "{line:?}"
        );
    }
    assert_eq!(set.layers().len(), 4);
}

#[test]
fn custom_offsets_start_from_a_face_of_the_chosen_layer() {
    let set = etics();
    // The design's "custom offset 0 mm from core exterior face".
    assert!(close(
        set.resolve(&custom(3, Face::Exterior, 0.0))
            .expect("inside"),
        150.0 * MM
    ));
    assert!(close(
        set.resolve(&custom(3, Face::Exterior, 50.0))
            .expect("inside"),
        200.0 * MM
    ));
    assert!(close(
        set.resolve(&custom(3, Face::Interior, -20.0))
            .expect("inside"),
        330.0 * MM
    ));
    assert!(close(
        set.resolve(&custom(4, Face::Interior, 0.0))
            .expect("inside"),
        363.0 * MM
    ));
    assert_eq!(
        set.resolve(&custom(1, Face::Exterior, -1.0)),
        Err(LocationError::OffsetOutsideSection)
    );
    assert_eq!(
        set.resolve(&custom(4, Face::Interior, 1.0)),
        Err(LocationError::OffsetOutsideSection)
    );
    for bad in [f64::NAN, f64::INFINITY, 2.0 * MODEL_EXTENT, 1e-310] {
        assert_eq!(
            set.resolve(&LocationLine::Custom {
                layer: LayerId(1),
                face: Face::Exterior,
                offset: bad
            }),
            Err(LocationError::InvalidOffset)
        );
    }
}

#[test]
fn custom_offsets_within_the_tolerance_of_a_face_are_clamped() {
    let set = etics();
    let outside = LocationLine::Custom {
        layer: LayerId(1),
        face: Face::Exterior,
        offset: -crate::LINEAR_TOLERANCE / 2.0,
    };
    assert_eq!(
        set.resolve(&outside).expect("within tolerance").to_bits(),
        0.0_f64.to_bits()
    );
}

#[test]
fn reordering_layers_keeps_the_choice_on_its_layer() {
    let reordered = LayerSet::new(vec![
        layer(4, 13.0, false),
        layer(3, 200.0, true),
        layer(1, 10.0, false),
        layer(2, 140.0, false),
    ])
    .expect("valid layers");
    let line = snap(3, SnapPosition::ExteriorFace);
    assert!(close(
        reordered.resolve(&line).expect("still there"),
        13.0 * MM
    ));
    assert!(close(
        reordered
            .resolve(&snap(2, SnapPosition::Centre))
            .expect("there"),
        293.0 * MM
    ));
}

#[test]
fn a_removed_layer_falls_back_to_the_core_centre_and_says_why() {
    let without_insulation = LayerSet::new(vec![
        layer(1, 10.0, false),
        layer(3, 200.0, true),
        layer(4, 13.0, false),
    ])
    .expect("valid layers");
    let line = snap(2, SnapPosition::Centre);
    assert_eq!(
        without_insulation.resolve(&line),
        Err(LocationError::UnknownLayer)
    );
    let resolved = without_insulation.resolve_or_core_centre(&line);
    assert!(close(resolved.distance, 110.0 * MM));
    assert_eq!(resolved.fallback, Some(LocationError::UnknownLayer));
    assert_eq!(
        without_insulation.resolve(&custom(2, Face::Interior, 0.0)),
        Err(LocationError::UnknownLayer)
    );
    let kept = without_insulation.resolve_or_core_centre(&snap(3, SnapPosition::Centre));
    assert_eq!(kept.fallback, None);
}

#[test]
fn invalid_layer_sets_are_rejected() {
    assert_eq!(LayerSet::new(vec![]), Err(LocationError::EmptyLayerSet));
    for bad in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        1e-310,
        2.0 * MODEL_EXTENT,
    ] {
        let set = LayerSet::new(vec![
            layer(1, 10.0, false),
            Layer {
                id: LayerId(2),
                thickness: bad,
                core: false,
            },
        ]);
        assert_eq!(set, Err(LocationError::InvalidThickness(1)), "{bad}");
    }
    assert_eq!(
        LayerSet::new(vec![
            layer(1, 10.0, false),
            layer(2, 10.0, false),
            layer(1, 5.0, true)
        ]),
        Err(LocationError::DuplicateLayer(2))
    );
    let half = MODEL_EXTENT / MM / 2.0 + 1.0;
    assert_eq!(
        LayerSet::new(vec![layer(1, half, false), layer(2, half, false)]),
        Err(LocationError::SectionTooThick)
    );
    let many = (0..=MAX_LAYERS as u64)
        .map(|id| layer(id, 1.0, false))
        .collect();
    assert_eq!(LayerSet::new(many), Err(LocationError::TooManyLayers));
}

#[test]
fn ifc_distances_map_back_to_snap_points_or_exact_custom_offsets() {
    let set = etics();
    assert_eq!(set.line_at(250.0 * MM), Ok(LocationLine::CoreCentre));
    assert_eq!(
        set.line_at(80.0 * MM + 1e-7),
        Ok(snap(2, SnapPosition::Centre))
    );
    assert_eq!(set.line_at(0.0), Ok(snap(1, SnapPosition::ExteriorFace)));
    let odd = 123.456 * MM;
    let line = set.line_at(odd).expect("inside");
    assert_eq!(
        line,
        LocationLine::Custom {
            layer: LayerId(1),
            face: Face::Exterior,
            offset: odd
        }
    );
    assert_eq!(set.resolve(&line).expect("inside").to_bits(), odd.to_bits());
    assert_eq!(set.line_at(-1.0), Err(LocationError::OffsetOutsideSection));
    assert_eq!(set.line_at(f64::NAN), Err(LocationError::InvalidOffset));
}

#[test]
fn placement_follows_ifc_layer_set_usage_and_flips_about_the_axis() {
    let set = etics();
    let d = set.core_centre();
    let placement = LayerPlacement::new(d, DirectionSense::default());
    assert_eq!(placement.direction_sense, DirectionSense::Positive);
    assert!(close(placement.offset_from_reference_line, -250.0 * MM));
    assert!(close(placement.interior_y(set.total()), 113.0 * MM));
    assert!(close(placement.distance(), d));

    let flipped = placement.flipped();
    assert_eq!(flipped.direction_sense, DirectionSense::Negative);
    assert!(close(flipped.offset_from_reference_line, 250.0 * MM));
    assert!(close(flipped.interior_y(set.total()), -113.0 * MM));
    assert!(close(flipped.distance(), d));
    assert_eq!(flipped.flipped(), placement);
    assert_eq!(LayerPlacement::new(d, DirectionSense::Negative), flipped);
}

#[test]
fn changing_the_location_line_moves_the_axis_not_the_wall() {
    let set = etics();
    let placement = LayerPlacement::new(set.core_centre(), DirectionSense::Positive);
    // Core centre (250) to exterior face (0): the axis moves 250 mm towards the exterior (−y).
    assert!(close(placement.axis_shift(0.0), -250.0 * MM));
    assert!(close(placement.flipped().axis_shift(0.0), 250.0 * MM));
    // The exterior face stays put: old axis + old offset = new axis + new offset.
    let shift = placement.axis_shift(80.0 * MM);
    let moved = LayerPlacement::new(80.0 * MM, DirectionSense::Positive);
    assert!(close(
        placement.offset_from_reference_line,
        shift + moved.offset_from_reference_line
    ));
}

#[test]
fn error_messages_carry_no_dimensions() {
    assert_eq!(
        LocationError::InvalidThickness(2).to_string(),
        "layer 2 has an invalid thickness"
    );
    assert_eq!(
        LocationError::TooManyLayers.to_string(),
        format!("the layer set has more than {MAX_LAYERS} layers")
    );
}
