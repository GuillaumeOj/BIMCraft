//! The georeference round-trips through IFC: read → write → read is lossless, and file → model
//! → file keeps the instances the georeference does not map (GEOREF-01, GEOREF-02, INT-03).

mod common;

use std::collections::BTreeSet;

use common::{read, write};
use core_model::SeededIdSource;
use georef::{EpsgCode, Georeference, ProjectedPoint};
use interop_ifc::georef::{
    Kept, SurveyPointError, SurveyProperty, Targets, read_georeference, write_georeference,
};
use interop_ifc::preserve::{Preserved, normalise};
use interop_ifc::step::{Instance, InstanceBody, Parameter};
use proptest::prelude::*;

const FIXTURE: &str = include_str!("fixtures/georeferenced.ifc");

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn largest_id(instances: &[Instance]) -> u64 {
    instances
        .iter()
        .map(|instance| instance.id)
        .max()
        .unwrap_or(0)
}

/// Exports `georeference` into the file `preserved` comes from, as an exporter would: the new
/// instances after the kept ones, which keep their ids, and references to the old CRS pointed
/// at the new one.
fn export(
    preserved: &Preserved,
    georeference: &Georeference,
    targets: Targets,
    kept: &Kept,
    old_crs: u64,
    length_unit: f64,
) -> Result<Vec<Instance>> {
    let first_id = largest_id(preserved.instances()) + 1;
    let written = write_georeference(
        georeference,
        targets,
        kept,
        length_unit,
        first_id,
        &mut SeededIdSource::new(3),
    )?;
    let used: BTreeSet<u64> = written.iter().map(|instance| instance.id).collect();
    let restored = preserved.restore(|id| (id == old_crs).then_some(first_id), &used);
    assert!(restored.dangling.is_empty(), "{:?}", restored.dangling);
    let mut instances = restored.instances;
    instances.extend(written);
    Ok(instances)
}

/// The scale goes through the project's length unit and back, which may cost a few ulps; every
/// length is written in metres, so it is exact.
fn assert_same(read_back: &Georeference, original: &Georeference) {
    assert_eq!(read_back.crs(), original.crs());
    assert_eq!(read_back.base_point(), original.base_point());
    let scale = (read_back.scale() - original.scale()).abs();
    assert!(scale <= 4.0 * f64::EPSILON * original.scale(), "{scale:e}");
    assert_eq!(read_back.survey_point(), original.survey_point());
    let rotation = (read_back.grid_rotation() - original.grid_rotation()).abs();
    assert!(rotation <= 2.0 * f64::EPSILON, "{rotation:e}");
}

#[test]
fn read_write_read_is_lossless_and_keeps_the_other_instances() {
    let (header, instances) = read(FIXTURE.as_bytes()).unwrap();
    let imported = read_georeference(&instances, 1.0).unwrap().unwrap();
    let (mapped, preserved) = Preserved::split_ids(instances.clone(), &imported.mapped);
    assert_eq!(mapped.len(), imported.mapped.len());
    let exported = export(
        &preserved,
        &imported.georeference,
        imported.targets,
        &imported.kept,
        imported.crs,
        1.0,
    )
    .unwrap();

    let again = read_georeference(&exported, 1.0).unwrap().unwrap();
    assert_same(&again.georeference, &imported.georeference);
    // The CRS keeps its other attributes, and the survey point its GlobalIds.
    assert_eq!(again.kept, imported.kept);
    let crs = exported
        .iter()
        .find(|instance| instance.id == again.crs)
        .unwrap();
    let InstanceBody::Simple(record) = &crs.body else {
        unreachable!("the CRS is a simple instance");
    };
    assert_eq!(
        record.parameters[1..6],
        [
            Parameter::String("RGF93 v1 / Lambert-93".into()),
            Parameter::String("RGF93 v1".into()),
            Parameter::Null,
            Parameter::Null,
            Parameter::Null,
        ]
    );

    // The kept instances come back as they were, except the well-known text, which now names
    // the new CRS.
    let new_crs = again.crs;
    let kept: Vec<Instance> = exported
        .iter()
        .filter(|instance| !again.mapped.contains(&instance.id))
        .cloned()
        .collect();
    let expected: Vec<Instance> = instances
        .into_iter()
        .filter(|instance| !imported.mapped.contains(&instance.id))
        .map(|mut instance| {
            if instance.id == 15 {
                let InstanceBody::Simple(record) = &mut instance.body else {
                    unreachable!("the well-known text is a simple instance");
                };
                record.parameters[1] = Parameter::Reference(new_crs);
            }
            instance
        })
        .collect();
    assert_eq!(
        normalise(&header, kept).unwrap(),
        normalise(&header, expected).unwrap()
    );
}

