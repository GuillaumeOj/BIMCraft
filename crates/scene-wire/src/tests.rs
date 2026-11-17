//! Tests of the format: one per rejection, the limits at their boundary, and a round-trip
//! property.

use proptest::prelude::*;

use crate::{
    Element, FLAG_FULL, HEADER_LEN, MAX_DELTA_BYTES, MAX_ELEMENTS, MAX_MESH_INDICES,
    MAX_MESH_VERTICES, MAX_MESHES, MAX_STRING_BYTES, MAX_STRINGS, MAX_WORLD_COORDINATE, Mesh,
    REVISION_LIMIT, SceneDelta, Section, WireError, decode, encode, mesh_is_valid,
    placement_is_valid,
};

/// Fixed seed, no persistence: runs are deterministic (AGENTS.md §5).
fn proptest_config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(71),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn triangle(id: u64) -> Mesh {
    Mesh {
        id,
        origin: [10.0, -20.0, 30.0],
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        indices: vec![0, 1, 2],
        bounds_min: [0.0, 0.0, 0.0],
        bounds_max: [1.0, 1.0, 0.0],
    }
}

fn element(id: u8, mesh: u64) -> Element {
    Element {
        id: [id; 16],
        mesh,
        rotation: IDENTITY,
        translation: [1.0, 2.0, 3.0],
        category: 0,
        discipline: 1,
    }
}

fn sample() -> SceneDelta {
    SceneDelta {
        full: false,
        revision: 7,
        base: 6,
        strings: vec!["Block".into(), "core".into()],
        meshes: vec![triangle(1), triangle(2)],
        removed_meshes: vec![9, 10],
        elements: vec![element(1, 1), element(2, 2)],
        removed_elements: vec![[3; 16]],
    }
}

fn refused(delta: &SceneDelta) -> WireError {
    decode(&encode(delta)).expect_err("the delta must be refused")
}

fn with_mesh(change: impl FnOnce(&mut Mesh)) -> WireError {
    let mut delta = sample();
    change(&mut delta.meshes[1]);
    refused(&delta)
}

fn with_element(change: impl FnOnce(&mut Element)) -> WireError {
    let mut delta = sample();
    change(&mut delta.elements[1]);
    refused(&delta)
}

#[test]
fn a_delta_round_trips() {
    for delta in [
        sample(),
        SceneDelta::default(),
        SceneDelta {
            full: true,
            ..sample()
        },
    ] {
        let delta = SceneDelta {
            base: if delta.full { 0 } else { delta.base },
            ..delta
        };
        assert_eq!(decode(&encode(&delta)), Ok(delta));
    }
}

#[test]
fn the_header_is_32_bytes_and_the_empty_delta_holds_five_counts() {
    assert_eq!(encode(&SceneDelta::default()).len(), HEADER_LEN + 5 * 4);
}

#[test]
fn the_encoder_allocates_exactly_what_it_writes() {
    for delta in [sample(), SceneDelta::default()] {
        let bytes = encode(&delta);
        assert_eq!(bytes.capacity(), bytes.len());
    }
}

#[test]
fn what_decodes_encodes_back_to_the_same_bytes() {
    let bytes = encode(&sample());
    assert_eq!(encode(&decode(&bytes).unwrap()), bytes);
}

#[test]
fn input_over_the_size_limit_is_refused_and_the_limit_itself_is_read() {
    let mut bytes = vec![0; MAX_DELTA_BYTES + 1];
    assert_eq!(
        decode(&bytes),
        Err(WireError::TooLarge {
            max: MAX_DELTA_BYTES
        })
    );
    bytes.pop();
    assert_eq!(decode(&bytes), Err(WireError::BadMagic));
}

#[test]
fn truncated_input_is_refused_at_every_length() {
    let bytes = encode(&sample());
    for len in 0..bytes.len() {
        assert_eq!(
            decode(&bytes[..len]),
            Err(WireError::Truncated),
            "length {len}"
        );
    }
}

#[test]
fn trailing_bytes_are_refused() {
    let mut bytes = encode(&sample());
    bytes.extend_from_slice(&[0, 0]);
    assert_eq!(decode(&bytes), Err(WireError::TrailingBytes { count: 2 }));
}

fn patched(offset: usize, value: &[u8]) -> Vec<u8> {
    let mut bytes = encode(&sample());
    bytes[offset..offset + value.len()].copy_from_slice(value);
    bytes
}

