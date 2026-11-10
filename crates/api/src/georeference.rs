//! The `georef.*` queries and commands: the project's georeference, the CRS list and the
//! conversion between local, projected and geographic coordinates (GEOREF-04).
//!
//! The georeference lives on the project's `Site` (ADR 0017). `georef.set` and `georef.clear`
//! run as a `model.batch`, so each is one undo step, is recorded with the caller's origin and
//! reports a change event like any other command. The CRS definitions and the convergence come
//! from the `georef` crate's registry; nothing here recomputes them.

use core_model::{AttributeValue, EntityId, Model};
use core_txn::{ModelCommand, Origin};
use georef::registry::{BasePointError, ProjectedCrs, RegistryError};
use georef::site::{self, SiteError};
use georef::{
    EpsgCode, GeodesyError, GeographicPoint, Georeference, GeoreferenceError, LocalPoint,
    MapConversion, ProjectedPoint, Registry as Crss,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::descriptor::NoParams;
use crate::schema::{array_of, def, nullable, object};
use crate::{
    ApiError, Descriptor, GeorefError, MAX_PAGE_SIZE, Outcome, Session, Signature, Version,
};

const V1: Version = Version::new(1, 0, 0);

/// A point `[x, y, z]`, or `[latitude, longitude, height]` for a geographic one.
type Point = [f64; 3];

fn point_schema() -> Value {
    let number = json!({ "type": "number" });
    json!({
        "type": "array",
        "prefixItems": [number, number, number],
        "items": false,
        "minItems": 3,
    })
}

/// One of these strings, written with `const` as the generated TypeScript types need.
pub(crate) fn one_of_strings(values: &[&str]) -> Value {
    let variants: Vec<Value> = values
        .iter()
        .map(|value| json!({ "const": value }))
        .collect();
    json!({ "oneOf": variants })
}

fn epsg_schema() -> Value {
    json!({ "type": "integer", "minimum": 1 })
}

/// The fields of a georeference, which `georef.set` takes with an optional `site`.
fn georeference_fields() -> [(&'static str, Value); 5] {
    [
        ("crs_epsg", epsg_schema()),
        ("base_point", point_schema()),
        ("grid_rotation", json!({ "type": "number" })),
        ("scale", json!({ "type": "number" })),
        ("survey_point", nullable(point_schema())),
    ]
}

/// The definitions the georeference signatures share, for the protocol schema.
pub(crate) fn definitions() -> [(String, Value); 2] {
    let epsg = epsg_schema;
    let text = || json!({ "type": "string" });
    let georeference = object(&georeference_fields(), &[]);
    let summary = object(
        &[
            ("epsg", epsg()),
            ("name", text()),
            ("kind", one_of_strings(&["projected", "geographic"])),
            ("geographic_name", text()),
            ("area", text()),
        ],
        &[],
    );
    [
        ("Georeference".to_owned(), georeference),
        ("CrsSummary".to_owned(), summary),
    ]
}

/// The georeference queries, then its commands.
pub(crate) fn descriptors() -> [Descriptor<Session>; 5] {
    let number = || json!({ "type": "number" });
    let list_crs = Signature {
        params: object(&[], &[("include_geographic", json!({ "type": "boolean" }))]),
        result: object(&[("crs", array_of(def("CrsSummary")))], &[]),
    };
    let derived = object(
        &[
            ("true_north", number()),
            ("convergence", number()),
            ("point_scale_factor", number()),
            ("base_point_geographic", point_schema()),
            ("survey_point_local", nullable(point_schema())),
        ],
        &[],
    );
    let described = object(
        &[
            ("site", def("EntityId")),
            ("value", def("Georeference")),
            ("crs", def("CrsSummary")),
            ("derived", derived),
        ],
        &[],
    );
    let get = Signature {
        params: object(&[], &[]),
        result: object(&[("georeference", nullable(described))], &[]),
    };
    let points = json!({ "type": "array", "items": point_schema(), "maxItems": MAX_PAGE_SIZE });
    let everywhere = object(
        &[
            ("local", point_schema()),
            ("projected", point_schema()),
            ("geographic", point_schema()),
        ],
        &[],
    );
    let convert = Signature {
        params: object(
            &[
                (
                    "from",
                    one_of_strings(&["local", "projected", "geographic"]),
                ),
                ("points", points),
            ],
            &[],
        ),
        result: object(&[("points", array_of(everywhere))], &[]),
    };
    let set = Signature {
        params: object(&georeference_fields(), &[("site", def("EntityId"))]),
        result: object(&[("site", def("EntityId"))], &[]),
    };
    let clear = Signature {
        params: object(&[], &[]),
        result: object(&[], &[]),
    };
    [
        Descriptor::query(
            "georef.convert",
            V1,
            "Converts points between local, projected and geographic coordinates.",
            convert,
            convert_points,
        ),
        Descriptor::query(
            "georef.get",
            V1,
            "Reads the project's georeference and the values derived from it.",
            get,
            get_georeference,
        ),
        Descriptor::query(
            "georef.list_crs",
            V1,
            "Lists the coordinate reference systems a project can be georeferenced on.",
            list_crs,
            list_crs_summaries,
        ),
        Descriptor::command(
            "georef.set",
            V1,
            "Sets the project's georeference.",
            set,
            set_georeference,
        ),
        Descriptor::command(
            "georef.clear",
            V1,
            "Removes the project's georeference.",
            clear,
            clear_georeference,
        ),
    ]
}

/// A georeference as the API exchanges it: SI units, radians.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeoreferenceValue {
    crs_epsg: u32,
    base_point: Point,
    grid_rotation: f64,
    scale: f64,
    survey_point: Option<Point>,
}

/// The parameters of `georef.set`: a georeference, and the id of the new `Site` when the project
/// has none. The caller draws it, as for `model.create`, so the command stays serialisable
/// (CORE-11) and no two projects share a `GlobalId`.
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    #[serde(default)]
    site: Option<EntityId>,
    crs_epsg: u32,
    base_point: Point,
    grid_rotation: f64,
    scale: f64,
    survey_point: Option<Point>,
}

