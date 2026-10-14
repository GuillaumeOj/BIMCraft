//! Compiles the generated sample under the workspace lints, as a crate's `include!` would, and
//! checks how its types serialize and that their JSON matches the generated JSON Schema.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Stands in for the model's identifier, which `RustOptions::id_type` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityId(u64);

mod generated {
    include!("snapshots/sample.rs");
}

use generated::{
    Beam, BeamSection, Containment, Opening, PsetWallCommon, Recess, Storey, Voiding, Wall,
    WallType,
};

/// The sample's JSON Schema, parsed once.
static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("snapshots/sample.schema.json")).unwrap_or_default()
});

/// Whether `instance` is valid for the definition `name` of the sample's JSON Schema.
fn matches_schema(name: &str, instance: &Value) -> Result<bool, String> {
    let mut schema = SCHEMA.clone();
    schema["$ref"] = json!(format!("#/$defs/{name}"));
    let validator = jsonschema::validator_for(&schema).map_err(|error| error.to_string())?;
    Ok(validator.is_valid(instance))
}

/// Round-trips `value` through JSON, checking the JSON against the definition `name`.
fn round_trip<T>(name: &str, value: &T) -> Result<(), serde_json::Error>
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_value(value)?;
    assert_eq!(
        matches_schema(name, &json),
        Ok(true),
        "{json} against `{name}`"
    );
    assert_eq!(&serde_json::from_value::<T>(json)?, value);
    Ok(())
}

#[test]
fn absent_values_take_their_defaults() {
    let wall: Wall = serde_json::from_value(json!({})).unwrap();
    assert_eq!(
        wall,
        Wall {
            height: None,
            base_offset: 0.0,
            mark: None,
        }
    );
    let wall_type: WallType = serde_json::from_value(json!({ "thickness": 0.2 })).unwrap();
    assert_eq!(
        wall_type,
        WallType {
            thickness: 0.2,
            layer_names: Vec::new(),
            height: 2.5,
        }
    );
    let beam: Beam = serde_json::from_value(json!({ "span": 6.0 })).unwrap();
    assert_eq!(
        beam,
        Beam {
            span: 6.0,
            roll: 0.0,
            stations: vec![0.0, 1_250_000.5],
        }
    );
    let storey: Storey = serde_json::from_value(json!({ "name": "Level 1" })).unwrap();
    assert_eq!(
        storey,
        Storey {
            name: "Level 1".into(),
            elevation: 0.0,
            index: 1_000_000,
        }
    );
    let property_set: PsetWallCommon = serde_json::from_value(json!({})).unwrap();
    assert!(!property_set.load_bearing);
}

#[test]
fn required_values_and_unknown_keys_are_errors() {
    assert!(serde_json::from_value::<WallType>(json!({})).is_err());
    assert!(serde_json::from_value::<Opening>(json!({})).is_err());
    assert!(serde_json::from_value::<Wall>(json!({ "thickness": 0.2 })).is_err());
    assert!(serde_json::from_value::<Recess>(json!({ "depth": 0.1 })).is_err());
}

/// What the Rust types refuse to read, the JSON Schema rejects too.
#[test]
fn the_json_schema_rejects_what_the_types_reject() {
    let rejected = [
        ("WallType", json!({})),
        ("Opening", json!({})),
        ("Wall", json!({ "thickness": 0.2 })),
        ("Recess", json!({ "depth": 0.1 })),
        ("Wall", json!({ "base_offset": "low" })),
        ("Containment", json!({ "structure": 1, "elements": [-2] })),
        (
            "Storey",
            json!({ "name": "Roof", "index": 10_000_000_000_000_000_000_u64 }),
        ),
    ];
    for (name, instance) in rejected {
        assert_eq!(
            matches_schema(name, &instance),
            Ok(false),
            "{name}: {instance}"
        );
    }
    assert_eq!(matches_schema("Wall", &json!({})), Ok(true));
}

/// `null` reads as an absent option, in the Rust types and in the JSON Schema alike.
#[test]
fn null_options_read_as_absent() {
    let instance = json!({ "height": null, "mark": null });
    assert_eq!(matches_schema("Wall", &instance), Ok(true));
    let wall: Wall = serde_json::from_value(instance).unwrap();
    assert_eq!((wall.height, wall.mark), (None, None));
    // Lists and defaulted values are not options: both reject `null`.
    for (name, instance) in [
        ("WallType", json!({ "thickness": 0.2, "layer_names": null })),
        ("Wall", json!({ "base_offset": null })),
    ] {
        assert_eq!(
            matches_schema(name, &instance),
            Ok(false),
            "{name}: {instance}"
        );
    }
    assert!(
        serde_json::from_value::<WallType>(json!({ "thickness": 0.2, "layer_names": null }))
            .is_err()
    );
    assert!(serde_json::from_value::<Wall>(json!({ "base_offset": null })).is_err());
}

#[test]
fn absent_options_are_not_written() {
    let wall = Wall {
        height: None,
        base_offset: 0.1,
        mark: Some("W1".into()),
    };
    assert_eq!(
        serde_json::to_value(&wall).unwrap(),
        json!({ "base_offset": 0.1, "mark": "W1" })
    );
    let wall_type = WallType {
        thickness: 0.3,
        layer_names: Vec::new(),
        height: 3.0,
    };
    assert_eq!(
        serde_json::to_value(&wall_type).unwrap(),
        json!({ "thickness": 0.3, "height": 3.0 })
    );
}

#[test]
fn property_names_keep_their_ifc_spelling() {
    let property_set = PsetWallCommon {
        is_external: Some(true),
        load_bearing: true,
        fire_rating: Some("REI 60".into()),
        u_value: Some(0.25),
    };
    assert_eq!(
        serde_json::to_value(&property_set).unwrap(),
        json!({ "IsExternal": true, "LoadBearing": true, "FireRating": "REI 60", "UValue": 0.25 })
    );
    round_trip("Pset_WallCommon", &property_set).unwrap();
}

#[test]
fn every_type_round_trips() -> Result<(), serde_json::Error> {
    round_trip(
        "Wall",
        &Wall {
            height: Some(3.1),
            base_offset: -0.2,
            mark: Some("W2".into()),
        },
    )?;
    round_trip(
        "WallType",
        &WallType {
            thickness: 0.25,
            layer_names: vec!["Brick".into(), "Insulation".into()],
            height: 2.7,
        },
    )?;
    round_trip(
        "Beam",
        &Beam {
            span: 5.4,
            roll: 0.1,
            stations: vec![0.0, 2.7, 5.4],
        },
    )?;
    round_trip("BeamSection", &BeamSection { section_area: 0.02 })?;
    round_trip(
        "Storey",
        &Storey {
            name: "Ground floor".into(),
            elevation: 0.0,
            index: 0,
        },
    )?;
    round_trip("Opening", &Opening { depth: 0.3 })?;
    round_trip("Recess", &Recess {})?;
    round_trip(
        "Containment",
        &Containment {
            structure: EntityId(1),
            elements: vec![EntityId(2), EntityId(3)],
        },
    )?;
    round_trip(
        "Voiding",
        &Voiding {
            host: EntityId(2),
            opening: EntityId(4),
        },
    )?;
    Ok(())
}
