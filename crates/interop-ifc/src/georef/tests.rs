use core::f64::consts::FRAC_PI_6;

use core_model::SeededIdSource;
use georef::{Field, GeoreferenceError, XAxisError};

use super::*;
use crate::step::test_support::typed;
use crate::step::{Limits, StepReader, StepWriter};

/// The model context, the site and the metre unit of the fixture.
const TARGETS: Targets = Targets {
    context: 4,
    site: Some(9),
    metre_unit: Some(3),
};
/// The same, for a file with no metre unit to reuse.
const FRESH: Targets = Targets {
    context: 4,
    site: Some(9),
    metre_unit: None,
};
const NO_SITE: Targets = Targets {
    context: 4,
    site: None,
    metre_unit: None,
};

const FIXTURE: &str = include_str!("../../tests/fixtures/georeferenced.ifc");

fn parse(text: &str) -> Vec<Instance> {
    StepReader::new(text.as_bytes(), Limits::default())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn fixture() -> Vec<Instance> {
    parse(FIXTURE)
}

/// The fixture with the line of instance `#id` replaced by `line`, or removed when `line` is
/// empty, and `extra` lines added before the end of the data section.
fn edited(edits: &[(u64, &str)], extra: &str) -> Vec<Instance> {
    let mut text = String::new();
    for line in FIXTURE.lines() {
        let edit = edits
            .iter()
            .find(|(id, _)| line.starts_with(&format!("#{id}=")));
        match edit {
            Some((_, "")) => {}
            Some((_, replacement)) => text.push_str(replacement),
            None if line == "ENDSEC;" && text.contains("DATA;") => {
                text.push_str(extra);
                text.push('\n');
                text.push_str(line);
            }
            None => text.push_str(line),
        }
        text.push('\n');
    }
    parse(&text)
}

/// Sets parameter `index` of instance `id`, or adds one past the last.
fn set_parameter(instances: &mut [Instance], id: u64, index: usize, value: Parameter) {
    let instance = instances
        .iter_mut()
        .find(|instance| instance.id == id)
        .unwrap();
    let InstanceBody::Simple(record) = &mut instance.body else {
        panic!("complex");
    };
    if index < record.parameters.len() {
        record.parameters[index] = value;
    } else {
        record.parameters.push(value);
    }
}

fn import(instances: &[Instance]) -> Result<Option<Imported>, GeorefImportError> {
    read_georeference(instances, 1.0)
}

fn import_ok(instances: &[Instance]) -> Imported {
    import(instances).unwrap().unwrap()
}

fn error(instances: &[Instance]) -> GeorefImportError {
    import(instances).unwrap_err()
}

#[test]
fn reads_the_georeference_of_the_fixture() {
    let imported = import_ok(&fixture());
    let georef = imported.georeference;
    assert_eq!(georef.crs().get(), 2154);
    assert_eq!(
        georef.base_point(),
        ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0)
    );
    assert!((georef.grid_rotation() - FRAC_PI_6).abs() < 1e-15);
    assert_eq!(georef.scale().to_bits(), 0.9996_f64.to_bits());
    assert_eq!(
        georef.survey_point(),
        Some(ProjectedPoint::new(652_010.5, 6_862_020.25, 36.125))
    );
    assert_eq!(imported.mapped, BTreeSet::from([7, 8, 10, 11, 12, 13, 14]));
    assert_eq!((imported.targets, imported.crs), (TARGETS, 7));
}

#[test]
fn an_invalid_survey_point_does_not_fail_the_import() {
    let imported = import_ok(&edited(&[(10, "")], ""));
    assert_eq!(imported.georeference.survey_point(), None);
    assert_eq!(
        imported.survey_error,
        Some(SurveyPointError::Dangling { reference: 10 })
    );
    assert_eq!(imported.targets.site, None);
    assert_eq!(imported.kept.survey_global_ids, None);
    // The set and its relationship are replaced by an export, not kept next to a new set; the
    // properties may be shared, so they stay.
    assert_eq!(imported.mapped, BTreeSet::from([7, 8, 13, 14]));
    assert_eq!(import_ok(&fixture()).survey_error, None);
}