#[test]
fn the_header_is_checked() {
    assert_eq!(decode(&patched(0, b"BCSX")), Err(WireError::BadMagic));
    assert_eq!(
        decode(&patched(4, &2_u16.to_le_bytes())),
        Err(WireError::UnsupportedVersion { version: 2 })
    );
    assert_eq!(
        decode(&patched(6, &2_u16.to_le_bytes())),
        Err(WireError::UnknownFlags)
    );
    assert_eq!(
        decode(&patched(24, &1_u64.to_le_bytes())),
        Err(WireError::ReservedNotZero)
    );
    let mut full = patched(6, &FLAG_FULL.to_le_bytes());
    assert_eq!(decode(&full), Err(WireError::FullWithBase));
    full[16..24].copy_from_slice(&0_u64.to_le_bytes());
    assert!(decode(&full).is_ok_and(|delta| delta.full));
}

#[test]
fn strings_are_bounded_and_utf8() {
    let at_limit = "x".repeat(MAX_STRING_BYTES);
    let mut delta = SceneDelta {
        strings: vec![at_limit.clone()],
        ..SceneDelta::default()
    };
    assert_eq!(decode(&encode(&delta)).unwrap().strings, vec![at_limit]);

    delta.strings = vec!["é".repeat(MAX_STRING_BYTES / 2 + 1)];
    assert_eq!(
        refused(&delta),
        WireError::StringTooLong {
            index: 0,
            len: MAX_STRING_BYTES + 2
        }
    );

    let mut bytes = encode(&SceneDelta {
        strings: vec!["ab".into()],
        ..SceneDelta::default()
    });
    bytes[HEADER_LEN + 4 + 2] = 0xff;
    assert_eq!(decode(&bytes), Err(WireError::InvalidUtf8 { index: 0 }));
}

#[test]
fn a_string_longer_than_its_length_field_cannot_be_encoded_into_a_valid_delta() {
    let delta = SceneDelta {
        strings: vec!["x".repeat(1 << 17)],
        ..SceneDelta::default()
    };
    assert!(matches!(refused(&delta), WireError::StringTooLong { .. }));
}

#[test]
fn the_string_count_limit_is_checked_before_reading() {
    let delta = SceneDelta {
        strings: vec![String::new(); MAX_STRINGS],
        ..SceneDelta::default()
    };
    assert_eq!(decode(&encode(&delta)).unwrap().strings.len(), MAX_STRINGS);
    let delta = SceneDelta {
        strings: vec![String::new(); MAX_STRINGS + 1],
        ..SceneDelta::default()
    };
    assert_eq!(
        refused(&delta),
        WireError::TooMany {
            section: Section::Strings,
            count: MAX_STRINGS + 1,
            max: MAX_STRINGS
        }
    );
}

/// The delta of an empty header with `count` announced for the section at `offset` (a count the
/// buffer cannot back).
fn announcing(section_offset: usize, count: usize) -> Vec<u8> {
    let mut bytes = encode(&SceneDelta::default());
    let count = u32::try_from(count).unwrap();
    bytes[section_offset..section_offset + 4].copy_from_slice(&count.to_le_bytes());
    bytes
}

#[test]
fn counts_are_limited_and_cannot_outrun_the_buffer() {
    // Offsets of the counts of an empty delta: strings, meshes, removed meshes, elements,
    // removed elements.
    let offsets: Vec<usize> = (0..5).map(|i| HEADER_LEN + 4 * i).collect();
    for (offset, section, max) in [
        (offsets[1], Section::Meshes, MAX_MESHES),
        (offsets[2], Section::RemovedMeshes, MAX_MESHES),
        (offsets[3], Section::Elements, MAX_ELEMENTS),
        (offsets[4], Section::RemovedElements, MAX_ELEMENTS),
    ] {
        // At the limit the count passes and the missing bytes are what fails; one more fails on
        // the limit itself.
        assert_eq!(decode(&announcing(offset, max)), Err(WireError::Truncated));
        assert_eq!(
            decode(&announcing(offset, max + 1)),
            Err(WireError::TooMany {
                section,
                count: max + 1,
                max
            })
        );
    }
    assert_eq!(
        decode(&announcing(offsets[0], 1 << 30)),
        Err(WireError::TooMany {
            section: Section::Strings,
            count: 1 << 30,
            max: MAX_STRINGS,
        })
    );
}