#[derive(Debug, PartialEq, Serialize)]
struct CrsSummary {
    epsg: u32,
    name: String,
    kind: &'static str,
    geographic_name: String,
    area: String,
}

fn registry() -> Result<&'static Crss, ApiError> {
    Crss::embedded().map_err(|_| ApiError::Internal)
}

impl From<RegistryError> for GeorefError {
    fn from(error: RegistryError) -> Self {
        match error {
            RegistryError::NotProjected => Self::NotProjected,
            RegistryError::UnknownCrs => Self::UnknownCrs,
        }
    }
}

const OUTSIDE_DOMAIN: GeorefError = GeorefError::OutsideDomain { index: None };

fn point_of(point: ProjectedPoint) -> Point {
    [point.eastings, point.northings, point.height]
}

fn projected(point: Point) -> ProjectedPoint {
    ProjectedPoint::new(point[0], point[1], point[2])
}

fn local_of(point: LocalPoint) -> Point {
    [point.x, point.y, point.z]
}

fn local(point: Point) -> LocalPoint {
    LocalPoint::new(point[0], point[1], point[2])
}

fn geographic_of(point: GeographicPoint) -> Point {
    [point.latitude, point.longitude, point.height]
}

fn geographic(point: Point) -> GeographicPoint {
    GeographicPoint::new(point[1], point[0], point[2])
}

// --- georef.list_crs ---

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListCrs {
    #[serde(default)]
    include_geographic: bool,
}

#[derive(Serialize)]
struct CrsList {
    crs: Vec<CrsSummary>,
}

/// The current CRSs by EPSG code. Deprecated ones are left out, though `get`, `convert` and
/// `set` still accept them.
fn list_crs_summaries(
    _: &Session,
    ListCrs { include_geographic }: ListCrs,
) -> Result<CrsList, ApiError> {
    let registry = registry()?;
    let mut crs: Vec<CrsSummary> = registry.list_projected().map(summary).collect();
    if include_geographic {
        crs.extend(registry.list_geographic().map(|crs| CrsSummary {
            epsg: crs.code.get(),
            name: crs.name.clone(),
            kind: "geographic",
            geographic_name: crs.name.clone(),
            area: crs.area.name.clone(),
        }));
    }
    crs.sort_unstable_by_key(|crs| crs.epsg);
    Ok(CrsList { crs })
}

fn summary(crs: &ProjectedCrs) -> CrsSummary {
    CrsSummary {
        epsg: crs.code.get(),
        name: crs.name.clone(),
        kind: "projected",
        geographic_name: crs.base.name.clone(),
        area: crs.area.name.clone(),
    }
}

// --- reading the project's georeference ---

