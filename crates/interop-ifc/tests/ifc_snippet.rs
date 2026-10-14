//! Reads small hand-written IFC 4.3 files end to end. The files in `fixtures/` also seed the
//! `step_reader` fuzz target.

use interop_ifc::step::{Instance, InstanceBody, Limits, Parameter, Record, StepReader};

const FILE: &str = include_str!("fixtures/wall.ifc");
const SYNTAX: &str = include_str!("fixtures/syntax.ifc");

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

fn record(name: &str, parameters: Vec<Parameter>) -> Record {
    Record {
        name: name.to_owned(),
        parameters,
    }
}

fn instance(id: u64, body: InstanceBody) -> Instance {
    Instance { id, body }
}

fn enumeration(value: &str) -> Parameter {
    Parameter::Enumeration(value.to_owned())
}

#[test]
fn reads_complex_instances_binaries_and_several_data_sections() {
    use Parameter::{Binary, Derived, Integer, List, Null, Real, Reference};

    let reader = StepReader::new(SYNTAX.as_bytes(), Limits::default()).unwrap();
    assert_eq!(reader.header().len(), 3);
    let instances = reader.collect::<Result<Vec<_>, _>>().unwrap();
    let expected = [
        instance(
            1,
            InstanceBody::Complex(vec![
                record("IFCNAMEDUNIT", vec![Derived, enumeration("LENGTHUNIT")]),
                record(
                    "IFCSIUNIT",
                    vec![enumeration("MILLI"), enumeration("METRE")],
                ),
            ]),
        ),
        instance(
            2,
            InstanceBody::Simple(record(
                "IFCPIXELTEXTURE",
                vec![
                    Null,
                    Null,
                    Null,
                    Null,
                    List(vec![]),
                    Integer(2),
                    Integer(1),
                    Integer(2),
                    List(vec![Binary("0FF".to_owned()), Binary("3E".to_owned())]),
                ],
            )),
        ),
        instance(
            3,
            InstanceBody::Simple(record(
                "!PRIVATE_ENTITY",
                vec![
                    Parameter::String("It's a long string wrapped on two lines".to_owned()),
                    Integer(-12),
                    Real(350.0),
                    List(vec![Reference(1), List(vec![Reference(2), Null])]),
                ],
            )),
        ),
        instance(
            10,
            InstanceBody::Simple(record(
                "IFCCARTESIANPOINT",
                vec![List(vec![Real(-1.5), Real(2.0), Real(0.25)])],
            )),
        ),
    ];
    assert_eq!(instances, expected);
}
