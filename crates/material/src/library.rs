//! The material library (MAT-02): the European grades shipped with `BIMCraft`, extensible with
//! libraries from users and plugins in the same JSON format.
//!
//! A library source is `{"format": 1, "materials": [...]}`. Each material gives its `name`,
//! `category`, `behaviour_law`, optional `design_code`, and its `properties` by attribute name,
//! each with a `value` in SI units and the `document` and `clause` it comes from (REP-05).
//! Sources from users and plugins are untrusted: their size, counts and texts are bounded,
//! unknown keys are refused and every material is validated before any is added.
//!
//! A grade is known by its stable key, `<design code>/<name>` such as `en1992-1-1/C25/30`; a
//! grade without design code takes the name of its source in place of the code.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

use crate::law::LawKind;
use crate::material::{Category, Material, MaterialError};
use crate::property::Property;
use crate::source::Sourced;
use crate::{Source, SourceError};

/// The format of library sources this version reads.
pub const LIBRARY_FORMAT: u32 = 1;
/// The largest library source accepted, in bytes.
pub const MAX_LIBRARY_SIZE: usize = 1024 * 1024;
/// The most materials one library may hold.
pub const MAX_MATERIALS: usize = 4096;
/// The longest source name accepted, in characters.
pub const MAX_SOURCE_NAME: usize = 32;

/// The source name of the grades shipped with `BIMCraft`, for those without a design code.
const BUILTIN_SOURCE: &str = "bimcraft";

/// The grades shipped with `BIMCraft`, one file per family.
const BUILTIN: [&str; 1] = [include_str!("../data/eurocode-grades.json")];

/// Why a library source was rejected. Messages give the position of the faulty material, never
/// its values (SEC-03).
#[derive(Clone, Debug, Error, PartialEq)]
pub enum LibraryError {
    /// The source is larger than [`MAX_LIBRARY_SIZE`].
    #[error("the library source is larger than {MAX_LIBRARY_SIZE} bytes")]
    TooLarge,
    /// The source name is not 1 to [`MAX_SOURCE_NAME`] lowercase letters, digits or hyphens.
    #[error("the library source name is not valid")]
    SourceName,
    /// The source is not valid library JSON.
    #[error("the library source is not valid at line {line}, column {column}")]
    Syntax {
        /// The 1-based line.
        line: usize,
        /// The 1-based column.
        column: usize,
    },
    /// The source declares another format.
    #[error("library format {0} is not supported; this version reads format {LIBRARY_FORMAT}")]
    Format(u32),
    /// The library would hold more than [`MAX_MATERIALS`] materials.
    #[error("the library holds more than {MAX_MATERIALS} materials")]
    TooMany,
    /// A material has the key of another one, in the source or already in the library.
    #[error("material #{0} has the key of another material")]
    Duplicate(usize),
    /// A material names a category this version does not know.
    #[error("material #{0} has an unknown category")]
    UnknownCategory(usize),
    /// A material names a behaviour law this version does not know.
    #[error("material #{0} has an unknown behaviour law")]
    UnknownLaw(usize),
    /// A material names a property this version does not know.
    #[error("material #{0} has an unknown property")]
    UnknownProperty(usize),
    /// A property's value is not a finite number.
    #[error("material #{index}: the value of {} is not a finite number", .property.name())]
    NotFinite {
        /// The 1-based position of the material in the source.
        index: usize,
        /// The property.
        property: Property,
    },
    /// A property's source is not valid.
    #[error("material #{index}: the source of {} is not valid: {error}", .property.name())]
    Source {
        /// The 1-based position of the material in the source.
        index: usize,
        /// The property.
        property: Property,
        /// What is wrong with its source.
        error: SourceError,
    },
    /// A material does not validate.
    #[error("material #{index} is not valid: {error}")]
    Invalid {
        /// The 1-based position of the material in the source.
        index: usize,
        /// Why it is not valid.
        error: MaterialError,
    },
}