fn site_error(error: &SiteError) -> ApiError {
    match error {
        SiteError::MultipleSites => GeorefError::MultipleSites.into(),
        // Stored values that are not a georeference are a defect of the model, not of the call.
        _ => ApiError::Internal,
    }
}

fn read_georeference(model: &Model) -> Result<Option<(EntityId, Georeference)>, ApiError> {
    site::read(model).map_err(|error| site_error(&error))
}

/// The projected CRS of `georeference`, deprecated or not.
fn crs_of(georeference: &Georeference) -> Result<&'static ProjectedCrs, ApiError> {
    Ok(registry()?
        .projected(georeference.crs())
        .map_err(GeorefError::from)?)
}

fn domain_error(error: BasePointError) -> ApiError {
    match error {
        BasePointError::Geodesy(_) => OUTSIDE_DOMAIN.into(),
        // The CRS is looked up from the georeference itself, so they always match.
        BasePointError::CrsMismatch => ApiError::Internal,
    }
}

// --- georef.get ---

#[derive(Serialize)]
struct Current {
    georeference: Option<Described>,
}

#[derive(Serialize)]
struct Described {
    site: EntityId,
    value: GeoreferenceValue,
    crs: CrsSummary,
    derived: Derived,
}

#[derive(Serialize)]
struct Derived {
    true_north: f64,
    convergence: f64,
    point_scale_factor: f64,
    base_point_geographic: Point,
    survey_point_local: Option<Point>,
}

fn value_of(georeference: &Georeference) -> GeoreferenceValue {
    GeoreferenceValue {
        crs_epsg: georeference.crs().get(),
        base_point: point_of(georeference.base_point()),
        grid_rotation: georeference.grid_rotation(),
        scale: georeference.scale(),
        survey_point: georeference.survey_point().map(point_of),
    }
}

fn get_georeference(session: &Session, _: NoParams) -> Result<Current, ApiError> {
    let Some((site, georeference)) = read_georeference(session.document().model())? else {
        return Ok(Current { georeference: None });
    };
    let crs = crs_of(&georeference)?;
    let factors = crs
        .factors_at_base_point(&georeference)
        .map_err(domain_error)?;
    let true_north = crs.true_north(&georeference).map_err(domain_error)?;
    let base = crs
        .projection
        .inverse(georeference.base_point())
        .map_err(|_| OUTSIDE_DOMAIN)?;
    Ok(Current {
        georeference: Some(Described {
            site,
            value: value_of(&georeference),
            crs: summary(crs),
            derived: Derived {
                true_north,
                convergence: factors.convergence,
                point_scale_factor: factors.scale,
                base_point_geographic: geographic_of(base),
                survey_point_local: georeference.survey_point_local().map(local_of),
            },
        }),
    })
}

