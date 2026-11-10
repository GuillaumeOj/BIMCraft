//! The `buildups.*` queries and commands: the layered build-ups of walls and slabs (MAT-03).
//!
//! A build-up and its layers are objects of the `MaterialLayerSet` and `MaterialLayer` entities.
//! The rules, the thermal resistance and the storing live in the `material` crate; nothing here
//! recomputes them (ENG-04). Every command runs as one batch, so it is one undo step, is recorded
//! with the caller's origin and reports a change event.
//!
//! A command takes the id of the build-up it creates, as `model.create` does. The ids of new
//! layers derive from the build-up's id (CORE-11), so the caller draws none; a layer that
//! already exists keeps its id (ADR 0010) when `buildups.update` names it.

use core_model::units::Length;
use core_model::{EntityId, Model};
use core_txn::Origin;
use material::{
    Layer, LayerFunction, LayerSet, LayerSetError, LayerSetReadError, MAX_LAYERS, MAX_NAME_LEN,
    Usage,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

use crate::georeference::one_of_strings;
use crate::materials::{run, text_attribute};
use crate::schema::{array_of, def, id, integer, nullable, number, object, text};
use crate::{ApiError, Descriptor, Outcome, Session, Signature, Version};

const V1: Version = Version::new(1, 0, 0);

/// Why a `buildups.*` call failed. Each has a stable `reason` the UI maps to a message, with the
/// position (from 0) of the layer at fault where one is, and none repeats a value (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the build-up call was refused")]
pub struct BuildUpsError {
    /// The stable `snake_case` name of the error.
    pub reason: &'static str,
    /// The layer at fault, counted from 0.
    pub index: Option<usize>,
}

impl BuildUpsError {
    const fn new(reason: &'static str) -> Self {
        Self {
            reason,
            index: None,
        }
    }
}

impl From<LayerSetError> for BuildUpsError {
    fn from(error: LayerSetError) -> Self {
        // Layer positions come counted from 1 in the messages and from 0 on the wire.
        let at = |reason, position: usize| Self {
            reason,
            index: Some(position.saturating_sub(1)),
        };
        match error {
            LayerSetError::EmptyName | LayerSetError::TooLong | LayerSetError::ControlCharacter => {
                Self::new("invalid_name")
            }
            LayerSetError::DuplicateName => Self::new("duplicate_name"),
            LayerSetError::NoLayers => Self::new("no_layers"),
            LayerSetError::TooManyLayers => Self::new("too_many_layers"),
            LayerSetError::InvalidThickness(index) => at("invalid_thickness", index),
            LayerSetError::InvalidPriority(index) => at("invalid_priority", index),
            LayerSetError::DuplicateLayer(index) => at("duplicate_layer", index),
            LayerSetError::CoreNotContiguous => Self::new("core_not_contiguous"),
            LayerSetError::TooThick => Self::new("too_thick"),
            LayerSetError::UnknownMaterial(index) => at("unknown_material", index),
            LayerSetError::InvalidMaterial(index) => at("invalid_material", index),
        }
    }
}

impl From<BuildUpsError> for ApiError {
    fn from(error: BuildUpsError) -> Self {
        Self::BuildUps(error)
    }
}

fn not_found() -> ApiError {
    BuildUpsError::new("not_found").into()
}

// --- shared shapes ---

fn name_schema() -> serde_json::Value {
    json!({ "type": "string", "maxLength": MAX_NAME_LEN })
}

fn layers_schema() -> serde_json::Value {
    let mut schema = array_of(def("BuildUpLayerInput"));
    schema["maxItems"] = json!(MAX_LAYERS + 1);
    schema
}

/// The definitions the build-up signatures share, for the protocol schema.
pub(crate) fn definitions() -> [(String, serde_json::Value); 4] {
    let usage = one_of_strings(&Usage::ALL.map(Usage::name));
    let function = one_of_strings(&LayerFunction::ALL.map(LayerFunction::name));
    let input = object(
        &[
            ("material", id()),
            ("thickness", number()),
            ("function", def("BuildUpFunction")),
        ],
        &[
            ("id", id()),
            ("priority", json!({ "type": "integer" })),
            ("core", json!({ "type": "boolean" })),
            ("name", text()),
        ],
    );
    let layer = object(
        &[
            ("id", id()),
            ("material", id()),
            ("material_name", text()),
            ("thickness", number()),
            ("function", def("BuildUpFunction")),
            ("priority", json!({ "type": "integer" })),
            ("core", json!({ "type": "boolean" })),
            ("name", nullable(text())),
            ("thermal_resistance", nullable(number())),
        ],
        &[],
    );
    [
        ("BuildUpUsage".to_owned(), usage),
        ("BuildUpFunction".to_owned(), function),
        ("BuildUpLayerInput".to_owned(), input),
        ("BuildUpLayer".to_owned(), layer),
    ]
}

/// The build-up queries, then their commands.
pub(crate) fn descriptors() -> impl Iterator<Item = Descriptor<Session>> {
    queries().into_iter().chain(commands())
}

fn queries() -> [Descriptor<Session>; 3] {
    let list = Signature {
        params: object(&[], &[("usage", def("BuildUpUsage"))]),
        result: object(
            &[(
                "items",
                array_of(object(
                    &[
                        ("id", id()),
                        ("name", text()),
                        ("usage", def("BuildUpUsage")),
                        ("layer_count", integer()),
                        ("total_thickness", number()),
                    ],
                    &[],
                )),
            )],
            &[],
        ),
    };
    let get = Signature {
        params: object(&[("buildup", id())], &[]),
        result: object(
            &[
                ("id", id()),
                ("name", text()),
                ("usage", def("BuildUpUsage")),
                ("description", nullable(text())),
                ("layers", array_of(def("BuildUpLayer"))),
                ("total_thickness", number()),
                ("thermal_resistance", nullable(number())),
                ("layers_without_thermal_data", integer()),
            ],
            &[],
        ),
    };
    let preview = Signature {
        params: object(
            &[("usage", def("BuildUpUsage")), ("layers", layers_schema())],
            &[],
        ),
        result: object(
            &[
                (
                    "layers",
                    array_of(object(
                        &[
                            ("priority", json!({ "type": "integer" })),
                            ("core", json!({ "type": "boolean" })),
                            ("thermal_resistance", nullable(number())),
                        ],
                        &[],
                    )),
                ),
                ("total_thickness", number()),
                ("thermal_resistance", nullable(number())),
                ("layers_without_thermal_data", integer()),
                (
                    "errors",
                    array_of(object(&[("code", text())], &[("index", integer())])),
                ),
            ],
            &[],
        ),
    };
    [
        Descriptor::query(
            "buildups.list",
            V1,
            "Lists the wall and slab build-ups of the project.",
            list,
            list_build_ups,
        ),
        Descriptor::query(
            "buildups.get",
            V1,
            "Reads one build-up with its layers and its thermal resistance.",
            get,
            get_build_up,
        ),
        Descriptor::query(
            "buildups.preview",
            V1,
            "Checks a draft build-up without saving it and returns its totals and every error.",
            preview,
            preview_build_up,
        ),
    ]
}

fn commands() -> [Descriptor<Session>; 4] {
    let ids = || array_of(id());
    let create = Signature {
        params: object(
            &[
                ("id", id()),
                ("name", name_schema()),
                ("usage", def("BuildUpUsage")),
                ("layers", layers_schema()),
            ],
            &[("description", text())],
        ),
        result: object(&[("id", id()), ("layers", ids())], &[]),
    };
    let update = Signature {
        params: object(
            &[("buildup", id())],
            &[
                ("name", name_schema()),
                ("usage", def("BuildUpUsage")),
                ("description", nullable(text())),
                ("layers", layers_schema()),
            ],
        ),
        result: object(&[("layers", ids())], &[]),
    };
    let duplicate = Signature {
        params: object(
            &[("buildup", id()), ("id", id()), ("name", name_schema())],
            &[],
        ),
        result: object(&[("id", id()), ("layers", ids())], &[]),
    };
    let delete = Signature {
        params: object(&[("buildup", id())], &[]),
        result: object(&[], &[]),
    };
    [
        Descriptor::command(
            "buildups.create",
            V1,
            "Creates a build-up with its layers.",
            create,
            create_build_up,
        ),
        Descriptor::command(
            "buildups.update",
            V1,
            "Renames a build-up and replaces its layers; layers it names keep their identity.",
            update,
            update_build_up,
        ),
        Descriptor::command(
            "buildups.duplicate",
            V1,
            "Copies a build-up under a new name, with new layers.",
            duplicate,
            duplicate_build_up,
        ),
        Descriptor::command(
            "buildups.delete",
            V1,
            "Deletes a build-up with its layers.",
            delete,
            delete_build_up,
        ),
    ]
}

// --- reading ---

fn read_error(error: &LayerSetReadError) -> ApiError {
    match error {
        LayerSetReadError::NotALayerSet => not_found(),
        _ => ApiError::Internal,
    }
}

fn read_build_up(model: &Model, id: EntityId) -> Result<LayerSet, ApiError> {
    material::read_layer_set(model, id).map_err(|error| read_error(&error))
}

fn build_up_ids(model: &Model) -> impl Iterator<Item = EntityId> + '_ {
    model
        .store()
        .ids()
        .filter(|&id| model.entity_of(id) == Some(material::LAYER_SET_ENTITY))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Serialize)]
