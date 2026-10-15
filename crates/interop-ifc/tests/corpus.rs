//! The parse → write → parse round-trip on the public sample corpus (INT-04), the Phase 0 gate.
//!
//! `scripts/fetch-ifc-corpus.sh` downloads the files listed in `corpus/manifest.txt` at a pinned
//! revision and checks their SHA-256; this test reads them from `IFC_CORPUS_DIR` and never touches
//! the network. Without the variable, as in a plain `cargo test`, it does nothing;
//! `scripts/check-all.sh corpus` sets it, and CI runs that section.

// Tests read the corpus files, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use interop_ifc::ifc::decode::DecodeError;
use interop_ifc::ifc::{self, Entity};
use interop_ifc::preserve::Preserved;
use interop_ifc::step::Record;

mod common;

use common::{File, read, write};

const MANIFEST: &str = include_str!("corpus/manifest.txt");

/// The file names of the corpus, as the fetch script saves them: from the lines that start with
/// 64 hexadecimal digits and two spaces, as the script reads them.
fn names() -> Vec<&'static str> {
    MANIFEST
        .lines()
        .filter_map(|line| line.split_once("  "))
        .filter(|(checksum, _)| {
            checksum.len() == 64
                && checksum
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        })
        .filter_map(|(_, path)| path.rsplit('/').next())
        .collect()
}

fn corpus_dir() -> Option<PathBuf> {
    std::env::var_os("IFC_CORPUS_DIR").map(PathBuf::from)
}

fn load(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    std::fs::read(dir.join(name))
        .map_err(|error| format!("{name}: {error}; run scripts/fetch-ifc-corpus.sh"))
}

/// What the IFC import of the MVP maps, roughly: building elements and spatial structure. A
/// stand-in until the core schema has entities; then it comes from their `[ifc]` mappings.
fn is_mapped(record: &Record) -> bool {
    [
        ifc::IfcWall::STEP_NAME,
        ifc::IfcSlab::STEP_NAME,
        ifc::IfcBeam::STEP_NAME,
        ifc::IfcColumn::STEP_NAME,
        ifc::IfcDoor::STEP_NAME,
        ifc::IfcWindow::STEP_NAME,
        ifc::IfcOpeningElement::STEP_NAME,
        ifc::IfcSite::STEP_NAME,
        ifc::IfcBuilding::STEP_NAME,
        ifc::IfcBuildingStorey::STEP_NAME,
    ]
    .contains(&record.name.as_str())
}

#[test]
fn the_manifest_lists_the_corpus() {
    let names = names();
    assert!(!names.is_empty());
    assert_eq!(
        names.iter().collect::<BTreeSet<_>>().len(),
        names.len(),
        "names must be unique"
    );
}

/// Reading the output back yields the file read, and writing that again yields the same bytes.
fn round_trips(file: &File) -> Result<(), String> {
    let written = write(file).map_err(|error| error.to_string())?;
    let read_back = read(&written).map_err(|error| error.to_string())?;
    if read_back != *file {
        return Err("read back differs".into());
    }
    let rewritten = write(&read_back).map_err(|error| error.to_string())?;
    if rewritten != written {
        return Err("written twice differs".into());
    }
    Ok(())
}

/// Every simple instance decodes into its IFC 4.3 class.
fn decodes((_, instances): &File) -> Result<(), String> {
    for instance in instances {
        match Entity::from_instance(&instance.body) {
            Ok(_) | Err(DecodeError::ComplexInstance) => {}
            Err(error) => return Err(format!("#{}: {error}", instance.id)),
        }
    }
    Ok(())
}

/// What the import does not map is written back unchanged (INT-03); returns how many instances
/// were mapped.
fn keeps_what_is_not_mapped((_, instances): &File) -> Result<usize, String> {
    let (mapped, preserved) = Preserved::split(instances.clone(), is_mapped);
    let mapped_count = mapped.len();
    let used = mapped.iter().map(|instance| instance.id).collect();
    let restored = preserved.restore(Some, &used);
    if !restored.dangling.is_empty() {
        return Err(format!("dangling references: {:?}", restored.dangling));
    }
    let mut exported: Vec<_> = mapped.into_iter().chain(restored.instances).collect();
    exported.sort_by_key(|instance| instance.id);
    let mut instances = instances.clone();
    instances.sort_by_key(|instance| instance.id);
    if exported != instances {
        return Err("not written back unchanged".into());
    }
    Ok(mapped_count)
}

/// Each sample is read once and goes through every check.
#[test]
fn every_sample_round_trips_decodes_and_keeps_what_is_not_mapped() {
    let Some(dir) = corpus_dir() else { return };
    let mut mapped_count = 0;
    for name in names() {
        let check = || {
            let file = read(&load(&dir, name)?).map_err(|error| error.to_string())?;
            round_trips(&file)?;
            decodes(&file)?;
            keeps_what_is_not_mapped(&file)
        };
        mapped_count += check().unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert!(mapped_count > 0, "the corpus has nothing to map");
}