// --- georef.convert ---

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum System {
    Local,
    Projected,
    Geographic,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Convert {
    from: System,
    points: Vec<Point>,
}

#[derive(Serialize)]
struct Converted {
    points: Vec<Everywhere>,
}

#[derive(Serialize)]
struct Everywhere {
    local: Point,
    projected: Point,
    geographic: Point,
}

fn convert_points(
    session: &Session,
    Convert { from, points }: Convert,
) -> Result<Converted, ApiError> {
    if points.len() > MAX_PAGE_SIZE {
        return Err(ApiError::InvalidParams {
            detail: format!("at most {MAX_PAGE_SIZE} points"),
        });
    }
    let Some((_, georeference)) = read_georeference(session.document().model())? else {
        return Err(GeorefError::NotGeoreferenced.into());
    };
    let crs = crs_of(&georeference)?;
    let conversion = MapConversion::new(&georeference);
    let points = points
        .iter()
        .enumerate()
        .map(|(index, &point)| {
            convert_point(crs, &conversion, from, point)
                .filter(finite)
                .ok_or(GeorefError::OutsideDomain { index: Some(index) })
        })
        .collect::<Result<_, _>>()?;
    Ok(Converted { points })
}

/// A conversion that overflowed is out of the domain, though the projection accepted the point.
fn finite(point: &Everywhere) -> bool {
    [point.local, point.projected, point.geographic]
        .iter()
        .flatten()
        .all(|value| value.is_finite())
}

/// `point`, in the system `from`, in all three systems; `None` when the projection cannot take it.
fn convert_point(
    crs: &ProjectedCrs,
    conversion: &MapConversion,
    from: System,
    point: Point,
) -> Option<Everywhere> {
    let projection = &crs.projection;
    let converted: Result<_, GeodesyError> = match from {
        System::Local => {
            let projected = conversion.to_projected(local(point));
            projection
                .inverse(projected)
                .map(|geographic| (local(point), projected, geographic))
        }
        System::Projected => {
            let projected = projected(point);
            projection
                .inverse(projected)
                .map(|geographic| (conversion.to_local(projected), projected, geographic))
        }
        System::Geographic => {
            let geographic = geographic(point);
            projection
                .forward(geographic)
                .map(|projected| (conversion.to_local(projected), projected, geographic))
        }
    };
    let (local, projected, geographic) = converted.ok()?;
    Some(Everywhere {
        local: local_of(local),
        projected: point_of(projected),
        geographic: geographic_of(geographic),
    })
}

// --- georef.set and georef.clear ---

#[derive(Serialize)]
struct SetResult {
    site: EntityId,
}

/// Validates the georeference before anything changes, then writes it on the `Site` as one
/// batch. Every attribute is set, so a survey point from an earlier georeference does not stay.
fn set_georeference(
    session: &mut Session,
    params: SetParams,
    origin: &Origin,
) -> Result<Outcome<SetResult>, ApiError> {
    let georeference = validated(params)?;
    let existing =
        site::find_site(session.document().model()).map_err(|error| site_error(&error))?;
    let values = site::values(Some(&georeference));
    let (site, command) = if let Some(site) = existing {
        if params.site.is_some_and(|given| given != site) {
            return Err(ApiError::InvalidParams {
                detail: "site is not the id of the project's site".to_owned(),
            });
        }
        (site, set_all(site, values))
    } else {
        let id = params.site.ok_or(GeorefError::MissingSiteId)?;
        let values = values
            .into_iter()
            .filter_map(|(attribute, value)| Some((attribute.to_owned(), value?)))
            .collect();
        let create = ModelCommand::Create {
            id,
            entity: site::SITE.to_owned(),
            values,
        };
        (id, create)
    };
    let event = session.document_mut().execute(command, origin.clone())?;
    Ok(Outcome {
        result: SetResult { site },
        event: Some(event),
    })
}

/// The georeference of `value`, if its CRS is a projected one of the registry, its numbers are
/// valid and its base point lies in the projection's domain.
fn validated(value: SetParams) -> Result<Georeference, ApiError> {
    let code = EpsgCode::new(value.crs_epsg).map_err(|_| GeorefError::UnknownCrs)?;
    let crs = registry()?.projected(code).map_err(GeorefError::from)?;
    let georeference = Georeference::new(
        code,
        projected(value.base_point),
        value.grid_rotation,
        value.scale,
        value.survey_point.map(projected),
    )
    .map_err(|error| match error {
        GeoreferenceError::InvalidScale => GeorefError::InvalidScale.into(),
        other => ApiError::InvalidParams {
            detail: other.to_string(),
        },
    })?;
    crs.projection
        .inverse(georeference.base_point())
        .map_err(|_| OUTSIDE_DOMAIN)?;
    Ok(georeference)
}

/// A batch that sets each georeference attribute of `site` to its value.
fn set_all(site: EntityId, values: [(&'static str, Option<AttributeValue>); 9]) -> ModelCommand {
    let commands = values
        .into_iter()
        .map(|(attribute, value)| ModelCommand::Set {
            id: site,
            attribute: attribute.to_owned(),
            value,
        })
        .collect();
    ModelCommand::Batch { commands }
}

/// Unsets the georeference attributes and keeps the `Site`. A project that is not georeferenced
/// is left alone, with no history entry.
fn clear_georeference(
    session: &mut Session,
    _: NoParams,
    origin: &Origin,
) -> Result<Outcome<Map<String, Value>>, ApiError> {
    let model = session.document().model();
    let site = site::find_site(model).map_err(|error| site_error(&error))?;
    // An unset CRS means the project is not georeferenced; the other attributes have defaults.
    let georeferenced = |site| matches!(model.get(site, site::CRS_EPSG), Ok(Some(_)));
    let event = match site.filter(|&site| georeferenced(site)) {
        Some(site) => {
            let command = set_all(site, site::values(None));
            Some(session.document_mut().execute(command, origin.clone())?)
        }
        None => None,
    };
    Ok(Outcome {
        result: Map::new(),
        event,
    })
}

#[cfg(test)]
mod tests;
