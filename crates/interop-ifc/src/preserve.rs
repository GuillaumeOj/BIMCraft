//! Keeps the IFC data the model does not map, and writes it back on export (INT-03).
//!
//! On import, [`Preserved::split`] separates the instances the importer maps from the rest, which
//! it keeps as read. On export, [`Preserved::restore`] gives the kept instances back, with their
//! references to mapped instances pointed at the instances the exporter wrote in their place.
//!
//! Instances stay in the reader's form ([`Instance`]), so what is kept is written back exactly
//! as it was read: complex instances, user-defined (`!NAME`) records, encoded strings and all.
//! [`normalise`] gives the canonical bytes of a file, to compare one before and after a
//! round-trip.

use std::collections::{BTreeMap, BTreeSet};

use crate::step::{Instance, InstanceBody, Parameter, Record, StepWriter, WriteError};

/// The instances of an IFC file that the importer did not map, as read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preserved {
    instances: Vec<Instance>,
}

/// The kept instances of a [`Preserved`], ready to write, and the references they lost.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Restored {
    /// The kept instances, in the order they were read, with their export ids.
    pub instances: Vec<Instance>,
    /// The export id of each kept instance, by its id in the file read, so that the mapped
    /// instances the export writes can refer to them.
    pub ids: BTreeMap<u64, u64>,
    /// The references the export could not resolve, in the order found.
    pub dangling: Vec<Dangling>,
}

/// A reference from a kept instance that the export could not resolve: to a mapped instance the
/// export no longer has, or to an instance the file read did not contain. It is removed from a
/// list, and replaced by `$` anywhere else. A list can end up empty where IFC requires at least
/// one element, such as the related objects of a relationship whose objects were all removed:
/// the exporter decides whether to drop such an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dangling {
    /// The export id of the kept instance that held the reference.
    pub instance: u64,
    /// The id the referenced instance had in the file read.
    pub reference: u64,
}

impl Preserved {
    /// Splits the `instances` of a file into those `is_mapped` accepts, returned in order, and
    /// the others, kept.
    ///
    /// `is_mapped` sees the record of each simple instance with a standard name. Complex
    /// instances and user-defined (`!NAME`) records are always kept: the model maps neither.
    pub fn split(
        instances: impl IntoIterator<Item = Instance>,
        mut is_mapped: impl FnMut(&Record) -> bool,
    ) -> (Vec<Instance>, Self) {
        let (mapped, instances) =
            instances
                .into_iter()
                .partition(|instance| match &instance.body {
                    InstanceBody::Simple(record) => {
                        !record.name.starts_with('!') && is_mapped(record)
                    }
                    InstanceBody::Complex(_) => false,
                });
        (mapped, Self { instances })
    }

    /// The kept instances, in the order they were read, with the ids they had.
    #[must_use]
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// The kept instances for an export that wrote the instances `used`, where `export_id` gives
    /// the export id of each mapped instance the export still has, by its id in the file read.
    ///
    /// A kept instance keeps its id unless the export used it; it then takes the smallest id
    /// that neither the export nor another kept instance has. References between kept instances
    /// follow them, and [`Restored::ids`] lists the new ids. A reference that resolves to nothing
    /// is removed from its list, or replaced by `$` outside a list, and reported in
    /// [`Restored::dangling`].
    pub fn restore(
        &self,
        mut export_id: impl FnMut(u64) -> Option<u64>,
        used: &BTreeSet<u64>,
    ) -> Restored {
        let order = self.export_ids(used);
        let ids: BTreeMap<u64, u64> = self
            .instances
            .iter()
            .map(|instance| instance.id)
            .zip(order.iter().copied())
            .collect();
        let mut dangling = Vec::new();
        let instances = self
            .instances
            .iter()
            .zip(order)
            .map(|(instance, id)| {
                let mut resolve = |reference| {
                    let target = ids
                        .get(&reference)
                        .copied()
                        .or_else(|| export_id(reference));
                    if target.is_none() {
                        dangling.push(Dangling {
                            instance: id,
                            reference,
                        });
                    }
                    target
                };
                let body = match &instance.body {
                    InstanceBody::Simple(record) => {
                        InstanceBody::Simple(relink_record(record, &mut resolve))
                    }
                    InstanceBody::Complex(records) => InstanceBody::Complex(
                        records
                            .iter()
                            .map(|record| relink_record(record, &mut resolve))
                            .collect(),
                    ),
                };
                Instance { id, body }
            })
            .collect();
        Restored {
            instances,
            ids,
            dangling,
        }
    }