#[test]
fn a_file_without_conversion_from_the_model_context_is_not_georeferenced() {
    assert_eq!(import(&edited(&[(8, "")], "")), Ok(None));
    let elsewhere = [
        // From the sub-context.
        "#8=IFCMAPCONVERSION(#16,#7,0.,0.,0.,$,$,$);",
        // From a 2D context.
        "#8=IFCMAPCONVERSION(#18,#7,0.,0.,0.,$,$,$);",
        // From a context of another type.
        "#8=IFCMAPCONVERSION(#19,#7,0.,0.,0.,$,$,$);",
        // From another CRS.
        "#8=IFCMAPCONVERSION(#20,#7,0.,0.,0.,$,$,$);",
        // From a complex instance.
        "#8=IFCMAPCONVERSION(#21,#7,0.,0.,0.,$,$,$);",
    ];
    let extra = "#18=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Plan',2,$,#5,$);\n\
                 #19=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Design',3,$,#5,$);\n\
                 #20=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,$);\n\
                 #21=(IFCREPRESENTATIONCONTEXT($,$)IFCGEOMETRICREPRESENTATIONCONTEXT(3,$,#5,$));";
    for line in elsewhere {
        assert_eq!(import(&edited(&[(8, line)], extra)), Ok(None), "{line}");
    }
    // Not from the model context, so not ours, and not decoded: a conversion that is malformed
    // is left to `Preserved`.
    let malformed = [
        "#8=IFCMAPCONVERSION(#16,#7,0.,0.,0.);",
        "#8=IFCMAPCONVERSIONSCALED(#18,#7);",
        "#8=IFCMAPCONVERSION(#98,#7,0.,0.,0.,$,$,$);",
        "#8=IFCMAPCONVERSION($,#7,0.,0.,0.,$,$,$);",
        "#8=IFCMAPCONVERSION(#8,#7,0.,0.,0.,$,$,$);",
        "#8=IFCMAPCONVERSION(#23,#7,0.,0.,0.,$,$,$);",
    ];
    let extra =
        format!("{extra}\n#23=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3.5,1.E-05,#5,$);");
    for line in malformed {
        assert_eq!(import(&edited(&[(8, line)], &extra)), Ok(None), "{line}");
    }
    // A valid conversion from the model context is still read next to a malformed one.
    let extra = "#18=IFCMAPCONVERSION(#16,#7,0.);";
    assert!(import_ok(&edited(&[], extra)).mapped.contains(&8));
    let untyped = "#8=IFCMAPCONVERSION(#22,#7,0.,0.,0.,$,$,$);";
    let context = "#22=IFCGEOMETRICREPRESENTATIONCONTEXT($,$,3,$,#5,$);";
    assert_eq!(import(&edited(&[(8, untyped)], context)), Ok(None));
}

#[test]
fn missing_axis_and_scale_take_their_defaults() {
    let line = "#8=IFCMAPCONVERSION(#4,#7,1.,2.,3.,$,$,$);";
    let georef = import_ok(&edited(&[(8, line)], "")).georeference;
    assert_eq!(georef.grid_rotation().to_bits(), 0.0_f64.to_bits());
    assert_eq!(georef.scale().to_bits(), 1.0_f64.to_bits());
    let line = "#8=IFCMAPCONVERSION(#4,#7,1.,2.,3.,$,1.,$);";
    let georef = import_ok(&edited(&[(8, line)], "")).georeference;
    assert!((georef.grid_rotation() - core::f64::consts::FRAC_PI_4).abs() < 1e-15);
}

#[test]
fn converts_lengths_from_the_file_and_map_units() {
    // Without a map unit, the map is in the project's unit, here millimetres.
    let crs = "#7=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,$);";
    let conversion = "#8=IFCMAPCONVERSION(#4,#7,652000000.,6862000000.,35000.,$,$,1.);";
    let survey = [
        (
            10,
            "#10=IFCPROPERTYSINGLEVALUE('Eastings',$,IFCLENGTHMEASURE(652010500.),$);",
        ),
        (
            11,
            "#11=IFCPROPERTYSINGLEVALUE('Northings',$,IFCLENGTHMEASURE(6862020250.),$);",
        ),
        (
            12,
            "#12=IFCPROPERTYSINGLEVALUE('OrthogonalHeight',$,IFCLENGTHMEASURE(36125.),$);",
        ),
    ];
    let mut edits = vec![(7, crs), (8, conversion)];
    edits.extend(survey);
    let instances = edited(&edits, "");
    let georef = read_georeference(&instances, 0.001)
        .unwrap()
        .unwrap()
        .georeference;
    assert_eq!(
        georef.base_point(),
        ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0)
    );
    assert_eq!(georef.scale().to_bits(), 1.0_f64.to_bits());
    let survey = georef.survey_point().unwrap();
    assert!((survey.eastings - 652_010.5).abs() < 1e-6);

    // A map in metres for a model in millimetres: the scale converts millimetres to metres.
    let conversion = "#8=IFCMAPCONVERSION(#4,#7,652000.,6862000.,35.,$,$,0.001);";
    let georef = read_georeference(&edited(&[(8, conversion)], ""), 0.001)
        .unwrap()
        .unwrap()
        .georeference;
    assert_eq!(
        georef.base_point(),
        ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0)
    );
    assert!((georef.scale() - 1.0).abs() < 1e-15);
}

