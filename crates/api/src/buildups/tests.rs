use core_txn::{ModelCommand, Origin};
use serde_json::{Value, json};

use super::*;
use crate::jsonrpc::codes;

fn model() -> Model {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    Model::new(&schema_gen::load_namespace(&dir).unwrap()).unwrap()
}

fn session() -> Session {
    Session::new(model())
}

fn call(session: &mut Session, name: &str, params: Value) -> Result<Outcome, ApiError> {
    Session::registry()
        .unwrap()
        .dispatch(session, name, params, &Origin::Ui)
}

fn ok(session: &mut Session, name: &str, params: Value) -> Value {
    call(session, name, params).unwrap().result
}

fn fails(result: Result<Outcome, ApiError>) -> BuildUpsError {
    match result {
        Err(ApiError::BuildUps(error)) => error,
        other => panic!("expected a build-up error, got {other:?}"),
    }
}

fn id(raw: u128) -> Value {
    json!(format!("{raw:032x}"))
}

const CONCRETE: u128 = 1;
const WOOL: u128 = 2;
const PLASTER: u128 = 3;
const STEEL: u128 = 4;
const BRICK: u128 = 5;
const WALL: u128 = 10;

/// A project with concrete, rock wool (which gives a conductivity), plaster and steel.
fn project() -> Session {
    let mut session = session();
    for (raw, key) in [
        (CONCRETE, "en1992-1-1/C30/37"),
        (WOOL, "bimcraft/Rock wool"),
        (PLASTER, "bimcraft/Gypsum plaster"),
        (STEEL, "en1993-1-1/S355"),
        (BRICK, "bimcraft/Clay brick"),
    ] {
        ok(
            &mut session,
            "materials.add_from_library",
            json!({ "id": id(raw), "key": key }),
        );
    }
    session
}

fn layer(material: u128, thickness: f64, function: &str) -> Value {
    json!({ "material": id(material), "thickness": thickness, "function": function })
}

fn wall_layers() -> Value {
    json!([
        layer(PLASTER, 0.02, "finish"),
        layer(WOOL, 0.12, "insulation"),
        layer(CONCRETE, 0.16, "structure"),
    ])
}

fn create(session: &mut Session, raw: u128, name: &str, layers: &Value) -> Outcome {
    let params = json!({ "id": id(raw), "name": name, "usage": "wall", "layers": layers });
    call(session, "buildups.create", params).unwrap()
}

fn refused(session: &mut Session, name: &str, params: Value) -> (&'static str, Option<usize>) {
    let error = fails(call(session, name, params));
    (error.reason, error.index)
}