/// Exporting twice writes the same bytes: the ids, `GlobalId`s included, are deterministic.
#[test]
fn writes_back_the_same_bytes_twice() {
    let (header, instances) = read(FIXTURE.as_bytes()).unwrap();
    let imported = read_georeference(&instances, 1.0).unwrap().unwrap();
    let (_, preserved) = Preserved::split_ids(instances, &imported.mapped);
    let [first, second] = [(); 2].map(|()| {
        export(
            &preserved,
            &imported.georeference,
            imported.targets,
            &imported.kept,
            imported.crs,
            1.0,
        )
        .unwrap()
    });
    assert_eq!(
        write(&(header.clone(), first)).unwrap(),
        write(&(header, second)).unwrap()
    );
}

/// The instances of `name` in `instances`.
fn named<'a>(instances: &'a [Instance], name: &str) -> Vec<&'a Instance> {
    instances
        .iter()
        .filter(|instance| matches!(&instance.body, InstanceBody::Simple(record) if record.name == name))
        .collect()
}

/// How many `BIMCraft_SurveyPoint` property sets `instances` holds.
fn survey_sets(instances: &[Instance]) -> usize {
    named(instances, "IFCPROPERTYSET")
        .into_iter()
        .filter(|instance| {
            matches!(&instance.body, InstanceBody::Simple(record)
                if record.parameters.get(2) == Some(&Parameter::String("BIMCraft_SurveyPoint".into())))
        })
        .count()
}

fn with_survey_point(georeference: &Georeference, point: ProjectedPoint) -> Result<Georeference> {
    Ok(Georeference::new(
        georeference.crs(),
        georeference.base_point(),
        georeference.grid_rotation(),
        georeference.scale(),
        Some(point),
    )?)
}

#[test]
fn re_exporting_with_a_changed_survey_point_writes_one_set() {
    let (_, instances) = read(FIXTURE.as_bytes()).unwrap();
    let imported = read_georeference(&instances, 1.0).unwrap().unwrap();
    assert_eq!(survey_sets(&instances), 1);
    let (_, preserved) = Preserved::split_ids(instances, &imported.mapped);
    let point = ProjectedPoint::new(652_100.0, 6_862_200.0, 40.0);
    let exported = export(
        &preserved,
        &with_survey_point(&imported.georeference, point).unwrap(),
        imported.targets,
        &imported.kept,
        imported.crs,
        1.0,
    )
    .unwrap();
    assert_eq!(survey_sets(&exported), 1);
    let again = read_georeference(&exported, 1.0).unwrap().unwrap();
    assert_eq!(again.survey_error, None);
    assert_eq!(again.georeference.survey_point(), Some(point));
    assert_eq!(
        again.kept.survey_global_ids,
        imported.kept.survey_global_ids
    );
}

#[test]
fn re_exporting_replaces_a_survey_point_set_that_was_not_valid() {
    let text = FIXTURE.replace("(#10,#11,#12)", "(#10,#11)");
    let (_, instances) = read(text.as_bytes()).unwrap();
    let imported = read_georeference(&instances, 1.0).unwrap().unwrap();
    assert_eq!(
        imported.survey_error,
        Some(SurveyPointError::Missing(SurveyProperty::OrthogonalHeight))
    );
    assert_eq!(imported.georeference.survey_point(), None);
    // Without a valid survey point, the importer has no site to give: the caller does.
    let targets = Targets {
        site: Some(9),
        ..imported.targets
    };
    let (_, preserved) = Preserved::split_ids(instances, &imported.mapped);
    let point = ProjectedPoint::new(1.0, 2.0, 3.0);
    let exported = export(
        &preserved,
        &with_survey_point(&imported.georeference, point).unwrap(),
        targets,
        &imported.kept,
        imported.crs,
        1.0,
    )
    .unwrap();
    assert_eq!(survey_sets(&exported), 1);
    let again = read_georeference(&exported, 1.0).unwrap().unwrap();
    assert_eq!(
        (again.survey_error, again.georeference.survey_point()),
        (None, Some(point))
    );
}