#[test]
fn mesh_sizes_are_limited_and_cannot_outrun_the_buffer() {
    let mesh_counts = HEADER_LEN + 4 + 4 + 8 + 24;
    let patch = |vertices: u32, indices: u32| {
        let mut bytes = encode(&SceneDelta {
            meshes: vec![triangle(1)],
            ..SceneDelta::default()
        });
        bytes[mesh_counts..mesh_counts + 4].copy_from_slice(&vertices.to_le_bytes());
        bytes[mesh_counts + 4..mesh_counts + 8].copy_from_slice(&indices.to_le_bytes());
        bytes
    };
    let vertices = u32::try_from(MAX_MESH_VERTICES).unwrap();
    let indices = u32::try_from(MAX_MESH_INDICES).unwrap();
    assert_eq!(decode(&patch(vertices, 3)), Err(WireError::Truncated));
    assert_eq!(
        decode(&patch(vertices + 1, 3)),
        Err(WireError::MeshTooLarge {
            index: 0,
            count: MAX_MESH_VERTICES + 1,
            max: MAX_MESH_VERTICES
        })
    );
    assert_eq!(decode(&patch(3, indices)), Err(WireError::Truncated));
    assert_eq!(
        decode(&patch(3, indices + 1)),
        Err(WireError::MeshTooLarge {
            index: 0,
            count: MAX_MESH_INDICES + 1,
            max: MAX_MESH_INDICES
        })
    );
    assert_eq!(
        decode(&patch(u32::MAX, u32::MAX)).unwrap_err(),
        WireError::MeshTooLarge {
            index: 0,
            count: usize::try_from(u32::MAX).unwrap(),
            max: MAX_MESH_VERTICES,
        }
    );
}

#[test]
fn an_empty_mesh_is_refused() {
    assert_eq!(
        with_mesh(|m| {
            m.positions.clear();
            m.normals.clear();
            m.indices.clear();
        }),
        WireError::EmptyMesh { index: 1 }
    );
    assert_eq!(
        with_mesh(|m| m.indices.clear()),
        WireError::EmptyMesh { index: 1 }
    );
}

#[test]
fn mesh_coordinates_must_be_finite_and_within_the_world() {
    let beyond = MAX_WORLD_COORDINATE * 1.5;
    let error = WireError::MeshCoordinate { index: 1 };
    assert_eq!(with_mesh(|m| m.origin[1] = f64::NAN), error);
    assert_eq!(with_mesh(|m| m.origin[2] = beyond), error);
    assert_eq!(with_mesh(|m| m.positions[1][0] = f32::INFINITY), error);
    assert_eq!(with_mesh(|m| m.positions[2][2] = f32::NAN), error);
    assert_eq!(with_mesh(|m| m.positions[0][1] = -1.5e9), error);
    assert_eq!(with_mesh(|m| m.bounds_min[0] = f32::NAN), error);
    assert_eq!(with_mesh(|m| m.bounds_max[2] = 2.0e9), error);
}

#[test]
fn coordinates_at_the_world_limit_are_accepted() {
    let mut delta = sample();
    delta.meshes[0].origin = [MAX_WORLD_COORDINATE, -MAX_WORLD_COORDINATE, 0.0];
    delta.elements[0].translation = [MAX_WORLD_COORDINATE, -MAX_WORLD_COORDINATE, 0.0];
    assert_eq!(decode(&encode(&delta)), Ok(delta));
}

#[test]
fn a_lone_empty_mesh_is_reported_as_empty_not_truncated() {
    let mut delta = SceneDelta {
        meshes: vec![triangle(1)],
        ..SceneDelta::default()
    };
    delta.meshes[0].positions.clear();
    delta.meshes[0].normals.clear();
    delta.meshes[0].indices.clear();
    assert_eq!(refused(&delta), WireError::EmptyMesh { index: 0 });
}

#[test]
fn bounds_must_enclose_every_vertex() {
    let error = WireError::BoundsDoNotEnclose { index: 1 };
    assert_eq!(with_mesh(|m| m.bounds_min[1] = 2.0), error);
    assert_eq!(with_mesh(|m| m.bounds_max[0] = 0.5), error);
    assert_eq!(with_mesh(|m| m.bounds_min[2] = 0.5), error);
}