#[test]
fn a_build_up_is_created_with_defaults_and_derived_layer_ids() {
    let mut session = project();
    let outcome = create(&mut session, WALL, "Exterior wall", &wall_layers());
    assert!(outcome.event.is_some());
    assert_eq!(outcome.result["id"], id(WALL));
    let layers = outcome.result["layers"].as_array().unwrap().clone();
    assert_eq!(layers.len(), 3);
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    assert_eq!(detail["name"], "Exterior wall");
    assert_eq!(detail["usage"], "wall");
    assert_eq!(detail["description"], Value::Null);
    let got: Vec<&Value> = detail["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| &l["id"])
        .collect();
    assert_eq!(got, layers.iter().collect::<Vec<_>>());
    // The defaults come from the function: finish 5, insulation 3, structure 1 and in the core.
    let defaults: Vec<(i64, bool)> = detail["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["priority"].as_i64().unwrap(),
                l["core"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(defaults, [(5, false), (3, false), (1, true)]);
    assert_eq!(detail["layers"][1]["material_name"], "Rock wool");
    assert!((detail["total_thickness"].as_f64().unwrap() - 0.30).abs() < 1e-12);
    // R: plaster 0.02 / 0.40 = 0.05, wool 0.12 / 0.044 = 2.7273, concrete has no λ in the library.
    let wool = detail["layers"][1]["thermal_resistance"].as_f64().unwrap();
    assert!((wool - 0.12 / 0.044).abs() < 1e-9);
    assert_eq!(detail["layers"][2]["thermal_resistance"], Value::Null);
    assert_eq!(detail["layers_without_thermal_data"], 1);
    assert!(
        (detail["thermal_resistance"].as_f64().unwrap() - (0.02 / 0.40 + 0.12 / 0.044)).abs()
            < 1e-9
    );
    // The same call on another session derives the same layers (CORE-11).
    let mut other = project();
    let again = create(&mut other, WALL, "Exterior wall", &wall_layers());
    assert_eq!(again.result["layers"].as_array().unwrap(), &layers);
}

#[test]
fn builds_up_are_listed_by_name_and_usage() {
    let mut session = project();
    create(&mut session, WALL, "Wall B", &wall_layers());
    let slab = json!({
        "id": id(11), "name": "a slab", "usage": "slab", "description": "Ground floor",
        "layers": [layer(CONCRETE, 0.2, "structure")],
    });
    ok(&mut session, "buildups.create", slab);
    let list = ok(&mut session, "buildups.list", json!({}));
    let names: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["a slab", "Wall B"]);
    assert_eq!(list["items"][1]["layer_count"], 3);
    let slabs = ok(&mut session, "buildups.list", json!({ "usage": "slab" }));
    assert_eq!(slabs["items"].as_array().unwrap().len(), 1);
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(11) }));
    assert_eq!(detail["description"], "Ground floor");
    assert_eq!(detail["thermal_resistance"], Value::Null);
    assert_eq!(detail["layers_without_thermal_data"], 1);
    assert_eq!(
        fails(call(
            &mut session,
            "buildups.get",
            json!({ "buildup": id(CONCRETE) })
        ))
        .reason,
        "not_found"
    );
}

#[test]
fn updating_keeps_the_layers_it_names_and_replaces_the_rest() {
    let mut session = project();
    let created = create(&mut session, WALL, "Wall", &wall_layers());
    let ids = created.result["layers"].as_array().unwrap().clone();
    let history = session.document().history().count();
    // Keep the structure (resized, renamed), drop the others, add an air gap before it.
    let update = json!({
        "buildup": id(WALL), "name": "Wall 2", "description": "Ventilated", "usage": "slab",
        "layers": [
            { "material": id(BRICK), "thickness": 0.025, "function": "air_gap", "priority": 2 },
            { "id": ids[2], "material": id(CONCRETE), "thickness": 0.2, "function": "structure",
              "name": "Core" },
        ],
    });
    let outcome = call(&mut session, "buildups.update", update).unwrap();
    assert_eq!(session.document().history().count(), history + 1);
    let layers = outcome.result["layers"].as_array().unwrap().clone();
    assert_eq!(layers[1], ids[2]);
    assert_ne!(layers[0], ids[0]);
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    assert_eq!(detail["name"], "Wall 2");
    assert_eq!(detail["usage"], "slab");
    assert_eq!(detail["description"], "Ventilated");
    assert_eq!(detail["layers"][1]["name"], "Core");
    assert_eq!(detail["layers"][0]["priority"], 2);
    assert!(!session.document().model().contains(EntityId::new(
        u128::from_str_radix(ids[0].as_str().unwrap(), 16).unwrap()
    )));
    // Null clears the description, and leaving everything out changes nothing but is still valid.
    ok(
        &mut session,
        "buildups.update",
        json!({ "buildup": id(WALL), "description": null }),
    );
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    assert_eq!(detail["description"], Value::Null);
    assert_eq!(detail["layers"].as_array().unwrap().len(), 2);
    session.document_mut().undo().unwrap();
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    assert_eq!(detail["description"], "Ventilated");
}

