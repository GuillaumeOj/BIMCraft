//! The version history of a namespace: snapshots, migration files and the check that ties them
//! to the definitions (ADR 0002, PERS-01).
//!
//! For a namespace at version `N`, `snapshots/0001.json` to `snapshots/000N.json` hold the
//! canonical model of every version, and `migrations/000K.toml` (for `K` from 2 to `N`, optional
//! when a version needs no declared step) the steps from `K - 1` to `K`. The check fails when
//! the definitions differ from the last snapshot (a schema change without a new version), when
//! a snapshot is missing, or when a migration does not turn one snapshot into the next.

use std::path::Path;

use schema::{DeclaredStep, Migration, Namespace, ValueType};
use serde::Deserialize;
use toml::Spanned;

use crate::parse::convert_value;
use crate::snapshot::parse_snapshot;
use crate::{Error, ErrorKind, FORMAT, Source};

/// Largest snapshot accepted, in bytes: a snapshot holds a whole namespace.
pub const MAX_SNAPSHOT_SIZE: usize = 16 * 1024 * 1024;

const SNAPSHOT_DIR: &str = "snapshots";
const MIGRATION_DIR: &str = "migrations";

/// A namespace and the migrations that bring data from its first version to the current one.
#[derive(Clone, Debug, PartialEq)]
pub struct History {
    /// The namespace as its definitions describe it.
    pub namespace: Namespace,
    /// One migration per version after the first, in order: the first goes to version 2.
    pub migrations: Vec<Migration>,
}

/// Where the snapshot of `version` lives, relative to the namespace directory.
#[must_use]
pub fn snapshot_path(version: u32) -> String {
    format!("{SNAPSHOT_DIR}/{version:04}.json")
}

/// Reads every file of `snapshots/` in `dir`, sorted by path.
pub(crate) fn load_snapshots(dir: &Path) -> Result<Vec<Source>, Error> {
    crate::load::load_files(dir, SNAPSHOT_DIR, "json", MAX_SNAPSHOT_SIZE)
}

/// The snapshots of the versions up to `current` found among `sources`, in version order, each
/// checked to hold its version. Missing and unexpected files are left to [`check_history`].
pub(crate) fn released_snapshots(
    sources: &[Source],
    current: u32,
) -> Result<Vec<Namespace>, Vec<Error>> {
    let mut errors = Vec::new();
    let snapshots = (1..=current)
        .filter_map(|version| {
            released_snapshot(sources, version)?
                .map_err(|message| errors.push(Error::definition(&snapshot_path(version), message)))
                .ok()
        })
        .collect();
    if errors.is_empty() {
        Ok(snapshots)
    } else {
        Err(errors)
    }
}

/// The snapshot of `version` among `sources`, read and checked, when there is one.
fn released_snapshot(sources: &[Source], version: u32) -> Option<Result<Namespace, String>> {
    let path = snapshot_path(version);
    sources
        .iter()
        .find(|source| source.path == path)
        .map(|source| read_snapshot(source, version))
}

fn migration_path(version: u32) -> String {
    format!("{MIGRATION_DIR}/{version:04}.toml")
}

/// Reads the definitions, snapshots and migration files in `dir` and checks them together.
///
/// # Errors
///
/// Returns every [`Error`] found, as [`check_history`] does, plus unreadable files.
pub fn load_history(dir: &Path) -> Result<History, Vec<Error>> {
    let namespace = crate::load_namespace(dir)?;
    let snapshots = load_snapshots(dir).map_err(|error| vec![error])?;
    let migrations = crate::load::load_files(dir, MIGRATION_DIR, "toml", crate::MAX_FILE_SIZE)
        .map_err(|error| vec![error])?;
    check_history(namespace, &snapshots, &migrations)
}

