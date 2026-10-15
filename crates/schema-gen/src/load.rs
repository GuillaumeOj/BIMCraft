use std::fs;
use std::path::Path;

use crate::{Error, ErrorKind, MAX_FILE_SIZE, Source};

/// Directories of a namespace that hold definitions. `migrations/` and `snapshots/` hold the
/// version history, read by [`load_history`](crate::load_history).
const DEFINITION_DIRS: [&str; 3] = ["entities", "relationships", "property-sets"];

/// Reads `namespace.toml` and every `*.toml` file in the definition directories of `dir`,
/// sorted by path. A definition directory may be absent.
///
/// # Errors
///
/// Returns an [`Error`] when a file or directory cannot be read, a file is larger than
/// [`MAX_FILE_SIZE`], a definition directory is not a directory, or it holds anything but
/// `*.toml` files.
pub fn load_dir(dir: &Path) -> Result<Vec<Source>, Error> {
    let mut sources = vec![read_file(dir, "namespace.toml", MAX_FILE_SIZE)?];
    for name in DEFINITION_DIRS {
        sources.extend(load_files(dir, name, "toml", MAX_FILE_SIZE)?);
    }
    Ok(sources)
}

/// Reads every `*.<extension>` file of the subdirectory `name` of `dir`, sorted by path; none
/// when the subdirectory is absent.
pub(crate) fn load_files(
    dir: &Path,
    name: &str,
    extension: &str,
    max_size: usize,
) -> Result<Vec<Source>, Error> {
    let subdir = dir.join(name);
    if !subdir.exists() {
        return Ok(Vec::new());
    }
    if !subdir.is_dir() {
        let message = format!("must be a directory of *.{extension} files");
        return Err(Error::definition(name, message));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&subdir).map_err(|error| io_error(&subdir, &error))? {
        let entry = entry.map_err(|error| io_error(&subdir, &error))?;
        let file_name = entry.file_name();
        let path = format!("{name}/{}", file_name.to_string_lossy());
        let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
        let has_extension = Path::new(&file_name)
            .extension()
            .is_some_and(|found| found == extension);
        if !is_file || !has_extension {
            let message = format!("{name}/ may hold only *.{extension} files");
            return Err(Error::definition(&path, message));
        }
        files.push(path);
    }
    files.sort();
    files
        .iter()
        .map(|path| read_file(dir, path, max_size))
        .collect()
}

fn read_file(dir: &Path, path: &str, max_size: usize) -> Result<Source, Error> {
    let full = dir.join(path);
    let size = fs::metadata(&full)
        .map_err(|error| io_error(&full, &error))?
        .len();
    if usize::try_from(size).map_or(true, |size| size > max_size) {
        let message = format!("file is larger than {max_size} bytes");
        return Err(Error::definition(path, message));
    }
    let contents = fs::read_to_string(&full).map_err(|error| io_error(&full, &error))?;
    Ok(Source {
        path: path.to_owned(),
        contents,
    })
}

fn io_error(path: &Path, error: &std::io::Error) -> Error {
    Error::new(ErrorKind::Io(format!("{}: {error}", path.display())))
}
