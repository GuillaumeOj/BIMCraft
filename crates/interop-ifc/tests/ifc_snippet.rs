//! Reads small hand-written IFC 4.3 files end to end. The files in `fixtures/` also seed the
//! `step_reader` fuzz target.

use interop_ifc::step::{Instance, InstanceBody, Limits, Parameter, Record, StepReader};

const WALL: &str = include_str!("fixtures/wall.ifc");
const SYNTAX: &str = include_str!("fixtures/syntax.ifc");

#[test]
fn reads_a_small_ifc_file() {
    let reader = StepReader::new(WALL.as_bytes(), Limits::default()).unwrap();
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
        Instance {
            id: 1,
            body: InstanceBody::Complex(vec![
                record("IFCNAMEDUNIT", vec![Derived, enumeration("LENGTHUNIT")]),
                record(
                    "IFCSIUNIT",
                    vec![enumeration("MILLI"), enumeration("METRE")],
                ),
            ]),
        },
        Instance {
            id: 2,
            body: InstanceBody::Simple(record(
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
        },
        Instance {
            id: 3,
            body: InstanceBody::Simple(record(
                "!PRIVATE_ENTITY",
                vec![
                    Parameter::String("It's a long string wrapped on two lines".to_owned()),
                    Integer(-12),
                    Real(350.0),
                    List(vec![Reference(1), List(vec![Reference(2), Null])]),
                ],
            )),
        },
        Instance {
            id: 10,
            body: InstanceBody::Simple(record(
                "IFCCARTESIANPOINT",
                vec![List(vec![Real(-1.5), Real(2.0), Real(0.25)])],
            )),
        },
    ];
    assert_eq!(instances, expected);
}

#[test]
fn decodes_every_instance_into_an_entity_class() {
    use interop_ifc::ifc::{Entity, IfcLabel, IfcLengthMeasure, IfcValue, IfcWallTypeEnum};

    let reader = StepReader::new(WALL.as_bytes(), Limits::default()).unwrap();
    let entities = reader
        .map(|instance| Entity::from_instance(&instance.unwrap().body))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let names: Vec<&str> = entities.iter().map(Entity::step_name).collect();
    assert_eq!(
        names,
        [
            "IFCPROJECT",
            "IFCUNITASSIGNMENT",
            "IFCSIUNIT",
            "IFCGEOMETRICREPRESENTATIONCONTEXT",
            "IFCAXIS2PLACEMENT3D",
            "IFCCARTESIANPOINT",
            "IFCWALL",
            "IFCPROPERTYSINGLEVALUE",
        ]
    );

    let Entity::IfcWall(wall) = &entities[6] else {
        panic!("the seventh instance is a wall");
    };
    assert_eq!(wall.global_id.0, "2O2Fr$t4X7Zf8NOew3FLOH");
    assert_eq!(wall.name, Some(IfcLabel(r"Wall \X2\00E9\X0\".to_owned())));
    assert_eq!(wall.predefined_type, Some(IfcWallTypeEnum::Standard));

    let Entity::IfcPropertySingleValue(width) = &entities[7] else {
        panic!("the eighth instance is a property");
    };
    assert_eq!(
        width.nominal_value,
        Some(IfcValue::IfcLengthMeasure(IfcLengthMeasure(0.2)))
    );
}
