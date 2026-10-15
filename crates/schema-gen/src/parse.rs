use std::collections::BTreeMap;

use schema::{
    Attribute, Entity, IfcMapping, Item, Location, MAX_LIST_DEPTH, Namespace, Owner, Presence,
    PropertySet, Representation, Role, TypeDef, Value, ValueType,
};
use serde::de::DeserializeOwned;
use toml::Spanned;

use crate::definition::{AttributeDef, EntityFile, NamespaceFile, PropertySetFile, RoleDef};
use crate::snapshot::parse_target;
use crate::{Error, ErrorKind};

/// The version of the definition file format this crate reads.
pub const FORMAT: u32 = 1;

/// Largest definition file accepted, in bytes. Real definitions are a few kilobytes; the cap
/// keeps hostile input cheap to reject.
pub const MAX_FILE_SIZE: usize = 256 * 1024;

const NAMESPACE_FILE: &str = "namespace.toml";
const ENTITY_DIRS: [&str; 2] = ["entities/", "relationships/"];
const PROPERTY_SET_DIR: &str = "property-sets/";

/// The contents of one definition file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// Path relative to the namespace directory, with `/` separators: `namespace.toml`,
    /// `entities/wall.toml`, `relationships/…` or `property-sets/…`. The directory says what the
    /// file defines.
    pub path: String,
    /// The TOML text.
    pub contents: String,
}

/// Builds the canonical, validated namespace from its definition files.
///
/// Files are read in path order, so the result and the order of errors never depend on the
/// order of `sources`.
///
/// # Errors
///
/// Returns every [`Error`] found, each with its file and, when known, line and column.
pub fn parse_sources(sources: &[Source]) -> Result<Namespace, Vec<Error>> {
    let mut sorted: Vec<&Source> = sources.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut builder = Builder::default();
    for source in sorted {
        builder.read(source);
    }
    builder.finish()
}

/// Where each definition and item came from, to place validation errors.
type Positions<'a> = BTreeMap<(Owner, Option<Item>), (&'a Source, usize)>;

