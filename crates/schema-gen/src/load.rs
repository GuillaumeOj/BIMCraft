use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, ErrorKind, MAX_FILE_SIZE, Source};

/// Directories of a namespace that hold definitions. `migrations/` and `snapshots/` belong to
/// the migration framework (#36) and are not read here.
const DEFINITION_DIRS: [&str; 3] = ["entities", "relationships", "property-sets"];

/// Definition files read from a namespace directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    /// The files, sorted by path.
    pub sources: Vec<Source>,
    /// Every file and definition directory read, for `cargo::rerun-if-changed`; a directory is
    /// listed so that adding a file to it triggers a rebuild.
    pub inputs: Vec<PathBuf>,
}

/// Reads `namespace.toml` and every `*.toml` file in the definition directories of `dir`.
///
/// # Errors
///
/// Returns an [`Error`] when a file or directory cannot be read, a file is larger than
/// [`MAX_FILE_SIZE`], or a definition directory holds anything but `*.toml` files.
pub fn load_dir(dir: &Path) -> Result<Loaded, Error> {
    let mut loaded = Loaded {
        sources: Vec::new(),
        inputs: Vec::new(),
    };
    read_file(dir, "namespace.toml", &mut loaded)?;
    for name in DEFINITION_DIRS {
        let subdir = dir.join(name);
        if !subdir.is_dir() {
            continue;
        }
        loaded.inputs.push(subdir.clone());
        let mut files = Vec::new();
        for entry in fs::read_dir(&subdir).map_err(|error| io_error(&subdir, &error))? {
            let entry = entry.map_err(|error| io_error(&subdir, &error))?;
            let file_name = entry.file_name();
            let file_name = file_name.to_str().unwrap_or_default();
            let path = format!("{name}/{file_name}");
            let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            if !is_file
                || Path::new(file_name)
                    .extension()
                    .is_none_or(|extension| extension != "toml")
            {
                let message = "definition directories hold only *.toml files".into();
                return Err(Error::at(&path, "", None, ErrorKind::Definition(message)));
            }
            files.push(path);
        }
        files.sort();
        for path in files {
            read_file(dir, &path, &mut loaded)?;
        }
    }
    Ok(loaded)
}

fn read_file(dir: &Path, path: &str, loaded: &mut Loaded) -> Result<(), Error> {
    let full = dir.join(path);
    let size = fs::metadata(&full)
        .map_err(|error| io_error(&full, &error))?
        .len();
    if usize::try_from(size).map_or(true, |size| size > MAX_FILE_SIZE) {
        let message = format!("file is larger than {MAX_FILE_SIZE} bytes");
        return Err(Error::at(path, "", None, ErrorKind::Definition(message)));
    }
    let contents = fs::read_to_string(&full).map_err(|error| io_error(&full, &error))?;
    loaded.sources.push(Source {
        path: path.to_owned(),
        contents,
    });
    loaded.inputs.push(full);
    Ok(())
}

fn io_error(path: &Path, error: &std::io::Error) -> Error {
    Error::new(ErrorKind::Io(format!("{}: {error}", path.display())))
}
