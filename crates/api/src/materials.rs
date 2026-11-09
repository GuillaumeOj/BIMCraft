//! The `materials.*` queries and commands: the library of European grades and the materials of
//! the project (MAT-01, MAT-02).
//!
//! A project material is an object of the `Material` entity whose values are sourced attributes
//! (ADR 0019). Every command runs as one batch, so it is one undo step, is recorded with the
//! caller's origin and reports a change event. The material maths, the library and the checks of
//! a value live in the `material` crate; nothing here recomputes them (ENG-04).
//!
//! Objects the caller does not name are never drawn here: a command takes the id of the material
//! it creates, as `model.create` does, so it can be replayed (CORE-11). The one exception is the
//! `MaterialOriginal` that keeps a library value when the user edits it: its id derives from the
//! material's id and the property, so it is the same on every replay.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use core_model::{EntityId, Model, Source};
use core_txn::{ModelCommand, Origin};
use material::{
    BehaviourLaw, Category, ENTITY, Edit, EditError, Factor, LawKind, Library, Material,
    MaterialError, NationalAnnex, Property, Quantity, Range, ReadError, Sourced,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::georeference::one_of_strings;
use crate::queries::page_size;
use crate::schema::{array_of, def, nullable, object};
use crate::{ApiError, Descriptor, Outcome, Session, Signature, Version};

const V1: Version = Version::new(1, 0, 0);

/// The longest material name accepted, in characters.
const MAX_NAME_LEN: usize = 120;
/// The most properties one `materials.create` or `materials.update` may carry.
const MAX_PROPERTIES: usize = 64;
/// The longest search text accepted, in characters.
const MAX_SEARCH_LEN: usize = 200;

/// Why a `materials.*` call failed. Each has a stable `reason` the UI maps to a message, and
/// none repeats a value of the model (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MaterialsError {
    /// The library has no grade with this key.
    #[error("the library has no such grade")]
    UnknownGrade,
    /// The object is not a material of the project.
    #[error("no such material")]
    NotFound,
    /// Another material of the project has this name, ignoring case.
    #[error("another material has this name")]
    DuplicateName,
    /// The name is empty, longer than 120 characters or holds a control character.
    #[error("the name is not valid")]
    InvalidName,
    /// The material's category has no property with this key, or the key is unknown.
    #[error("unknown property")]
    UnknownProperty {
        /// The key as the caller wrote it.
        key: String,
    },
    /// A value is not finite, out of range, or inconsistent with the others. `key` names the
    /// property when one is at fault.
    #[error("invalid value")]
    InvalidValue {
        /// The property at fault, when there is one.
        key: Option<String>,
    },
    /// A property the material needs is missing, or the change would remove it.
    #[error("a required property is missing")]
    RequiredProperty {
        /// The property.
        key: String,
    },
    /// Other objects use the material.
    #[error("the material is in use")]
    InUse {
        /// How many objects use it.
        used_by: usize,
    },
}

impl MaterialsError {
    /// The stable `snake_case` name of the error, the `data.reason` of its response.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::UnknownGrade => "unknown_grade",
            Self::NotFound => "not_found",
            Self::DuplicateName => "duplicate_name",
            Self::InvalidName => "invalid_name",
            Self::UnknownProperty { .. } => "unknown_property",
            Self::InvalidValue { .. } => "invalid_value",
            Self::RequiredProperty { .. } => "required_property",
            Self::InUse { .. } => "in_use",
        }
    }

    /// The property at fault, when the error names one.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::UnknownProperty { key } | Self::RequiredProperty { key } => Some(key),
            Self::InvalidValue { key } => key.as_deref(),
            _ => None,
        }
    }
}

// --- shared shapes ---

fn text() -> serde_json::Value {
    json!({ "type": "string" })
}

fn category_schema() -> serde_json::Value {
    one_of_strings(&Category::ALL.map(Category::name))
}

fn annex_schema() -> serde_json::Value {
    one_of_strings(&NationalAnnex::ALL.map(NationalAnnex::name))
}