/// A set of grades by key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    materials: BTreeMap<String, Material>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    format: u32,
    materials: Vec<MaterialEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialEntry {
    name: String,
    category: String,
    behaviour_law: String,
    design_code: Option<String>,
    properties: BTreeMap<String, ValueEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueEntry {
    value: f64,
    document: String,
    clause: String,
}

impl Library {
    /// The grades shipped with `BIMCraft`.
    ///
    /// # Errors
    ///
    /// Fails like [`Library::extend`]; the shipped files are tested to load.
    pub fn builtin() -> Result<Self, LibraryError> {
        BUILTIN
            .into_iter()
            .try_fold(Self::default(), |library, contents| {
                library.with_source(BUILTIN_SOURCE, contents)
            })
    }

    /// This library with the grades of `contents` added, as [`Library::extend`] does.
    ///
    /// # Errors
    ///
    /// Fails like [`Library::extend`].
    pub fn with_source(mut self, source: &str, contents: &str) -> Result<Self, LibraryError> {
        self.extend(source, contents)?;
        Ok(self)
    }

    /// Adds the grades of a library source, such as one from a user or a plugin, named `source`
    /// (lowercase letters, digits and hyphens). Nothing is added unless every material is valid
    /// and has a key different from the others.
    ///
    /// # Errors
    ///
    /// Fails when the name or the source is invalid, too large or of another format, the
    /// library would hold too many materials, or a material is unknown, duplicated or invalid.
    pub fn extend(&mut self, source: &str, contents: &str) -> Result<(), LibraryError> {
        if !is_source_name(source) {
            return Err(LibraryError::SourceName);
        }
        if contents.len() > MAX_LIBRARY_SIZE {
            return Err(LibraryError::TooLarge);
        }
        let file: LibraryFile =
            serde_json::from_str(contents).map_err(|error| LibraryError::Syntax {
                line: error.line(),
                column: error.column(),
            })?;
        if file.format != LIBRARY_FORMAT {
            return Err(LibraryError::Format(file.format));
        }
        if self.materials.len() + file.materials.len() > MAX_MATERIALS {
            return Err(LibraryError::TooMany);
        }
        let mut added = BTreeMap::new();
        for (index, entry) in (1..).zip(file.materials) {
            let key = entry.key(source);
            let material = entry
                .material(index)?
                .with_library_key(&key)
                .map_err(|error| LibraryError::Invalid { index, error })?;
            if self.materials.contains_key(&key) || added.insert(key, material).is_some() {
                return Err(LibraryError::Duplicate(index));
            }
        }
        self.materials.append(&mut added);
        Ok(())
    }

    /// The grade with key `key`, if any. Its [`Material::library_key`] is `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Material> {
        self.materials.get(key)
    }

    /// Every grade with its key, by key.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Material)> {
        self.materials
            .iter()
            .map(|(key, material)| (key.as_str(), material))
    }

    /// The number of grades.
    #[must_use]
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Whether the library holds no grade.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

/// The key of design code `code` on the wire: lowercase, without edition or spaces, such as
/// `en1992-1-1` for `EN 1992-1-1:2004`.
#[must_use]
pub fn design_code_key(code: &str) -> String {
    let code = code.split(':').next().unwrap_or(code);
    code.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_source_name(name: &str) -> bool {
    (1..=MAX_SOURCE_NAME).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl MaterialEntry {
    /// `<design code key>/<name>`, or `<source>/<name>` without a design code.
    fn key(&self, source: &str) -> String {
        let prefix = match self.design_code.as_deref().map(str::trim) {
            Some(code) if !code.is_empty() => design_code_key(code),
            _ => source.to_owned(),
        };
        format!("{prefix}/{}", self.name.trim())
    }

    fn material(self, index: usize) -> Result<Material, LibraryError> {
        let category =
            Category::from_name(&self.category).ok_or(LibraryError::UnknownCategory(index))?;
        let law = LawKind::from_name(&self.behaviour_law).ok_or(LibraryError::UnknownLaw(index))?;
        let mut properties = BTreeMap::new();
        for (name, entry) in self.properties {
            let property =
                Property::from_name(&name).ok_or(LibraryError::UnknownProperty(index))?;
            if !entry.value.is_finite() {
                return Err(LibraryError::NotFinite { index, property });
            }
            let source = Source::new(&entry.document, &entry.clause).map_err(|error| {
                LibraryError::Source {
                    index,
                    property,
                    error,
                }
            })?;
            properties.insert(property, Sourced::new(entry.value, source));
        }
        Material::new(
            &self.name,
            category,
            law,
            self.design_code.as_deref(),
            properties,
        )
        .map_err(|error| LibraryError::Invalid { index, error })
    }
}

#[cfg(test)]
mod tests;
