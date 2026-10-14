//! Streaming reader and writer for ISO 10303-21 (STEP Part 21) exchange files.
//!
//! [`StepReader`] reads the header when it is created, then yields the instances of the data
//! sections one at a time, so memory holds one instance, not the whole file. It accepts the
//! second edition of the format, which IFC uses: one header section and one or more `DATA;`
//! sections.
//!
//! Every input is treated as hostile. [`Limits`] bounds the length of a token (string, binary,
//! keyword, number), the length of an instance, the nesting of lists and typed parameters, and the
//! number of instances. A malformed or oversized file yields a [`StepError`] with the line and
//! column where reading stopped; reading never panics.
//!
//! Strings keep their encoding directives (`\X\`, `\X2\`, `\S\`, `\P`…) as written, and only the
//! doubled quote `''` is unescaped. Decoding is left to the caller, so data the importer does not
//! map can be written back unchanged (INT-03). Bytes outside the printable ASCII range are rejected
//! inside strings, as the standard requires; line breaks inside strings are ignored.
//!
//! [`StepWriter`] is the mirror of the reader: it writes a header and a stream of instances,
//! deterministically, and reading its output back yields the same values, so a file read and
//! written again keeps its content.

mod error;
mod lexer;
mod limits;
mod reader;
#[cfg(test)]
mod test_support;
mod value;
mod writer;

pub use error::{Position, StepError, StepErrorKind};
pub use limits::Limits;
pub use reader::StepReader;
pub use value::{Instance, InstanceBody, Parameter, Record};
pub use writer::{StepWriter, WriteError};