/// Checks the history of `namespace` from the contents of its snapshot and migration files,
/// whose paths are relative to the namespace directory (`snapshots/0001.json`…).
///
/// # Errors
///
/// Returns every [`Error`] found: unexpected or missing files, unreadable snapshots or
/// migration files, definitions that differ from the snapshot of the current version, and
/// migrations that do not turn one snapshot into the next.
pub fn check_history(
    namespace: Namespace,
    snapshots: &[Source],
    migrations: &[Source],
) -> Result<History, Vec<Error>> {
    let mut errors = Vec::new();
    let current = namespace.version;
    let mut versions = Vec::new();
    for version in 1..=current {
        let path = snapshot_path(version);
        let snapshot = released_snapshot(snapshots, version).unwrap_or_else(|| {
            Err(if version == current {
                "missing: run `UPDATE_SNAPSHOTS=1 cargo test -p schema-gen` to write it"
            } else {
                "missing: snapshots of released versions are never removed"
            }
            .to_owned())
        });
        versions.push(
            snapshot
                .map_err(|message| errors.push(Error::definition(&path, message)))
                .ok(),
        );
    }
    let unexpected =
        snapshots
            .iter()
            .filter(|source| !(1..=current).any(|version| source.path == snapshot_path(version)))
            .chain(migrations.iter().filter(|source| {
                !(2..=current).any(|version| source.path == migration_path(version))
            }));
    for source in unexpected {
        let message = format!("unexpected file: the namespace is at version {current}");
        errors.push(Error::definition(&source.path, message));
    }
    if let Some(Some(last)) = versions.last()
        && *last != namespace
    {
        let message = format!(
            "the definitions differ from this snapshot: raise the version in namespace.toml to {} and describe the change in {}",
            current + 1,
            migration_path(current + 1)
        );
        errors.push(Error::definition(&snapshot_path(current), message));
    }
    let mut resolved = Vec::new();
    for (version, pair) in (2..).zip(versions.windows(2)) {
        let [Some(before), Some(after)] = pair else {
            continue;
        };
        let path = migration_path(version);
        let declared = match migrations.iter().find(|source| source.path == path) {
            Some(source) => match parse_migration(source) {
                Ok(declared) => declared,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            None => Vec::new(),
        };
        match schema::resolve(before, &declared, after) {
            Ok(migration) => resolved.push(migration),
            Err(failures) => errors.extend(failures.into_iter().map(|failure| {
                Error::at(&path, "", None, ErrorKind::Migration(Box::new(failure)))
            })),
        }
    }
    if errors.is_empty() {
        Ok(History {
            namespace,
            migrations: resolved,
        })
    } else {
        Err(errors)
    }
}

/// Reads and validates the snapshot of `version`.
fn read_snapshot(source: &Source, version: u32) -> Result<Namespace, String> {
    let snapshot = parse_snapshot(&source.contents)?;
    if snapshot.version != version {
        return Err(format!("holds version {}, not {version}", snapshot.version));
    }
    schema::validate(&snapshot).map_err(|errors| {
        let messages: Vec<_> = errors.iter().map(ToString::to_string).collect();
        format!(
            "the snapshot is not a valid schema: {}",
            messages.join("; ")
        )
    })?;
    Ok(snapshot)
}

/// `migrations/NNNN.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationFile {
    format: Spanned<u32>,
    #[serde(default, rename = "step")]
    steps: Vec<Spanned<StepDef>>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum StepDef {
    RenameEntity {
        old_name: String,
        new_name: String,
    },
    RenameAttribute {
        entity: String,
        old_name: String,
        new_name: String,
    },
    ChangeValue {
        entity: String,
        attribute: String,
        value: String,
        factor: Option<f64>,
    },
    SetValue {
        entity: String,
        attribute: String,
        value: toml::Value,
    },
    Custom {
        entities: Vec<String>,
        function: String,
        /// Required so every custom step explains itself; only reviewers read it.
        #[serde(rename = "doc")]
        _doc: String,
    },
}

/// Reads the declared steps of a migration file.
fn parse_migration(source: &Source) -> Result<Vec<DeclaredStep>, Error> {
    let at = |offset: Option<usize>, kind| Error::at(&source.path, &source.contents, offset, kind);
    let file: MigrationFile = toml::from_str(&source.contents).map_err(|error| {
        at(
            error.span().map(|span| span.start),
            ErrorKind::Syntax(error.message().to_owned()),
        )
    })?;
    if *file.format.get_ref() != FORMAT {
        let message = format!(
            "format {} is not supported; this version reads format {FORMAT}",
            file.format.get_ref()
        );
        return Err(at(
            Some(file.format.span().start),
            ErrorKind::Definition(message),
        ));
    }
    file.steps
        .into_iter()
        .map(|step| {
            let offset = Some(step.span().start);
            declared_step(step.into_inner())
                .map_err(|message| at(offset, ErrorKind::Definition(message)))
        })
        .collect()
}

fn declared_step(step: StepDef) -> Result<DeclaredStep, String> {
    Ok(match step {
        StepDef::RenameEntity { old_name, new_name } => {
            DeclaredStep::RenameEntity { old_name, new_name }
        }
        StepDef::RenameAttribute {
            entity,
            old_name,
            new_name,
        } => DeclaredStep::RenameAttribute {
            entity,
            old_name,
            new_name,
        },
        StepDef::ChangeValue {
            entity,
            attribute,
            value,
            factor,
        } => DeclaredStep::ChangeValue {
            entity,
            attribute,
            value: value
                .parse::<ValueType>()
                .map_err(|error| error.to_string())?,
            factor: factor.unwrap_or(1.0),
        },
        StepDef::SetValue {
            entity,
            attribute,
            value,
        } => DeclaredStep::SetValue {
            entity,
            attribute,
            // `convert_value` speaks of defaults, which this file has none of.
            value: convert_value(value, 0)
                .map_err(|message| message.replace("defaults", "values"))?,
        },
        StepDef::Custom {
            entities, function, ..
        } => DeclaredStep::Custom { entities, function },
    })
}

#[cfg(test)]
mod tests;
