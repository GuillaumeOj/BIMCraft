//! Reads a small hand-written IFC 4.3 file end to end.

use interop_ifc::step::{InstanceBody, Limits, Parameter, StepReader};

const FILE: &str = include_str!("fixtures/wall.ifc");

#[test]
fn reads_a_small_ifc_file() {
    let reader = StepReader::new(FILE.as_bytes(), Limits::default()).unwrap();
    let schemas: Vec<&str> = reader.header().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(schemas, ["FILE_DESCRIPTION", "FILE_NAME", "FILE_SCHEMA"]);

    let instances = reader.collect::<Result<Vec<_>, _>>().unwrap();
    let ids: Vec<u64> = instances.iter().map(|i| i.id).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8]);

    let InstanceBody::Simple(wall) = &instances[6].body else {
        panic!("the wall is a simple instance");
    };
    assert_eq!(wall.name, "IFCWALL");
    assert_eq!(
        wall.parameters[2],
        Parameter::String(r"Wall \X2\00E9\X0\".to_owned())
    );
    assert_eq!(
        wall.parameters[8],
        Parameter::Enumeration("STANDARD".to_owned())
    );

    let InstanceBody::Simple(width) = &instances[7].body else {
        panic!("the property is a simple instance");
    };
    assert_eq!(
        width.parameters[2],
        Parameter::Typed {
            type_name: "IFCLENGTHMEASURE".to_owned(),
            value: Box::new(Parameter::Real(0.2)),
        }
    );
}
