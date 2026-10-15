//! Reading and writing whole files, shared by the round-trip tests.

use interop_ifc::step::{Instance, Limits, Record, StepError, StepReader, StepWriter, WriteError};

/// A file's header and instances.
pub type File = (Vec<Record>, Vec<Instance>);

pub fn read(input: &[u8]) -> Result<File, StepError> {
    let reader = StepReader::new(input, Limits::default())?;
    let header = reader.header().to_vec();
    Ok((header, reader.collect::<Result<_, _>>()?))
}

pub fn write((header, instances): &File) -> Result<Vec<u8>, WriteError> {
    let mut writer = StepWriter::new(Vec::new(), header)?;
    for instance in instances {
        writer.write_instance(instance)?;
    }
    writer.finish()
}