    /// The export id of each kept instance, in order. New ids fill the gaps from 1 up, so they
    /// never run past the largest id, whatever the file holds.
    fn export_ids(&self, used: &BTreeSet<u64>) -> Vec<u64> {
        let ids = self.instances.iter().map(|instance| instance.id);
        if !ids.clone().any(|id| used.contains(&id)) {
            return ids.collect();
        }
        let mut taken = used.clone();
        taken.extend(self.instances.iter().map(|instance| instance.id));
        let mut candidate = 1_u64;
        self.instances
            .iter()
            .map(|instance| {
                if !used.contains(&instance.id) {
                    return instance.id;
                }
                while taken.contains(&candidate) {
                    candidate = candidate.saturating_add(1);
                }
                taken.insert(candidate);
                candidate
            })
            .collect()
    }
}

fn relink_record(record: &Record, resolve: &mut impl FnMut(u64) -> Option<u64>) -> Record {
    Record {
        name: record.name.clone(),
        parameters: record
            .parameters
            .iter()
            .map(|parameter| relink(parameter, resolve).unwrap_or(Parameter::Null))
            .collect(),
    }
}

/// The parameter with its references resolved, or `None` for a reference that no longer
/// resolves.
fn relink(
    parameter: &Parameter,
    resolve: &mut impl FnMut(u64) -> Option<u64>,
) -> Option<Parameter> {
    Some(match parameter {
        Parameter::Reference(id) => Parameter::Reference(resolve(*id)?),
        Parameter::List(items) => Parameter::List(
            items
                .iter()
                .filter_map(|item| relink(item, resolve))
                .collect(),
        ),
        Parameter::Typed { type_name, value } => Parameter::Typed {
            type_name: type_name.clone(),
            value: Box::new(relink(value, resolve).unwrap_or(Parameter::Null)),
        },
        other => other.clone(),
    })
}