/// The definitions the materials signatures share, for the protocol schema.
pub(crate) fn definitions() -> [(String, serde_json::Value); 5] {
    let number = || json!({ "type": "number" });
    let nullable_number = || nullable(number());
    let quantities = Quantity::ALL.map(Quantity::name);
    let range = object(
        &[
            ("min", nullable_number()),
            ("min_inclusive", json!({ "type": "boolean" })),
            ("max", nullable_number()),
            ("max_inclusive", json!({ "type": "boolean" })),
        ],
        &[],
    );
    let property = object(
        &[
            ("key", text()),
            ("quantity", one_of_strings(&quantities)),
            ("value", nullable_number()),
            ("source", nullable(def("Source"))),
            ("library_value", nullable_number()),
            ("range", range),
            ("required", json!({ "type": "boolean" })),
        ],
        &[("annex", def("MaterialAnnex"))],
    );
    let law = object(
        &[
            (
                "kind",
                one_of_strings(&[
                    "parabola_rectangle",
                    "bilinear",
                    "linear_elastic",
                    "elastic_plastic",
                    "elastic_brittle",
                    "none",
                ]),
            ),
            ("source", nullable(def("Source"))),
            (
                "branch",
                nullable(one_of_strings(&["horizontal", "inclined"])),
            ),
        ],
        &[],
    );
    let input = object(
        &[("key", text()), ("value", nullable_number())],
        &[("annex", def("MaterialAnnex"))],
    );
    [
        ("MaterialCategory".to_owned(), category_schema()),
        ("MaterialAnnex".to_owned(), annex_schema()),
        ("MaterialProperty".to_owned(), property),
        ("MaterialLaw".to_owned(), law),
        ("MaterialPropertyInput".to_owned(), input),
    ]
}

fn name_schema() -> serde_json::Value {
    json!({ "type": "string", "maxLength": MAX_NAME_LEN })
}

fn search_schema() -> serde_json::Value {
    json!({ "type": "string", "maxLength": MAX_SEARCH_LEN })
}

fn inputs_schema() -> serde_json::Value {
    let mut schema = array_of(def("MaterialPropertyInput"));
    schema["maxItems"] = json!(MAX_PROPERTIES);
    schema
}

/// The materials queries, then their commands.
pub(crate) fn descriptors() -> impl Iterator<Item = Descriptor<Session>> {
    queries().into_iter().chain(commands())
}

/// The shared shorthands of the signatures.
fn id() -> serde_json::Value {
    def("EntityId")
}

fn integer() -> serde_json::Value {
    json!({ "type": "integer", "minimum": 0 })
}