#[test]
fn revisions_stay_below_two_to_the_53() {
    let below = REVISION_LIMIT - 1;
    let delta = SceneDelta {
        revision: below,
        base: below - 1,
        ..sample()
    };
    assert_eq!(decode(&encode(&delta)), Ok(delta));
    for (revision, base) in [(REVISION_LIMIT, 0), (1, REVISION_LIMIT), (u64::MAX, 0)] {
        let delta = SceneDelta {
            revision,
            base,
            ..sample()
        };
        assert_eq!(refused(&delta), WireError::RevisionTooLarge);
    }
}

#[test]
fn normals_must_be_unit_vectors() {
    let error = WireError::InvalidNormal {
        index: 1,
        vertex: 2,
    };
    assert_eq!(with_mesh(|m| m.normals[2] = [0.0; 3]), error);
    assert_eq!(with_mesh(|m| m.normals[2] = [0.0, 0.0, 1.01]), error);
    assert_eq!(with_mesh(|m| m.normals[2] = [f32::NAN, 0.0, 1.0]), error);
    assert_eq!(
        with_mesh(|m| m.normals[2] = [f32::INFINITY, 0.0, 0.0]),
        error
    );
    let mut delta = sample();
    delta.meshes[1].normals[2] = [0.0, 0.0, 1.0005];
    assert!(decode(&encode(&delta)).is_ok());
}

#[test]
fn normals_are_measured_in_f32_as_the_renderer_does() {
    // 1.000_999_999 long in f64, within the tolerance, but 1.001_000_05 once the length is
    // computed in f32, as the renderer computes it before refusing the normal.
    let normal = [0x3ec8_2b5b, 0x3ef2_4bba, 0x3f4a_6b38].map(f32::from_bits);
    assert_eq!(
        with_mesh(|m| m.normals[2] = normal),
        WireError::InvalidNormal {
            index: 1,
            vertex: 2,
        }
    );
}

#[test]
fn a_mesh_with_fewer_normals_than_positions_is_refused() {
    // The encoder writes what it is given, so the rest of the mesh shifts and fails some check.
    let mut delta = sample();
    delta.meshes[1].normals.pop();
    assert!(decode(&encode(&delta)).is_err());
}

#[test]
fn indices_must_make_whole_triangles_inside_the_vertices() {
    assert_eq!(
        with_mesh(|m| m.indices.push(0)),
        WireError::IndexCountNotMultipleOfThree { index: 1 }
    );
    assert_eq!(
        with_mesh(|m| m.indices[2] = 3),
        WireError::IndexOutOfRange {
            index: 1,
            position: 2
        }
    );
    assert_eq!(
        with_mesh(|m| m.indices[0] = u32::MAX),
        WireError::IndexOutOfRange {
            index: 1,
            position: 0
        }
    );
}

#[test]
fn added_ids_are_unique() {
    let mut delta = sample();
    delta.meshes[1].id = 1;
    assert_eq!(refused(&delta), WireError::DuplicateMesh { index: 1 });
    let mut delta = sample();
    delta.elements[1].id = [1; 16];
    assert_eq!(refused(&delta), WireError::DuplicateElement { index: 1 });
}

#[test]
fn element_translations_are_bounded() {
    let error = WireError::ElementTranslation { index: 1 };
    assert_eq!(with_element(|e| e.translation[0] = f64::INFINITY), error);
    assert_eq!(with_element(|e| e.translation[1] = f64::NAN), error);
    assert_eq!(with_element(|e| e.translation[2] = 1.0e9 + 1.0), error);
}

#[test]
fn element_rotations_must_be_orthonormal() {
    let error = WireError::InvalidRotation { index: 1 };
    assert_eq!(with_element(|e| e.rotation = [0.0; 9]), error);
    assert_eq!(with_element(|e| e.rotation[0] = 1.001), error);
    assert_eq!(with_element(|e| e.rotation[3] = 1e-3), error);
    assert_eq!(with_element(|e| e.rotation[8] = f64::NAN), error);
    assert_eq!(with_element(|e| e.rotation = [f64::INFINITY; 9]), error);
    // A shear: unit columns that are not at right angles.
    assert_eq!(
        with_element(|e| e.rotation = [1.0, 0.0, 0.0, 0.6, 0.8, 0.0, 0.0, 0.0, 1.0]),
        error
    );
}

