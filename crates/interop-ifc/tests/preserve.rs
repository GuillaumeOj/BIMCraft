//! IFC data the importer does not map survives a round-trip (INT-03): kept on import, written
//! back on export, byte-equivalent to the file read once both are normalised.

use std::collections::{BTreeMap, BTreeSet};

use interop_ifc::preserve::{Dangling, Preserved, normalise};
use interop_ifc::step::{Instance, Limits, Record, StepError, StepReader};

const FILE: &str = include_str!("fixtures/unmapped.ifc");

fn read(input: &[u8]) -> Result<(Vec<Record>, Vec<Instance>), StepError> {
    let reader = StepReader::new(input, Limits::default())?;
    let header = reader.header().to_vec();
    Ok((header, reader.collect::<Result<_, _>>()?))
}

/// What the tests map: the storey and the walls.
fn is_mapped(record: &Record) -> bool {
    matches!(record.name.as_str(), "IFCBUILDINGSTOREY" | "IFCWALL")
}

#[test]
fn an_export_that_keeps_the_ids_writes_the_file_back() {
    let (header, instances) = read(FILE.as_bytes()).unwrap();
    let (mapped, preserved) = Preserved::split(instances.clone(), is_mapped);
    assert_eq!(mapped.len(), 3);
    assert_eq!(preserved.instances().len(), 10);

    let used = mapped.iter().map(|instance| instance.id).collect();
    let restored = preserved.restore(Some, &used);
    assert_eq!(restored.dangling, []);
    let exported = mapped.into_iter().chain(restored.instances);
    assert_eq!(
        normalise(&header, exported).unwrap(),
        normalise(&header, instances).unwrap()
    );
}

/// The exporter numbers its instances from 1, in a different order, and dropped wall #4: kept
/// instances follow the new numbers and lose their references to the wall.
#[test]
fn kept_instances_follow_the_exported_instances() {
    let (header, instances) = read(FILE.as_bytes()).unwrap();
    let (_, preserved) = Preserved::split(instances, is_mapped);
    let export_ids = BTreeMap::from([(3, 1), (2, 2)]);
    let used = BTreeSet::from([1, 2]);
    let restored = preserved.restore(|id| export_ids.get(&id).copied(), &used);
    assert_eq!(
        restored.dangling,
        [
            Dangling {
                instance: 5,
                reference: 4
            },
            Dangling {
                instance: 9,
                reference: 4
            },
        ]
    );
    let text = String::from_utf8(normalise(&header, restored.instances.clone()).unwrap()).unwrap();
    assert!(
        text.contains(
            "#5=IFCRELCONTAINEDINSPATIALSTRUCTURE('0Tz0Vp$Xr1VBpsJJVkVxZ5',$,$,$,(#1),#2);"
        )
    );
    assert!(text.contains("#9=IFCRELDEFINESBYPROPERTIES('2fNGqfyL5DHB6Dw5HTx8xH',$,$,$,(#1),#8);"));
    assert!(text.contains("#13=!ACME_TAG(#1,'Inspected',0.0015);"));

    // What is written reads back to what was restored.
    let (_, read_back) = read(text.as_bytes()).unwrap();
    let mut restored = restored.instances;
    restored.sort_by_key(|instance| instance.id);
    assert_eq!(read_back, restored);
}
