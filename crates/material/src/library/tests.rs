use core_model::units::Quantity;
use core_model::units::StressUnit::Megapascal;

use super::*;

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12 * expected.abs(),
        "{actual} ≠ {expected}"
    );
}

fn builtin() -> Library {
    Library::builtin().unwrap()
}

fn value(library: &Library, key: &str, property: Property) -> f64 {
    library.get(key).unwrap().property(property).unwrap().value
}

/// A library source holding one material, as a user or plugin would write it.
fn file(name: &str, category: &str, law: &str, properties: &str) -> String {
    format!(
        r#"{{"format": 1, "materials": [{{"name": "{name}", "category": "{category}",
            "behaviour_law": "{law}", "properties": {{{properties}}}}}]}}"#
    )
}

fn modulus(value: &str, clause: &str) -> String {
    format!(
        r#""e_modulus": {{"value": {value}, "document": "Maker sheet 12", "clause": "{clause}"}}"#
    )
}

const C30: &str = "en1992-1-1/C30/37";
const S355: &str = "en1993-1-1/S355";
const B500B: &str = "en1992-1-1/B500B";

#[test]
fn the_builtin_library_ships_the_european_grades_under_stable_keys() {
    let library = builtin();
    let keys: Vec<&str> = library.iter().map(|(key, _)| key).collect();
    for key in [C30, S355, B500B, "en1992-1-1/C20/25", "en1993-1-1/S460"] {
        assert!(keys.contains(&key), "{key}");
    }
    assert!(!library.is_empty());
    for (key, material) in library.iter() {
        assert_eq!(material.library_key(), Some(key));
        assert!(key.ends_with(material.name()), "{key}");
    }
}

#[test]
fn design_codes_become_keys_without_edition_or_spaces() {
    assert_eq!(design_code_key("EN 1992-1-1:2004"), "en1992-1-1");
    assert_eq!(design_code_key("NF EN 1995-1-1/NA:2010"), "nfen1995-1-1/na");
    assert_eq!(design_code_key("EN 1993-1-1"), "en1993-1-1");
}

// Spot checks against EN 1992-1-1:2004 Table 3.1, Annex C and EN 1993-1-1:2005 Table 3.1.
#[test]
fn builtin_values_match_the_code_tables() {
    let library = builtin();
    let mpa = |key, property| value(&library, key, property) / 1e6;
    assert_close(mpa(C30, Property::CharacteristicCompressiveStrength), 30.0);
    assert_close(mpa(C30, Property::MeanCompressiveStrength), 38.0);
    assert_close(mpa(C30, Property::MeanTensileStrength), 2.9);
    assert_close(value(&library, C30, Property::YoungModulus) / 1e9, 33.0);
    assert_close(mpa(S355, Property::YieldStrength), 355.0);
    assert_close(mpa(S355, Property::UltimateStrength), 510.0);
    assert_close(mpa(B500B, Property::YieldStrength), 500.0);
    assert_close(value(&library, B500B, Property::HardeningRatio), 1.08);
    let source = &library
        .get(B500B)
        .unwrap()
        .property(Property::HardeningRatio)
        .unwrap()
        .source;
    assert_eq!(source.clause(), Some("Annex C, Table C.1"));
}

// Hand calculations (ENG-03) with the shipped laws.
#[test]
fn builtin_laws_give_the_expected_stresses() {
    let library = builtin();
    let stress = |key: &str, strain| library.get(key).unwrap().law().stress(strain).unwrap();
    let mpa = |value| Quantity::new(value, Megapascal);
    // C30/37 at 1 ‰: 30 × (1 − (1 − 0.5)²) = 22.5 MPa in compression.
    assert_eq!(stress(C30, -0.001), mpa(-22.5));
    // S355 yields at 355 MPa; beyond, a horizontal branch.
    assert_eq!(stress(S355, 0.01), mpa(355.0));
    // B500B at 5 %: 1.08 × 500 = 540 MPa.
    assert!((stress(B500B, 0.05).get(Megapascal) - 540.0).abs() < 1e-9);
}

