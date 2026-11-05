use core_model::{AttributeValue, EntityId, Model};
use core_txn::{Command, CommandError, Document, Origin, Transaction};
use schema_gen::{Source, parse_sources};

use super::*;

/// The `core` schema, as the desktop host embeds it.
fn core_model() -> Model {
    model_on(include_str!("../../../../schema/core/entities/site.toml"))
}

/// A model whose `Site` entity is defined by `site`.
fn model_on(site: &str) -> Model {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: include_str!("../../../../schema/core/namespace.toml").into(),
        },
        Source {
            path: "entities/site.toml".into(),
            contents: site.into(),
        },
    ];
    Model::new(&parse_sources(&sources).unwrap()).unwrap()
}

const SITE_ID: EntityId = EntityId::new(1);
const OTHER_ID: EntityId = EntityId::new(2);

fn sample(survey: Option<ProjectedPoint>) -> Georeference {
    Georeference::new(
        EpsgCode::new(2154).unwrap(),
        ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0),
        0.25,
        0.9996,
        survey,
    )
    .unwrap()
}

const SURVEY: ProjectedPoint = ProjectedPoint::new(652_010.5, 6_862_020.25, 36.125);

#[test]
fn a_model_without_site_is_not_georeferenced() {
    let model = core_model();
    assert_eq!(find_site(&model), Ok(None));
    assert_eq!(read(&model), Ok(None));
}

#[test]
fn a_site_without_crs_is_not_georeferenced() {
    let mut model = core_model();
    model
        .insert(
            SITE_ID,
            SITE,
            [(BASE_POINT_EASTINGS, AttributeValue::Real(5.0))],
        )
        .unwrap();
    assert_eq!(find_site(&model), Ok(Some(SITE_ID)));
    assert_eq!(read(&model), Ok(None));
}

#[test]
fn writes_a_new_site_and_reads_it_back() {
    let mut model = core_model();
    let georef = sample(Some(SURVEY));
    assert_eq!(write(&mut model, SITE_ID, &georef), Ok(SITE_ID));
    assert_eq!(read(&model), Ok(Some((SITE_ID, georef))));
}

#[test]
fn defaults_fill_the_unset_values() {
    let mut model = core_model();
    model
        .insert(SITE_ID, SITE, [(CRS_EPSG, AttributeValue::Integer(2154))])
        .unwrap();
    let (_, georef) = read(&model).unwrap().unwrap();
    assert_eq!(georef.base_point(), ProjectedPoint::new(0.0, 0.0, 0.0));
    assert_eq!(georef.grid_rotation().to_bits(), 0.0_f64.to_bits());
    assert_eq!(georef.scale().to_bits(), 1.0_f64.to_bits());
    assert_eq!(georef.survey_point(), None);
}

#[test]
fn updates_the_existing_site_and_clears_a_previous_survey_point() {
    let mut model = core_model();
    write(&mut model, SITE_ID, &sample(Some(SURVEY))).unwrap();
    let without = sample(None);
    // The id for a new site is not used: the existing one is updated.
    assert_eq!(write(&mut model, OTHER_ID, &without), Ok(SITE_ID));
    assert!(!model.contains(OTHER_ID));
    assert_eq!(read(&model), Ok(Some((SITE_ID, without))));
}

#[test]
fn values_list_every_attribute_in_order() {
    let georef = sample(Some(SURVEY));
    let all = values(Some(&georef));
    assert_eq!(all.clone().map(|(name, _)| name), ATTRIBUTES);
    assert_eq!(all[0].1, Some(AttributeValue::Integer(2154)));
    assert_eq!(all[5].1, Some(AttributeValue::Real(0.9996)));
    assert_eq!(all[8].1, Some(AttributeValue::Real(36.125)));
    assert!(values(None).iter().all(|(_, value)| value.is_none()));
    assert!(
        values(Some(&sample(None)))[6..]
            .iter()
            .all(|(_, value)| value.is_none())
    );
}