#[test]
fn accepts_decimal_multiples_of_the_metre_as_map_unit() {
    let cases = [
        ("$", 1.0),
        (".EXA.", 1e18),
        (".PETA.", 1e15),
        (".TERA.", 1e12),
        (".GIGA.", 1e9),
        (".MEGA.", 1e6),
        (".KILO.", 1e3),
        (".HECTO.", 1e2),
        (".DECA.", 1e1),
        (".DECI.", 1e-1),
        (".CENTI.", 1e-2),
        (".MILLI.", 1e-3),
        (".MICRO.", 1e-6),
        (".NANO.", 1e-9),
        (".PICO.", 1e-12),
        (".FEMTO.", 1e-15),
        (".ATTO.", 1e-18),
    ];
    for (prefix, size) in cases {
        let unit = format!("#3=IFCSIUNIT(*,.LENGTHUNIT.,{prefix},.METRE.);");
        let georef = read_georeference(&edited(&[(3, &unit)], ""), 1.0)
            .unwrap()
            .unwrap()
            .georeference;
        let eastings = georef.base_point().eastings;
        assert!(
            (eastings / (652_000.0 * size) - 1.0).abs() < 1e-12,
            "{prefix}"
        );
        assert!((georef.scale() - 0.9996 * size).abs() < 1e-12, "{prefix}");
    }
}

#[test]
fn rejects_unsupported_map_units() {
    let units = [
        "#3=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);",
        "#3=IFCSIUNIT(*,.LENGTHUNIT.,$,.SQUARE_METRE.);",
        "#3=IFCCONVERSIONBASEDUNIT(#23,.LENGTHUNIT.,'FOOT',#24);",
        "#3=(IFCNAMEDUNIT(*,.LENGTHUNIT.)IFCSIUNIT($,.METRE.));",
    ];
    for unit in units {
        assert_eq!(
            error(&edited(&[(3, unit)], "")),
            GeorefImportError::UnsupportedMapUnit,
            "{unit}"
        );
    }
    let crs = "#7=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#99);";
    assert_eq!(
        error(&edited(&[(7, crs)], "")),
        GeorefImportError::Dangling { reference: 99 }
    );
    let bad_unit = "#3=IFCSIUNIT(*,.LENGTHUNIT.,$);";
    assert!(matches!(
        error(&edited(&[(3, bad_unit)], "")),
        GeorefImportError::Decode { instance: 3, .. }
    ));
}

#[test]
fn reads_crs_names_and_rejects_others() {
    for (name, code) in [
        ("EPSG:2154", 2154),
        ("epsg:32631", 32631),
        (" EPSG:3946 ", 3946),
        ("EPSG:2147483647", 2_147_483_647),
    ] {
        let crs = format!("#7=IFCPROJECTEDCRS('{name}',$,$,$,$,$,$);");
        let georef = import_ok(&edited(&[(7, &crs)], "")).georeference;
        assert_eq!(georef.crs().get(), code, "{name}");
    }
    for name in [
        "'EPSG:'",
        "'EPSG:abc'",
        "'EPSG:-1'",
        "'EPSG:+5'",
        "'EPSG:0'",
        "'EPSG:2147483648'",
        "'EPSG:99999999999999999999'",
        "'IGNF:LAMB93'",
        "'EPS'",
        "$",
    ] {
        let crs = format!("#7=IFCPROJECTEDCRS({name},$,$,$,$,$,$);");
        assert_eq!(
            error(&edited(&[(7, &crs)], "")),
            GeorefImportError::UnknownCrsName,
            "{name}"
        );
    }
    assert_eq!(epsg_code("EPSG:\u{e9}"), None);
    assert_eq!(epsg_code("\u{e9}PSG:1"), None);
}

#[test]
fn rejects_wrong_or_missing_instances() {
    let geographic = "#7=IFCGEOGRAPHICCRS('EPSG:4326',$,$,$,$,$);";
    assert_eq!(
        error(&edited(&[(7, geographic)], "")),
        GeorefImportError::UnexpectedEntity { instance: 7 }
    );
    let complex = "#7=(IFCCOORDINATEREFERENCESYSTEM('EPSG:2154',$,$)IFCPROJECTEDCRS($,$,$,$));";
    assert_eq!(
        error(&edited(&[(7, complex)], "")),
        GeorefImportError::UnexpectedEntity { instance: 7 }
    );
    assert_eq!(
        error(&edited(&[(7, "")], "")),
        GeorefImportError::Dangling { reference: 7 }
    );
    let arity = "#8=IFCMAPCONVERSION(#4,#7,0.,0.,0.,$,$);";
    assert!(matches!(
        error(&edited(&[(8, arity)], "")),
        GeorefImportError::Decode { instance: 8, .. }
    ));
    let scaled_arity = "#8=IFCMAPCONVERSIONSCALED(#4,#7,0.,0.,0.,$,$,$);";
    assert!(matches!(
        error(&edited(&[(8, scaled_arity)], "")),
        GeorefImportError::Decode { instance: 8, .. }
    ));
    let crs_arity = "#7=IFCPROJECTEDCRS('EPSG:2154');";
    assert!(matches!(
        error(&edited(&[(7, crs_arity)], "")),
        GeorefImportError::Decode { instance: 7, .. }
    ));
}

