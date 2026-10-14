use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use platform::{BoxFuture, Storage, StorageError};
use tempfile::Builder;

/// Suffix of the file a value is written to before it replaces the previous one. Keys cannot
/// end with it, so a temporary file never overwrites a stored value.
const TEMP_SUFFIX: &str = ".bimcraft-tmp";

/// Characters Windows does not allow in file names; `/` already separates names in keys.
const FORBIDDEN: [char; 8] = ['<', '>', ':', '"', '\\', '|', '?', '*'];

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
/// Windows trims trailing dots and spaces (so `a.` would be `a`), treats `\\` and `:` as
/// separators and drive or stream markers, and maps device names such as `nul` to devices.
fn is_plain_name(name: &str) -> bool {
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !name.contains(|c: char| c.is_control() || FORBIDDEN.contains(&c))
        && !is_windows_device(stem.trim_end())
}

/// Whether Windows reserves `stem` for a device, in any case and with any extension.
fn is_windows_device(stem: &str) -> bool {
    let is_port = stem.split_at_checked(3).is_some_and(|(prefix, digit)| {
        let mut chars = digit.chars();
        (prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT"))
            && chars
                .next()
                .is_some_and(|c| c.is_ascii_digit() || "¹²³".contains(c))
            && chars.next().is_none()
    });
    is_port
        || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
            .iter()
            .any(|device| stem.eq_ignore_ascii_case(device))
}

/// Writes `bytes` to `path` through a temporary file, so a crash or a full disk leaves either the
/// old value or the new one, never a mix.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // The root always holds the key, so the path has a parent.
    let folder = path.parent().unwrap_or(path);
    fs::create_dir_all(folder)?;
    // A uniquely named temporary file, deleted on drop if anything below fails.
    let mut temp = Builder::new().suffix(TEMP_SUFFIX).tempfile_in(folder)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}

/// Maps a failed read or remove to a [`StorageError`]. A folder in the key that is a file means
/// nothing is stored there, as Windows already reports.
fn storage_error(error: &io::Error) -> StorageError {
    if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) {
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
        for name in [
            "console",
            "com",
            "com10",
            "lpt",
            "nul-model",
            " a b",
            "a.bimcraft-tmp.x",
        ] {
            assert!(storage.path(name).is_ok(), "{name:?}");
        }
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
            "line\nbreak",
            "a?b",
            "a*",
            "trailing.",
            "trailing ",
            "nul",
            "Con",
            "aux.txt",
            "prn .bim",
            "folder/COM1/x",
            "lpt9",
            "com²",
            "conin$",
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
        for kind in [ErrorKind::NotFound, ErrorKind::NotADirectory] {
            let missing = io::Error::new(kind, "/home/someone/secret.bim");
            assert_eq!(storage_error(&missing), StorageError::NotFound);
        }
    }
}
