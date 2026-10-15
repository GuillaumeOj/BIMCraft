//! Migrations between consecutive schema versions of a namespace (ADR 0002, PERS-01).
//!
//! Authors declare only what a difference between two versions cannot tell
//! ([`DeclaredStep`]): renames, value type changes, values for new required attributes and
//! custom code. [`resolve`] adds what the difference does tell — added and removed entities and
//! attributes — and checks that the result really turns every object stored by the older
//! version into a valid object of the newer one. The resolved [`Migration`] lists every change
//! to stored data explicitly, so a file can be migrated without the older schemas.
//!
//! Only what objects store matters here: the attributes of instances and of type objects, each
//! kind in its own table as the store keeps them (CORE-02); a type's table is named after the
//! type (`WallType`), and steps name it like an entity. Documentation, IFC mappings or roles may
//! change without a step.

use std::collections::{BTreeMap, BTreeSet};

use crate::validate::{is_finite, value_matches};
use crate::{Attribute, Namespace, Presence, Side, StoredTable, Value, ValueType, stored_tables};

/// Most declared steps one migration may hold, so a hostile plugin bundle stays cheap to check.
pub const MAX_STEPS: usize = 256;

/// The changes to stored objects from version `version - 1` of a namespace to `version`.
#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    /// The version it migrates to.
    pub version: u32,
    /// The steps, in the order they run.
    pub steps: Vec<Step>,
}

/// A step written in a migration file (`migrations/NNNN.toml`).
#[derive(Clone, Debug, PartialEq)]
pub enum DeclaredStep {
    /// An entity is renamed; its objects and references to it are kept.
    RenameEntity {
        /// Name in the older version.
        old_name: String,
        /// Name in the newer version.
        new_name: String,
    },
    /// An attribute is renamed; its values are kept.
    RenameAttribute {
        /// The entity, by its name after any entity rename.
        entity: String,
        /// Name in the older version.
        old_name: String,
        /// Name in the newer version.
        new_name: String,
    },
    /// An attribute changes value type; reals are multiplied by `factor`.
    ChangeValue {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
        /// The new value type.
        value: ValueType,
        /// Factor applied to real and quantity values, such as `0.001` from millimetres.
        factor: f64,
    },
    /// Objects without a value for an attribute get this one.
    SetValue {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
        /// The value.
        value: Value,
    },
    /// Code migrates the objects of some entities; [`resolve`] checks nothing about them.
    Custom {
        /// The entities the code handles.
        entities: Vec<String>,
        /// The function to call, `migrations::v2::split_wall_layers` for the core.
        function: String,
    },
}

/// An attribute as objects store it: a name and a value type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredAttribute {
    /// The attribute name.
    pub name: String,
    /// Its value type.
    pub value: ValueType,
}

/// One explicit change to stored objects.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Renames an entity.
    RenameEntity {
        /// Current name.
        from: String,
        /// New name.
        to: String,
    },
    /// Renames an attribute.
    RenameAttribute {
        /// The entity.
        entity: String,
        /// Current name.
        from: String,
        /// New name.
        to: String,
    },
    /// Adds an entity, with no objects yet.
    AddEntity {
        /// The entity.
        entity: String,
        /// Its stored attributes, in declaration order.
        attributes: Vec<StoredAttribute>,
    },
    /// Adds an attribute; existing objects take `default`, or no value.
    AddAttribute {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: StoredAttribute,
        /// The value existing objects take.
        default: Option<Value>,
    },
    /// Converts the values of an attribute to another value type.
    ChangeValue {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
        /// The current value type.
        from: ValueType,
        /// The new value type.
        to: ValueType,
        /// Factor applied to real and quantity values.
        factor: f64,
    },
    /// Gives a value to objects that have none for an attribute.
    SetValue {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
        /// The value.
        value: Value,
    },
    /// Runs code on the objects of some entities.
    Custom {
        /// The entities the code handles.
        entities: Vec<String>,
        /// The function to call.
        function: String,
    },
    /// Removes an entity and its objects.
    RemoveEntity {
        /// The entity.
        entity: String,
    },
    /// Removes an attribute and its values.
    RemoveAttribute {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
    },
}