/// A file with the fixture's context and site but no georeference.
fn blank() -> Result<Preserved> {
    let (_, instances) = read(FIXTURE.as_bytes())?;
    let georeference: BTreeSet<u64> = BTreeSet::from([7, 8, 10, 11, 12, 13, 14, 15]);
    Ok(Preserved::split_ids(instances, &georeference).1)
}

fn export_blank(georeference: &Georeference, length_unit: f64) -> Result<Vec<Instance>> {
    let targets = Targets {
        context: 4,
        site: Some(9),
        metre_unit: None,
    };
    export(
        &blank()?,
        georeference,
        targets,
        &Kept::default(),
        0,
        length_unit,
    )
}

fn round_trip(georeference: &Georeference, length_unit: f64) -> Result<Georeference> {
    let exported = export_blank(georeference, length_unit)?;
    Ok(read_georeference(&exported, length_unit)?
        .ok_or("no georeference")?
        .georeference)
}

/// Whether `id` refers to the plain metre, an `IfcSIUnit` of `instances`.
fn metre(instances: &[Instance], id: &Parameter) -> bool {
    let Parameter::Reference(id) = id else {
        return false;
    };
    instances.iter().any(|instance| {
        instance.id == *id
            && matches!(&instance.body, InstanceBody::Simple(record)
            if record.name == "IFCSIUNIT"
                && record.parameters[1..]
                    == [
                        Parameter::Enumeration("LENGTHUNIT".into()),
                        Parameter::Null,
                        Parameter::Enumeration("METRE".into()),
                    ])
    })
}

#[test]
fn a_millimetre_file_keeps_the_georeference_and_names_the_metre() {
    let original = Georeference::new(
        EpsgCode::new(25831).unwrap(),
        ProjectedPoint::new(430_000.125, 4_580_000.5, 12.25),
        -0.75,
        1.0,
        Some(ProjectedPoint::new(430_010.0, 4_580_020.0, 13.0)),
    )
    .unwrap();
    let exported = export_blank(&original, 0.001).unwrap();
    assert_same(&round_trip(&original, 0.001).unwrap(), &original);

    // A tool that reads an absent or unitless length in the project's unit, millimetres, still
    // finds the right place: the CRS and every survey point property name the metre.
    let [crs] = named(&exported, "IFCPROJECTEDCRS")[..] else {
        panic!("one CRS");
    };
    let InstanceBody::Simple(crs) = &crs.body else {
        unreachable!("the CRS is a simple instance");
    };
    assert!(metre(&exported, &crs.parameters[6]));
    let properties = named(&exported, "IFCPROPERTYSINGLEVALUE");
    assert_eq!(properties.len(), 3);
    for property in properties {
        let InstanceBody::Simple(property) = &property.body else {
            unreachable!("a property is a simple instance");
        };
        assert!(metre(&exported, &property.parameters[3]));
    }
    // The base point is in metres, and the scale takes the unit of the model into account.
    let [conversion] = named(&exported, "IFCMAPCONVERSION")[..] else {
        panic!("one conversion");
    };
    let InstanceBody::Simple(conversion) = &conversion.body else {
        unreachable!("the conversion is a simple instance");
    };
    assert_eq!(conversion.parameters[2], Parameter::Real(430_000.125));
    assert_eq!(conversion.parameters[7], Parameter::Real(0.001));
}

fn projected() -> impl Strategy<Value = ProjectedPoint> {
    (-1e7..1e7_f64, -1e7..1e7_f64, -1e4..1e4_f64)
        .prop_map(|(eastings, northings, height)| ProjectedPoint::new(eastings, northings, height))
}

proptest! {
    #![proptest_config(proptest::test_runner::Config {
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    })]

    /// BIMCraft → IFC → BIMCraft gives every value back, the survey point included.
    #[test]
    fn any_georeference_round_trips(
        code in 1..=EpsgCode::MAX,
        base in projected(),
        rotation in -core::f64::consts::PI..core::f64::consts::PI,
        scale in 1e-3..1e3_f64,
        survey in proptest::option::of(projected()),
        length_unit in prop_oneof![Just(1.0), Just(0.001), Just(0.3048)],
    ) {
        let original = Georeference::new(EpsgCode::new(code).unwrap(), base, rotation, scale, survey)
            .unwrap();
        assert_same(&round_trip(&original, length_unit).unwrap(), &original);
    }
}