#[test]
fn rotations_are_orthonormal_to_the_precision_of_the_renderer() {
    // 1e-8 off: far below the old 1e-6 tolerance, but the renderer's placement refuses it.
    assert_eq!(
        with_element(|e| e.rotation[0] = 1.0 + 1e-8),
        WireError::InvalidRotation { index: 1 }
    );
}

#[test]
fn rotations_need_orthonormal_rows_as_the_renderer_checks_them() {
    // M = Vᵀ·(I + S/2), with S = s·(J − I) of eigenvalues 2s, −s, −s along the rows of Vᵀ:
    // MᵀM ≈ I + S keeps the columns within 1e-9, but MMᵀ ≈ I + diag(2s, −s, −s) puts the first
    // row 1.6e-9 from unit length.
    let s = 0.8e-9;
    let (r2, r3, r6) = (2.0_f64.sqrt(), 3.0_f64.sqrt(), 6.0_f64.sqrt());
    let v_t = [
        [1.0 / r3, 1.0 / r3, 1.0 / r3],
        [1.0 / r2, -1.0 / r2, 0.0],
        [1.0 / r6, 1.0 / r6, -2.0 / r6],
    ];
    let half = |i: usize, j: usize| if i == j { 1.0 } else { s / 2.0 };
    let m = |row: usize, column: usize| (0..3).map(|k| v_t[row][k] * half(k, column)).sum::<f64>();
    let rotation: [f64; 9] = core::array::from_fn(|i| m(i % 3, i / 3));
    assert_eq!(
        with_element(|e| e.rotation = rotation),
        WireError::InvalidRotation { index: 1 }
    );
}