#[test]
fn a_build_up_is_duplicated_and_deleted_with_its_layers_in_one_step() {
    let mut session = project();
    create(&mut session, WALL, "Wall", &wall_layers());
    let copy = call(
        &mut session,
        "buildups.duplicate",
        json!({ "buildup": id(WALL), "id": id(11), "name": "Wall copy" }),
    )
    .unwrap();
    assert_eq!(copy.result["id"], id(11));
    let original = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    let duplicate = ok(&mut session, "buildups.get", json!({ "buildup": id(11) }));
    assert_eq!(
        original["layers"].as_array().unwrap().len(),
        duplicate["layers"].as_array().unwrap().len()
    );
    for (a, b) in original["layers"]
        .as_array()
        .unwrap()
        .iter()
        .zip(duplicate["layers"].as_array().unwrap())
    {
        assert_ne!(a["id"], b["id"]);
        assert_eq!(a["material"], b["material"]);
    }
    let before = session.document().model().len();
    let history = session.document().history().count();
    let outcome = call(
        &mut session,
        "buildups.delete",
        json!({ "buildup": id(WALL) }),
    )
    .unwrap();
    assert!(outcome.event.is_some());
    assert_eq!(session.document().model().len(), before - 4);
    assert_eq!(session.document().history().count(), history + 1);
    session.document_mut().undo().unwrap();
    assert_eq!(session.document().model().len(), before);
    assert_eq!(
        fails(call(
            &mut session,
            "buildups.delete",
            json!({ "buildup": id(STEEL) })
        ))
        .reason,
        "not_found"
    );
    assert_eq!(
        fails(call(
            &mut session,
            "buildups.duplicate",
            json!({ "buildup": id(WALL), "id": id(12), "name": "wall COPY" })
        ))
        .reason,
        "duplicate_name"
    );
}

#[test]
fn a_material_a_layer_uses_cannot_be_deleted() {
    let mut session = project();
    create(&mut session, WALL, "Wall", &wall_layers());
    let used = ok(
        &mut session,
        "materials.get",
        json!({ "material": id(CONCRETE) }),
    );
    assert_eq!(used["used_by"], 1);
    let result = call(
        &mut session,
        "materials.delete",
        json!({ "material": id(CONCRETE) }),
    );
    assert!(matches!(
        result,
        Err(ApiError::Materials(crate::MaterialsError::InUse {
            used_by: 1
        }))
    ));
    ok(
        &mut session,
        "buildups.delete",
        json!({ "buildup": id(WALL) }),
    );
    ok(
        &mut session,
        "materials.delete",
        json!({ "material": id(CONCRETE) }),
    );
}

#[test]
fn problems_are_refused_with_a_reason_and_the_layer_at_fault() {
    let mut session = project();
    create(&mut session, WALL, "Wall", &wall_layers());
    let base =
        |layers: Value| json!({ "id": id(20), "name": "New", "usage": "wall", "layers": layers });
    let layer_at = |change: &dyn Fn(&mut Value)| {
        let mut layers = wall_layers();
        change(&mut layers[1]);
        base(layers)
    };
    let mut check = |params: Value| refused(&mut session, "buildups.create", params);
    assert_eq!(check(base(json!([]))), ("no_layers", None));
    assert_eq!(
        check(layer_at(&|l| l["thickness"] = json!(0.0))),
        ("invalid_thickness", Some(1))
    );
    assert_eq!(
        check(layer_at(&|l| l["priority"] = json!(6))),
        ("invalid_priority", Some(1))
    );
    assert_eq!(
        check(layer_at(&|l| l["priority"] = json!(99999))),
        ("invalid_priority", Some(1))
    );
    assert_eq!(
        check(layer_at(&|l| l["material"] = id(WALL))),
        ("unknown_material", Some(1))
    );
    let apart = {
        let mut layers = wall_layers();
        layers[0]["core"] = json!(true);
        base(layers)
    };
    assert_eq!(check(apart), ("core_not_contiguous", None));
    assert_eq!(
        check(layer_at(&|l| l["id"] = id(77))),
        ("unknown_layer", Some(1))
    );
    assert_eq!(
        check(layer_at(&|l| {
            l["function"] = json!("air_gap");
            l["material"] = id(STEEL);
        })),
        ("invalid_material", Some(1))
    );
    let mut named = base(wall_layers());
    named["name"] = json!("  ");
    assert_eq!(check(named.clone()), ("invalid_name", None));
    named["name"] = json!("wall");
    assert_eq!(check(named), ("duplicate_name", None));
    let many: Vec<Value> = (0..=MAX_LAYERS)
        .map(|_| layer(CONCRETE, 0.01, "finish"))
        .collect();
    assert_eq!(check(base(json!(many))), ("too_many_layers", None));
    let huge = [
        layer(CONCRETE, 4000.0, "finish"),
        layer(CONCRETE, 4000.0, "finish"),
        layer(CONCRETE, 4000.0, "finish"),
    ];
    assert_eq!(check(base(json!(huge))), ("too_thick", None));
    assert!(!session.document().model().contains(EntityId::new(20)));
    // A refused update changes nothing.
    let before = session.document().model().clone();
    let update = json!({ "buildup": id(WALL), "layers": [layer(CONCRETE, -1.0, "structure")] });
    assert_eq!(
        refused(&mut session, "buildups.update", update),
        ("invalid_thickness", Some(0))
    );
    assert_eq!(session.document().model(), &before);
    assert_eq!(
        refused(
            &mut session,
            "buildups.update",
            json!({ "buildup": id(CONCRETE) })
        ),
        ("not_found", None)
    );
    // Two layers with the same existing id.
    let layers = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }))["layers"].clone();
    let same = layers[0]["id"].clone();
    let twice = json!({ "buildup": id(WALL), "layers": [
        { "id": same, "material": id(PLASTER), "thickness": 0.02, "function": "finish" },
        { "id": same, "material": id(PLASTER), "thickness": 0.02, "function": "finish" },
    ]});
    assert_eq!(
        refused(&mut session, "buildups.update", twice),
        ("duplicate_layer", Some(1))
    );
}

