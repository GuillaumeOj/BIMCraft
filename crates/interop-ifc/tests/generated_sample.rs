//! Compiles the code `express` generates for its sample schema against this crate's decode
//! runtime, under the workspace lints, and runs the tests generated with it.

#[allow(
    dead_code,
    clippy::too_many_lines,
    reason = "markers of abstract entities are only reference targets, and generated matches \
              and tests have one arm or check per declaration of the schema"
)]
mod sample {
    include!("../../express/tests/snapshots/sample.rs");
}

use interop_ifc::ifc::decode::{DecodeError, Parameter, Record, Ref, ValueError};
use sample::{Entity, Group, Id, Kind, Measure, Ratio, Value, Wall};

fn record(name: &str, parameters: Vec<Parameter>) -> Record {
    Record {
        name: name.to_owned(),
        parameters,
    }
}

fn string(value: &str) -> Parameter {
    Parameter::String(value.to_owned())
}

#[test]
fn decodes_inherited_and_derived_attributes() {
    let group = record(
        "GROUP",
        vec![
            string("g"),
            Parameter::Derived,
            Parameter::List(vec![Parameter::Reference(4)]),
        ],
    );
    let Entity::Group(group) = Entity::from_record(&group).unwrap() else {
        panic!("a group");
    };
    assert_eq!(
        *group,
        Group {
            tag: Id("g".to_owned()),
            members: vec![Ref::new(4)],
        }
    );
}

#[test]
fn decodes_nested_aggregates_and_selects() {
    let point = Parameter::List(vec![Parameter::Real(0.0); 3]);
    let wall = record(
        "WALL",
        vec![
            string("w"),
            Parameter::Null,
            Parameter::Null,
            Parameter::Enumeration("T_BEAM".to_owned()),
            Parameter::List(vec![point.clone(), point]),
            Parameter::Null,
            Parameter::Typed {
                type_name: "RATIO".to_owned(),
                value: Box::new(Parameter::Real(0.5)),
            },
            Parameter::Enumeration("T".to_owned()),
        ],
    );
    let Entity::Wall(wall) = Entity::from_record(&wall).unwrap() else {
        panic!("a wall");
    };
    let Wall {
        kind, points, mark, ..
    } = *wall;
    assert_eq!(kind, Kind::TBeam);
    assert_eq!(points, vec![vec![0.0; 3]; 2]);
    assert_eq!(mark, sample::Target::Ratio(Ratio(0.5)));
}

#[test]
fn reports_where_a_record_goes_wrong() {
    let short = record("GROUP", vec![string("g")]);
    assert_eq!(
        Entity::from_record(&short),
        Err(DecodeError::WrongArity {
            expected: 3,
            found: 1
        })
    );
    let not_derived = record(
        "GROUP",
        vec![string("g"), string("n"), Parameter::List(Vec::new())],
    );
    assert_eq!(
        Entity::from_record(&not_derived),
        Err(DecodeError::NotDerived { index: 1 })
    );
    let empty = record(
        "GROUP",
        vec![string("g"), Parameter::Derived, Parameter::List(Vec::new())],
    );
    assert_eq!(
        Entity::from_record(&empty),
        Err(DecodeError::Attribute {
            index: 2,
            error: ValueError::WrongCount {
                min: 1,
                max: None,
                found: 0
            }
        })
    );
}

#[test]
fn selects_accept_only_their_members() {
    let label = Parameter::Typed {
        type_name: "LABEL".to_owned(),
        value: Box::new(string("x")),
    };
    assert_eq!(
        interop_ifc::ifc::decode::decode::<Measure>(&label),
        Err(ValueError::NotInSelect)
    );
    assert_eq!(
        interop_ifc::ifc::decode::decode::<Value>(&Parameter::Reference(1)),
        Err(ValueError::NotInSelect)
    );
    let unknown = Parameter::Enumeration("ROUND".to_owned());
    assert_eq!(
        interop_ifc::ifc::decode::decode::<Kind>(&unknown),
        Err(ValueError::UnknownItem)
    );
}
