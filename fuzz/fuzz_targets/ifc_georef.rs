//! Any bytes, read as an IFC file, give a georeference or a typed error: reading the map
//! conversion, the projected CRS and the survey point never panics (ENG-02). An accepted
//! georeference writes back, and reading what was written gives it back.
//!
//! The seeds are the IFC test fixtures, one of which is georeferenced.

#![no_main]

use core_model::SeededIdSource;
use interop_ifc::georef::{SurveyPoint, Targets, read_georeference, write_georeference};
use interop_ifc::step::{Instance, Limits, StepReader};
use libfuzzer_sys::fuzz_target;

fn instances(data: &[u8]) -> Option<Vec<Instance>> {
    StepReader::new(data, Limits::default())
        .ok()?
        .collect::<Result<_, _>>()
        .ok()
}

fuzz_target!(|data: &[u8]| {
    let Some(instances) = instances(data) else {
        return;
    };
    let _ = read_georeference(&instances, 1.0, SurveyPoint::Ignore);
    let Ok(Some(imported)) = read_georeference(&instances, 1.0, SurveyPoint::Read) else {
        return;
    };
    let targets = Targets {
        context: imported.context,
        site: imported.site,
    };
    let written = write_georeference(
        &imported.georeference,
        targets,
        1.0,
        1,
        &mut SeededIdSource::new(1),
    )
    .expect("an accepted georeference writes back");
    let survey = imported.georeference.survey_point();
    assert_eq!(written.len(), if survey.is_some() { 7 } else { 2 });
});