/// The signature of a read of one material's detail, plus what each reader adds.
fn detail(extra: &[(&'static str, serde_json::Value)]) -> Vec<(&'static str, serde_json::Value)> {
    let mut fields = vec![
        ("name", text()),
        ("category", def("MaterialCategory")),
        ("design_code", nullable(text())),
        ("law", def("MaterialLaw")),
        ("properties", array_of(def("MaterialProperty"))),
    ];
    fields.extend(extra.iter().cloned());
    fields
}

/// The materials queries.
fn queries() -> [Descriptor<Session>; 4] {
    let boolean = || json!({ "type": "boolean" });
    let grade_fields = detail(&[("key", text())]);
    let grade = Signature {
        params: object(&[("key", text())], &[]),
        result: object(&grade_fields, &[]),
    };
    let library = Signature {
        params: object(
            &[],
            &[
                ("search", search_schema()),
                ("category", def("MaterialCategory")),
                ("offset", integer()),
                ("limit", integer()),
            ],
        ),
        result: object(
            &[
                (
                    "items",
                    array_of(object(
                        &[
                            ("key", text()),
                            ("name", text()),
                            ("category", def("MaterialCategory")),
                            ("design_code", nullable(text())),
                        ],
                        &[],
                    )),
                ),
                ("total", integer()),
            ],
            &[],
        ),
    };
    let list = Signature {
        params: object(
            &[],
            &[
                ("search", search_schema()),
                ("category", def("MaterialCategory")),
            ],
        ),
        result: object(
            &[(
                "items",
                array_of(object(
                    &[
                        ("id", id()),
                        ("name", text()),
                        ("category", def("MaterialCategory")),
                        ("grade", nullable(text())),
                        ("used_by", integer()),
                    ],
                    &[],
                )),
            )],
            &[],
        ),
    };
    let mut get_fields = detail(&[
        ("id", id()),
        ("grade", nullable(text())),
        ("used_by", integer()),
    ]);
    get_fields.push(("edited", boolean()));
    let get = Signature {
        params: object(&[("material", id())], &[]),
        result: object(&get_fields, &[]),
    };
    [
        Descriptor::query(
            "materials.library",
            V1,
            "Lists the grades of the material library, with a search and a category filter.",
            library,
            library_grades,
        ),
        Descriptor::query(
            "materials.grade",
            V1,
            "Reads one library grade with its law and its sourced values.",
            grade,
            library_grade,
        ),
        Descriptor::query(
            "materials.list",
            V1,
            "Lists the materials of the project.",
            list,
            list_materials,
        ),
        Descriptor::query(
            "materials.get",
            V1,
            "Reads one project material with its law, its values and where they come from.",
            get,
            get_material,
        ),
    ]
}

/// The materials commands.
fn commands() -> [Descriptor<Session>; 4] {
    let add = Signature {
        params: object(&[("id", id()), ("key", text())], &[("name", name_schema())]),
        result: object(&[("id", id())], &[]),
    };
    let create = Signature {
        params: object(
            &[
                ("id", id()),
                ("name", name_schema()),
                ("category", def("MaterialCategory")),
            ],
            &[("properties", inputs_schema())],
        ),
        result: object(&[("id", id())], &[]),
    };
    let update = Signature {
        params: object(
            &[("material", id())],
            &[("name", name_schema()), ("properties", inputs_schema())],
        ),
        result: object(&[], &[]),
    };
    let delete = Signature {
        params: object(&[("material", id())], &[]),
        result: object(&[], &[]),
    };
    [
        Descriptor::command(
            "materials.add_from_library",
            V1,
            "Adds a project material with the values of a library grade.",
            add,
            add_from_library,
        ),
        Descriptor::command(
            "materials.create",
            V1,
            "Creates a project material from scratch; every value is entered by the user.",
            create,
            create_material,
        ),
        Descriptor::command(
            "materials.update",
            V1,
            "Renames a material and edits or resets its values.",
            update,
            update_material,
        ),
        Descriptor::command(
            "materials.delete",
            V1,
            "Deletes a material that nothing uses.",
            delete,
            delete_material,
        ),
    ]
}

// --- the library ---

fn library() -> Result<&'static Library, ApiError> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| Library::builtin().ok())
        .as_ref()
        .ok_or(ApiError::Internal)
}

/// A design code as the wire writes it: lowercase, without edition.
fn code_key(material: &Material) -> Option<String> {
    material.design_code().map(material::design_code_key)
}

fn matches(search: Option<&str>, texts: &[&str]) -> bool {
    let Some(search) = search.map(str::trim).filter(|search| !search.is_empty()) else {
        return true;
    };
    let search = search.to_lowercase();
    texts
        .iter()
        .any(|text| text.to_lowercase().contains(&search))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryParams {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    category: Option<Category>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct GradeSummary {
    key: String,
    name: String,
    category: Category,
    design_code: Option<String>,
}

#[derive(Serialize)]
struct GradePage {
    items: Vec<GradeSummary>,
    total: usize,
}

fn library_grades(
    _: &Session,
    LibraryParams {
        search,
        category,
        offset,
        limit,
    }: LibraryParams,
) -> Result<GradePage, ApiError> {
    let limit = page_size(limit)?;
    let offset = offset.unwrap_or(0);
    let found: Vec<GradeSummary> = library()?
        .iter()
        .filter(|(_, grade)| category.is_none_or(|category| grade.category() == category))
        .filter(|(key, grade)| matches(search.as_deref(), &[grade.name(), key]))
        .map(|(key, grade)| GradeSummary {
            key: key.to_owned(),
            name: grade.name().to_owned(),
            category: grade.category(),
            design_code: code_key(grade),
        })
        .collect();
    let total = found.len();
    Ok(GradePage {
        items: found.into_iter().skip(offset).take(limit).collect(),
        total,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GradeParams {
    key: String,
}

#[derive(Serialize)]
struct Grade {
    key: String,
    name: String,
    category: Category,
    design_code: Option<String>,
    law: LawView,
    properties: Vec<PropertyView>,
}

fn library_grade(_: &Session, GradeParams { key }: GradeParams) -> Result<Grade, ApiError> {
    let grade = library()?.get(&key).ok_or(MaterialsError::UnknownGrade)?;
    Ok(Grade {
        key,
        name: grade.name().to_owned(),
        category: grade.category(),
        design_code: code_key(grade),
        law: law_view(grade.category(), grade.law()),
        properties: property_views(grade, &BTreeMap::new(), true),
    })
}

// --- views ---

#[derive(Serialize)]
struct LawView {
    kind: &'static str,
    source: Option<Source>,
    branch: Option<&'static str>,
}

fn cited(document: &str, clause: &str) -> Option<Source> {
    Source::new(document, clause).ok()
}

/// The law as the wire names it, with the clause that defines it where one is certain.
fn law_view(category: Category, law: &BehaviourLaw) -> LawView {
    let (kind, source, branch) = match law {
        BehaviourLaw::LinearElastic(_) => ("linear_elastic", None, None),
        BehaviourLaw::ConcreteParabolaRectangle(_) => (
            "parabola_rectangle",
            cited("EN 1992-1-1:2004", "3.1.7(1)"),
            None,
        ),
        BehaviourLaw::ConcreteBilinear(_) => {
            ("bilinear", cited("EN 1992-1-1:2004", "3.1.7(2)"), None)
        }
        BehaviourLaw::SteelBilinear(steel) => {
            let branch = Some(if steel.hardening().is_some() {
                "inclined"
            } else {
                "horizontal"
            });
            if category == Category::Reinforcement {
                ("bilinear", cited("EN 1992-1-1:2004", "3.2.7(2)"), branch)
            } else {
                (
                    "elastic_plastic",
                    cited("EN 1993-1-1:2005", "5.4.3"),
                    branch,
                )
            }
        }
        BehaviourLaw::TimberElasticBrittle(_) => ("elastic_brittle", None, None),
        BehaviourLaw::None => ("none", None, None),
    };
    LawView {
        kind,
        source,
        branch,
    }
}

#[derive(Serialize)]
struct PropertyView {
    key: &'static str,
    quantity: &'static str,
    value: Option<f64>,
    source: Option<Source>,
    library_value: Option<f64>,
    range: Range,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    annex: Option<&'static str>,
}

/// Every property the material's category gives, those the material has and those it could
/// have. `kept` holds the library values of edited properties; `from_library` says that the
/// material is a library grade or copy, whose cited values are then the library's.
fn property_views(
    material: &Material,
    kept: &BTreeMap<Property, f64>,
    from_library: bool,
) -> Vec<PropertyView> {
    let category = material.category();
    Property::ALL
        .into_iter()
        .filter(|property| category.gives(*property))
        .map(|property| {
            let given = material.property(property);
            let library_value = kept.get(&property).copied().or_else(|| {
                given
                    .filter(|value| from_library && !value.source.is_user())
                    .map(|value| value.value)
            });
            PropertyView {
                key: property.key(),
                quantity: property.quantity().name(),
                value: given.map(|value| value.value),
                source: given.map(|value| value.source.clone()),
                library_value,
                range: property.range(),
                required: category.required().contains(&property),
                annex: property.annex().map(NationalAnnex::name),
            }
        })
        .collect()
}

// --- the project's materials ---

fn material_ids(model: &Model) -> impl Iterator<Item = EntityId> + '_ {
    model
        .store()
        .ids()
        .filter(|&id| model.entity_of(id) == Some(ENTITY))
}

fn read_material(model: &Model, id: EntityId) -> Result<Material, ApiError> {
    material::read(model, id).map_err(|error| match error {
        ReadError::NotAMaterial => MaterialsError::NotFound.into(),
        _ => ApiError::Internal,
    })
}

/// How many objects other than its own `MaterialOriginal`s refer to the material.
fn used_by(model: &Model, id: EntityId) -> Result<usize, ApiError> {
    let originals = material::original_ids(model, id).map_err(|_| ApiError::Internal)?;
    Ok(model.required_referrers(id).saturating_sub(originals.len()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    category: Option<Category>,
}

#[derive(Serialize)]
struct MaterialSummary {
    id: EntityId,
    name: String,
    category: Category,
    grade: Option<String>,
    used_by: usize,
}

#[derive(Serialize)]
struct MaterialList {
    items: Vec<MaterialSummary>,
}

fn list_materials(
    session: &Session,
    ListParams { search, category }: ListParams,
) -> Result<MaterialList, ApiError> {
    let model = session.document().model();
    let mut items = Vec::new();
    for id in material_ids(model) {
        // Only the three attributes the list shows, and the filters first.
        let name = text_attribute(model, id, "name")?.unwrap_or_default();
        let found = text_attribute(model, id, "category")?
            .and_then(|stored| Category::from_name(&stored))
            .ok_or(ApiError::Internal)?;
        if category.is_some_and(|category| found != category)
            || !matches(search.as_deref(), &[&name])
        {
            continue;
        }
        items.push(MaterialSummary {
            id,
            name,
            category: found,
            grade: text_attribute(model, id, "library_key")?,
            used_by: used_by(model, id)?,
        });
    }
    items.sort_by_cached_key(|item| item.name.to_lowercase());
    Ok(MaterialList { items })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialParams {
    material: EntityId,
}

#[derive(Serialize)]
struct MaterialDetail {
    id: EntityId,
    name: String,
    category: Category,
    design_code: Option<String>,
    grade: Option<String>,
    law: LawView,
    properties: Vec<PropertyView>,
    used_by: usize,
    /// Whether the user changed a value of a library grade; a reset has something to restore.
    edited: bool,
}

fn get_material(
    session: &Session,
    MaterialParams { material: id }: MaterialParams,
) -> Result<MaterialDetail, ApiError> {
    let model = session.document().model();
    let found = read_material(model, id)?;
    let kept: BTreeMap<Property, f64> = material::originals(model, id)
        .map_err(|_| ApiError::Internal)?
        .into_iter()
        .map(|original| (original.property, original.value.value))
        .collect();
    Ok(MaterialDetail {
        id,
        name: found.name().to_owned(),
        category: found.category(),
        design_code: code_key(&found),
        grade: found.library_key().map(str::to_owned),
        law: law_view(found.category(), found.law()),
        properties: property_views(&found, &kept, found.library_key().is_some()),
        used_by: used_by(model, id)?,
        edited: !kept.is_empty(),
    })
}

// --- commands ---

fn check_name(model: &Model, name: &str, except: Option<EntityId>) -> Result<(), ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_LEN {
        return Err(MaterialsError::InvalidName.into());
    }
    let lowered = trimmed.to_lowercase();
    for id in material_ids(model).filter(|&id| Some(id) != except) {
        if text_attribute(model, id, "name")?.is_some_and(|name| name.to_lowercase() == lowered) {
            return Err(MaterialsError::DuplicateName.into());
        }
    }
    Ok(())
}

/// A text attribute of a stored object, read without decoding the whole material.
fn text_attribute(
    model: &Model,
    id: EntityId,
    attribute: &str,
) -> Result<Option<String>, ApiError> {
    match model.get(id, attribute) {
        Ok(Some(core_model::AttributeValue::Text(text))) => Ok(Some(text)),
        Ok(_) => Ok(None),
        Err(_) => Err(ApiError::Internal),
    }
}

/// The error a failed material check becomes.
fn material_error(error: &MaterialError) -> MaterialsError {
    let key = |property: &Property| Some(property.key().to_owned());
    match error {
        MaterialError::EmptyName
        | MaterialError::TooLong(_)
        | MaterialError::ControlCharacter(_) => MaterialsError::InvalidName,
        MaterialError::OutOfRange(property) | MaterialError::Inconsistent { property, .. } => {
            MaterialsError::InvalidValue { key: key(property) }
        }
        MaterialError::MissingProperty(property) => MaterialsError::RequiredProperty {
            key: property.key().to_owned(),
        },
        MaterialError::PropertyNotForCategory { property, .. } => MaterialsError::UnknownProperty {
            key: property.key().to_owned(),
        },
        MaterialError::Law(_)
        | MaterialError::LawNotForCategory { .. }
        | MaterialError::MissingAnnexValue { .. } => MaterialsError::InvalidValue { key: None },
    }
}

fn edit_error(error: &EditError) -> ApiError {
    match error {
        EditError::Read(ReadError::NotAMaterial) => MaterialsError::NotFound.into(),
        EditError::Invalid(error) => material_error(error).into(),
        EditError::Repeated(property) => MaterialsError::InvalidValue {
            key: Some(property.key().to_owned()),
        }
        .into(),
        EditError::NothingToReset(_) | EditError::Read(_) | EditError::Model(_) => {
            ApiError::Internal
        }
    }
}

/// A property as a caller names it: a key, and the annex of a partial factor.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertyInput {
    key: String,
    #[serde(default)]
    annex: Option<String>,
    value: Option<f64>,
}

impl PropertyInput {
    /// The property named. A partial factor needs its annex and no other property takes one.
    fn property(&self) -> Result<Property, MaterialsError> {
        let invalid = || MaterialsError::InvalidValue {
            key: Some(self.key.clone()),
        };
        let annex = self
            .annex
            .as_deref()
            .map(|wire| NationalAnnex::from_name(wire).ok_or_else(invalid))
            .transpose()?;
        if let Some(property) = Property::from_key(&self.key, annex) {
            return Ok(property);
        }
        let known =
            Factor::from_key(&self.key).is_some() || Property::from_key(&self.key, None).is_some();
        Err(if known {
            invalid()
        } else {
            MaterialsError::UnknownProperty {
                key: self.key.clone(),
            }
        })
    }
}

/// The property of each input, refusing a missing value where one is needed.
fn properties_of(inputs: &[PropertyInput]) -> Result<Vec<(Property, Option<f64>)>, MaterialsError> {
    // The schema promises this bound, so the server holds the caller to it.
    if inputs.len() > MAX_PROPERTIES {
        return Err(MaterialsError::InvalidValue { key: None });
    }
    inputs
        .iter()
        .map(|input| Ok((input.property()?, input.value)))
        .collect()
}

#[derive(Serialize)]
struct Created {
    id: EntityId,
}

fn run(
    session: &mut Session,
    command: ModelCommand,
    origin: &Origin,
) -> Result<Option<core_txn::ChangeEvent>, ApiError> {
    Ok(Some(
        session.document_mut().execute(command, origin.clone())?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddParams {
    id: EntityId,
    key: String,
    #[serde(default)]
    name: Option<String>,
}

fn add_from_library(
    session: &mut Session,
    AddParams { id, key, name }: AddParams,
    origin: &Origin,
) -> Result<Outcome<Created>, ApiError> {
    let grade = library()?.get(&key).ok_or(MaterialsError::UnknownGrade)?;
    let name = name.as_deref().unwrap_or_else(|| grade.name());
    check_name(session.document().model(), name, None)?;
    let copy = grade
        .with_name(name)
        .map_err(|error| ApiError::from(material_error(&error)))?;
    let event = run(session, material::commands(id, &copy), origin)?;
    Ok(Outcome {
        result: Created { id },
        event,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateParams {
    id: EntityId,
    name: String,
    category: Category,
    #[serde(default)]
    properties: Vec<PropertyInput>,
}

/// The laws a custom material of `category` may follow, the most specific first.
fn laws_for(category: Category) -> &'static [LawKind] {
    match category {
        Category::Concrete => &[
            LawKind::ConcreteParabolaRectangle,
            LawKind::ConcreteBilinear,
            LawKind::LinearElastic,
        ],
        Category::Reinforcement | Category::Steel => {
            &[LawKind::SteelBilinear, LawKind::LinearElastic]
        }
        Category::Timber => &[LawKind::TimberElasticBrittle, LawKind::LinearElastic],
        Category::Masonry | Category::Insulation | Category::Finish | Category::Generic => {
            &[LawKind::LinearElastic, LawKind::None]
        }
    }
}

fn create_material(
    session: &mut Session,
    CreateParams {
        id,
        name,
        category,
        properties: inputs,
    }: CreateParams,
    origin: &Origin,
) -> Result<Outcome<Created>, ApiError> {
    check_name(session.document().model(), &name, None)?;
    let mut properties = BTreeMap::new();
    for (property, value) in properties_of(&inputs)? {
        let value = value.ok_or_else(|| MaterialsError::InvalidValue {
            key: Some(property.key().to_owned()),
        })?;
        if properties
            .insert(property, Sourced::new(value, Source::user()))
            .is_some()
        {
            return Err(MaterialsError::InvalidValue {
                key: Some(property.key().to_owned()),
            }
            .into());
        }
    }
    // A law the values do not complete gives way to the next one; a law whose values are wrong
    // does not, or the material would silently follow another law.
    let mut first_missing = None;
    for &law in laws_for(category) {
        match Material::new(&name, category, law, None, properties.clone()) {
            Ok(created) => {
                let event = run(session, material::commands(id, &created), origin)?;
                return Ok(Outcome {
                    result: Created { id },
                    event,
                });
            }
            Err(error @ MaterialError::MissingProperty(_)) => {
                first_missing = first_missing.or(Some(error));
            }
            Err(error) => return Err(material_error(&error).into()),
        }
    }
    Err(first_missing.map_or(ApiError::Internal, |error| material_error(&error).into()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateParams {
    material: EntityId,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    properties: Vec<PropertyInput>,
}

/// The `MaterialOriginal` that keeps `property` of `material`: its id derives from both, so a
/// replayed command makes the same object (CORE-11). FNV-1a over 128 bits, no randomness.
fn original_id(material: EntityId, property: Property) -> EntityId {
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;
    let mut state = material.get() ^ 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D;
    for byte in b"original:".iter().chain(property.name().as_bytes()) {
        state = (state ^ u128::from(*byte)).wrapping_mul(PRIME);
    }
    EntityId::from_random_bits(state)
}

fn update_material(
    session: &mut Session,
    UpdateParams {
        material: id,
        name,
        properties: inputs,
    }: UpdateParams,
    origin: &Origin,
) -> Result<Outcome<serde_json::Map<String, serde_json::Value>>, ApiError> {
    let model = session.document().model();
    let current = read_material(model, id)?;
    let mut commands = Vec::new();
    if let Some(name) = name.as_deref().filter(|name| name.trim() != current.name()) {
        check_name(model, name, Some(id))?;
        let renamed = current
            .with_name(name)
            .map_err(|error| ApiError::from(material_error(&error)))?;
        commands.push(ModelCommand::Set {
            id,
            attribute: "name".to_owned(),
            value: Some(core_model::AttributeValue::Text(renamed.name().to_owned())),
        });
    }
    let edits: Vec<(Property, Edit)> = properties_of(&inputs)?
        .into_iter()
        .map(|(property, value)| (property, value.map_or(Edit::Reset, Edit::Set)))
        .collect();
    let edit = material::edit(model, id, &edits, |property| original_id(id, property))
        .map_err(|error| edit_error(&error))?;
    // A change that changes nothing leaves no history entry.
    if !matches!(&edit, ModelCommand::Batch { commands } if commands.is_empty()) {
        commands.push(edit);
    }
    if commands.is_empty() {
        return Ok(Outcome {
            result: serde_json::Map::new(),
            event: None,
        });
    }
    let event = run(session, ModelCommand::Batch { commands }, origin)?;
    Ok(Outcome {
        result: serde_json::Map::new(),
        event,
    })
}

fn delete_material(
    session: &mut Session,
    MaterialParams { material: id }: MaterialParams,
    origin: &Origin,
) -> Result<Outcome<serde_json::Map<String, serde_json::Value>>, ApiError> {
    let model = session.document().model();
    read_material(model, id)?;
    let used_by = used_by(model, id)?;
    if used_by > 0 {
        return Err(MaterialsError::InUse { used_by }.into());
    }
    let command = material::delete(model, id).map_err(|error| edit_error(&error))?;
    let event = run(session, command, origin)?;
    Ok(Outcome {
        result: serde_json::Map::new(),
        event,
    })
}

#[cfg(test)]
mod tests;
