use core_model::AttributeValue;
use core_txn::Origin;
use schema::{Attribute, EntityKind, Placement, Presence, ValueType};
use serde_json::{Value, json};

use super::*;
use crate::jsonrpc::codes;

/// The core schema plus a `Layer` that refers to a material, like the build-up layers to come.
fn model() -> Model {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    let mut namespace = schema_gen::load_namespace(&dir).unwrap();
    let mut layer = namespace.entities["MaterialOriginal"].clone();
    layer.name = "Layer".into();
    layer.kind = EntityKind::Resource;
    layer.ifc.as_mut().unwrap().entity = "IfcBuildingElementProxy".into();
    layer.attributes = vec![Attribute {
        name: "material".into(),
        value: ValueType::Ref("Material".into()),
        presence: Presence::Required,
        placement: Placement::Instance,
        doc: None,
        sourced: false,
    }];
    namespace.entities.insert("Layer".into(), layer);
    Model::new(&namespace).unwrap()
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

fn fails(result: Result<Outcome, ApiError>) -> MaterialsError {
    match result {
        Err(ApiError::Materials(error)) => error,
        other => panic!("expected a materials error, got {other:?}"),
    }
}

fn id(raw: u128) -> Value {
    json!(format!("{raw:032x}"))
}

const C30: &str = "en1992-1-1/C30/37";
const S355: &str = "en1993-1-1/S355";

fn add(session: &mut Session, raw: u128, key: &str, name: Option<&str>) -> Outcome {
    let mut params = json!({ "id": id(raw), "key": key });
    if let Some(name) = name {
        params["name"] = json!(name);
    }
    call(session, "materials.add_from_library", params).unwrap()
}

fn property<'a>(detail: &'a Value, key: &str, annex: Option<&str>) -> &'a Value {
    detail["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| {
            property["key"] == key && property.get("annex").and_then(Value::as_str) == annex
        })
        .unwrap_or_else(|| panic!("no property {key}"))
}