#[test]
fn reflections_are_refused_and_rotations_accepted() {
    let mirror = [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    assert_eq!(
        with_element(|e| e.rotation = mirror),
        WireError::InvalidRotation { index: 1 }
    );
    let quarter_turn = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut delta = sample();
    delta.elements[0].rotation = quarter_turn;
    assert_eq!(decode(&encode(&delta)), Ok(delta));
}

#[test]
fn elements_name_strings_that_exist() {
    let error = WireError::StringIndexOutOfRange { index: 1 };
    assert_eq!(with_element(|e| e.category = 2), error);
    assert_eq!(with_element(|e| e.discipline = u32::MAX), error);
}

#[test]
fn errors_do_not_print_content() {
    let text = refused(&{
        let mut delta = sample();
        delta.meshes[1].origin[0] = 123_456_789_012.5;
        delta
    })
    .to_string();
    assert!(!text.contains("123456"));
}

// -- Round trip over generated deltas --------------------------------------------------------

fn unit_vectors() -> impl Strategy<Value = [f32; 3]> {
    (-1.0_f32..1.0, -1.0_f32..1.0, -1.0_f32..1.0).prop_filter_map("a direction", |(x, y, z)| {
        let length = (x * x + y * y + z * z).sqrt();
        (length > 0.1).then(|| [x / length, y / length, z / length])
    })
}

fn coordinate() -> impl Strategy<Value = f32> {
    -1.0e6_f32..1.0e6
}

fn meshes() -> impl Strategy<Value = Mesh> {
    (
        any::<u64>(),
        prop::array::uniform3(-1.0e9_f64..1.0e9),
        3_usize..12,
    )
        .prop_flat_map(|(id, origin, vertices)| {
            (
                prop::collection::vec(prop::array::uniform3(coordinate()), vertices),
                prop::collection::vec(unit_vectors(), vertices),
                prop::collection::vec(0..u32::try_from(vertices).unwrap(), 3..12),
            )
                .prop_map(move |(positions, normals, mut indices)| {
                    indices.truncate(indices.len() / 3 * 3);
                    let bound = |init: f32, pick: fn(f32, f32) -> f32| {
                        [0, 1, 2].map(|k| positions.iter().fold(init, |acc, p| pick(acc, p[k])))
                    };
                    let (bounds_min, bounds_max) =
                        (bound(f32::MAX, f32::min), bound(f32::MIN, f32::max));
                    Mesh {
                        id,
                        origin,
                        positions,
                        normals,
                        indices,
                        bounds_min,
                        bounds_max,
                    }
                })
        })
}

/// A signed permutation of the axes with determinant +1: exactly orthonormal and proper.
fn rotations() -> impl Strategy<Value = [f64; 9]> {
    (0..6_usize, prop::array::uniform3(any::<bool>())).prop_map(|(permutation, flips)| {
        const PERMUTATIONS: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let rows = PERMUTATIONS[permutation];
        // Odd permutations and each flip change the determinant's sign; the last flip is
        // chosen to make it +1.
        let odd = [false, true, true, false, false, true][permutation];
        let mut signs = [flips[0], flips[1], false].map(|f| if f { -1.0 } else { 1.0 });
        let product: f64 = signs[0] * signs[1] * if odd { -1.0 } else { 1.0 };
        signs[2] = product;
        let mut matrix = [0.0; 9];
        for (column, &row) in rows.iter().enumerate() {
            matrix[3 * column + row] = signs[column];
        }
        matrix
    })
}

fn deltas() -> impl Strategy<Value = SceneDelta> {
    let strings = prop::collection::vec("[a-zA-Zé ]{0,20}", 1..5);
    (strings, any::<bool>(), 0..REVISION_LIMIT, 0..REVISION_LIMIT).prop_flat_map(
        |(strings, full, revision, base)| {
            let count = u32::try_from(strings.len()).unwrap();
            (
                prop::collection::btree_map(any::<u64>(), meshes(), 0..4),
                prop::collection::vec(any::<u64>(), 0..4),
                prop::collection::btree_map(
                    any::<[u8; 16]>(),
                    (
                        any::<u64>(),
                        rotations(),
                        prop::array::uniform3(-1.0e9_f64..1.0e9),
                        0..count,
                        0..count,
                    ),
                    0..4,
                ),
                prop::collection::vec(any::<[u8; 16]>(), 0..4),
            )
                .prop_map(
                    move |(meshes, removed_meshes, elements, removed_elements)| SceneDelta {
                        full,
                        revision,
                        base: if full { 0 } else { base },
                        strings: strings.clone(),
                        meshes: meshes.into_iter().map(|(id, m)| Mesh { id, ..m }).collect(),
                        removed_meshes,
                        elements: elements
                            .into_iter()
                            .map(
                                |(id, (mesh, rotation, translation, category, discipline))| {
                                    Element {
                                        id,
                                        mesh,
                                        rotation,
                                        translation,
                                        category,
                                        discipline,
                                    }
                                },
                            )
                            .collect(),
                        removed_elements,
                    },
                )
        },
    )
}

proptest! {
    #![proptest_config(proptest_config())]

    #[test]
    fn generated_deltas_round_trip(delta in deltas()) {
        let bytes = encode(&delta);
        prop_assert_eq!(decode(&bytes), Ok(delta));
    }

    #[test]
    fn arbitrary_bytes_never_panic_and_what_decodes_re_encodes_the_same(
        bytes in prop::collection::vec(any::<u8>(), 0..200)
    ) {
        if let Ok(delta) = decode(&bytes) {
            prop_assert_eq!(encode(&delta), bytes);
        }
    }

    #[test]
    fn a_corrupted_valid_delta_never_panics(
        position in 0_usize..400,
        value in any::<u8>(),
    ) {
        let mut bytes = encode(&sample());
        let position = position % bytes.len();
        bytes[position] = value;
        if let Ok(delta) = decode(&bytes) {
            prop_assert_eq!(encode(&delta), bytes);
        }
    }
}

#[test]
fn the_validity_checks_agree_with_the_decoder() {
    assert!(mesh_is_valid(&triangle(1)));
    let mut beyond = triangle(1);
    beyond.positions[1][0] = 2e9;
    beyond.bounds_max[0] = 2e9;
    assert!(!mesh_is_valid(&beyond));
    let mut empty = triangle(1);
    empty.indices.clear();
    assert!(!mesh_is_valid(&empty));
    let mut unpaired = triangle(1);
    unpaired.normals.pop();
    assert!(!mesh_is_valid(&unpaired));

    assert!(placement_is_valid(&IDENTITY, &[1.0, 2.0, 3.0]));
    assert!(!placement_is_valid(&IDENTITY, &[2e9, 0.0, 0.0]));
    let mut reflection = IDENTITY;
    reflection[8] = -1.0;
    assert!(!placement_is_valid(&reflection, &[0.0; 3]));
}