struct Summary {
    id: EntityId,
    name: String,
    usage: Usage,
    layer_count: usize,
    total_thickness: f64,
}

#[derive(Serialize)]
struct SummaryList {
    items: Vec<Summary>,
}

fn list_build_ups(
    session: &Session,
    ListParams { usage }: ListParams,
) -> Result<SummaryList, ApiError> {
    let model = session.document().model();
    // A build-up this version cannot read, such as one saved by a newer version, is left out
    // of the list instead of failing it.
    let mut items = Vec::new();
    for id in build_up_ids(model) {
        match material::read_layer_set(model, id) {
            Ok(set) if usage.is_none_or(|usage| set.usage() == usage) => items.push(Summary {
                id,
                name: set.name().to_owned(),
                usage: set.usage(),
                layer_count: set.layers().len(),
                total_thickness: set.total_thickness().si(),
            }),
            Err(LayerSetReadError::Model(_)) => return Err(ApiError::Internal),
            _ => {}
        }
    }
    items.sort_by_cached_key(|item| item.name.to_lowercase());
    Ok(SummaryList { items })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildUpParams {
    buildup: EntityId,
}

#[derive(Serialize)]
struct LayerView {
    id: EntityId,
    material: EntityId,
    material_name: String,
    thickness: f64,
    function: LayerFunction,
    priority: u8,
    core: bool,
    name: Option<String>,
    thermal_resistance: Option<f64>,
}

#[derive(Serialize)]
struct Detail {
    id: EntityId,
    name: String,
    usage: Usage,
    description: Option<String>,
    layers: Vec<LayerView>,
    total_thickness: f64,
    thermal_resistance: Option<f64>,
    layers_without_thermal_data: usize,
}

fn get_build_up(
    session: &Session,
    BuildUpParams { buildup: id }: BuildUpParams,
) -> Result<Detail, ApiError> {
    let model = session.document().model();
    let set = read_build_up(model, id)?;
    let thermal = set.thermal(|layer| material::conductivity(model, layer.material));
    let layers = set
        .layers()
        .iter()
        .zip(&thermal.layers)
        .map(|(layer, resistance)| {
            Ok(LayerView {
                id: layer.id,
                material: layer.material,
                material_name: text_attribute(model, layer.material, "name")?.unwrap_or_default(),
                thickness: layer.thickness.si(),
                function: layer.function,
                priority: layer.priority,
                core: layer.core,
                name: layer.name.clone(),
                thermal_resistance: *resistance,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Detail {
        id,
        name: set.name().to_owned(),
        usage: set.usage(),
        description: set.description().map(str::to_owned),
        layers,
        total_thickness: set.total_thickness().si(),
        thermal_resistance: thermal.total,
        layers_without_thermal_data: thermal.layers_without_data,
    })
}

// --- drafts ---

/// A layer as a caller writes it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerInput {
    #[serde(default)]
    id: Option<EntityId>,
    material: EntityId,
    thickness: f64,
    function: LayerFunction,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    core: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

impl LayerInput {
    /// The layer with its defaults filled in: the priority and the core of its function.
    fn layer(&self, id: EntityId) -> Layer {
        Layer {
            id,
            material: self.material,
            thickness: Length::from_si(self.thickness),
            function: self.function,
            // A priority outside 1 to 5, even one that does not fit a byte, is refused later.
            priority: self
                .priority
                .map_or(self.function.default_priority(), |priority| {
                    u8::try_from(priority).unwrap_or(0)
                }),
            core: self.core.unwrap_or(self.function.default_core()),
            name: self.name.clone(),
        }
    }
}

fn too_many(inputs: &[LayerInput]) -> Result<(), ApiError> {
    if inputs.len() > MAX_LAYERS {
        return Err(BuildUpsError::from(LayerSetError::TooManyLayers).into());
    }
    Ok(())
}

/// Reads the layers of a draft as new layers of `build_up`, or as layers it already has: an
/// input that names an id keeps that layer, which must be one of `existing`, and the others get
/// fresh ids.
fn layers_of(
    model: &Model,
    build_up: EntityId,
    existing: &[EntityId],
    inputs: &[LayerInput],
) -> Result<Vec<Layer>, ApiError> {
    too_many(inputs)?;
    if let Some(index) = inputs
        .iter()
        .position(|input| input.id.is_some_and(|id| !existing.contains(&id)))
    {
        return Err(BuildUpsError {
            reason: "unknown_layer",
            index: Some(index),
        }
        .into());
    }
    let mut fresh = material::fresh_layer_ids(
        model,
        build_up,
        inputs.iter().filter(|input| input.id.is_none()).count(),
    )
    .into_iter();
    inputs
        .iter()
        .map(|input| {
            let id = match input.id {
                Some(id) => id,
                None => fresh.next().ok_or(ApiError::Internal)?,
            };
            Ok(input.layer(id))
        })
        .collect()
}

/// The build-up of `layers`, or the first problem found in them.
fn checked(
    model: &Model,
    name: &str,
    usage: Usage,
    description: Option<&str>,
    layers: Vec<Layer>,
    except: Option<EntityId>,
) -> Result<LayerSet, ApiError> {
    if let Some(problem) = material::check_layers(model, name, description, &layers, except)
        .into_iter()
        .next()
    {
        return Err(BuildUpsError::from(problem).into());
    }
    LayerSet::new(name, usage, description, layers)
        .map_err(|error| BuildUpsError::from(error).into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewParams {
    /// The usage of the draft; no rule depends on it yet, but the call names it so that the
    /// contract need not change when one does.
    #[serde(rename = "usage")]
    _usage: Usage,
    layers: Vec<LayerInput>,
}

#[derive(Serialize)]
struct PreviewLayer {
    priority: u8,
    core: bool,
    thermal_resistance: Option<f64>,
}

#[derive(Serialize)]
struct PreviewError {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
}

#[derive(Serialize)]
struct Preview {
    layers: Vec<PreviewLayer>,
    total_thickness: f64,
    thermal_resistance: Option<f64>,
    layers_without_thermal_data: usize,
    errors: Vec<PreviewError>,
}

/// Validates a draft without saving it and returns its totals and every error at once, so the
/// editor never recomputes material maths (ENG-04).
fn preview_build_up(
    session: &Session,
    PreviewParams { layers, .. }: PreviewParams,
) -> Result<Preview, ApiError> {
    // The schema bounds a draft to one layer over the limit, which is enough to report it.
    if layers.len() > MAX_LAYERS + 1 {
        return Err(ApiError::InvalidParams {
            detail: format!("at most {} layers", MAX_LAYERS + 1),
        });
    }
    let model = session.document().model();
    let mut errors = Vec::new();
    if layers.len() > MAX_LAYERS {
        errors.push(LayerSetError::TooManyLayers);
    }
    // A draft layer without an id gets one of its own, so only a repeated given id is a clash.
    let drafts: Vec<Layer> = layers
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let synthetic = EntityId::new(u128::MAX - u128::try_from(index).unwrap_or(0));
            input.layer(input.id.unwrap_or(synthetic))
        })
        .collect();
    errors.extend(
        material::check_layers(model, "Draft", None, &drafts, None)
            .into_iter()
            .filter(|problem| {
                !matches!(
                    problem,
                    LayerSetError::DuplicateName | LayerSetError::TooManyLayers
                )
            }),
    );
    let thermal = material::thermal(
        drafts
            .iter()
            .map(|layer| (layer, material::conductivity(model, layer.material))),
    );
    // A sum that overflows is not a number a client can read; the thickness error says why.
    let total: f64 = drafts
        .iter()
        .map(|layer| layer.thickness.si())
        .filter(|thickness| thickness.is_finite())
        .sum();
    let total = if total.is_finite() { total } else { 0.0 };
    Ok(Preview {
        layers: drafts
            .iter()
            .zip(&thermal.layers)
            .map(|(layer, resistance)| PreviewLayer {
                priority: layer.priority,
                core: layer.core,
                thermal_resistance: *resistance,
            })
            .collect(),
        total_thickness: total,
        thermal_resistance: thermal.total,
        layers_without_thermal_data: thermal.layers_without_data,
        errors: errors
            .into_iter()
            .fold(Vec::new(), |mut unique, error| {
                if !unique.contains(&error) {
                    unique.push(error);
                }
                unique
            })
            .into_iter()
            .map(|error| {
                let error = BuildUpsError::from(error);
                PreviewError {
                    code: error.reason,
                    index: error.index,
                }
            })
            .collect(),
    })
}

// --- commands ---

#[derive(Serialize)]
struct Created {
    id: EntityId,
    layers: Vec<EntityId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateParams {
    id: EntityId,
    name: String,
    usage: Usage,
    #[serde(default)]
    description: Option<String>,
    layers: Vec<LayerInput>,
}

fn layer_ids_of(set: &LayerSet) -> Vec<EntityId> {
    set.layers().iter().map(|layer| layer.id).collect()
}

fn create_build_up(
    session: &mut Session,
    CreateParams {
        id,
        name,
        usage,
        description,
        layers,
    }: CreateParams,
    origin: &Origin,
) -> Result<Outcome<Created>, ApiError> {
    let model = session.document().model();
    let layers = layers_of(model, id, &[], &layers)?;
    let set = checked(model, &name, usage, description.as_deref(), layers, None)?;
    let event = run(session, material::layer_set_commands(id, &set), origin)?;
    Ok(Outcome {
        result: Created {
            id,
            layers: layer_ids_of(&set),
        },
        event,
    })
}

/// A field that may be absent (keep it), `null` (clear it) or a value: `Some(None)` is `null`.
#[expect(
    clippy::option_option,
    reason = "absent, null and a value are three different requests"
)]
fn nullable_text<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateParams {
    buildup: EntityId,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default, deserialize_with = "nullable_text")]
    #[expect(clippy::option_option, reason = "see nullable_text")]
    description: Option<Option<String>>,
    #[serde(default)]
    layers: Option<Vec<LayerInput>>,
}

#[derive(Serialize)]
struct Updated {
    layers: Vec<EntityId>,
}

fn update_build_up(
    session: &mut Session,
    UpdateParams {
        buildup: id,
        name,
        usage,
        description,
        layers,
    }: UpdateParams,
    origin: &Origin,
) -> Result<Outcome<Updated>, ApiError> {
    let model = session.document().model();
    let current = read_build_up(model, id)?;
    let existing = material::layer_ids(model, id).map_err(|error| read_error(&error))?;
    let layers = match layers {
        Some(inputs) => layers_of(model, id, &existing, &inputs)?,
        None => current.layers().to_vec(),
    };
    let description = match &description {
        Some(new) => new.as_deref(),
        None => current.description(),
    };
    let set = checked(
        model,
        name.as_deref().unwrap_or_else(|| current.name()),
        usage.unwrap_or_else(|| current.usage()),
        description,
        layers,
        Some(id),
    )?;
    let command = material::replace_layer_set(model, id, &set).map_err(|_| ApiError::Internal)?;
    let event = run(session, command, origin)?;
    Ok(Outcome {
        result: Updated {
            layers: layer_ids_of(&set),
        },
        event,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateParams {
    buildup: EntityId,
    id: EntityId,
    name: String,
}

fn duplicate_build_up(
    session: &mut Session,
    DuplicateParams { buildup, id, name }: DuplicateParams,
    origin: &Origin,
) -> Result<Outcome<Created>, ApiError> {
    let model = session.document().model();
    let source = read_build_up(model, buildup)?;
    let fresh = material::fresh_layer_ids(model, id, source.layers().len());
    let layers = source
        .layers()
        .iter()
        .zip(fresh)
        .map(|(layer, new_id)| Layer {
            id: new_id,
            ..layer.clone()
        })
        .collect();
    let set = checked(
        model,
        &name,
        source.usage(),
        source.description(),
        layers,
        None,
    )?;
    let event = run(session, material::layer_set_commands(id, &set), origin)?;
    Ok(Outcome {
        result: Created {
            id,
            layers: layer_ids_of(&set),
        },
        event,
    })
}

fn delete_build_up(
    session: &mut Session,
    BuildUpParams { buildup: id }: BuildUpParams,
    origin: &Origin,
) -> Result<Outcome<serde_json::Map<String, serde_json::Value>>, ApiError> {
    let command =
        material::delete_layer_set(session.document().model(), id).map_err(
            |error| match error {
                LayerSetReadError::NotALayerSet => not_found(),
                _ => ApiError::Internal,
            },
        )?;
    let event = run(session, command, origin)?;
    Ok(Outcome {
        result: serde_json::Map::new(),
        event,
    })
}

#[cfg(test)]
mod tests;