#[test]
fn two_conversions_from_the_model_context_are_rejected() {
    let extra = "#18=IFCMAPCONVERSION(#4,#7,0.,0.,0.,$,$,$);";
    assert_eq!(
        error(&edited(&[], extra)),
        GeorefImportError::MultipleConversions
    );
}

#[test]
fn reads_scaled_conversions_only_with_unit_factors() {
    let unit = "#8=IFCMAPCONVERSIONSCALED(#4,#7,652000.,6862000.,35.,$,$,0.9996,1.,1.,1.);";
    let georef = import_ok(&edited(&[(8, unit)], "")).georeference;
    assert_eq!(georef.scale().to_bits(), 0.9996_f64.to_bits());
    for factors in ["0.9996,0.9996,1.", "1.,1.,2.", "1.,0.5,1."] {
        let line = format!("#8=IFCMAPCONVERSIONSCALED(#4,#7,0.,0.,0.,$,$,$,{factors});");
        assert_eq!(
            error(&edited(&[(8, &line)], "")),
            GeorefImportError::UnsupportedScaledConversion,
            "{factors}"
        );
    }
}

#[test]
fn scaled_conversions_from_other_contexts_or_crss_are_left_alone() {
    for source in ["#16", "#7"] {
        let extra = format!("#18=IFCMAPCONVERSIONSCALED({source},#7,0.,0.,0.,$,$,$,2.,2.,1.);");
        let imported = import_ok(&edited(&[], &extra));
        assert!(imported.mapped.contains(&8), "{source}");
        assert!(!imported.mapped.contains(&18), "{source}");
    }
}

#[test]
fn rejects_invalid_values() {
    let zero_axis = "#8=IFCMAPCONVERSION(#4,#7,0.,0.,0.,0.,0.,$);";
    assert_eq!(
        error(&edited(&[(8, zero_axis)], "")),
        GeorefImportError::XAxis(XAxisError::Zero)
    );
    for scale in ["0.", "-1."] {
        let line = format!("#8=IFCMAPCONVERSION(#4,#7,0.,0.,0.,$,$,{scale});");
        assert_eq!(
            error(&edited(&[(8, &line)], "")),
            GeorefImportError::Invalid(GeoreferenceError::InvalidScale),
            "{scale}"
        );
    }
    let cases = [
        (2, Field::BasePoint),
        (3, Field::BasePoint),
        (4, Field::BasePoint),
        (7, Field::Scale),
    ];
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (index, field) in cases {
            let mut instances = fixture();
            set_parameter(&mut instances, 8, index, Parameter::Real(bad));
            assert_eq!(
                error(&instances),
                GeorefImportError::Invalid(GeoreferenceError::NonFinite { field }),
                "{index}"
            );
        }
        let mut instances = fixture();
        set_parameter(&mut instances, 8, 5, Parameter::Real(bad));
        assert_eq!(
            error(&instances),
            GeorefImportError::XAxis(XAxisError::NonFinite)
        );
    }
    for length_unit in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            read_georeference(&fixture(), length_unit),
            Err(GeorefImportError::InvalidLengthUnit)
        );
    }
}

/// Why the survey point of the edited fixture is not valid. The import itself succeeds.
fn survey_error(edits: &[(u64, &str)], extra: &str) -> SurveyPointError {
    import_ok(&edited(edits, extra))
        .survey_error
        .expect("the survey point is not valid")
}

const SET: &str = "IFCPROPERTYSET('1kTvXnbbzCWw8lcMd1dR4o',$,'BIMCraft_SurveyPoint',$,";
const REL: &str = "IFCRELDEFINESBYPROPERTIES('2kTvXnbbzCWw8lcMd1dR4o',$,$,$,";

#[test]
fn rejects_a_second_survey_point() {
    let extras = [
        "#18=IFCRELDEFINESBYPROPERTIES('3kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#13);".to_owned(),
        format!("#18={SET}(#10,#11,#12));\n#19={REL}(#9),#18);"),
    ];
    for extra in extras {
        assert_eq!(
            survey_error(&[], &extra),
            SurveyPointError::MultipleSets,
            "{extra}"
        );
    }
}