#[test]
fn a_membrane_without_thickness_is_valid_and_other_layers_are_not() {
    let mut session = project();
    let layers = json!([
        layer(PLASTER, 0.02, "finish"),
        layer(PLASTER, 0.0, "membrane"),
        layer(CONCRETE, 0.2, "structure"),
    ]);
    create(&mut session, WALL, "With membrane", &layers);
    let detail = ok(&mut session, "buildups.get", json!({ "buildup": id(WALL) }));
    assert_eq!(detail["layers"][1]["thickness"], 0.0);
    assert_eq!(detail["layers"][1]["priority"], 4);
}

#[test]
fn a_preview_returns_the_totals_and_every_error_without_saving() {
    let mut session = project();
    let history = session.document().history().count();
    let good = ok(
        &mut session,
        "buildups.preview",
        json!({ "usage": "wall", "layers": wall_layers() }),
    );
    assert_eq!(good["errors"], json!([]));
    assert_eq!(
        good["layers"][0],
        json!({ "priority": 5, "core": false, "thermal_resistance": 0.02 / 0.40 })
    );
    assert_eq!(good["layers"][2]["thermal_resistance"], Value::Null);
    assert!((good["total_thickness"].as_f64().unwrap() - 0.30).abs() < 1e-12);
    assert_eq!(good["layers_without_thermal_data"], 1);
    assert!(
        (good["thermal_resistance"].as_f64().unwrap() - (0.02 / 0.40 + 0.12 / 0.044)).abs() < 1e-9
    );

    // Every problem at once, each with its layer, and a partial total all the same.
    let bad = json!([
        layer(CONCRETE, 0.0, "structure"),
        { "material": id(WALL), "thickness": 0.1, "function": "finish", "priority": 9 },
        { "material": id(STEEL), "thickness": 0.05, "function": "air_gap" },
        layer(WOOL, 0.1, "insulation"),
        layer(CONCRETE, 0.1, "structure"),
    ]);
    let preview = ok(
        &mut session,
        "buildups.preview",
        json!({ "usage": "slab", "layers": bad }),
    );
    let errors: Vec<(&str, Option<u64>)> = preview["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["code"].as_str().unwrap(),
                e.get("index").and_then(Value::as_u64),
            )
        })
        .collect();
    assert_eq!(
        errors,
        [
            ("invalid_thickness", Some(0)),
            ("invalid_priority", Some(1)),
            ("invalid_material", Some(2)),
            ("core_not_contiguous", None),
            ("unknown_material", Some(1)),
        ]
    );
    assert_eq!(preview["layers"].as_array().unwrap().len(), 5);
    // The same id twice is a clash, and a preview of too many layers says so.
    let same =
        json!({ "id": id(50), "material": id(CONCRETE), "thickness": 0.1, "function": "finish" });
    let twice = ok(
        &mut session,
        "buildups.preview",
        json!({ "usage": "wall", "layers": [same.clone(), same] }),
    );
    assert_eq!(
        twice["errors"],
        json!([{ "code": "duplicate_layer", "index": 1 }])
    );
    let many: Vec<Value> = (0..=MAX_LAYERS)
        .map(|_| layer(CONCRETE, 0.01, "finish"))
        .collect();
    let many = ok(
        &mut session,
        "buildups.preview",
        json!({ "usage": "wall", "layers": many }),
    );
    assert_eq!(many["errors"][0]["code"], "too_many_layers");
    assert_eq!(session.document().history().count(), history);
}