#[test]
fn clear_unsets_the_georeference_and_keeps_the_site() {
    let mut model = core_model();
    assert_eq!(clear(&mut model), Ok(None));
    write(&mut model, SITE_ID, &sample(Some(SURVEY))).unwrap();
    assert_eq!(clear(&mut model), Ok(Some(SITE_ID)));
    assert!(model.contains(SITE_ID));
    assert_eq!(read(&model), Ok(None));
    assert_eq!(model.get(SITE_ID, SURVEY_POINT_EASTINGS), Ok(None));
}

#[test]
fn two_sites_are_reported() {
    let mut model = core_model();
    model
        .insert(SITE_ID, SITE, Vec::<(&str, _)>::new())
        .unwrap();
    model
        .insert(OTHER_ID, SITE, Vec::<(&str, _)>::new())
        .unwrap();
    assert_eq!(find_site(&model), Err(SiteError::MultipleSites));
    assert_eq!(read(&model), Err(SiteError::MultipleSites));
    assert_eq!(
        write(&mut model, EntityId::new(3), &sample(None)),
        Err(SiteError::MultipleSites)
    );
    assert_eq!(clear(&mut model), Err(SiteError::MultipleSites));
}

#[test]
fn invalid_stored_values_are_reported() {
    let cases = [
        (
            vec![(CRS_EPSG, AttributeValue::Integer(0))],
            SiteError::Invalid(GeoreferenceError::InvalidCrsCode),
        ),
        (
            vec![(CRS_EPSG, AttributeValue::Integer(-2154))],
            SiteError::Invalid(GeoreferenceError::InvalidCrsCode),
        ),
        (
            vec![(CRS_EPSG, AttributeValue::Integer(i64::from(u32::MAX) + 1))],
            SiteError::Invalid(GeoreferenceError::InvalidCrsCode),
        ),
        (
            vec![
                (CRS_EPSG, AttributeValue::Integer(2154)),
                (SCALE, AttributeValue::Real(0.0)),
            ],
            SiteError::Invalid(GeoreferenceError::InvalidScale),
        ),
        (
            vec![
                (CRS_EPSG, AttributeValue::Integer(2154)),
                (SURVEY_POINT_EASTINGS, AttributeValue::Real(1.0)),
            ],
            SiteError::IncompleteSurveyPoint,
        ),
        (
            vec![
                (CRS_EPSG, AttributeValue::Integer(2154)),
                (SURVEY_POINT_EASTINGS, AttributeValue::Real(1.0)),
                (SURVEY_POINT_HEIGHT, AttributeValue::Real(1.0)),
            ],
            SiteError::IncompleteSurveyPoint,
        ),
    ];
    for (stored, error) in cases {
        let mut model = core_model();
        model.insert(SITE_ID, SITE, stored).unwrap();
        assert_eq!(read(&model), Err(error));
    }
}

/// A `Site` defined with other value types and no defaults, as a later schema or a plugin
/// could have it.
const ODD_SITE: &str = r#"format = 1

[entity]
name = "Site"
kind = "spatial"

[ifc]
entity = "IfcSite"

[[attribute]]
name = "crs_epsg"
value = "text"

[[attribute]]
name = "base_point_eastings"
value = "quantity:length"

[[attribute]]
name = "base_point_northings"
value = "quantity:length"

[[attribute]]
name = "base_point_height"
value = "quantity:length"

[[attribute]]
name = "grid_rotation"
value = "integer"

[[attribute]]
name = "scale"
value = "real"

[[attribute]]
name = "survey_point_eastings"
value = "quantity:length"

[[attribute]]
name = "survey_point_northings"
value = "quantity:length"
"#;

