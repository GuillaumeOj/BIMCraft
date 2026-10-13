use crate::BoxFuture;

/// Why a [`Storage`] operation failed.
///
/// Messages never repeat the key: on desktop it is a file path that can hold personal data.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// No data is stored under the key.
    #[error("nothing is stored under the requested key")]
    NotFound,
    /// The host storage failed, for example a full disk or a denied permission.
    #[error("storage failed: {message}")]
    Other {
        /// Host-provided description, without model content or personal data.
        message: String,
    },
}

/// Byte storage addressed by keys, such as project files on disk or the browser's OPFS.
///
/// Operations are asynchronous because browser storage is.
pub trait Storage {
    /// Reads the bytes stored under `key`, or fails with [`StorageError::NotFound`] if there are none.
    fn read<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Vec<u8>, StorageError>>;

    /// Stores `bytes` under `key`, replacing any previous value.
    fn write<'a>(
        &'a self,
        key: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), StorageError>>;

    /// Removes the value stored under `key`, or fails with [`StorageError::NotFound`] if there is none.
    fn remove<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StorageError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_describe_themselves_without_the_key() {
        assert_eq!(
            StorageError::NotFound.to_string(),
            "nothing is stored under the requested key"
        );
        let other = StorageError::Other {
            message: "disk full".to_owned(),
        };
        assert_eq!(other.to_string(), "storage failed: disk full");
    }
}
