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
    assert_close(mpa(S355, Property::UltimateStrength), 490.0);
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
