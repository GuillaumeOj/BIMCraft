//! The registry every transport dispatches through (API-01).

use std::collections::BTreeMap;

use core_txn::Origin;
use serde_json::Value;

use crate::{ApiError, Descriptor, Outcome, RegistryError, Version};

/// The commands and queries callers can reach, by name then version. Iteration is sorted, so
/// everything generated from a registry, such as its schema, is deterministic.
pub struct Registry<Ctx> {
    descriptors: BTreeMap<&'static str, BTreeMap<Version, Descriptor<Ctx>>>,
}

impl<Ctx> Default for Registry<Ctx> {
    fn default() -> Self {
        Self {
            descriptors: BTreeMap::new(),
        }
    }
}

impl<Ctx> Registry<Ctx> {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `descriptor`. Several versions of one name may coexist, so callers can move to a new
    /// major version during a deprecation period (API-06).
    ///
    /// # Errors
    ///
    /// [`RegistryError::InvalidName`] when the name is not dotted `snake_case`, and
    /// [`RegistryError::Duplicate`] when this name and version are already registered.
    pub fn register(&mut self, descriptor: Descriptor<Ctx>) -> Result<(), RegistryError> {
        let name = descriptor.name();
        if !is_valid_name(name) {
            return Err(RegistryError::InvalidName(name));
        }
        let versions = self.descriptors.entry(name).or_default();
        let version = descriptor.version();
        if versions.contains_key(&version) {
            return Err(RegistryError::Duplicate { name, version });
        }
        versions.insert(version, descriptor);
        Ok(())
    }

    /// The highest version registered under `name`.
    ///
    /// # Errors
    ///
    /// [`ApiError::UnknownName`] when nothing is registered under `name`.
    pub fn get(&self, name: &str) -> Result<&Descriptor<Ctx>, ApiError> {
        self.descriptors
            .get(name)
            .and_then(|versions| versions.values().next_back())
            .ok_or_else(|| ApiError::UnknownName(name.to_owned()))
    }

    /// The descriptor registered under `name` and `version`.
    ///
    /// # Errors
    ///
    /// [`ApiError::UnknownName`] when nothing is registered under `name`, and
    /// [`ApiError::UnknownVersion`] when `name` exists in other versions only.
    pub fn get_version(&self, name: &str, version: Version) -> Result<&Descriptor<Ctx>, ApiError> {
        let versions = self
            .descriptors
            .get(name)
            .ok_or_else(|| ApiError::UnknownName(name.to_owned()))?;
        versions
            .get(&version)
            .ok_or_else(|| ApiError::UnknownVersion {
                name: name.to_owned(),
                version,
            })
    }

    /// Every descriptor, sorted by name then version.
    pub fn iter(&self) -> impl Iterator<Item = &Descriptor<Ctx>> {
        self.descriptors.values().flat_map(BTreeMap::values)
    }

    /// Runs the highest version of `name` on `ctx`; see [`Descriptor::call`].
    ///
    /// # Errors
    ///
    /// [`ApiError::UnknownName`] when nothing is registered under `name`, or whatever the call
    /// returns.
    pub fn dispatch(
        &self,
        ctx: &mut Ctx,
        name: &str,
        params: Value,
        origin: &Origin,
    ) -> Result<Outcome, ApiError> {
        self.get(name)?.call(ctx, params, origin)
    }
}

/// Lower-case `snake_case` segments joined by dots, at least two: the first names the area, such
/// as `model` or `history`, and is never `rpc`, which JSON-RPC reserves for its own methods.
fn is_valid_name(name: &str) -> bool {
    if name.starts_with("rpc.") {
        return false;
    }
    let mut segments = 0;
    for segment in name.split('.') {
        let mut chars = segment.chars();
        let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_is_valid = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !(starts_with_letter && rest_is_valid) {
            return false;
        }
        segments += 1;
    }
    segments >= 2
}

#[cfg(test)]
mod tests;