#[test]
fn rejects_a_survey_point_not_on_one_site() {
    use SurveyPointError as E;
    let cases = [
        ("(#17),#13", E::NotOnSite),
        ("(#9,#17),#13", E::NotOnSite),
        ("(#97),#13", E::Dangling { reference: 97 }),
        ("#9,#13", E::Malformed),
    ];
    for (parameters, expected) in cases {
        let line = format!("#14={REL}{parameters});");
        assert_eq!(survey_error(&[(14, &line)], ""), expected, "{parameters}");
    }
}

#[test]
fn rejects_malformed_survey_point_sets() {
    use SurveyPointError as E;
    use SurveyProperty as P;
    let sets = [
        ("(#10,#11,#96))", E::Dangling { reference: 96 }),
        ("(#10,#11))", E::Missing(P::OrthogonalHeight)),
        ("(#10,#11,#12,#10))", E::Duplicate(P::Eastings)),
        ("(#10,#11,#12,#17))", E::Malformed),
        ("())", E::Malformed),
        ("#10)", E::Malformed),
    ];
    for (properties, expected) in sets {
        let line = format!("#13={SET}{properties};");
        assert_eq!(survey_error(&[(13, &line)], ""), expected, "{properties}");
    }
}

#[test]
fn rejects_malformed_survey_point_properties() {
    use SurveyPointError as E;
    use SurveyProperty as P;
    let northings = [
        ("'Northings',$,IFCREAL(1.),$", E::NotALength(P::Northings)),
        ("'Northings',$,$,$", E::NotALength(P::Northings)),
        (
            "'Northings',$,IFCLENGTHMEASURE(1.),#17",
            E::UnsupportedUnit(P::Northings),
        ),
        (
            "'Northings',$,IFCLENGTHMEASURE(1.),#18",
            E::UnsupportedUnit(P::Northings),
        ),
        (
            "'Northings',$,IFCLENGTHMEASURE(1.),#19",
            E::UnsupportedUnit(P::Northings),
        ),
        (
            "'Northings',$,IFCLENGTHMEASURE(1.),#97",
            E::Dangling { reference: 97 },
        ),
        ("'Width',$,IFCLENGTHMEASURE(1.),$", E::Malformed),
    ];
    let units = "#18=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);\n\
                 #19=IFCSIUNIT(*,.LENGTHUNIT.,$,.SQUARE_METRE.);";
    for (parameters, expected) in northings {
        let line = format!("#11=IFCPROPERTYSINGLEVALUE({parameters});");
        assert_eq!(
            survey_error(&[(11, &line)], units),
            expected,
            "{parameters}"
        );
    }
    for bad in [f64::NAN, f64::INFINITY] {
        let mut instances = fixture();
        set_parameter(
            &mut instances,
            12,
            2,
            typed("IFCLENGTHMEASURE", Parameter::Real(bad)),
        );
        assert_eq!(
            import_ok(&instances).survey_error,
            Some(E::NonFinite(P::OrthogonalHeight))
        );
    }
    let complex = "#12=(IFCPROPERTY('OrthogonalHeight',$)IFCSIMPLEPROPERTY()\
                   IFCPROPERTYSINGLEVALUE(IFCLENGTHMEASURE(1.),$));";
    assert_eq!(survey_error(&[(12, complex)], ""), E::Malformed);
}

#[test]
fn survey_point_lengths_are_in_their_unit_or_in_the_project_unit() {
    let property = |name, value, unit| {
        format!("IFCPROPERTYSINGLEVALUE('{name}',$,IFCLENGTHMEASURE({value}),{unit});")
    };
    let units = "#18=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n\
                 #19=IFCSIUNIT(*,.LENGTHUNIT.,.KILO.,.METRE.);";
    let edits = [
        (
            10,
            format!("#10={}", property("Eastings", "652010500.", "#18")),
        ),
        (
            11,
            format!("#11={}", property("Northings", "6862.02025", "#19")),
        ),
        (
            12,
            format!("#12={}", property("OrthogonalHeight", "36125.", "$")),
        ),
    ];
    let edits: Vec<(u64, &str)> = edits
        .iter()
        .map(|(id, line)| (*id, line.as_str()))
        .collect();
    // A project in millimetres: the last property has no unit, so it is in millimetres.
    let instances = edited(&edits, units);
    let imported = read_georeference(&instances, 0.001).unwrap().unwrap();
    assert_eq!(imported.survey_error, None);
    let survey = imported.georeference.survey_point().unwrap();
    assert!((survey.eastings - 652_010.5).abs() < 1e-6);
    assert!((survey.northings - 6_862_020.25).abs() < 1e-6);
    assert!((survey.height - 36.125).abs() < 1e-9);
    // The same file in metres reads the last property in metres.
    let survey = import_ok(&instances).georeference.survey_point().unwrap();
    assert!((survey.height - 36_125.0).abs() < 1e-9);
    // A length that overflows once converted is refused, not turned into infinity.
    let huge = [(
        10,
        "#10=IFCPROPERTYSINGLEVALUE('Eastings',$,IFCLENGTHMEASURE(1.E300),#20);",
    )];
    let exa = "#20=IFCSIUNIT(*,.LENGTHUNIT.,.EXA.,.METRE.);";
    assert_eq!(
        survey_error(&huge, exa),
        SurveyPointError::NonFinite(SurveyProperty::Eastings)
    );
}