/// Why a migration does not turn one version into the next. Messages name schema items only.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MigrationError {
    /// The newer version does not follow the older one.
    #[error("a migration goes from one version to the next, not from {from} to {to}")]
    VersionGap {
        /// Older version.
        from: u32,
        /// Newer version.
        to: u32,
    },
    /// The two versions belong to different namespaces.
    #[error("a migration stays in one namespace")]
    NamespaceChanged,
    /// More than [`MAX_STEPS`] declared steps.
    #[error("a migration has at most {MAX_STEPS} steps")]
    TooManySteps,
    /// A step names an entity that does not exist at that point.
    #[error("step {step}: no entity \"{entity}\"")]
    UnknownEntity {
        /// 1-based declared step.
        step: usize,
        /// The entity.
        entity: String,
    },
    /// A step names an attribute that objects do not store at that point.
    #[error("step {step}: entity \"{entity}\" stores no attribute \"{attribute}\"")]
    UnknownAttribute {
        /// 1-based declared step.
        step: usize,
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
    },
    /// A rename targets a name already in use.
    #[error("step {step}: the name \"{name}\" is already taken")]
    NameTaken {
        /// 1-based declared step.
        step: usize,
        /// The name.
        name: String,
    },
    /// A value type change that has no conversion, or a factor on non-real values.
    #[error("step {step}: values of type {from} cannot become {to} with factor {factor}")]
    Inconvertible {
        /// 1-based declared step.
        step: usize,
        /// Current value type.
        from: ValueType,
        /// New value type.
        to: ValueType,
        /// Requested factor.
        factor: f64,
    },
    /// A `set_value` value does not have the attribute's type, or is not finite.
    #[error("step {step}: the value does not fit the type {expected}")]
    ValueMismatch {
        /// 1-based declared step.
        step: usize,
        /// The attribute's value type.
        expected: ValueType,
    },
    /// After the steps, an attribute still has its older value type.
    #[error(
        "attribute \"{attribute}\" of \"{entity}\" would hold {found} values but the new version expects {expected}: add a change_value or custom step"
    )]
    TypeChanged {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
        /// Value type after the steps.
        found: ValueType,
        /// Value type in the newer version.
        expected: ValueType,
    },
    /// A required attribute may be left without a value.
    #[error(
        "attribute \"{attribute}\" of \"{entity}\" is required but objects may have no value: add a set_value or custom step"
    )]
    Unfilled {
        /// The entity.
        entity: String,
        /// The attribute.
        attribute: String,
    },
}

/// A stored attribute while steps run.
#[derive(Clone, Debug)]
struct Column {
    value: ValueType,
    /// Every object has a value.
    filled: bool,
    /// Every object must have a value.
    required: bool,
}

/// Entity name to stored attributes.
type Layout = BTreeMap<String, BTreeMap<String, Column>>;

/// Completes the declared steps of a migration from `before` to `after` and checks them.
///
/// Steps run in this order: declared renames, inferred additions, the other declared steps as
/// written, then inferred removals, so custom code can still read what is about to be removed.
///
/// # Errors
///
/// Returns every [`MigrationError`] found: steps naming missing items, conversions that do not
/// exist, and differences between versions that no step covers.
pub fn resolve(
    before: &Namespace,
    declared: &[DeclaredStep],
    after: &Namespace,
) -> Result<Migration, Vec<MigrationError>> {
    if before.name != after.name {
        return Err(vec![MigrationError::NamespaceChanged]);
    }
    if before.version.checked_add(1) != Some(after.version) {
        return Err(vec![MigrationError::VersionGap {
            from: before.version,
            to: after.version,
        }]);
    }
    if declared.len() > MAX_STEPS {
        return Err(vec![MigrationError::TooManySteps]);
    }
    let mut resolver = Resolver {
        layout: layout(&stored_tables(before)),
        steps: Vec::new(),
        errors: Vec::new(),
        custom: BTreeSet::new(),
    };
    let numbered = (1..).zip(declared);
    for (step, declared) in numbered.clone() {
        resolver.rename(step, declared);
    }
    let after_tables = stored_tables(after);
    let target = layout(&after_tables);
    resolver.add(&after_tables);
    for (step, declared) in numbered {
        resolver.change(step, declared);
    }
    resolver.remove(&target);
    resolver.check(&target);
    if resolver.errors.is_empty() {
        Ok(Migration {
            version: after.version,
            steps: resolver.steps,
        })
    } else {
        Err(resolver.errors)
    }
}

/// Whether values of type `from` convert to `to`, reals being multiplied by `factor`.
///
/// Numbers become reals or quantities, text and enumeration values convert into each other,
/// references keep their id, lists convert item by item. A factor other than 1 needs real
/// values to multiply, and must be finite and non-zero.
#[must_use]
pub fn converts(from: &ValueType, to: &ValueType, factor: f64) -> bool {
    let scalar = |mut value: &ValueType| {
        while let ValueType::List(inner) = value {
            value = inner;
        }
        matches!(value, ValueType::Real | ValueType::Quantity(_))
    };
    // Exactly 1 means "no scaling"; any other factor must scale reals.
    let identity = factor.to_bits() == 1.0_f64.to_bits();
    let factor_fits = identity || (factor.is_finite() && factor != 0.0 && scalar(to));
    factor_fits && types_convert(from, to)
}