#[test]
fn the_library_lists_filters_and_pages_its_grades() {
    let mut session = session();
    let all = ok(&mut session, "materials.library", json!({}));
    let total = all["total"].as_u64().unwrap();
    assert!(total >= 35);
    assert_eq!(all["items"].as_array().unwrap().len() as u64, total);
    let steel = ok(
        &mut session,
        "materials.library",
        json!({ "category": "steel" }),
    );
    assert_eq!(steel["total"], 5);
    let searched = ok(
        &mut session,
        "materials.library",
        json!({ "search": " c30 " }),
    );
    let keys: Vec<&str> = searched["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&C30), "{keys:?}");
    assert!(keys.iter().all(|key| key.to_lowercase().contains("c30")));
    let page = ok(
        &mut session,
        "materials.library",
        json!({ "offset": 2, "limit": 3 }),
    );
    assert_eq!(page["items"].as_array().unwrap().len(), 3);
    assert_eq!(page["total"], all["total"]);
    assert_eq!(page["items"][0], all["items"][2]);
    let item = &steel["items"][0];
    assert_eq!(item["design_code"], "en1993-1-1");
    let finish = ok(
        &mut session,
        "materials.library",
        json!({ "category": "finish" }),
    );
    assert_eq!(finish["items"][0]["design_code"], Value::Null);
    assert!(call(&mut session, "materials.library", json!({ "limit": 0 })).is_err());
    assert!(
        call(
            &mut session,
            "materials.library",
            json!({ "category": "wood" })
        )
        .is_err()
    );
}

#[test]
fn a_grade_gives_its_law_and_sourced_values() {
    let mut session = session();
    let grade = ok(&mut session, "materials.grade", json!({ "key": C30 }));
    assert_eq!(grade["name"], "C30/37");
    assert_eq!(grade["category"], "concrete");
    assert_eq!(grade["design_code"], "en1992-1-1");
    assert_eq!(grade["law"]["kind"], "parabola_rectangle");
    assert_eq!(grade["law"]["source"]["clause"], "3.1.7(1)");
    let fck = property(&grade, "fck", None);
    assert_eq!(fck["value"], 30e6);
    assert_eq!(fck["library_value"], 30e6);
    assert_eq!(fck["quantity"], "stress");
    assert_eq!(fck["required"], true);
    assert_eq!(fck["source"]["document"], "EN 1992-1-1:2004");
    let nu = property(&grade, "poisson_ratio", None);
    assert_eq!(nu["range"]["max"], 0.5);
    assert_eq!(nu["range"]["max_inclusive"], false);
    assert_eq!(nu["range"]["min_inclusive"], true);
    // Partial factors come once per annex, and a value the grade lacks is null.
    let gamma = property(&grade, "gamma_c", Some("fr"));
    assert_eq!(gamma["value"], 1.5);
    assert_eq!(gamma["source"]["document"], "NF EN 1992-1-1/NA:2016");
    let none = property(&grade, "thermal_conductivity", None);
    assert_eq!(
        (&none["value"], &none["source"]),
        (&Value::Null, &Value::Null)
    );
    assert_eq!(
        fails(call(
            &mut session,
            "materials.grade",
            json!({ "key": "nope" })
        )),
        MaterialsError::UnknownGrade
    );
}

#[test]
fn laws_are_named_for_the_wire() {
    let mut session = session();
    let kind = |session: &mut Session, key: &str| {
        let grade = ok(session, "materials.grade", json!({ "key": key }));
        (grade["law"]["kind"].clone(), grade["law"]["branch"].clone())
    };
    assert_eq!(
        kind(&mut session, S355),
        (json!("elastic_plastic"), json!("horizontal"))
    );
    assert_eq!(
        kind(&mut session, "en1992-1-1/B500B"),
        (json!("bilinear"), json!("inclined"))
    );
    assert_eq!(
        kind(&mut session, "en1995-1-1/C24"),
        (json!("elastic_brittle"), Value::Null)
    );
    assert_eq!(
        kind(&mut session, "bimcraft/Mineral wool"),
        (json!("none"), Value::Null)
    );
    assert_eq!(
        kind(&mut session, "en1996-1-1/Clay masonry fb10 M10"),
        (json!("linear_elastic"), Value::Null)
    );
    // The bi-linear concrete law has no library grade; it comes from a custom material.
    let mut custom = Session::new(model());
    let created = json!({
        "id": id(1), "name": "Mix", "category": "concrete",
        "properties": [
            { "key": "fck", "value": 30e6 }, { "key": "eps_c3", "value": 0.00175 },
            { "key": "eps_cu3", "value": 0.0035 },
        ],
    });
    ok(&mut custom, "materials.create", created);
    let detail = ok(&mut custom, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["law"]["kind"], "bilinear");
    assert_eq!(detail["law"]["source"]["clause"], "3.1.7(2)");
}

#[test]
fn a_grade_is_added_with_its_values_and_can_be_listed_and_read() {
    let mut session = session();
    let outcome = add(&mut session, 1, C30, None);
    assert_eq!(outcome.result, json!({ "id": id(1) }));
    assert!(outcome.event.is_some());
    add(&mut session, 2, S355, Some("Beam steel"));
    let list = ok(&mut session, "materials.list", json!({}));
    let names: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Beam steel", "C30/37"]);
    assert_eq!(list["items"][1]["grade"], C30);
    assert_eq!(list["items"][1]["used_by"], 0);
    let steel_only = ok(
        &mut session,
        "materials.list",
        json!({ "category": "steel" }),
    );
    assert_eq!(steel_only["items"].as_array().unwrap().len(), 1);
    let searched = ok(&mut session, "materials.list", json!({ "search": "BEAM" }));
    assert_eq!(searched["items"][0]["id"], id(2));
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["grade"], C30);
    assert_eq!(detail["edited"], false);
    let fck = property(&detail, "fck", None);
    assert_eq!(
        (&fck["value"], &fck["library_value"]),
        (&json!(30e6), &json!(30e6))
    );
    assert_eq!(
        fails(call(
            &mut session,
            "materials.get",
            json!({ "material": id(9) })
        )),
        MaterialsError::NotFound
    );
}

