use core_model::Model;
use core_txn::{ModelCommand, Origin};
use schema_gen::{Source, parse_sources};
use serde_json::{Value, json};

use super::*;
use crate::jsonrpc::{Server, codes};

fn model() -> Model {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: include_str!("../../../../schema/core/namespace.toml").into(),
        },
        Source {
            path: "entities/site.toml".into(),
            contents: include_str!("../../../../schema/core/entities/site.toml").into(),
        },
    ];
    Model::new(&parse_sources(&sources).unwrap()).unwrap()
}

fn session() -> Session {
    Session::new(model())
}

fn call(session: &mut Session, name: &str, params: Value) -> Result<Outcome, ApiError> {
    Session::registry()
        .unwrap()
        .dispatch(session, name, params, &Origin::Ui)
}

fn reason(result: Result<Outcome, ApiError>) -> GeorefError {
    match result {
        Err(ApiError::Georef(error)) => error,
        other => panic!("expected a georef error, got {other:?}"),
    }
}

/// Lambert-93, near Strasbourg, turned by 0.25 rad.
fn lambert(extra: Value) -> Value {
    let mut value = json!({
        "crs_epsg": 2154,
        "base_point": [1_050_000.0, 6_840_000.0, 150.0],
        "grid_rotation": 0.25,
        "scale": 1.0,
        "survey_point": null,
    });
    let Value::Object(extra) = extra else {
        panic!("the changes are an object");
    };
    value.as_object_mut().unwrap().extend(extra);
    value
}

/// The id the tests give the new `Site`.
const SITE: u128 = 0x51e;

fn site_id() -> Value {
    json!(format!("{SITE:032x}"))
}

/// `lambert(extra)` with the id of the site `georef.set` creates.
fn settable(extra: Value) -> Value {
    let mut value = lambert(extra);
    value["site"] = site_id();
    value
}

fn georeferenced(value: Value) -> Session {
    let mut session = session();
    call(&mut session, "georef.set", settable(value)).unwrap();
    session
}

fn get(session: &mut Session) -> Value {
    call(session, "georef.get", json!({})).unwrap().result["georeference"].clone()
}

fn site_ids(session: &Session) -> Vec<EntityId> {
    let model = session.document().model();
    model
        .ids()
        .filter(|&id| model.entity_of(id) == Some("Site"))
        .collect()
}

fn close(a: &Value, b: &Value) {
    let (a, b) = (a.as_array().unwrap(), b.as_array().unwrap());
    for (a, b) in a.iter().zip(b) {
        let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
        assert!((a - b).abs() < 1e-6, "{a} is not {b}");
    }
}

#[test]
fn every_georef_call_is_registered() {
    let registry = Session::registry().unwrap();
    for (name, kind) in [
        ("georef.convert", crate::Kind::Query),
        ("georef.get", crate::Kind::Query),
        ("georef.list_crs", crate::Kind::Query),
        ("georef.set", crate::Kind::Command),
        ("georef.clear", crate::Kind::Command),
    ] {
        assert_eq!(registry.get(name).map(Descriptor::kind), Ok(kind), "{name}");
    }
}