fn types_convert(from: &ValueType, to: &ValueType) -> bool {
    use ValueType::{Bool, Enum, Integer, List, Quantity, Real, Ref, Text};
    match (from, to) {
        (Integer | Real | Quantity(_), Real | Quantity(_))
        | (Bool, Bool)
        | (Integer, Integer)
        | (Text | Enum(_), Text | Enum(_))
        | (Ref(_), Ref(_)) => true,
        (List(from), List(to)) => types_convert(from, to),
        _ => false,
    }
}

/// What objects of each of `tables` store.
fn layout(tables: &[StoredTable<'_>]) -> Layout {
    tables
        .iter()
        .map(|table| {
            let columns = table
                .attributes
                .iter()
                .map(|attribute| (attribute.name.clone(), column(table.side, attribute)))
                .collect();
            (table.name.to_owned(), columns)
        })
        .collect()
}

fn column(side: Side, attribute: &Attribute) -> Column {
    Column {
        value: attribute.value.clone(),
        filled: side.always_set(attribute),
        required: side.applies_presence(attribute) && attribute.presence == Presence::Required,
    }
}

/// The value objects on `side` take when `attribute` is added.
fn default_value(side: Side, attribute: &Attribute) -> Option<Value> {
    match &attribute.presence {
        Presence::Default(value) if side.applies_presence(attribute) => Some(value.clone()),
        _ => None,
    }
}

impl From<&Attribute> for StoredAttribute {
    fn from(attribute: &Attribute) -> Self {
        Self {
            name: attribute.name.clone(),
            value: attribute.value.clone(),
        }
    }
}

struct Resolver {
    layout: Layout,
    steps: Vec<Step>,
    errors: Vec<MigrationError>,
    /// Entities handled by custom steps, exempt from the final check.
    custom: BTreeSet<String>,
}

impl Resolver {
    fn entity(&mut self, step: usize, entity: &str) -> Option<&mut BTreeMap<String, Column>> {
        if !self.layout.contains_key(entity) {
            self.errors.push(MigrationError::UnknownEntity {
                step,
                entity: entity.to_owned(),
            });
        }
        self.layout.get_mut(entity)
    }

    fn column(&mut self, step: usize, entity: &str, attribute: &str) -> Option<&mut Column> {
        if !self.entity(step, entity)?.contains_key(attribute) {
            self.errors.push(MigrationError::UnknownAttribute {
                step,
                entity: entity.to_owned(),
                attribute: attribute.to_owned(),
            });
            return None;
        }
        self.layout.get_mut(entity)?.get_mut(attribute)
    }

    fn name_taken(&mut self, step: usize, name: &str) {
        self.errors.push(MigrationError::NameTaken {
            step,
            name: name.to_owned(),
        });
    }

    fn rename(&mut self, step: usize, declared: &DeclaredStep) {
        match declared {
            DeclaredStep::RenameEntity { old_name, new_name } => {
                if self.entity(step, old_name).is_none() {
                    return;
                }
                if self.layout.contains_key(new_name) {
                    return self.name_taken(step, new_name);
                }
                if let Some(columns) = self.layout.remove(old_name) {
                    self.layout.insert(new_name.clone(), columns);
                }
                for column in self.layout.values_mut().flat_map(BTreeMap::values_mut) {
                    retarget(&mut column.value, old_name, new_name);
                }
                self.steps.push(Step::RenameEntity {
                    from: old_name.clone(),
                    to: new_name.clone(),
                });
            }
            DeclaredStep::RenameAttribute {
                entity,
                old_name,
                new_name,
            } => {
                if self.column(step, entity, old_name).is_none() {
                    return;
                }
                let Some(columns) = self.layout.get_mut(entity) else {
                    return;
                };
                if columns.contains_key(new_name) {
                    return self.name_taken(step, new_name);
                }
                if let Some(column) = columns.remove(old_name) {
                    columns.insert(new_name.clone(), column);
                }
                self.steps.push(Step::RenameAttribute {
                    entity: entity.clone(),
                    from: old_name.clone(),
                    to: new_name.clone(),
                });
            }
            _ => {}
        }
    }

    fn add(&mut self, tables: &[StoredTable<'_>]) {
        for table in tables {
            let Some(columns) = self.layout.get_mut(table.name) else {
                self.steps.push(Step::AddEntity {
                    entity: table.name.to_owned(),
                    attributes: table.attributes.iter().map(|&a| a.into()).collect(),
                });
                // A new table has no objects, so every requirement holds.
                let columns = table
                    .attributes
                    .iter()
                    .map(|attribute| {
                        let column = Column {
                            filled: true,
                            required: false,
                            ..column(table.side, attribute)
                        };
                        (attribute.name.clone(), column)
                    })
                    .collect();
                self.layout.insert(table.name.to_owned(), columns);
                continue;
            };
            for &attribute in &table.attributes {
                if columns.contains_key(&attribute.name) {
                    continue;
                }
                let default = default_value(table.side, attribute);
                let column = Column {
                    filled: default.is_some(),
                    required: false,
                    ..column(table.side, attribute)
                };
                columns.insert(attribute.name.clone(), column);
                self.steps.push(Step::AddAttribute {
                    entity: table.name.to_owned(),
                    attribute: attribute.into(),
                    default,
                });
            }
        }
    }

    fn change(&mut self, step: usize, declared: &DeclaredStep) {
        match declared {
            DeclaredStep::ChangeValue {
                entity,
                attribute,
                value,
                factor,
            } => {
                let Some(column) = self.column(step, entity, attribute) else {
                    return;
                };
                let from = column.value.clone();
                if !converts(&from, value, *factor) {
                    self.errors.push(MigrationError::Inconvertible {
                        step,
                        from,
                        to: value.clone(),
                        factor: *factor,
                    });
                    return;
                }
                column.value = value.clone();
                self.steps.push(Step::ChangeValue {
                    entity: entity.clone(),
                    attribute: attribute.clone(),
                    from,
                    to: value.clone(),
                    factor: *factor,
                });
            }
            DeclaredStep::SetValue {
                entity,
                attribute,
                value,
            } => {
                let Some(column) = self.column(step, entity, attribute) else {
                    return;
                };
                if !value_matches(value, &column.value) || !is_finite(value) {
                    let expected = column.value.clone();
                    self.errors
                        .push(MigrationError::ValueMismatch { step, expected });
                    return;
                }
                column.filled = true;
                self.steps.push(Step::SetValue {
                    entity: entity.clone(),
                    attribute: attribute.clone(),
                    value: value.clone(),
                });
            }
            DeclaredStep::Custom { entities, function } => {
                let known = entities
                    .iter()
                    .all(|entity| self.entity(step, entity).is_some());
                if known {
                    self.custom.extend(entities.iter().cloned());
                    self.steps.push(Step::Custom {
                        entities: entities.clone(),
                        function: function.clone(),
                    });
                }
            }
            DeclaredStep::RenameEntity { .. } | DeclaredStep::RenameAttribute { .. } => {}
        }
    }

    fn remove(&mut self, target: &Layout) {
        let mut removed = Vec::new();
        for (entity, columns) in &mut self.layout {
            let Some(kept) = target.get(entity) else {
                removed.push(entity.clone());
                continue;
            };
            columns.retain(|attribute, _| {
                let keep = kept.contains_key(attribute);
                if !keep {
                    self.steps.push(Step::RemoveAttribute {
                        entity: entity.clone(),
                        attribute: attribute.clone(),
                    });
                }
                keep
            });
        }
        for entity in removed {
            self.layout.remove(&entity);
            self.steps.push(Step::RemoveEntity { entity });
        }
    }

    fn check(&mut self, target: &Layout) {
        for (entity, columns) in target {
            if self.custom.contains(entity) {
                continue;
            }
            for (attribute, expected) in columns {
                let Some(found) = self
                    .layout
                    .get(entity)
                    .and_then(|found| found.get(attribute))
                else {
                    continue;
                };
                if found.value != expected.value {
                    self.errors.push(MigrationError::TypeChanged {
                        entity: entity.clone(),
                        attribute: attribute.clone(),
                        found: found.value.clone(),
                        expected: expected.value.clone(),
                    });
                } else if expected.required && !found.filled {
                    self.errors.push(MigrationError::Unfilled {
                        entity: entity.clone(),
                        attribute: attribute.clone(),
                    });
                }
            }
        }
    }
}

/// Points references to `old` at `new`.
fn retarget(value: &mut ValueType, old: &str, new: &str) {
    match value {
        ValueType::Ref(target) if target == old => new.clone_into(target),
        ValueType::List(inner) => retarget(inner, old, new),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