#[test]
fn adding_checks_the_key_and_the_name() {
    let mut session = session();
    add(&mut session, 1, C30, None);
    let attempt = |session: &mut Session, raw, key: &str, name: Option<&str>| {
        let mut params = json!({ "id": id(raw), "key": key });
        if let Some(name) = name {
            params["name"] = json!(name);
        }
        fails(call(session, "materials.add_from_library", params))
    };
    assert_eq!(
        attempt(&mut session, 2, "nope", None),
        MaterialsError::UnknownGrade
    );
    assert_eq!(
        attempt(&mut session, 2, C30, None),
        MaterialsError::DuplicateName
    );
    assert_eq!(
        attempt(&mut session, 2, S355, Some(" c30/37 ")),
        MaterialsError::DuplicateName
    );
    assert_eq!(
        attempt(&mut session, 2, S355, Some("  ")),
        MaterialsError::InvalidName
    );
    assert_eq!(
        attempt(&mut session, 2, S355, Some("A\nB")),
        MaterialsError::InvalidName
    );
    let long = "x".repeat(MAX_NAME_LEN + 1);
    assert_eq!(
        attempt(&mut session, 2, S355, Some(&long)),
        MaterialsError::InvalidName
    );
    assert!(!session.document().model().contains(EntityId::new(2)));
}

#[test]
fn a_custom_material_takes_the_user_as_the_source_of_every_value() {
    let mut session = session();
    let params = json!({
        "id": id(1), "name": "Site mix", "category": "concrete",
        "properties": [
            { "key": "fck", "value": 25e6 }, { "key": "eps_c2", "value": 0.002 },
            { "key": "eps_cu2", "value": 0.0035 }, { "key": "n", "value": 2.0 },
            { "key": "gamma_c", "annex": "fr", "value": 1.5 },
        ],
    });
    assert_eq!(
        ok(&mut session, "materials.create", params),
        json!({ "id": id(1) })
    );
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["grade"], Value::Null);
    assert_eq!(detail["law"]["kind"], "parabola_rectangle");
    let fck = property(&detail, "fck", None);
    assert_eq!(fck["source"], json!({ "entered_by_user": true }));
    assert_eq!(fck["library_value"], Value::Null);
    assert_eq!(property(&detail, "gamma_c", Some("fr"))["value"], 1.5);
    assert_eq!(
        property(&detail, "gamma_c", Some("recommended"))["value"],
        Value::Null
    );
    // Without strains a concrete material falls back to linear elasticity, if it has a modulus.
    let elastic = json!({
        "id": id(2), "name": "Elastic", "category": "concrete",
        "properties": [{ "key": "fck", "value": 25e6 }, { "key": "e_modulus", "value": 30e9 }],
    });
    ok(&mut session, "materials.create", elastic);
    let detail = ok(&mut session, "materials.get", json!({ "material": id(2) }));
    assert_eq!(detail["law"]["kind"], "linear_elastic");
    // Non-structural materials need nothing.
    let finish = json!({ "id": id(3), "name": "Plaster", "category": "finish" });
    ok(&mut session, "materials.create", finish);
    assert_eq!(
        ok(&mut session, "materials.get", json!({ "material": id(3) }))["law"]["kind"],
        "none"
    );
}

fn create(session: &mut Session, name: &str, category: &str, properties: Value) -> MaterialsError {
    let mut params = json!({ "id": id(1), "name": name, "category": category });
    params["properties"] = properties;
    fails(call(session, "materials.create", params))
}

fn missing(key: &str) -> MaterialsError {
    MaterialsError::RequiredProperty { key: key.into() }
}

#[test]
fn creating_reports_what_is_missing_unknown_or_out_of_range() {
    let mut session = session();
    assert_eq!(
        create(&mut session, "A", "concrete", json!([])),
        missing("fck")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "steel",
            json!([{ "key": "fy", "value": 355e6 }])
        ),
        missing("e_modulus")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "finish",
            json!([{ "key": "colour", "value": 1 }])
        ),
        MaterialsError::UnknownProperty {
            key: "colour".into()
        }
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "finish",
            json!([{ "key": "fy", "value": 1 }])
        ),
        MaterialsError::UnknownProperty { key: "fy".into() }
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "finish",
            json!([{ "key": "density", "value": -1 }])
        ),
        invalid("density")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "finish",
            json!([{ "key": "poisson_ratio", "value": 0.5 }])
        ),
        invalid("poisson_ratio")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "finish",
            json!([{ "key": "density", "value": null }])
        ),
        invalid("density")
    );
}

