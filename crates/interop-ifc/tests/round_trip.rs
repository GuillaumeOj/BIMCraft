//! Writes the sample files back and checks that nothing is lost and the output is deterministic.

use interop_ifc::step::{Instance, Limits, Record, StepError, StepReader, StepWriter, WriteError};

const CORPUS: [(&str, &str); 2] = [
    ("wall.ifc", include_str!("fixtures/wall.ifc")),
    ("edge-cases.ifc", include_str!("fixtures/edge-cases.ifc")),
];

type File = (Vec<Record>, Vec<Instance>);

fn read(input: &[u8]) -> Result<File, StepError> {
    let reader = StepReader::new(input, Limits::default())?;
    let header = reader.header().to_vec();
    Ok((header, reader.collect::<Result<_, _>>()?))
}

fn write((header, instances): &File) -> Result<Vec<u8>, WriteError> {
    let mut writer = StepWriter::new(Vec::new(), header)?;
    for instance in instances {
        writer.write_instance(instance)?;
    }
    writer.finish()
}

/// Reading the output back yields the file read, and writing that again yields the same bytes.
#[test]
fn reads_back_what_it_writes_and_writes_the_same_bytes() {
    for (name, file) in CORPUS {
        let file = read(file.as_bytes()).unwrap();
        let written = write(&file).unwrap();
        let read_back = read(&written).unwrap();
        assert_eq!(read_back, file, "{name}");
        assert_eq!(write(&read_back).unwrap(), written, "{name}");
    }
}

#[test]
fn writes_the_wall_file_in_canonical_form() {
    let file = read(CORPUS[0].1.as_bytes()).unwrap();
    let written = String::from_utf8(write(&file).unwrap()).unwrap();
    assert_eq!(
        written,
        "ISO-10303-21;\nHEADER;\n\
         FILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');\n\
         FILE_NAME('wall.ifc','2026-10-13T12:00:00',('Author'),('Office'),'','BIMCraft','');\n\
         FILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n\
         #1=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'Project',$,$,$,$,(#4),#2);\n\
         #2=IFCUNITASSIGNMENT((#3));\n\
         #3=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n\
         #4=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-5,#5,$);\n\
         #5=IFCAXIS2PLACEMENT3D(#6,$,$);\n\
         #6=IFCCARTESIANPOINT((0.0,0.0,0.0));\n\
         #7=IFCWALL('2O2Fr$t4X7Zf8NOew3FLOH',$,'Wall \\X2\\00E9\\X0\\',$,$,$,$,$,.STANDARD.);\n\
         #8=IFCPROPERTYSINGLEVALUE('Width',$,IFCLENGTHMEASURE(0.2),$);\n\
         ENDSEC;\nEND-ISO-10303-21;\n"
    );
}