#[test]
fn users_and_plugins_extend_the_library() {
    let mut library = builtin();
    let before = library.len();
    let board = file(
        "Board",
        "generic",
        "linear_elastic",
        &modulus("11.5e9", "Table 5"),
    );
    library.extend("my-office", &board).unwrap();
    assert_eq!(library.len(), before + 1);
    let board = library.get("my-office/Board").unwrap();
    assert_eq!(board.library_key(), Some("my-office/Board"));
    let source = &board.property(Property::YoungModulus).unwrap().source;
    assert_eq!(source.clause(), Some("Table 5"));
    // The builder form gives the same library.
    let built = builtin().with_source(
        "my-office",
        &file(
            "Board",
            "generic",
            "linear_elastic",
            &modulus("11.5e9", "Table 5"),
        ),
    );
    assert_eq!(built, Ok(library));
}

#[test]
fn a_rejected_source_adds_nothing() {
    let mut library = builtin();
    let clash = r#"{"format": 1, "materials": [{"name": "S355", "category": "steel",
        "behaviour_law": "steel_bilinear", "design_code": "EN 1993-1-1:2005",
        "properties": {"fy": {"value": 3.55e8, "document": "D", "clause": "1"},
        "e_modulus": {"value": 2.1e11, "document": "D", "clause": "1"}}}]}"#;
    assert_eq!(
        library.extend("mine", clash),
        Err(LibraryError::Duplicate(1))
    );
    let entry = |name: &str| {
        format!(
            r#"{{"name": "{name}", "category": "generic", "behaviour_law": "linear_elastic",
                "properties": {{{}}}}}"#,
            modulus("1e9", "1")
        )
    };
    let twice = format!(
        r#"{{"format": 1, "materials": [{}, {}]}}"#,
        entry("A"),
        entry("A")
    );
    assert_eq!(
        library.extend("mine", &twice),
        Err(LibraryError::Duplicate(2))
    );
    assert_eq!(library, builtin());
}