#[test]
fn creating_checks_annexes_names_and_repeats() {
    let mut session = session();
    assert_eq!(
        create(
            &mut session,
            "A",
            "concrete",
            json!([{ "key": "gamma_c", "value": 1.5 }])
        ),
        invalid("gamma_c")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "concrete",
            json!([{ "key": "fck", "annex": "fr", "value": 1 }])
        ),
        invalid("fck")
    );
    assert_eq!(
        create(
            &mut session,
            "A",
            "concrete",
            json!([{ "key": "gamma_c", "annex": "de", "value": 1.5 }])
        ),
        invalid("gamma_c")
    );
    let twice = json!([{ "key": "density", "value": 1 }, { "key": "density", "value": 2 }]);
    assert_eq!(
        create(&mut session, "A", "finish", twice),
        invalid("density")
    );
    assert_eq!(
        create(&mut session, " ", "finish", json!([])),
        MaterialsError::InvalidName
    );
    assert!(!session.document().model().contains(EntityId::new(1)));
    assert!(session.document().history().next().is_none());
}

#[test]
fn updating_renames_edits_and_resets_in_one_undo_step() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    let history = session.document().history().count();
    let update = json!({
        "material": id(1), "name": "Beam steel",
        "properties": [
            { "key": "fy", "value": 345e6 }, { "key": "thermal_conductivity", "value": 50.0 },
            { "key": "gamma_m0", "annex": "fr", "value": 1.1 },
        ],
    });
    let outcome = call(&mut session, "materials.update", update).unwrap();
    assert_eq!(outcome.result, json!({}));
    assert!(outcome.event.is_some());
    assert_eq!(session.document().history().count(), history + 1);
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["name"], "Beam steel");
    assert_eq!(detail["edited"], true);
    let fy = property(&detail, "fy", None);
    assert_eq!(fy["value"], 345e6);
    assert_eq!(fy["source"], json!({ "entered_by_user": true }));
    // The library value stays to restore; the conductivity the library never gave has none.
    assert_eq!(fy["library_value"], 355e6);
    assert_eq!(
        property(&detail, "thermal_conductivity", None)["library_value"],
        Value::Null
    );
    assert_eq!(
        property(&detail, "gamma_m0", Some("fr"))["library_value"],
        1.0
    );

    // Null resets: the library value and source come back, the extra value goes.
    let reset = json!({
        "material": id(1),
        "properties": [
            { "key": "fy", "value": null }, { "key": "thermal_conductivity", "value": null },
            { "key": "gamma_m0", "annex": "fr", "value": null },
        ],
    });
    ok(&mut session, "materials.update", reset);
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["edited"], false);
    let fy = property(&detail, "fy", None);
    assert_eq!(fy["value"], 355e6);
    assert_eq!(fy["source"]["clause"], "Table 3.1");
    assert_eq!(
        property(&detail, "thermal_conductivity", None)["value"],
        Value::Null
    );
    session.document_mut().undo().unwrap();
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(detail["edited"], true);
}

#[test]
fn an_update_that_changes_nothing_leaves_no_history_entry() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    let history = session.document().history().count();
    for params in [
        json!({ "material": id(1) }),
        json!({ "material": id(1), "name": "S355" }),
        json!({ "material": id(1), "properties": [{ "key": "fy", "value": null }] }),
    ] {
        let outcome = call(&mut session, "materials.update", params).unwrap();
        assert_eq!(outcome.event, None);
    }
    assert_eq!(session.document().history().count(), history);
}

/// `params` as an update of material 1.
fn update(session: &mut Session, mut params: Value) -> MaterialsError {
    params["material"] = id(1);
    fails(call(session, "materials.update", params))
}

fn invalid(key: &str) -> MaterialsError {
    MaterialsError::InvalidValue {
        key: Some(key.into()),
    }
}