#[test]
fn a_malformed_survey_point_is_replaced_not_kept() {
    // The set and its relationship are mapped, so an export that writes a survey point does
    // not leave a second set behind.
    let one_missing = edited(&[(13, &format!("#13={SET}(#10,#11));"))], "");
    let imported = import_ok(&one_missing);
    assert_eq!(
        imported.survey_error,
        Some(SurveyPointError::Missing(SurveyProperty::OrthogonalHeight))
    );
    assert!(imported.mapped.is_superset(&BTreeSet::from([13, 14])));
    assert!(!imported.mapped.contains(&10));
    // Two sets: both go.
    let extra = "#18=IFCRELDEFINESBYPROPERTIES('3kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#13);";
    let imported = import_ok(&edited(&[], extra));
    assert_eq!(imported.survey_error, Some(SurveyPointError::MultipleSets));
    assert!(imported.mapped.is_superset(&BTreeSet::from([13, 14, 18])));
}

#[test]
fn the_metre_unit_of_the_crs_is_reused() {
    assert_eq!(import_ok(&fixture()).targets.metre_unit, Some(3));
    let cases = [
        // No map unit: the file's unit is not necessarily the metre.
        ("#7=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,$);", None),
        // A prefixed metre is read but not reused as the metre.
        ("#7=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#18);", None),
        ("#7=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#19);", Some(19)),
    ];
    let units = "#18=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n\
                 #19=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);";
    for (crs, metre_unit) in cases {
        assert_eq!(
            import_ok(&edited(&[(7, crs)], units)).targets.metre_unit,
            metre_unit,
            "{crs}"
        );
    }
}

#[test]
fn other_property_sets_and_relationships_are_not_the_survey_point() {
    let extra = "#18=IFCPROPERTYSET('4kTvXnbbzCWw8lcMd1dR4o',$,'Pset_SiteCommon',$,(#10));\n\
                 #19=IFCRELDEFINESBYPROPERTIES('5kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#18);\n\
                 #20=IFCRELDEFINESBYPROPERTIES('6kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#95);\n\
                 #21=IFCRELDEFINESBYPROPERTIES('7kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#17);\n\
                 #22=IFCRELDEFINESBYPROPERTIES('8kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9));\n\
                 #23=IFCRELDEFINESBYPROPERTIES('9kTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#24);\n\
                 #24=(IFCPROPERTYSET('AkTvXnbbzCWw8lcMd1dR4o',$,'BIMCraft_SurveyPoint',$,(#10)));\n\
                 #25=(IFCRELDEFINESBYPROPERTIES('BkTvXnbbzCWw8lcMd1dR4o',$,$,$,(#9),#13));";
    let imported = import_ok(&edited(&[], extra));
    assert!(imported.georeference.survey_point().is_some());
    assert_eq!(imported.mapped, BTreeSet::from([7, 8, 10, 11, 12, 13, 14]));
    // Without a survey point property set, there is no survey point.
    let imported = import_ok(&edited(&[(14, "")], ""));
    assert_eq!(imported.georeference.survey_point(), None);
    assert_eq!(imported.targets.site, None);
}

fn sample(survey: Option<ProjectedPoint>) -> Georeference {
    Georeference::new(
        EpsgCode::new(2154).unwrap(),
        ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0),
        0.0,
        0.9996,
        survey,
    )
    .unwrap()
}

fn written(instances: &[Instance]) -> String {
    let mut writer = StepWriter::new(Vec::new(), &[]).unwrap();
    for instance in instances {
        writer.write_instance(instance).unwrap();
    }
    let text = String::from_utf8(writer.finish().unwrap()).unwrap();
    let data = text.split_once("DATA;\n").unwrap().1;
    data.split_once("ENDSEC;").unwrap().0.to_owned()
}

#[test]
fn writes_the_crs_and_the_conversion() {
    let instances = write_georeference(
        &sample(None),
        FRESH,
        &Kept::default(),
        1.0,
        100,
        &mut SeededIdSource::new(1),
    )
    .unwrap();
    assert_eq!(
        written(&instances),
        "#100=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#102);\n\
         #101=IFCMAPCONVERSION(#4,#100,652000.0,6862000.0,35.0,1.0,0.0,0.9996);\n\
         #102=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n"
    );
}

