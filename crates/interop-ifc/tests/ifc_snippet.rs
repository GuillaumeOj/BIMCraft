//! Reads a small hand-written IFC 4.3 file end to end.

use interop_ifc::step::{InstanceBody, Limits, Parameter, StepReader};

const FILE: &str = r"ISO-10303-21;
HEADER;
/* Written for this test; not taken from a real project. */
FILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');
FILE_NAME('wall.ifc','2026-10-13T12:00:00',('Author'),('Office'),'','BIMCraft','');
FILE_SCHEMA(('IFC4X3_ADD2'));
ENDSEC;
DATA;
#1=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'Project',$,$,$,$,(#4),#2);
#2=IFCUNITASSIGNMENT((#3));
#3=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#4=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#5,$);
#5=IFCAXIS2PLACEMENT3D(#6,$,$);
#6=IFCCARTESIANPOINT((0.,0.,0.));
#7=IFCWALL('2O2Fr$t4X7Zf8NOew3FLOH',$,'Wall \X2\00E9\X0\',$,$,$,$,$,.STANDARD.);
#8=IFCPROPERTYSINGLEVALUE('Width',$,IFCLENGTHMEASURE(0.2),$);
ENDSEC;
END-ISO-10303-21;
";

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