#[derive(Default)]
struct Builder<'a> {
    namespace: Option<(String, u32)>,
    namespace_file: Option<&'a Source>,
    entities: BTreeMap<String, (Entity, &'a Source)>,
    property_sets: BTreeMap<String, (PropertySet, &'a Source)>,
    positions: Positions<'a>,
    errors: Vec<Error>,
}

impl<'a> Builder<'a> {
    fn error(&mut self, source: &Source, offset: Option<usize>, kind: ErrorKind) {
        self.errors
            .push(Error::at(&source.path, &source.contents, offset, kind));
    }

    fn definition_error(&mut self, source: &Source, offset: Option<usize>, message: String) {
        self.error(source, offset, ErrorKind::Definition(message));
    }

    fn read(&mut self, source: &'a Source) {
        if source.contents.len() > MAX_FILE_SIZE {
            let message = format!("file is larger than {MAX_FILE_SIZE} bytes");
            self.definition_error(source, None, message);
        } else if source.path == NAMESPACE_FILE {
            self.read_namespace(source);
        } else if ENTITY_DIRS.iter().any(|dir| source.path.starts_with(dir)) {
            self.read_entity(source);
        } else if source.path.starts_with(PROPERTY_SET_DIR) {
            self.read_property_set(source);
        } else {
            let message = format!(
                "unexpected file: definitions go in {NAMESPACE_FILE}, {}, {} or {PROPERTY_SET_DIR}",
                ENTITY_DIRS[0], ENTITY_DIRS[1]
            );
            self.definition_error(source, None, message);
        }
    }

    /// Deserializes a file and checks its `format`.
    fn deserialize<T: DeserializeOwned>(
        &mut self,
        source: &Source,
        format: fn(&T) -> &Spanned<u32>,
    ) -> Option<T> {
        match toml::from_str::<T>(&source.contents) {
            Ok(file) => {
                let format = format(&file);
                if *format.get_ref() == FORMAT {
                    Some(file)
                } else {
                    let message = format!(
                        "format {} is not supported; this version reads format {FORMAT}",
                        format.get_ref()
                    );
                    self.definition_error(source, Some(format.span().start), message);
                    None
                }
            }
            Err(error) => {
                let offset = error.span().map(|span| span.start);
                let message = error.message().to_owned();
                self.error(source, offset, ErrorKind::Syntax(message));
                None
            }
        }
    }

    fn read_namespace(&mut self, source: &'a Source) {
        self.namespace_file = Some(source);
        let Some(file) = self.deserialize::<NamespaceFile>(source, |file| &file.format) else {
            return;
        };
        self.namespace = Some((file.namespace.name, file.namespace.version));
    }

    fn read_entity(&mut self, source: &'a Source) {
        let Some(file) = self.deserialize::<EntityFile>(source, |file| &file.format) else {
            return;
        };
        let name_span = file.entity.name.span().start;
        let name = file.entity.name.into_inner();
        let owner = Owner::Entity(name.clone());
        self.mark(&owner, None, source, name_span);

        let type_def = file.type_def.map(|section| {
            self.mark(&owner, Some(Item::Type), source, section.span().start);
            let section = section.into_inner();
            TypeDef {
                name: section.name.unwrap_or_else(|| format!("{name}Type")),
                doc: section.doc,
            }
        });
        let ifc = file.ifc.map(|section| {
            self.mark(&owner, Some(Item::Ifc), source, section.span().start);
            let section = section.into_inner();
            // The IFC type entity follows the IFC convention unless the definition overrides it.
            let type_entity = section
                .type_entity
                .or_else(|| type_def.as_ref().map(|_| format!("{}Type", section.entity)));
            IfcMapping {
                entity: section.entity,
                predefined_type: section.predefined_type,
                object_type: section.object_type,
                type_entity,
            }
        });
        let attributes = self.attributes(source, &owner, file.attributes, Item::Attribute);
        let roles = file
            .roles
            .into_iter()
            .filter_map(|definition| {
                let item = Item::Role(definition.name.get_ref().clone());
                self.mark(&owner, Some(item), source, definition.name.span().start);
                self.role(source, definition)
            })
            .collect();
        let representations = file
            .representations
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let item = Item::Representation(index);
                self.mark(&owner, Some(item), source, definition.lod.span().start);
                Representation {
                    lod: definition.lod.into_inner(),
                    context: definition.context,
                    generator: definition.generator,
                    depends_on: definition.depends_on,
                }
            })
            .collect();
        let entity = Entity {
            name: name.clone(),
            kind: file.entity.kind.into(),
            doc: file.entity.doc,
            deprecated: file.entity.deprecated,
            property_sets: file.entity.property_sets,
            type_def,
            ifc,
            attributes,
            roles,
            representations,
        };
        if let Some((_, first)) = self.entities.get(&name) {
            let message = format!("entity \"{name}\" is already defined in {}", first.path);
            self.definition_error(source, Some(name_span), message);
        } else {
            self.entities.insert(name, (entity, source));
        }
    }

    fn read_property_set(&mut self, source: &'a Source) {
        let Some(file) = self.deserialize::<PropertySetFile>(source, |file| &file.format) else {
            return;
        };
        let name_span = file.property_set.name.span().start;
        let name = file.property_set.name.into_inner();
        let owner = Owner::PropertySet(name.clone());
        self.mark(&owner, None, source, name_span);
        let properties = self.attributes(source, &owner, file.properties, Item::Property);
        let property_set = PropertySet {
            name: name.clone(),
            applies_to: file.property_set.applies_to,
            properties,
        };
        if let Some((_, first)) = self.property_sets.get(&name) {
            let message = format!(
                "property set \"{name}\" is already defined in {}",
                first.path
            );
            self.definition_error(source, Some(name_span), message);
        } else {
            self.property_sets.insert(name, (property_set, source));
        }
    }

    /// Records where a definition or one of its items starts, to place validation errors.
    fn mark(&mut self, owner: &Owner, item: Option<Item>, source: &'a Source, offset: usize) {
        self.positions
            .insert((owner.clone(), item), (source, offset));
    }

    /// Converts attributes or properties, skipping (and reporting) invalid ones.
    fn attributes(
        &mut self,
        source: &'a Source,
        owner: &Owner,
        definitions: Vec<AttributeDef>,
        item: fn(String) -> Item,
    ) -> Vec<Attribute> {
        definitions
            .into_iter()
            .filter_map(|definition| {
                let name = definition.name.get_ref().clone();
                self.mark(
                    owner,
                    Some(item(name)),
                    source,
                    definition.name.span().start,
                );
                self.attribute(source, definition)
            })
            .collect()
    }

    fn attribute(&mut self, source: &Source, definition: AttributeDef) -> Option<Attribute> {
        let value = match definition.value.get_ref().parse::<ValueType>() {
            Ok(value) => value,
            Err(error) => {
                self.definition_error(
                    source,
                    Some(definition.value.span().start),
                    error.to_string(),
                );
                return None;
            }
        };
        let presence = match (definition.required, definition.default) {
            (true, Some(default)) => {
                let message = "an attribute cannot be both required and have a default".into();
                self.definition_error(source, Some(default.span().start), message);
                return None;
            }
            (true, None) => Presence::Required,
            (false, None) => Presence::Optional,
            (false, Some(default)) => {
                let offset = default.span().start;
                match convert_value(default.into_inner(), 0) {
                    Ok(value) => Presence::Default(value),
                    Err(message) => {
                        self.definition_error(source, Some(offset), message);
                        return None;
                    }
                }
            }
        };
        Some(Attribute {
            name: definition.name.into_inner(),
            value,
            presence,
            placement: definition.on.into(),
            doc: definition.doc,
        })
    }

    fn role(&mut self, source: &Source, definition: RoleDef) -> Option<Role> {
        let mut targets = Vec::with_capacity(definition.targets.len());
        let mut valid = true;
        for target in definition.targets {
            let offset = target.span().start;
            let target = target.into_inner();
            match parse_target(&target) {
                Ok(target) => targets.push(target),
                Err(message) => {
                    self.definition_error(source, Some(offset), message);
                    valid = false;
                }
            }
        }
        valid.then(|| Role {
            name: definition.name.into_inner(),
            targets,
            cardinality: definition.cardinality.into(),
            exclusive: definition.exclusive,
        })
    }

    fn finish(mut self) -> Result<Namespace, Vec<Error>> {
        if self.namespace_file.is_none() {
            let message = format!("{NAMESPACE_FILE} is missing");
            self.errors.push(Error::new(ErrorKind::Definition(message)));
        }
        // A partial namespace would only add errors caused by the ones already reported.
        let (Some((name, version)), true) = (self.namespace.take(), self.errors.is_empty()) else {
            return Err(self.errors);
        };
        let namespace = Namespace {
            name,
            version,
            entities: self
                .entities
                .into_iter()
                .map(|(name, (entity, _))| (name, entity))
                .collect(),
            property_sets: self
                .property_sets
                .into_iter()
                .map(|(name, (property_set, _))| (name, property_set))
                .collect(),
        };
        schema::validate(&namespace).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| locate(&self.positions, self.namespace_file, error))
                .collect::<Vec<_>>()
        })?;
        Ok(namespace)
    }
}

