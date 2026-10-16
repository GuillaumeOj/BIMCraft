//! Semantic versions of commands and queries (API-01).

use std::fmt;

/// The semantic version of a command or query: a new major version breaks its callers, a new
/// minor version adds to its input or output without breaking them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Incremented by a change that breaks callers.
    pub major: u32,
    /// Incremented by a compatible addition.
    pub minor: u32,
    /// Incremented by a fix that changes neither input nor output.
    pub patch: u32,
}

impl Version {
    /// The version `major.minor.patch`.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests;