/// The canonical bytes of a file: `header`, then `instances` sorted by id, as [`StepWriter`]
/// writes them. Two files with the same content, whatever their spacing, comments, number forms
/// or instance order, give the same bytes.
///
/// # Errors
///
/// Returns a [`WriteError`] if the header or an instance cannot be written as Part 21.
pub fn normalise(
    header: &[Record],
    instances: impl IntoIterator<Item = Instance>,
) -> Result<Vec<u8>, WriteError> {
    let mut instances: Vec<Instance> = instances.into_iter().collect();
    instances.sort_by_key(|instance| instance.id);
    let mut writer = StepWriter::new(Vec::new(), header)?;
    for instance in &instances {
        writer.write_instance(instance)?;
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;

    use super::{Dangling, Preserved, normalise};
    use crate::step::test_support::{instances, record, simple, typed};
    use crate::step::{Instance, InstanceBody, Parameter, Record, WriteError};

    fn ids(instances: &[Instance]) -> Vec<u64> {
        instances.iter().map(|instance| instance.id).collect()
    }

    fn is_wall(record: &Record) -> bool {
        record.name == "IFCWALL"
    }

    fn file() -> Vec<Instance> {
        vec![
            simple(1, "IFCWALL", vec![Parameter::Null]),
            simple(
                2,
                "IFCPROPERTYSINGLEVALUE",
                vec![Parameter::String("Width".into())],
            ),
            simple(
                3,
                "IFCPROPERTYSET",
                vec![Parameter::List(vec![Parameter::Reference(2)])],
            ),
            simple(
                4,
                "IFCRELDEFINESBYPROPERTIES",
                vec![
                    Parameter::List(vec![Parameter::Reference(1)]),
                    Parameter::Reference(3),
                ],
            ),
            Instance {
                id: 5,
                body: InstanceBody::Complex(vec![record("IFCWALL", vec![])]),
            },
            simple(6, "!IFCWALL", vec![Parameter::Reference(1)]),
        ]
    }

    #[test]
    fn keeps_what_is_not_mapped_complex_and_user_defined_instances_included() {
        let (mapped, preserved) = Preserved::split(file(), is_wall);
        assert_eq!(ids(&mapped), [1]);
        assert_eq!(ids(preserved.instances()), [2, 3, 4, 5, 6]);
    }

    #[test]
    fn gives_back_what_it_kept_when_the_export_keeps_the_ids() {
        let (mapped, preserved) = Preserved::split(file(), is_wall);
        let used = BTreeSet::from([1]);
        let restored = preserved.restore(Some, &used);
        assert_eq!(restored.instances, preserved.instances());
        assert_eq!(restored.dangling, []);
        let mut merged = mapped;
        merged.extend(restored.instances);
        assert_eq!(
            normalise(&[], merged).unwrap(),
            normalise(&[], file()).unwrap()
        );
    }

    #[test]
    fn points_references_at_the_exported_instances_and_moves_kept_ids_the_export_used() {
        let (_, preserved) = Preserved::split(file(), is_wall);
        // The wall #1 is exported as #2, which a kept instance had: that one takes the free #1.
        let used = BTreeSet::from([2]);
        let restored = preserved.restore(|id| (id == 1).then_some(2), &used);
        assert_eq!(ids(&restored.instances), [1, 3, 4, 5, 6]);
        assert_eq!(
            restored.ids,
            BTreeMap::from([(2, 1), (3, 3), (4, 4), (5, 5), (6, 6)])
        );
        assert_eq!(
            restored.instances[1],
            simple(
                3,
                "IFCPROPERTYSET",
                vec![Parameter::List(vec![Parameter::Reference(1)])]
            )
        );
        assert_eq!(
            restored.instances[2],
            simple(
                4,
                "IFCRELDEFINESBYPROPERTIES",
                vec![
                    Parameter::List(vec![Parameter::Reference(2)]),
                    Parameter::Reference(3)
                ],
            )
        );
        assert_eq!(
            restored.instances[4],
            simple(6, "!IFCWALL", vec![Parameter::Reference(2)])
        );
    }

    #[test]
    fn drops_references_to_instances_the_export_no_longer_has() {
        let file = [
            simple(1, "IFCWALL", vec![]),
            simple(
                2,
                "IFCREL",
                vec![
                    Parameter::List(vec![Parameter::Reference(1), Parameter::Reference(9)]),
                    Parameter::Reference(1),
                    typed("IFCREF", Parameter::Reference(1)),
                ],
            ),
            Instance {
                id: 3,
                body: InstanceBody::Complex(vec![record("A", vec![Parameter::Reference(1)])]),
            },
        ];
        let (_, preserved) = Preserved::split(file, is_wall);
        let restored = preserved.restore(|id| (id == 9).then_some(9), &BTreeSet::from([9]));
        assert_eq!(
            restored.instances,
            [
                simple(
                    2,
                    "IFCREL",
                    vec![
                        Parameter::List(vec![Parameter::Reference(9)]),
                        Parameter::Null,
                        typed("IFCREF", Parameter::Null),
                    ],
                ),
                Instance {
                    id: 3,
                    body: InstanceBody::Complex(vec![record("A", vec![Parameter::Null])]),
                },
            ]
        );
        let dangling = |instance| Dangling {
            instance,
            reference: 1,
        };
        assert_eq!(
            restored.dangling,
            [dangling(2), dangling(2), dangling(2), dangling(3)]
        );
    }

    /// New ids fill gaps, so a file that already uses the largest id still gets distinct ones.
    #[test]
    fn moved_instances_get_distinct_ids_next_to_the_largest_one() {
        let file = [
            simple(1, "A", vec![]),
            simple(2, "B", vec![]),
            simple(u64::MAX, "C", vec![]),
        ];
        let (_, preserved) = Preserved::split(file, is_wall);
        let restored = preserved.restore(Some, &BTreeSet::from([1, 2, 3]));
        assert_eq!(ids(&restored.instances), [4, 5, u64::MAX]);
    }

    #[test]
    fn normalising_reports_what_cannot_be_written() {
        let invalid = simple(1, "NOT A NAME", vec![]);
        assert!(matches!(
            normalise(&[], [invalid]),
            Err(WriteError::InvalidName)
        ));
        let header = [record("ENDSEC", vec![])];
        assert!(matches!(
            normalise(&header, []),
            Err(WriteError::InvalidName)
        ));
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Whatever the importer maps, an export that keeps the ids gives the file back.
        #[test]
        fn split_then_restore_gives_the_file_back(
            file in instances(0..12),
            mapped_names in proptest::collection::btree_set("[A-Za-z_]", 0..4),
        ) {
            let (mapped, preserved) = Preserved::split(file.clone(), |record| {
                mapped_names.iter().any(|name| record.name.starts_with(name.as_str()))
            });
            let used: BTreeSet<u64> = mapped.iter().map(|instance| instance.id).collect();
            let restored = preserved.restore(Some, &used);
            prop_assert!(restored.dangling.is_empty());
            let mut merged = mapped;
            merged.extend(restored.instances);
            merged.sort_by_key(|instance| instance.id);
            let mut file = file;
            file.sort_by_key(|instance| instance.id);
            prop_assert_eq!(merged, file);
        }
    }
}