#[test]
fn malformed_sources_are_rejected_with_the_reason() {
    let mut library = Library::default();
    let elastic = |properties: &str| file("A", "generic", "linear_elastic", properties);
    let cases = [
        (" ".repeat(MAX_LIBRARY_SIZE + 1), LibraryError::TooLarge),
        (
            "{\n  \"format\": 1,\n  \"x\": 2}".to_owned(),
            LibraryError::Syntax { line: 3, column: 5 },
        ),
        (
            r#"{"format": 2, "materials": []}"#.to_owned(),
            LibraryError::Format(2),
        ),
        (
            file("A", "wood", "linear_elastic", ""),
            LibraryError::UnknownCategory(1),
        ),
        (
            file("A", "generic", "plastic", ""),
            LibraryError::UnknownLaw(1),
        ),
        (
            elastic(r#""colour": {"value": 1, "document": "D", "clause": "1"}"#),
            LibraryError::UnknownProperty(1),
        ),
        (
            elastic(&modulus("1e9", " ")),
            LibraryError::Source {
                index: 1,
                property: Property::YoungModulus,
                error: SourceError::EmptyClause,
            },
        ),
        (
            elastic(&modulus("-1e9", "1")),
            LibraryError::Invalid {
                index: 1,
                error: MaterialError::OutOfRange(Property::YoungModulus),
            },
        ),
        (
            file(r"A\nB", "generic", "linear_elastic", &modulus("1e9", "1")),
            LibraryError::Invalid {
                index: 1,
                error: MaterialError::ControlCharacter("name"),
            },
        ),
    ];
    for (json, error) in cases {
        let result = library.extend("mine", &json);
        assert_eq!(result, Err(error.clone()), "{error}");
    }
    assert!(library.is_empty());
}

#[test]
fn source_names_are_checked() {
    let json = r#"{"format": 1, "materials": []}"#;
    for name in [
        "",
        "Mine",
        "my office",
        "a/b",
        &"x".repeat(MAX_SOURCE_NAME + 1),
    ] {
        assert_eq!(
            Library::default().extend(name, json),
            Err(LibraryError::SourceName),
            "{name}"
        );
    }
    assert_eq!(Library::default().extend("my-office-2", json), Ok(()));
}

#[test]
fn libraries_are_bounded_in_materials() {
    let entries: Vec<String> = (0..=MAX_MATERIALS)
        .map(|index| {
            format!(
                r#"{{"name": "A{index}", "category": "generic", "behaviour_law": "none", "properties": {{}}}}"#
            )
        })
        .collect();
    let json = format!(r#"{{"format": 1, "materials": [{}]}}"#, entries.join(","));
    assert_eq!(
        Library::default().extend("mine", &json),
        Err(LibraryError::TooMany)
    );
}

#[test]
fn errors_name_positions_not_values() {
    let error = LibraryError::Source {
        index: 3,
        property: Property::YieldStrength,
        error: SourceError::EmptyClause,
    };
    assert_eq!(
        error.to_string(),
        "material #3: the source of fy is not valid: the source names no clause"
    );
    let error = LibraryError::Invalid {
        index: 2,
        error: MaterialError::EmptyName,
    };
    assert_eq!(
        error.to_string(),
        "material #2 is not valid: the material has no name"
    );
    assert_eq!(
        LibraryError::Syntax { line: 1, column: 2 }.to_string(),
        "the library source is not valid at line 1, column 2"
    );
    assert_eq!(
        LibraryError::Format(2).to_string(),
        "library format 2 is not supported; this version reads format 1"
    );
    let error = LibraryError::NotFinite {
        index: 4,
        property: Property::YoungModulus,
    };
    assert_eq!(
        error.to_string(),
        "material #4: the value of e_modulus is not a finite number"
    );
}

fn grades(prefix: &str) -> Vec<(&'static str, &'static Material)> {
    // Leak the library once per call: tests only, and it keeps the borrows simple.
    let library: &'static Library = Box::leak(Box::new(builtin()));
    library
        .iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .collect()
}

fn si(material: &Material, property: Property) -> f64 {
    material.property(property).unwrap().value
}

#[test]
fn the_builtin_library_holds_about_forty_grades_of_every_family() {
    let library = builtin();
    assert_eq!(library.len(), 37);
    for (prefix, count) in [
        ("en1992-1-1/C", 9),
        ("en1992-1-1/B", 3),
        ("en1993-1-1/", 5),
        ("en1995-1-1/", 10),
        ("en1996-1-1/", 3),
        ("bimcraft/", 7),
    ] {
        assert_eq!(grades(prefix).len(), count, "{prefix}");
    }
}

#[test]
fn every_builtin_value_cites_a_document_and_a_clause() {
    for (key, material) in builtin().iter() {
        assert!(material.properties().count() > 0, "{key}");
        for (property, value) in material.properties() {
            let (Some(document), Some(clause)) = (value.source.document(), value.source.clause())
            else {
                panic!("{key} {property:?}: the source cites nothing");
            };
            assert!(
                !document.is_empty() && !clause.is_empty(),
                "{key} {property:?}"
            );
            assert!(!value.source.is_user(), "{key} {property:?}");
        }
    }
}

// ENG-03: the derived concrete values follow the formulas of EN 1992-1-1 3.1.3 within the
// rounding of Table 3.1: fcm = fck + 8, fctm = 0.30 fck^(2/3), Ecm = 22 (fcm / 10)^0.3.
#[test]
fn concrete_values_follow_the_code_formulas() {
    let mpa = |material: &Material, property| si(material, property) / 1e6;
    let grades = grades("en1992-1-1/C");
    for (key, material) in grades {
        let fck = mpa(material, Property::CharacteristicCompressiveStrength);
        let fcm = mpa(material, Property::MeanCompressiveStrength);
        assert!((fcm - (fck + 8.0)).abs() < 1e-9, "{key} fcm");
        let tensile = 0.30 * libm::pow(fck, 2.0 / 3.0);
        assert!(
            (mpa(material, Property::MeanTensileStrength) - tensile).abs() <= 0.05,
            "{key} fctm"
        );
        let ecm = 22.0 * libm::pow(fcm / 10.0, 0.3);
        assert!(
            (si(material, Property::YoungModulus) / 1e9 - ecm).abs() <= 0.5,
            "{key} Ecm"
        );
    }
}

// ENG-03: G = E / (2 (1 + ν)) for steel (EN 1993-1-1 3.2.6(1)), about 81 GPa; reinforcement has
// the ductility of its class (EN 1992-1-1 Annex C): k 1.05, 1.08, 1.15 and εuk 2.5, 5, 7.5 %.
#[test]
fn steel_values_follow_the_code_formulas() {
    for (key, material) in grades("en1993-1-1/") {
        let e = si(material, Property::YoungModulus);
        let nu = si(material, Property::PoissonRatio);
        let g = si(material, Property::ShearModulus);
        assert!((g - e / (2.0 * (1.0 + nu))).abs() <= 0.5e9, "{key} G");
        assert!(si(material, Property::UltimateStrength) > si(material, Property::YieldStrength));
    }
    let library = builtin();
    for (key, k, eps) in [("A", 1.05, 0.025), ("B", 1.08, 0.05), ("C", 1.15, 0.075)] {
        let bar = library.get(&format!("en1992-1-1/B500{key}")).unwrap();
        assert!(
            (si(bar, Property::HardeningRatio) - k).abs() < 1e-12,
            "{key}"
        );
        assert!(
            (si(bar, Property::UltimateStrain) - eps).abs() < 1e-12,
            "{key}"
        );
    }
}

// ENG-03: solid timber follows EN 338: E0,05 = 2/3 E0,mean and G = E0,mean / 16, within the
// rounding of the table.
#[test]
fn solid_timber_values_follow_the_strength_class_relations() {
    for (key, material) in grades("en1995-1-1/C") {
        let e0 = si(material, Property::TimberMeanModulus);
        let e05 = si(material, Property::TimberFifthPercentileModulus);
        let g = si(material, Property::TimberMeanShearModulus);
        assert!((e05 / e0 - 2.0 / 3.0).abs() < 0.01, "{key} E0,05");
        assert!((g - e0 / 16.0).abs() <= 0.01 * (e0 / 16.0), "{key} G");
    }
    for (key, material) in grades("en1995-1-1/") {
        assert!(
            si(material, Property::TimberCompressiveStrength)
                >= si(material, Property::TimberTensileStrength),
            "{key}"
        );
        assert!(
            si(material, Property::MeanDensity) > si(material, Property::CharacteristicDensity)
        );
    }
}

// ENG-03: masonry fk = K fb^0.7 fm^0.3 with K = 0.55 (EN 1996-1-1 3.6.1.2(1), Table 3.3), and
// E = 1000 fk (3.7.2(2)).
#[test]
fn masonry_values_follow_the_code_formulas() {
    let library = builtin();
    for (name, fb, fm) in [
        ("Clay masonry fb10 M10", 10.0, 10.0),
        ("Clay masonry fb20 M10", 20.0, 10.0),
        ("Concrete block masonry fb10 M10", 10.0, 10.0),
    ] {
        let material = library.get(&format!("en1996-1-1/{name}")).unwrap();
        let fk = 0.55 * libm::pow(fb, 0.7) * libm::pow(fm, 0.3);
        let given = si(material, Property::MasonryCompressiveStrength) / 1e6;
        assert!((given - fk).abs() <= 0.05, "{name}");
        assert!(
            (si(material, Property::YoungModulus) - 1000.0 * given * 1e6).abs() < 1.0,
            "{name}"
        );
    }
}

#[test]
fn structural_grades_have_the_partial_factors_of_both_annexes_and_design_laws() {
    use crate::{NationalAnnex, annex::Factor};
    for (key, material) in builtin().iter() {
        let Some((factor, _)) = material.category().strength_divisor() else {
            continue;
        };
        if material.category() == Category::Masonry {
            continue;
        }
        for annex in NationalAnnex::ALL {
            let given = material
                .partial_factor(factor, annex)
                .unwrap_or_else(|_| panic!("{key} {annex:?}"));
            assert!(given.value >= 1.0, "{key}");
            assert!(material.design_law(annex).is_ok(), "{key} {annex:?}");
        }
    }
    // The values the codes recommend: γc 1.5 and αcc 1.0, γs 1.15, γM0 1.0, γM2 1.25, γM 1.25 / 1.3.
    let library = builtin();
    let c30 = library.get(C30).unwrap();
    assert!(
        (c30.partial_factor(Factor::GammaC, NationalAnnex::Recommended)
            .unwrap()
            .value
            - 1.5)
            .abs()
            < 1e-12
    );
    let s355 = library.get(S355).unwrap();
    assert!(
        (s355
            .partial_factor(Factor::GammaM2, NationalAnnex::France)
            .unwrap()
            .value
            - 1.25)
            .abs()
            < 1e-12
    );
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(crate::proptest_config())]

        // Whatever a user or plugin writes, loading answers and a failure adds nothing (ENG-02).
        #[test]
        fn arbitrary_text_never_panics_and_a_failure_adds_nothing(text in "\\PC{0,400}") {
            let mut library = builtin();
            let before = library.clone();
            if library.extend("mine", &text).is_err() {
                prop_assert_eq!(library, before);
            }
        }

        // Structured libraries with odd names and numbers: every material is either added whole
        // under its key or the whole source is refused.
        #[test]
        fn structured_sources_are_added_whole_or_refused_whole(
            names in proptest::collection::vec("[ -~]{0,40}", 0..6),
            value in -1e12f64..1e12,
        ) {
            let entries: Vec<String> = names
                .iter()
                .map(|name| format!(
                    r#"{{"name": {name:?}, "category": "generic", "behaviour_law": "linear_elastic",
                        "properties": {{"e_modulus": {{"value": {value}, "document": "D", "clause": "1"}}}}}}"#
                ))
                .collect();
            let json = format!(r#"{{"format": 1, "materials": [{}]}}"#, entries.join(","));
            let mut library = Library::default();
            match library.extend("mine", &json) {
                Ok(()) => {
                    prop_assert_eq!(library.len(), names.len());
                    for (key, material) in library.iter() {
                        prop_assert_eq!(material.library_key(), Some(key));
                    }
                }
                Err(_) => prop_assert!(library.is_empty()),
            }
        }
    }
}

#[test]
fn the_engineer_review_corrections_are_in_the_library() {
    let library = builtin();
    let near = |key: &str, property: Property, expected: f64| {
        assert!((si(library.get(key).unwrap(), property) - expected).abs() <= expected * 1e-12);
    };
    // EN 338:2016, Table 1 (the engineer's reading, MPa converted to Pa).
    near("en1995-1-1/C16", Property::TimberTensileStrength, 8.5e6);
    near("en1995-1-1/C27", Property::MeanDensity, 430.0);
    // Règles Th-Bât, Fascicule Matériaux (2017): conductivities and the removed entry.
    near(
        "bimcraft/Mineral wool",
        Property::ThermalConductivity,
        0.044,
    );
    near("bimcraft/Clay brick", Property::ThermalConductivity, 0.69);
    assert!(library.get("bimcraft/Wood-fibre board").is_none());
    let wool = library.get("bimcraft/Mineral wool").unwrap();
    let source = &wool.property(Property::ThermalConductivity).unwrap().source;
    assert_eq!(
        source.document(),
        Some("Règles Th-Bât, Fascicule Matériaux (2017)")
    );
    assert_eq!(source.clause(), Some("2.6.2.1"));
}