#[test]
fn writes_the_map_unit_and_the_survey_point_in_metres_whatever_the_project_unit() {
    let survey = Some(ProjectedPoint::new(652_010.5, 6_862_020.25, 36.125));
    let mut ids = SeededIdSource::new(7);
    let mut expected_ids = SeededIdSource::new(7);
    let set_id = expected_ids.next_id().to_global_id();
    let relationship_id = expected_ids.next_id().to_global_id();
    // A unit that is a power of two keeps the scale, 0.9996 * 0.5, exact in the text.
    let instances =
        write_georeference(&sample(survey), FRESH, &Kept::default(), 0.5, 1, &mut ids).unwrap();
    assert_eq!(
        written(&instances),
        format!(
            "#1=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#3);\n\
             #2=IFCMAPCONVERSION(#4,#1,652000.0,6862000.0,35.0,1.0,0.0,0.4998);\n\
             #3=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n\
             #4=IFCPROPERTYSINGLEVALUE('Eastings',$,IFCLENGTHMEASURE(652010.5),#3);\n\
             #5=IFCPROPERTYSINGLEVALUE('Northings',$,IFCLENGTHMEASURE(6862020.25),#3);\n\
             #6=IFCPROPERTYSINGLEVALUE('OrthogonalHeight',$,IFCLENGTHMEASURE(36.125),#3);\n\
             #7=IFCPROPERTYSET('{set_id}',$,'BIMCraft_SurveyPoint',$,(#4,#5,#6));\n\
             #8=IFCRELDEFINESBYPROPERTIES('{relationship_id}',$,$,$,(#9),#7);\n"
        )
    );
}

#[test]
fn reuses_the_metre_unit_it_is_given() {
    let survey = Some(ProjectedPoint::new(1.0, 2.0, 3.0));
    let targets = Targets {
        metre_unit: Some(77),
        ..FRESH
    };
    let instances = write_georeference(
        &sample(survey),
        targets,
        &Kept::default(),
        1.0,
        10,
        &mut SeededIdSource::new(1),
    )
    .unwrap();
    let text = written(&instances);
    assert!(!text.contains("IFCSIUNIT"));
    assert!(text.starts_with("#10=IFCPROJECTEDCRS('EPSG:2154',$,$,$,$,$,#77);\n"));
    assert_eq!(text.matches(",#77)").count(), 4);
    assert_eq!(instances.len(), 7);
}

#[test]
fn writes_back_what_it_kept() {
    let imported = import_ok(&fixture());
    assert_eq!(
        imported.kept.survey_global_ids,
        Some([
            "1kTvXnbbzCWw8lcMd1dR4o".to_owned(),
            "2kTvXnbbzCWw8lcMd1dR4o".to_owned()
        ])
    );
    let ids = &mut SeededIdSource::new(1);
    let instances =
        write_georeference(&imported.georeference, FRESH, &imported.kept, 1.0, 100, ids).unwrap();
    let text = written(&instances);
    assert!(text.starts_with(
        "#100=IFCPROJECTEDCRS('EPSG:2154','RGF93 v1 / Lambert-93','RGF93 v1',$,$,$,#102);\n"
    ));
    assert!(text.contains("IFCPROPERTYSET('1kTvXnbbzCWw8lcMd1dR4o',"));
    assert!(text.contains("IFCRELDEFINESBYPROPERTIES('2kTvXnbbzCWw8lcMd1dR4o',"));

    // Under another CRS, the attributes of the old one no longer apply.
    let other = Georeference::new(
        EpsgCode::new(3946).unwrap(),
        imported.georeference.base_point(),
        0.0,
        1.0,
        None,
    )
    .unwrap();
    let instances = write_georeference(&other, FRESH, &imported.kept, 1.0, 100, ids).unwrap();
    assert!(written(&instances).starts_with("#100=IFCPROJECTEDCRS('EPSG:3946',$,$,$,$,$,#102);\n"));
}

fn scaled(scale: f64) -> Georeference {
    Georeference::new(
        EpsgCode::new(2154).unwrap(),
        ProjectedPoint::new(0.0, 0.0, 0.0),
        0.0,
        scale,
        None,
    )
    .unwrap()
}