#[test]
fn list_crs_lists_current_projected_crss_by_code() {
    let mut session = session();
    let list = call(&mut session, "georef.list_crs", json!({}))
        .unwrap()
        .result;
    let crs = list["crs"].as_array().unwrap();
    let codes: Vec<u64> = crs
        .iter()
        .map(|crs| crs["epsg"].as_u64().unwrap())
        .collect();
    assert!(codes.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(codes.contains(&2154));
    assert!(!codes.contains(&25838), "a deprecated CRS is not listed");
    assert!(!codes.contains(&4326), "a geographic CRS is not listed");
    let lambert = crs.iter().find(|crs| crs["epsg"] == 2154).unwrap();
    assert_eq!(lambert["name"], "RGF93 v1 / Lambert-93");
    assert_eq!(lambert["kind"], "projected");
    assert_eq!(lambert["geographic_name"], "RGF93 v1");
    assert!(lambert["area"].as_str().unwrap().starts_with("France"));
}

#[test]
fn list_crs_can_include_geographic_crss() {
    let mut session = session();
    let params = json!({ "include_geographic": true });
    let list = call(&mut session, "georef.list_crs", params)
        .unwrap()
        .result;
    let crs = list["crs"].as_array().unwrap();
    let wgs84 = crs.iter().find(|crs| crs["epsg"] == 4326).unwrap();
    assert_eq!(wgs84["kind"], "geographic");
    assert_eq!(wgs84["geographic_name"], wgs84["name"]);
    let codes: Vec<u64> = crs
        .iter()
        .map(|crs| crs["epsg"].as_u64().unwrap())
        .collect();
    assert!(codes.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(codes.contains(&2154));
}

#[test]
fn a_project_without_georeference_reads_as_none() {
    let mut session = session();
    assert_eq!(get(&mut session), Value::Null);
    call(&mut session, "georef.set", settable(json!({}))).unwrap();
    call(&mut session, "georef.clear", json!({})).unwrap();
    assert_eq!(get(&mut session), Value::Null);
}

#[test]
fn set_creates_the_site_and_get_derives_its_values() {
    let mut session = session();
    let outcome = call(&mut session, "georef.set", settable(json!({}))).unwrap();
    let site = outcome.result["site"].clone();
    let event = outcome.event.unwrap();
    assert_eq!(event.created.len(), 1);
    assert_eq!(site_ids(&session).len(), 1);

    let current = get(&mut session);
    assert_eq!(current["site"], site);
    assert_eq!(current["value"], lambert(json!({})));
    assert_eq!(current["crs"]["epsg"], 2154);
    let derived = &current["derived"];
    // The registry's own convergence and true north: the API only reports them.
    let registry = Crss::embedded().unwrap();
    let crs = registry.projected(EpsgCode::new(2154).unwrap()).unwrap();
    let (_, georeference) = site::read(session.document().model()).unwrap().unwrap();
    let factors = crs.factors_at_base_point(&georeference).unwrap();
    assert_eq!(derived["convergence"], json!(factors.convergence));
    assert_eq!(derived["point_scale_factor"], json!(factors.scale));
    assert_eq!(
        derived["true_north"],
        json!(crs.true_north(&georeference).unwrap())
    );
    // East of 3° E, grid north is clockwise of true north.
    assert!(factors.convergence > 0.0 && factors.scale > 0.99);
    // Latitude and longitude of the base point, in radians; the height is the projected one.
    let geographic = derived["base_point_geographic"].as_array().unwrap();
    assert!((geographic[0].as_f64().unwrap() * (180.0 / std::f64::consts::PI) - 48.6).abs() < 0.2);
    assert!((geographic[1].as_f64().unwrap() * (180.0 / std::f64::consts::PI) - 7.7).abs() < 0.3);
    assert_eq!(derived["survey_point_local"], Value::Null);
}

#[test]
fn the_survey_point_is_read_back_in_local_coordinates() {
    let survey = json!([1_050_010.0, 6_840_020.0, 152.0]);
    let mut session = georeferenced(lambert(
        json!({ "survey_point": survey, "grid_rotation": 0.0 }),
    ));
    let current = get(&mut session);
    assert_eq!(current["value"]["survey_point"], survey);
    close(
        &current["derived"]["survey_point_local"],
        &json!([10.0, 20.0, 2.0]),
    );
}

#[test]
fn set_updates_the_existing_site_and_clears_what_it_leaves_out() {
    let mut session = georeferenced(lambert(json!({ "survey_point": [1.0, 2.0, 3.0] })));
    let site = site_ids(&session)[0];
    let update = lambert(json!({ "crs_epsg": 32631, "base_point": [500_000.0, 5_400_000.0, 0.0] }));
    let outcome = call(&mut session, "georef.set", update.clone()).unwrap();
    assert_eq!(outcome.result, json!({ "site": site }));
    assert_eq!(site_ids(&session), [site]);
    let current = get(&mut session);
    assert_eq!(current["value"], update);
    assert_eq!(current["value"]["survey_point"], Value::Null);
}

#[test]
fn set_accepts_a_deprecated_crs_that_the_list_omits() {
    let params = lambert(json!({ "crs_epsg": 25838, "base_point": [500_000.0, 5_400_000.0, 0.0] }));
    let mut session = georeferenced(params);
    assert_eq!(get(&mut session)["crs"]["epsg"], 25838);
    let converted = call(
        &mut session,
        "georef.convert",
        json!({ "from": "local", "points": [[0.0, 0.0, 0.0]] }),
    );
    assert!(converted.is_ok());
}

#[test]
fn set_and_clear_are_single_undo_steps_recorded_with_their_origin() {
    let mut session = session();
    let registry = Session::registry().unwrap();
    let origin = Origin::Plugin {
        name: "survey".into(),
    };
    registry
        .dispatch(&mut session, "georef.set", settable(json!({})), &origin)
        .unwrap();
    registry
        .dispatch(&mut session, "georef.clear", json!({}), &Origin::Ui)
        .unwrap();
    let history: Vec<_> = session
        .document()
        .history()
        .map(|entry| entry.origin.clone())
        .collect();
    assert_eq!(history, [origin, Origin::Ui]);

    call(&mut session, "history.undo", json!({})).unwrap();
    assert_eq!(get(&mut session)["value"]["crs_epsg"], 2154);
    call(&mut session, "history.undo", json!({})).unwrap();
    assert!(site_ids(&session).is_empty());
    call(&mut session, "history.redo", json!({})).unwrap();
    assert_eq!(get(&mut session)["value"]["crs_epsg"], 2154);
}

#[test]
fn clear_keeps_the_site_and_leaves_an_ungeoreferenced_project_alone() {
    let mut session = session();
    let outcome = call(&mut session, "georef.clear", json!({})).unwrap();
    assert_eq!((outcome.result, outcome.event), (json!({}), None));
    assert!(!session.document().can_undo());

    call(&mut session, "georef.set", settable(json!({}))).unwrap();
    let site = site_ids(&session)[0];
    let outcome = call(&mut session, "georef.clear", json!({})).unwrap();
    assert!(outcome.event.is_some());
    assert_eq!(site_ids(&session), [site]);
    // Clearing twice changes nothing the second time.
    let history = session.document().history().count();
    let outcome = call(&mut session, "georef.clear", json!({})).unwrap();
    assert_eq!(outcome.event, None);
    assert_eq!(session.document().history().count(), history);
}

#[test]
fn set_refuses_what_is_not_a_projected_crs_of_the_registry() {
    for (code, error) in [
        (999_999, GeorefError::UnknownCrs),
        (0, GeorefError::UnknownCrs),
        (4326, GeorefError::NotProjected),
    ] {
        let mut session = session();
        let params = lambert(json!({ "crs_epsg": code }));
        assert_eq!(reason(call(&mut session, "georef.set", params)), error);
        assert!(
            !session.document().can_undo(),
            "a failed call changes nothing"
        );
    }
}

#[test]
fn set_refuses_a_scale_that_is_not_positive() {
    for scale in [0.0, -1.0] {
        let mut session = session();
        let params = lambert(json!({ "scale": scale }));
        assert_eq!(
            reason(call(&mut session, "georef.set", params)),
            GeorefError::InvalidScale
        );
    }
}

#[test]
fn set_refuses_a_base_point_outside_the_projection() {
    let mut session = session();
    let params = lambert(json!({ "crs_epsg": 32631, "base_point": [3.0e7, 0.0, 0.0] }));
    assert_eq!(
        reason(call(&mut session, "georef.set", params)),
        GeorefError::OutsideDomain { index: None }
    );
    assert!(site_ids(&session).is_empty());
}

#[test]
fn set_rejects_malformed_params() {
    let mut session = session();
    for params in [
        json!({}),
        lambert(json!({ "extra": 1 })),
        lambert(json!({ "base_point": [1.0, 2.0] })),
        lambert(json!({ "crs_epsg": -1 })),
        lambert(json!({ "scale": "1" })),
    ] {
        assert!(matches!(
            call(&mut session, "georef.set", params),
            Err(ApiError::InvalidParams { .. })
        ));
    }
    // A number JSON cannot hold is refused by the georeference itself.
    let value = SetParams {
        site: None,
        crs_epsg: 2154,
        base_point: [f64::NAN, 0.0, 0.0],
        grid_rotation: 0.0,
        scale: 1.0,
        survey_point: None,
    };
    assert!(matches!(
        validated(value),
        Err(ApiError::InvalidParams { .. })
    ));
}

#[test]
fn convert_gives_the_point_in_every_system() {
    let mut session = georeferenced(lambert(json!({ "grid_rotation": 0.0 })));
    let local = json!([10.0, 20.0, 1.0]);
    let mut converted = |from: &str, point: &Value| {
        let params = json!({ "from": from, "points": [point] });
        call(&mut session, "georef.convert", params).unwrap().result["points"][0].clone()
    };
    let from_local = converted("local", &local);
    close(
        &from_local["projected"],
        &json!([1_050_010.0, 6_840_020.0, 151.0]),
    );
    close(&from_local["local"], &local);
    for system in ["projected", "geographic"] {
        let again = converted(system, &from_local[system]);
        for other in ["local", "projected", "geographic"] {
            // Geographic angles are in radians: a micrometre is about 1.6e-13 rad.
            let tolerance = if other == "geographic" { 1e-9 } else { 1e-6 };
            for (a, b) in again[other]
                .as_array()
                .unwrap()
                .iter()
                .zip(from_local[other].as_array().unwrap())
            {
                assert!(
                    (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < tolerance,
                    "{system} -> {other}"
                );
            }
        }
    }
}

#[test]
fn convert_keeps_the_order_and_accepts_no_points() {
    let mut session = georeferenced(lambert(json!({})));
    let points = json!([[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [0.0, 5.0, 0.0]]);
    let result = call(
        &mut session,
        "georef.convert",
        json!({ "from": "local", "points": points }),
    )
    .unwrap()
    .result;
    let projected: Vec<_> = result["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["projected"].clone())
        .collect();
    close(&projected[0], &json!([1_050_000.0, 6_840_000.0, 150.0]));
    assert_ne!(projected[1], projected[2]);
    let none = call(
        &mut session,
        "georef.convert",
        json!({ "from": "local", "points": [] }),
    )
    .unwrap();
    assert_eq!(none.result, json!({ "points": [] }));
}

#[test]
fn convert_takes_at_most_a_page_of_points() {
    let mut session = georeferenced(lambert(json!({})));
    let mut convert = |count: usize| {
        let points = vec![json!([0.0, 0.0, 0.0]); count];
        call(
            &mut session,
            "georef.convert",
            json!({ "from": "local", "points": points }),
        )
    };
    assert!(convert(MAX_PAGE_SIZE).is_ok());
    assert!(matches!(
        convert(MAX_PAGE_SIZE + 1),
        Err(ApiError::InvalidParams { .. })
    ));
}

#[test]
fn convert_needs_a_georeference() {
    let mut session = session();
    let params = json!({ "from": "local", "points": [[0.0, 0.0, 0.0]] });
    assert_eq!(
        reason(call(&mut session, "georef.convert", params)),
        GeorefError::NotGeoreferenced
    );
}

#[test]
fn convert_names_the_first_point_outside_the_domain() {
    let mut session = georeferenced(lambert(json!({})));
    for (from, bad) in [
        ("geographic", json!([2.0, 0.0, 0.0])),
        ("projected", json!([1e300, 1e300, 0.0])),
        ("local", json!([1e308, 1e308, 0.0])),
    ] {
        let params = json!({ "from": from, "points": [[0.8, 0.13, 0.0], bad, bad] });
        let error = reason(call(&mut session, "georef.convert", params));
        assert_eq!(
            error,
            GeorefError::OutsideDomain { index: Some(1) },
            "{from}"
        );
    }
}

#[test]
fn convert_rejects_malformed_params() {
    let mut session = georeferenced(lambert(json!({})));
    for params in [
        json!({ "from": "map", "points": [] }),
        json!({ "from": "local" }),
        json!({ "from": "local", "points": [[1.0, 2.0]] }),
    ] {
        assert!(matches!(
            call(&mut session, "georef.convert", params),
            Err(ApiError::InvalidParams { .. })
        ));
    }
}

#[test]
fn a_model_with_two_sites_is_refused() {
    let mut session = georeferenced(lambert(json!({})));
    let second = ModelCommand::Create {
        id: EntityId::new(7),
        entity: "Site".into(),
        values: Vec::new(),
    };
    session.document_mut().execute(second, Origin::Ui).unwrap();
    let history = session.document().history().count();
    for (name, params) in [
        ("georef.get", json!({})),
        ("georef.set", settable(json!({}))),
        ("georef.clear", json!({})),
        (
            "georef.convert",
            json!({ "from": "local", "points": [[0.0, 0.0, 0.0]] }),
        ),
    ] {
        assert_eq!(
            reason(call(&mut session, name, params)),
            GeorefError::MultipleSites,
            "{name}"
        );
    }
    assert_eq!(session.document().history().count(), history);
}

#[test]
fn stored_values_that_are_not_a_georeference_are_internal_errors() {
    let mut session = session();
    let values = vec![("crs_epsg".to_owned(), AttributeValue::Integer(-5))];
    let site = ModelCommand::Create {
        id: EntityId::new(1),
        entity: "Site".into(),
        values,
    };
    session.document_mut().execute(site, Origin::Ui).unwrap();
    assert_eq!(
        call(&mut session, "georef.get", json!({})),
        Err(ApiError::Internal)
    );
}

#[test]
fn a_stored_crs_the_registry_lacks_is_reported_as_unknown() {
    let mut session = session();
    let values = vec![("crs_epsg".to_owned(), AttributeValue::Integer(999_999))];
    let site = ModelCommand::Create {
        id: EntityId::new(1),
        entity: "Site".into(),
        values,
    };
    session.document_mut().execute(site, Origin::Ui).unwrap();
    assert_eq!(
        reason(call(&mut session, "georef.get", json!({}))),
        GeorefError::UnknownCrs
    );
}

#[test]
fn a_stored_base_point_outside_the_projection_is_reported() {
    let mut session = session();
    let values = vec![
        ("crs_epsg".to_owned(), AttributeValue::Integer(32631)),
        (
            "base_point_eastings".to_owned(),
            AttributeValue::Real(3.0e7),
        ),
    ];
    let site = ModelCommand::Create {
        id: EntityId::new(1),
        entity: "Site".into(),
        values,
    };
    session.document_mut().execute(site, Origin::Ui).unwrap();
    assert_eq!(
        reason(call(&mut session, "georef.get", json!({}))),
        GeorefError::OutsideDomain { index: None }
    );
}

#[test]
fn errors_name_a_stable_reason_and_never_a_value() {
    let errors = [
        (GeorefError::UnknownCrs, "unknown_crs"),
        (GeorefError::NotProjected, "not_projected"),
        (GeorefError::InvalidScale, "invalid_scale"),
        (
            GeorefError::OutsideDomain { index: Some(3) },
            "outside_domain",
        ),
        (GeorefError::NotGeoreferenced, "not_georeferenced"),
        (GeorefError::MultipleSites, "multiple_sites"),
        (GeorefError::MissingSiteId, "missing_site_id"),
    ];
    for (error, reason) in errors {
        assert_eq!(error.reason(), reason);
        assert!(!error.to_string().chars().any(|c| c.is_ascii_digit()));
    }
}

#[test]
fn json_rpc_answers_with_the_reason_and_the_index() {
    let mut server = Server::new(session()).unwrap();
    let mut send = |method: &str, params: Value| -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let handled = server.handle(message.to_string().as_bytes(), &Origin::Ui);
        serde_json::from_str(handled.response.as_deref().unwrap()).unwrap()
    };
    let response = send(
        "georef.convert",
        json!({ "from": "local", "points": [[0.0, 0.0, 0.0]] }),
    );
    assert_eq!(response["error"]["code"], codes::REJECTED);
    assert_eq!(
        response["error"]["data"],
        json!({ "reason": "not_georeferenced" })
    );
    assert_eq!(
        response["error"]["message"],
        "the project is not georeferenced"
    );

    assert!(send("georef.set", settable(json!({})))["error"].is_null());
    let response = send(
        "georef.convert",
        json!({ "from": "geographic", "points": [[0.8, 0.13, 0.0], [2.0, 0.0, 0.0]] }),
    );
    assert_eq!(
        response["error"]["data"],
        json!({ "reason": "outside_domain", "index": 1 })
    );
}

#[test]
fn set_needs_the_id_of_the_site_it_creates() {
    let mut session = session();
    assert_eq!(
        reason(call(&mut session, "georef.set", lambert(json!({})))),
        GeorefError::MissingSiteId
    );
    assert!(
        !session.document().can_undo(),
        "a failed call changes nothing"
    );
    let outcome = call(&mut session, "georef.set", settable(json!({}))).unwrap();
    assert_eq!(outcome.result, json!({ "site": site_id() }));
    assert_eq!(site_ids(&session), [EntityId::new(SITE)]);
}

#[test]
fn set_takes_the_id_of_an_existing_site_or_none() {
    let mut session = georeferenced(json!({}));
    // The same id, or none, updates the site.
    call(
        &mut session,
        "georef.set",
        settable(json!({ "scale": 2.0 })),
    )
    .unwrap();
    call(&mut session, "georef.set", lambert(json!({ "scale": 3.0 }))).unwrap();
    assert_eq!(get(&mut session)["value"]["scale"], 3.0);
    // Another id is refused, and the site is left as it was.
    let other = lambert(json!({ "scale": 4.0, "site": format!("{:032x}", SITE + 1) }));
    assert!(matches!(
        call(&mut session, "georef.set", other),
        Err(ApiError::InvalidParams { .. })
    ));
    assert_eq!(get(&mut session)["value"]["scale"], 3.0);
    assert_eq!(site_ids(&session), [EntityId::new(SITE)]);
}