#[test]
fn values_of_another_type_are_reported() {
    let mut model = model_on(ODD_SITE);
    model
        .insert(
            SITE_ID,
            SITE,
            [(CRS_EPSG, AttributeValue::Text("2154".into()))],
        )
        .unwrap();
    assert_eq!(
        read(&model),
        Err(SiteError::WrongValueType {
            attribute: CRS_EPSG
        })
    );

    let odd = ODD_SITE.replacen("value = \"text\"", "value = \"integer\"", 1)
        + "\n[[attribute]]\nname = \"survey_point_height\"\nvalue = \"quantity:length\"\n";
    let mut model = model_on(&odd);
    model
        .insert(
            SITE_ID,
            SITE,
            [
                (CRS_EPSG, AttributeValue::Integer(2154)),
                (GRID_ROTATION, AttributeValue::Integer(1)),
            ],
        )
        .unwrap();
    assert_eq!(
        read(&model),
        Err(SiteError::WrongValueType {
            attribute: GRID_ROTATION
        })
    );
}

#[test]
fn a_missing_attribute_is_a_model_error() {
    let odd = ODD_SITE.replacen("value = \"text\"", "value = \"integer\"", 1);
    let mut model = model_on(&odd);
    model
        .insert(SITE_ID, SITE, [(CRS_EPSG, AttributeValue::Integer(2154))])
        .unwrap();
    // `survey_point_height` is not defined on this `Site`.
    assert!(matches!(read(&model), Err(SiteError::Model(_))));
    assert!(matches!(
        write(&mut model, SITE_ID, &sample(None)),
        Err(SiteError::Model(_))
    ));
}

#[test]
fn errors_name_attributes_but_not_values() {
    assert_eq!(
        SiteError::MultipleSites.to_string(),
        "the model holds more than one site"
    );
    assert_eq!(
        SiteError::IncompleteSurveyPoint.to_string(),
        "the survey point is incomplete"
    );
    assert_eq!(
        SiteError::WrongValueType { attribute: SCALE }.to_string(),
        "attribute \"scale\" holds a value of the wrong type"
    );
    assert_eq!(
        SiteError::Invalid(GeoreferenceError::InvalidScale).to_string(),
        "the scale is out of range"
    );
}

/// Sets, or with `None` clears, the georeference in one command.
#[derive(serde::Serialize, serde::Deserialize)]
struct SetGeoreference {
    site: EntityId,
    value: Option<(u32, [f64; 3], f64, f64)>,
}

impl Command for SetGeoreference {
    fn name(&self) -> &'static str {
        "set_georeference"
    }

    fn apply(&self, transaction: &mut Transaction<'_>) -> Result<(), CommandError> {
        let result = match self.value {
            Some((code, [e, n, h], rotation, scale)) => {
                let georef = Georeference::new(
                    EpsgCode::new(code).unwrap(),
                    ProjectedPoint::new(e, n, h),
                    rotation,
                    scale,
                    None,
                )
                .unwrap();
                write(transaction, self.site, &georef).map(Some)
            }
            None => clear(transaction),
        };
        match result {
            Ok(_) => Ok(()),
            Err(SiteError::Model(error)) => Err(error.into()),
            Err(other) => panic!("{other}"),
        }
    }
}

#[test]
fn a_transaction_sets_and_clears_the_georeference_in_one_undo_step() {
    let mut document = Document::new(core_model());
    let set = SetGeoreference {
        site: SITE_ID,
        value: Some((2154, [652_000.0, 6_862_000.0, 35.0], 0.25, 0.9996)),
    };
    let event = document.execute(set, Origin::Ui).unwrap();
    assert!(!format!("{event:?}").is_empty());
    assert_eq!(read(document.model()), Ok(Some((SITE_ID, sample(None)))));

    let clear = SetGeoreference {
        site: SITE_ID,
        value: None,
    };
    document.execute(clear, Origin::Ui).unwrap();
    assert_eq!(read(document.model()), Ok(None));

    document.undo().unwrap();
    assert_eq!(read(document.model()), Ok(Some((SITE_ID, sample(None)))));
    document.undo().unwrap();
    assert_eq!(find_site(document.model()), Ok(None));
}