#[test]
fn a_build_up_this_version_cannot_read_does_not_break_the_list() {
    let mut session = project();
    create(&mut session, WALL, "Wall", &wall_layers());
    // A build-up whose layers list names something that is not a layer.
    let broken = ModelCommand::Create {
        id: EntityId::new(60),
        entity: "MaterialLayerSet".into(),
        values: vec![
            (
                "name".into(),
                core_model::AttributeValue::Text("Broken".into()),
            ),
            (
                "usage".into(),
                core_model::AttributeValue::Text("wall".into()),
            ),
            (
                "layers".into(),
                core_model::AttributeValue::List(vec![core_model::AttributeValue::Ref(
                    EntityId::new(CONCRETE),
                )]),
            ),
        ],
    };
    session.document_mut().execute(broken, Origin::Ui).unwrap();
    let list = ok(&mut session, "buildups.list", json!({}));
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert!(matches!(
        call(&mut session, "buildups.get", json!({ "buildup": id(60) })),
        Err(ApiError::Internal)
    ));
}

#[test]
fn errors_carry_a_reason_and_the_layer_and_no_values() {
    let error = BuildUpsError {
        reason: "invalid_thickness",
        index: Some(2),
    };
    let response = crate::jsonrpc::error_response(&json!(1), &ApiError::BuildUps(error));
    assert_eq!(response["error"]["code"], codes::REJECTED);
    assert_eq!(
        response["error"]["data"],
        json!({ "reason": "invalid_thickness", "index": 2 })
    );
    let plain = crate::jsonrpc::error_response(
        &json!(1),
        &ApiError::BuildUps(BuildUpsError {
            reason: "no_layers",
            index: None,
        }),
    );
    assert_eq!(plain["error"]["data"], json!({ "reason": "no_layers" }));
    assert_eq!(plain["error"]["message"], "the build-up call was refused");
    let all = [
        LayerSetError::EmptyName,
        LayerSetError::TooLong,
        LayerSetError::ControlCharacter,
        LayerSetError::DuplicateName,
        LayerSetError::NoLayers,
        LayerSetError::TooManyLayers,
        LayerSetError::InvalidThickness(1),
        LayerSetError::InvalidPriority(1),
        LayerSetError::DuplicateLayer(1),
        LayerSetError::CoreNotContiguous,
        LayerSetError::TooThick,
        LayerSetError::UnknownMaterial(1),
        LayerSetError::InvalidMaterial(1),
    ];
    for problem in all {
        let mapped = BuildUpsError::from(problem);
        assert_ne!(mapped.reason, "");
        assert_eq!(
            mapped.index.is_some(),
            mapped.reason.ends_with("_thickness")
                || mapped.reason.ends_with("_priority")
                || mapped.reason.ends_with("_layer")
                || mapped.reason.ends_with("_material")
        );
    }
}

#[test]
fn a_preview_of_far_too_many_layers_is_refused_as_a_bad_request() {
    let mut session = project();
    let many: Vec<Value> = (0..MAX_LAYERS + 2)
        .map(|_| layer(CONCRETE, 0.01, "finish"))
        .collect();
    let result = call(
        &mut session,
        "buildups.preview",
        json!({ "usage": "wall", "layers": many }),
    );
    assert!(matches!(result, Err(ApiError::InvalidParams { .. })));
}