#[test]
fn rejects_what_it_cannot_write() {
    let survey = Some(ProjectedPoint::new(1.0, 2.0, 3.0));
    let ids = &mut SeededIdSource::new(1);
    let write = |georeference: &Georeference, targets, length_unit, first_id, ids: &mut _| {
        write_georeference(
            georeference,
            targets,
            &Kept::default(),
            length_unit,
            first_id,
            ids,
        )
    };
    assert_eq!(
        write(&sample(survey), NO_SITE, 1.0, 1, ids),
        Err(GeorefExportError::SurveyPointWithoutSite)
    );
    for length_unit in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            write(&sample(None), NO_SITE, length_unit, 1, ids),
            Err(GeorefExportError::InvalidLengthUnit)
        );
    }
    // The scale times the unit overflows, or underflows to zero.
    for (scale, length_unit) in [(1e10, 1e300), (1e-300, 1e-300)] {
        assert_eq!(
            write(&scaled(scale), NO_SITE, length_unit, 1, ids),
            Err(GeorefExportError::ScaleOutOfRange),
            "{scale} {length_unit}"
        );
    }
    // The CRS, the conversion and the metre unit take 3 ids; with a survey point, 8.
    assert_eq!(
        write(&sample(None), NO_SITE, 1.0, u64::MAX - 1, ids),
        Err(GeorefExportError::IdOverflow)
    );
    assert!(write(&sample(None), NO_SITE, 1.0, u64::MAX - 2, ids).is_ok());
    let reuse = Targets {
        metre_unit: Some(3),
        ..NO_SITE
    };
    assert!(write(&sample(None), reuse, 1.0, u64::MAX - 1, ids).is_ok());
    assert_eq!(
        write(&sample(survey), FRESH, 1.0, u64::MAX - 6, ids),
        Err(GeorefExportError::IdOverflow)
    );
    assert!(write(&sample(survey), FRESH, 1.0, u64::MAX - 7, ids).is_ok());
}

#[test]
fn messages_never_quote_values() {
    let messages = [
        GeorefImportError::InvalidLengthUnit.to_string(),
        GeorefImportError::Dangling { reference: 3 }.to_string(),
        GeorefImportError::UnexpectedEntity { instance: 3 }.to_string(),
        GeorefImportError::MultipleConversions.to_string(),
        GeorefImportError::UnknownCrsName.to_string(),
        GeorefImportError::UnsupportedMapUnit.to_string(),
        GeorefImportError::UnsupportedScaledConversion.to_string(),
        SurveyPointError::MultipleSets.to_string(),
        SurveyPointError::NotOnSite.to_string(),
        SurveyPointError::Dangling { reference: 2 }.to_string(),
        SurveyPointError::Missing(SurveyProperty::Eastings).to_string(),
        SurveyPointError::Duplicate(SurveyProperty::Northings).to_string(),
        SurveyPointError::NotALength(SurveyProperty::OrthogonalHeight).to_string(),
        SurveyPointError::UnsupportedUnit(SurveyProperty::Northings).to_string(),
        SurveyPointError::NonFinite(SurveyProperty::Eastings).to_string(),
        SurveyPointError::Malformed.to_string(),
        GeorefExportError::InvalidLengthUnit.to_string(),
        GeorefExportError::ScaleOutOfRange.to_string(),
        GeorefExportError::SurveyPointWithoutSite.to_string(),
        GeorefExportError::IdOverflow.to_string(),
    ];
    assert_eq!(
        messages,
        [
            "the length unit must be a finite number greater than zero",
            "a georeference instance refers to a missing instance",
            "instance 3 has an unexpected entity",
            "more than one map conversion starts from the model context",
            "the projected CRS is not named after an EPSG code",
            "the map unit is not supported",
            "map conversions with scale factors per axis are not supported",
            "the file holds more than one survey point",
            "the survey point is not attached to exactly one site",
            "the survey point refers to a missing instance",
            "the survey point has no Eastings property",
            "the survey point has more than one Northings property",
            "the OrthogonalHeight property of the survey point is not a length",
            "the Northings property of the survey point has an unsupported unit",
            "the Eastings property of the survey point is not finite",
            "the survey point property set is malformed",
            "the length unit must be a finite number greater than zero",
            "the scale does not fit the length unit",
            "the survey point needs a site",
            "no instance ids are left",
        ]
    );
}

mod properties {
    use proptest::prelude::*;

    use super::*;
    use crate::step::test_support::parameter;

    /// The instances the georeference reads, and the units and site it looks at.
    const READ: [u64; 10] = [3, 4, 7, 8, 9, 10, 11, 12, 13, 14];

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Any value in any attribute of the instances the georeference reads gives a
        /// georeference or a typed error, never a panic (ENG-02).
        #[test]
        fn hostile_values_give_typed_errors(
            edits in proptest::collection::vec(
                (proptest::sample::select(&READ[..]), 0..16_usize, parameter()),
                1..4,
            ),
            length_unit in prop_oneof![Just(1.0), Just(0.001), any::<f64>()],
        ) {
            let mut instances = fixture();
            for (id, index, value) in edits {
                set_parameter(&mut instances, id, index, value);
            }
            if let Ok(Some(imported)) = read_georeference(&instances, length_unit) {
                prop_assert!(imported.georeference.scale() > 0.0);
                prop_assert!(imported.mapped.contains(&imported.crs));
            }
        }
    }
}