/// Places a validation error at the item it is about, else at its definition.
fn locate(
    positions: &Positions<'_>,
    namespace_file: Option<&Source>,
    error: schema::ValidationError,
) -> Error {
    let Location { owner, item } = &error.location;
    let position = positions
        .get(&(owner.clone(), item.clone()))
        .or_else(|| positions.get(&(owner.clone(), None)))
        .map(|&(source, offset)| (source, Some(offset)))
        .or_else(|| namespace_file.map(|source| (source, None)));
    let kind = ErrorKind::Validation(Box::new(error));
    match position {
        Some((source, offset)) => Error::at(&source.path, &source.contents, offset, kind),
        None => Error::new(kind),
    }
}

/// Converts a TOML default. Nesting is capped like value types are, so hostile input cannot
/// recurse deeply.
pub(crate) fn convert_value(value: toml::Value, depth: usize) -> Result<Value, String> {
    match value {
        toml::Value::Boolean(value) => Ok(Value::Bool(value)),
        toml::Value::Integer(value) => Ok(Value::Integer(value)),
        toml::Value::Float(value) => Ok(Value::Real(value)),
        toml::Value::String(value) => Ok(Value::Text(value)),
        toml::Value::Array(_) if depth >= MAX_LIST_DEPTH => {
            Err(format!("lists nest more than {MAX_LIST_DEPTH} levels deep"))
        }
        toml::Value::Array(values) => values
            .into_iter()
            .map(|value| convert_value(value, depth + 1))
            .collect::<Result<_, _>>()
            .map(Value::List),
        toml::Value::Datetime(_) | toml::Value::Table(_) => {
            Err("defaults must be booleans, numbers, text or lists of them".into())
        }
    }
}
