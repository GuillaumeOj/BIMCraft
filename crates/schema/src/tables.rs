//! Where objects keep their attribute values (CORE-02): each entity's instances in one table,
//! and its type objects, when it has a type, in a table named after the type (`WallType`).
//!
//! The in-memory store, the project file and the migration check all lay objects out this way;
//! this module is the one place that says which attributes each table holds.

use crate::{Attribute, Namespace, Placement, Presence};

/// Which objects a table holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Instances: `instance` and `type_overridable` attributes.
    Instance,
    /// Type objects: `type` and `type_overridable` attributes.
    Type,
}

impl Side {
    /// Whether objects on this side store attributes placed on `placement`.
    #[must_use]
    pub fn holds(self, placement: Placement) -> bool {
        match self {
            Self::Instance => placement != Placement::Type,
            Self::Type => placement != Placement::Instance,
        }
    }

    /// Whether the attribute's presence applies to objects on this side. On instances a
    /// `type_overridable` value is an optional override, as the type provides the value.
    #[must_use]
    pub fn applies_presence(self, attribute: &Attribute) -> bool {
        self == Self::Type || attribute.placement == Placement::Instance
    }

    /// Whether every object on this side has a value for the attribute: it is required or has
    /// a default the store fills in.
    #[must_use]
    pub fn always_set(self, attribute: &Attribute) -> bool {
        self.applies_presence(attribute) && attribute.presence != Presence::Optional
    }
}

/// One table of a namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredTable<'a> {
    /// The entity or type it is named after.
    pub name: &'a str,
    /// Which objects it holds.
    pub side: Side,
    /// The attributes its objects store, in declaration order.
    pub attributes: Vec<&'a Attribute>,
}

/// Every table of `namespace`: per entity, its instances' table then, when it has a type, its
/// type objects' table.
#[must_use]
pub fn stored_tables(namespace: &Namespace) -> Vec<StoredTable<'_>> {
    namespace
        .entities
        .values()
        .flat_map(|entity| {
            let table = move |name, side: Side| StoredTable {
                name,
                side,
                attributes: entity
                    .attributes
                    .iter()
                    .filter(|attribute| side.holds(attribute.placement))
                    .collect(),
            };
            let types = entity
                .type_def
                .iter()
                .map(move |type_def| table(type_def.name.as_str(), Side::Type));
            std::iter::once(table(entity.name.as_str(), Side::Instance)).chain(types)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{Entity, EntityKind, TypeDef, Value};

    fn attribute(name: &str, presence: Presence, placement: Placement) -> Attribute {
        Attribute {
            name: name.into(),
            value: crate::ValueType::Real,
            presence,
            placement,
            doc: None,
        }
    }

    #[test]
    fn instances_and_types_hold_their_attributes() {
        let default = || Presence::Default(Value::Real(1.0));
        let attributes = vec![
            attribute("width", Presence::Required, Placement::Instance),
            attribute("height", default(), Placement::TypeOverridable),
            attribute("rating", Presence::Optional, Placement::Type),
        ];
        let wall = Entity {
            name: "Wall".into(),
            kind: EntityKind::Element,
            doc: None,
            deprecated: None,
            property_sets: Vec::new(),
            type_def: Some(TypeDef {
                name: "WallType".into(),
                doc: None,
            }),
            ifc: None,
            attributes,
            roles: Vec::new(),
            representations: Vec::new(),
        };
        let slab = Entity {
            name: "Slab".into(),
            type_def: None,
            ..wall.clone()
        };
        let namespace = Namespace {
            name: "core".into(),
            version: 1,
            entities: BTreeMap::from([("Slab".into(), slab), ("Wall".into(), wall)]),
            property_sets: BTreeMap::new(),
        };
        let tables: Vec<_> = stored_tables(&namespace)
            .iter()
            .map(|table| {
                let names: Vec<_> = table.attributes.iter().map(|a| a.name.as_str()).collect();
                (table.name, table.side, names)
            })
            .collect();
        assert_eq!(
            tables,
            [
                ("Slab", Side::Instance, vec!["width", "height"]),
                ("Wall", Side::Instance, vec!["width", "height"]),
                ("WallType", Side::Type, vec!["height", "rating"]),
            ]
        );
        let entity = &namespace.entities["Wall"];
        let [width, height, rating] = [0, 1, 2].map(|i| &entity.attributes[i]);
        assert!(Side::Instance.always_set(width));
        assert!(!Side::Instance.always_set(height));
        assert!(Side::Type.always_set(height));
        assert!(!Side::Type.always_set(rating));
        assert!(Side::Type.applies_presence(rating));
    }
}
