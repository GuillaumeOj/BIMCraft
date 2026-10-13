//! The definition file format of ADR 0002, as serde structures. Unknown keys are errors, and
//! names keep their position so later errors can point at the right line.

use serde::Deserialize;
use toml::Spanned;

/// `namespace.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceFile {
    pub format: Spanned<u32>,
    pub namespace: NamespaceSection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceSection {
    pub name: String,
    pub version: u32,
}

/// An entity or relationship file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityFile {
    pub format: Spanned<u32>,
    pub entity: EntitySection,
    #[serde(rename = "type")]
    pub type_def: Option<Spanned<TypeSection>>,
    pub ifc: Option<Spanned<IfcSection>>,
    #[serde(default, rename = "attribute")]
    pub attributes: Vec<AttributeDef>,
    #[serde(default, rename = "role")]
    pub roles: Vec<RoleDef>,
    #[serde(default, rename = "representation")]
    pub representations: Vec<RepresentationDef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitySection {
    pub name: Spanned<String>,
    pub kind: KindDef,
    pub doc: Option<String>,
    pub deprecated: Option<u32>,
    #[serde(default)]
    pub property_sets: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindDef {
    Element,
    Spatial,
    Relationship,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeSection {
    pub name: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IfcSection {
    pub entity: String,
    pub predefined_type: Option<String>,
    pub object_type: Option<String>,
    pub type_entity: Option<String>,
}

/// An `[[attribute]]` of an entity or a `[[property]]` of a property set.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeDef {
    pub name: Spanned<String>,
    pub value: Spanned<String>,
    #[serde(default)]
    pub required: bool,
    pub default: Option<Spanned<toml::Value>>,
    #[serde(default)]
    pub on: PlacementDef,
    pub doc: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementDef {
    #[default]
    Instance,
    Type,
    TypeOverridable,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDef {
    pub name: Spanned<String>,
    pub targets: Vec<Spanned<String>>,
    pub cardinality: CardinalityDef,
    #[serde(default)]
    pub exclusive: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardinalityDef {
    One,
    Many,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepresentationDef {
    pub lod: Spanned<u16>,
    pub context: String,
    pub generator: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// A property set file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertySetFile {
    pub format: Spanned<u32>,
    pub property_set: PropertySetSection,
    #[serde(default, rename = "property")]
    pub properties: Vec<AttributeDef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertySetSection {
    pub name: Spanned<String>,
    pub applies_to: Vec<String>,
}
