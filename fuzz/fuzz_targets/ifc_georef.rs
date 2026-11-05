//! Any bytes, read as an IFC file, give a georeference or a typed error: reading the map
//! conversion, the projected CRS and the survey point never panics (ENG-02). An accepted
//! georeference writes back, as the instances its survey point needs. A survey point that is not
//! valid is reported with the import, never as a failure.
//!
//! The seeds are the IFC test fixtures, one of which is georeferenced.

#![no_main]

use core_model::SeededIdSource;
use interop_ifc::georef::{read_georeference, write_georeference};
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
    let Ok(Some(imported)) = read_georeference(&instances, 1.0) else {
        return;
    };
    let written = write_georeference(
        &imported.georeference,
        imported.targets,
        &imported.kept,
        1.0,
        1,
        &mut SeededIdSource::new(1),
    )
    .expect("an accepted georeference writes back");
    // The CRS and the conversion, a metre unit unless the file's is reused, and a survey point's
    // three properties, set and relationship.
    let metre = usize::from(imported.targets.metre_unit.is_none());
    let survey = if imported.georeference.survey_point().is_some() {
        5
    } else {
        0
    };
    assert_eq!(written.len(), 2 + metre + survey);
});