#[test]
fn values_that_belong_together_are_checked_together() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    // fy above fu is refused alone, accepted with a higher fu.
    let alone = json!({ "properties": [{ "key": "fy", "value": 600e6 }] });
    assert_eq!(update(&mut session, alone), invalid("fu"));
    let both = json!({
        "material": id(1),
        "properties": [{ "key": "fy", "value": 600e6 }, { "key": "fu", "value": 700e6 }],
    });
    ok(&mut session, "materials.update", both);
    let detail = ok(&mut session, "materials.get", json!({ "material": id(1) }));
    assert_eq!(property(&detail, "fu", None)["value"], 700e6);
}

#[test]
fn a_refused_update_changes_nothing() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    add(&mut session, 2, C30, None);
    let before = session.document().model().clone();
    let history = session.document().history().count();
    let renamed = json!({ "name": "C30/37", "properties": [{ "key": "fy", "value": 1 }] });
    assert_eq!(update(&mut session, renamed), MaterialsError::DuplicateName);
    let foreign = json!({ "properties": [{ "key": "fck", "value": 1 }] });
    assert_eq!(
        update(&mut session, foreign),
        MaterialsError::UnknownProperty { key: "fck".into() }
    );
    let negative = json!({ "properties": [{ "key": "fy", "value": -1 }] });
    assert_eq!(update(&mut session, negative), invalid("fy"));
    let zero = json!({ "properties": [{ "key": "e_modulus", "value": 0 }] });
    assert_eq!(update(&mut session, zero), invalid("e_modulus"));
    let twice = json!({
        "properties": [{ "key": "fy", "value": 400e6 }, { "key": "fy", "value": null }],
    });
    assert_eq!(update(&mut session, twice), invalid("fy"));
    assert_eq!(
        update(&mut session, json!({ "name": "" })),
        MaterialsError::InvalidName
    );
    assert_eq!(session.document().model(), &before);
    assert_eq!(session.document().history().count(), history);
    assert_eq!(
        fails(call(
            &mut session,
            "materials.update",
            json!({ "material": id(9) })
        )),
        MaterialsError::NotFound
    );
}

#[test]
fn original_ids_derive_from_the_material_and_the_property() {
    let a = original_id(EntityId::new(1), Property::YieldStrength);
    assert_eq!(a, original_id(EntityId::new(1), Property::YieldStrength));
    assert_ne!(a, original_id(EntityId::new(2), Property::YieldStrength));
    assert_ne!(a, original_id(EntityId::new(1), Property::UltimateStrength));
    // Replaying an update on another session gives the same objects.
    let run = || {
        let mut session = session();
        add(&mut session, 1, S355, None);
        let update = json!({ "material": id(1), "properties": [{ "key": "fy", "value": 345e6 }] });
        ok(&mut session, "materials.update", update);
        session
    };
    let (first, second) = (run(), run());
    assert_eq!(first.document().model(), second.document().model());
}

#[test]
fn a_material_that_other_objects_use_cannot_be_deleted() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    let update = json!({ "material": id(1), "properties": [{ "key": "fy", "value": 345e6 }] });
    ok(&mut session, "materials.update", update);
    // The original that the edit kept does not count as a user.
    assert_eq!(
        ok(&mut session, "materials.list", json!({}))["items"][0]["used_by"],
        0
    );
    let layer = ModelCommand::Create {
        id: EntityId::new(50),
        entity: "Layer".into(),
        values: vec![("material".into(), AttributeValue::Ref(EntityId::new(1)))],
    };
    session.document_mut().execute(layer, Origin::Ui).unwrap();
    assert_eq!(
        ok(&mut session, "materials.get", json!({ "material": id(1) }))["used_by"],
        1
    );
    let result = call(
        &mut session,
        "materials.delete",
        json!({ "material": id(1) }),
    );
    assert_eq!(fails(result), MaterialsError::InUse { used_by: 1 });
    assert!(session.document().model().contains(EntityId::new(1)));
}

#[test]
fn a_material_nothing_uses_is_deleted_with_its_originals_in_one_step() {
    let mut session = session();
    add(&mut session, 1, S355, None);
    let update = json!({ "material": id(1), "properties": [{ "key": "fy", "value": 345e6 }] });
    ok(&mut session, "materials.update", update);
    let before = session.document().model().len();
    let history = session.document().history().count();
    let outcome = call(
        &mut session,
        "materials.delete",
        json!({ "material": id(1) }),
    )
    .unwrap();
    assert_eq!(outcome.result, json!({}));
    assert!(outcome.event.is_some());
    assert_eq!(session.document().model().len(), before - 2);
    assert_eq!(session.document().history().count(), history + 1);
    assert_eq!(
        ok(&mut session, "materials.list", json!({}))["items"],
        json!([])
    );
    session.document_mut().undo().unwrap();
    assert_eq!(session.document().model().len(), before);
    assert_eq!(
        fails(call(
            &mut session,
            "materials.delete",
            json!({ "material": id(7) })
        )),
        MaterialsError::NotFound
    );
}

