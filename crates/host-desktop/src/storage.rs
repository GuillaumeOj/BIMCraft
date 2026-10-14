use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use platform::{BoxFuture, Storage, StorageError};

/// Suffix of the file a value is written to before it replaces the previous one. Keys cannot
/// end with it, so a temporary file never overwrites a stored value.
const TEMP_SUFFIX: &str = ".bimcraft-tmp";

/// A [`Storage`] that keeps each value in a file under a root directory.
///
/// Keys are relative paths with `/` between folders, such as `projects/tower/model.bim`; missing
/// folders are created on write. Keys that could reach outside the root, such as `../x` or
/// `/etc/x`, are rejected.
#[derive(Clone, Debug)]
pub struct FileStorage {
    root: PathBuf,
}

impl FileStorage {
    /// Creates a storage that keeps its files under `root`, which is created on the first write.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the directory the files are kept in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the file that holds the value of `key`.
    fn path(&self, key: &str) -> Result<PathBuf, StorageError> {
        if key.ends_with(TEMP_SUFFIX) {
            return Err(invalid_key());
        }
        let mut path = self.root.clone();
        for folder_or_name in key.split('/') {
            if !is_plain_name(folder_or_name) {
                return Err(invalid_key());
            }
            path.push(folder_or_name);
        }
        Ok(path)
    }
}

impl Storage for FileStorage {
    fn read<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Vec<u8>, StorageError>> {
        Box::pin(async move { fs::read(self.path(key)?).map_err(|error| storage_error(&error)) })
    }

    fn write<'a>(
        &'a self,
        key: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), StorageError>> {
        Box::pin(
            async move { write_atomically(&self.path(key)?, bytes).map_err(|error| other(&error)) },
        )
    }

    fn remove<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StorageError>> {
        Box::pin(
            async move { fs::remove_file(self.path(key)?).map_err(|error| storage_error(&error)) },
        )
    }
}

/// Whether `name` names one file or folder inside its parent on every desktop system.
///
/// `\` and `:` are separators or drive and stream markers on Windows.
fn is_plain_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['\\', ':', '\0'])
}

/// Writes `bytes` to `path` through a temporary file, so a crash or a full disk leaves either the
/// old value or the new one, never a mix.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temp = OsString::from(path);
    temp.push(TEMP_SUFFIX);
    let temp = PathBuf::from(temp);
    let written = write_and_sync(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        // Best effort: the write already failed, and that is the error worth reporting.
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Writes `bytes` to a new file at `path` and waits until they reach the disk.
fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn storage_error(error: &io::Error) -> StorageError {
    if error.kind() == ErrorKind::NotFound {
        StorageError::NotFound
    } else {
        other(error)
    }
}

/// Describes `error` by its kind only: the full message can repeat the path (SEC-03).
fn other(error: &io::Error) -> StorageError {
    StorageError::Other {
        message: error.kind().to_string(),
    }
}

fn invalid_key() -> StorageError {
    StorageError::Other {
        message: "the key is not a relative path inside the storage".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_files_under_the_root() {
        let storage = FileStorage::new("root");
        assert_eq!(storage.root(), Path::new("root"));
        assert_eq!(
            storage.path("projects/tower/model.bim"),
            Ok(Path::new("root")
                .join("projects")
                .join("tower")
                .join("model.bim"))
        );
        assert_eq!(
            storage.path(".hidden/a..b"),
            Ok(Path::new("root").join(".hidden").join("a..b"))
        );
    }

    #[test]
    fn rejects_keys_that_could_leave_the_root() {
        let storage = FileStorage::new("root");
        for key in [
            "",
            "/etc/passwd",
            "a//b",
            "a/",
            ".",
            "a/./b",
            "..",
            "a/../../b",
            "a\\..\\b",
            "C:/x",
            "C:x",
            "file:stream",
            "nul\0byte",
            "model.bim.bimcraft-tmp",
        ] {
            assert_eq!(storage.path(key), Err(invalid_key()), "{key:?}");
        }
    }

    #[test]
    fn error_messages_hold_only_the_error_kind() {
        let error = io::Error::new(ErrorKind::PermissionDenied, "/home/someone/secret.bim");
        assert_eq!(
            storage_error(&error),
            StorageError::Other {
                message: ErrorKind::PermissionDenied.to_string()
            }
        );
        let missing = io::Error::new(ErrorKind::NotFound, "/home/someone/secret.bim");
        assert_eq!(storage_error(&missing), StorageError::NotFound);
    }
}
