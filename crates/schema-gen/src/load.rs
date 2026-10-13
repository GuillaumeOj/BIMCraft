use std::fs;
use std::path::Path;

use crate::{Error, ErrorKind, MAX_FILE_SIZE, Source};

/// Directories of a namespace that hold definitions. `migrations/` and `snapshots/` belong to
/// the migration framework (#36) and are not read here.
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
    let mut sources = vec![read_file(dir, "namespace.toml")?];
    for name in DEFINITION_DIRS {
        let subdir = dir.join(name);
        if !subdir.exists() {
            continue;
        }
        if !subdir.is_dir() {
            let message = "must be a directory of *.toml files".into();
            return Err(Error::at(name, "", None, ErrorKind::Definition(message)));
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&subdir).map_err(|error| io_error(&subdir, &error))? {
            let entry = entry.map_err(|error| io_error(&subdir, &error))?;
            let file_name = entry.file_name();
            let path = format!("{name}/{}", file_name.to_string_lossy());
            let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            let is_toml = Path::new(&file_name)
                .extension()
                .is_some_and(|extension| extension == "toml");
            if !is_file || !is_toml {
                let message = "definition directories hold only *.toml files".into();
                return Err(Error::at(&path, "", None, ErrorKind::Definition(message)));
            }
            files.push(path);
        }
        files.sort();
        for path in files {
            sources.push(read_file(dir, &path)?);
        }
    }
    Ok(sources)
}

fn read_file(dir: &Path, path: &str) -> Result<Source, Error> {
    let full = dir.join(path);
    let size = fs::metadata(&full)
        .map_err(|error| io_error(&full, &error))?
        .len();
    if usize::try_from(size).map_or(true, |size| size > MAX_FILE_SIZE) {
        let message = format!("file is larger than {MAX_FILE_SIZE} bytes");
        return Err(Error::at(path, "", None, ErrorKind::Definition(message)));
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