#[test]
fn errors_carry_a_reason_and_no_values() {
    let cases = [
        (MaterialsError::UnknownGrade, "unknown_grade", None),
        (MaterialsError::NotFound, "not_found", None),
        (MaterialsError::DuplicateName, "duplicate_name", None),
        (MaterialsError::InvalidName, "invalid_name", None),
        (
            MaterialsError::UnknownProperty { key: "x".into() },
            "unknown_property",
            Some("x"),
        ),
        (
            MaterialsError::InvalidValue {
                key: Some("fy".into()),
            },
            "invalid_value",
            Some("fy"),
        ),
        (
            MaterialsError::InvalidValue { key: None },
            "invalid_value",
            None,
        ),
        (
            MaterialsError::RequiredProperty { key: "fck".into() },
            "required_property",
            Some("fck"),
        ),
        (MaterialsError::InUse { used_by: 2 }, "in_use", None),
    ];
    for (error, reason, key) in cases {
        assert_eq!(error.reason(), reason);
        assert_eq!(error.key(), key);
        assert!(!error.to_string().is_empty());
        let response = crate::jsonrpc::error_response(&json!(1), &ApiError::Materials(error));
        assert_eq!(response["error"]["code"], codes::REJECTED);
        assert_eq!(response["error"]["data"]["reason"], reason);
        assert_eq!(
            response["error"]["data"].get("key").and_then(Value::as_str),
            key
        );
    }
    let in_use = crate::jsonrpc::error_response(
        &json!(1),
        &ApiError::Materials(MaterialsError::InUse { used_by: 2 }),
    );
    assert_eq!(in_use["error"]["data"]["used_by"], 2);
}

#[test]
fn more_properties_than_the_schema_allows_are_refused() {
    let mut session = session();
    add(&mut session, 1, C30, None);
    let inputs: Vec<Value> = (0..=MAX_PROPERTIES)
        .map(|_| json!({ "key": "density", "value": 2400.0 }))
        .collect();
    let refused = call(
        &mut session,
        "materials.update",
        json!({ "material": id(1), "properties": inputs }),
    );
    assert_eq!(fails(refused), MaterialsError::InvalidValue { key: None });
}

#[test]
fn a_law_with_wrong_values_is_reported_not_replaced_by_another_law() {
    let mut session = session();
    // εc2 above εcu2 breaks the parabola-rectangle law; the modulus must not hide it.
    let properties = json!([
        { "key": "fck", "value": 30e6 }, { "key": "e_modulus", "value": 33e9 },
        { "key": "eps_c2", "value": 0.004 }, { "key": "eps_cu2", "value": 0.0035 },
        { "key": "n", "value": 2.0 },
    ]);
    assert_eq!(
        create(&mut session, "Mix", "concrete", properties),
        MaterialsError::InvalidValue { key: None }
    );
    assert!(!session.document().model().contains(EntityId::new(1)));
}

#[test]
fn annexes_are_named_as_the_material_crate_names_them() {
    assert_eq!(
        annex_schema()["oneOf"],
        json!([{ "const": "recommended" }, { "const": "fr" }])
    );
    for annex in NationalAnnex::ALL {
        let input = PropertyInput {
            key: "gamma_c".into(),
            annex: Some(annex.name().into()),
            value: Some(1.5),
        };
        assert_eq!(input.property().unwrap().annex(), Some(annex));
    }
}

#[test]
fn the_laws_a_custom_material_may_follow_are_those_its_category_allows() {
    for category in Category::ALL {
        assert!(!laws_for(category).is_empty(), "{category:?}");
        for law in laws_for(category) {
            assert!(category.allows(*law), "{category:?} {law:?}");
        }
    }
}
