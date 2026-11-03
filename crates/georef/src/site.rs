//! The georeference stored on the project's `Site` (spec D1, ADR 0017).
//!
//! A project has at most one `Site` object, whose attributes hold the georeference. An unset
//! `crs_epsg` means the project is not georeferenced. Writes go through a [`SiteTarget`]: a
//! [`Model`] directly, for an import, or a [`Transaction`], so that a command sets the whole
//! georeference as one undoable step (CORE-06).

use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::Transaction;

use crate::{EpsgCode, Georeference, GeoreferenceError, ProjectedPoint};

/// The entity that stores the georeference.
pub const SITE: &str = "Site";
/// The EPSG code of the projected CRS; unset when the project is not georeferenced.
pub const CRS_EPSG: &str = "crs_epsg";
/// The easting of the base point.
pub const BASE_POINT_EASTINGS: &str = "base_point_eastings";
/// The northing of the base point.
pub const BASE_POINT_NORTHINGS: &str = "base_point_northings";
/// The orthogonal height of the base point.
pub const BASE_POINT_HEIGHT: &str = "base_point_height";
/// The grid rotation.
pub const GRID_ROTATION: &str = "grid_rotation";
/// The map conversion scale.
pub const SCALE: &str = "scale";
/// The easting of the survey point.
pub const SURVEY_POINT_EASTINGS: &str = "survey_point_eastings";
/// The northing of the survey point.
pub const SURVEY_POINT_NORTHINGS: &str = "survey_point_northings";
/// The orthogonal height of the survey point.
pub const SURVEY_POINT_HEIGHT: &str = "survey_point_height";

/// Every georeference attribute of the `Site`, in the order of [`values`].
pub const ATTRIBUTES: [&str; 9] = [
    CRS_EPSG,
    BASE_POINT_EASTINGS,
    BASE_POINT_NORTHINGS,
    BASE_POINT_HEIGHT,
    GRID_ROTATION,
    SCALE,
    SURVEY_POINT_EASTINGS,
    SURVEY_POINT_NORTHINGS,
    SURVEY_POINT_HEIGHT,
];

/// Why the georeference of a model cannot be read or written. Messages name attributes, never
/// values or ids (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SiteError {
    /// The model holds more than one `Site`.
    #[error("the model holds more than one site")]
    MultipleSites,
    /// The stored values do not make a valid georeference.
    #[error(transparent)]
    Invalid(#[from] GeoreferenceError),
    /// Some, but not all, of the survey point coordinates are set.
    #[error("the survey point is incomplete")]
    IncompleteSurveyPoint,
    /// An attribute holds a value of another type than the georeference expects.
    #[error("attribute \"{attribute}\" holds a value of the wrong type")]
    WrongValueType {
        /// The attribute.
        attribute: &'static str,
    },
    /// The model rejected a read or a write.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// Where the georeference is written: a [`Model`], or a [`Transaction`] on one.
pub trait SiteTarget {
    /// The model, with the changes made so far.
    fn model(&self) -> &Model;

    /// See [`Model::insert`].
    ///
    /// # Errors
    ///
    /// As [`Model::insert`].
    fn insert(
        &mut self,
        id: EntityId,
        entity: &str,
        values: Vec<(&'static str, AttributeValue)>,
    ) -> Result<(), ModelError>;

    /// See [`Model::set`].
    ///
    /// # Errors
    ///
    /// As [`Model::set`].
    fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError>;
}

impl SiteTarget for Model {
    fn model(&self) -> &Model {
        self
    }

    fn insert(
        &mut self,
        id: EntityId,
        entity: &str,
        values: Vec<(&'static str, AttributeValue)>,
    ) -> Result<(), ModelError> {
        Self::insert(self, id, entity, values)
    }

    fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        Self::set(self, id, attribute, value)
    }
}

impl SiteTarget for Transaction<'_> {
    fn model(&self) -> &Model {
        Transaction::model(self)
    }

    fn insert(
        &mut self,
        id: EntityId,
        entity: &str,
        values: Vec<(&'static str, AttributeValue)>,
    ) -> Result<(), ModelError> {
        Transaction::insert(self, id, entity, values)
    }

    fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        Transaction::set(self, id, attribute, value)
    }
}

/// The project's `Site`, if it has one.
///
/// # Errors
///
/// [`SiteError::MultipleSites`] when the model holds more than one.
pub fn find_site(model: &Model) -> Result<Option<EntityId>, SiteError> {
    let mut sites = model.ids().filter(|&id| model.entity_of(id) == Some(SITE));
    let site = sites.next();
    if sites.next().is_some() {
        return Err(SiteError::MultipleSites);
    }
    Ok(site)
}

/// The project's `Site` and its georeference, or `None` when the project has no `Site` or its
/// CRS is unset.
///
/// # Errors
///
/// A [`SiteError`] when the model holds several sites or the stored values are invalid: an EPSG
/// code out of range, a scale that is not positive, or a partial survey point.
pub fn read(model: &Model) -> Result<Option<(EntityId, Georeference)>, SiteError> {
    let Some(site) = find_site(model)? else {
        return Ok(None);
    };
    let Some(code) = integer(model, site, CRS_EPSG)? else {
        return Ok(None);
    };
    let crs = u32::try_from(code)
        .map_err(|_| GeoreferenceError::InvalidCrsCode)
        .and_then(EpsgCode::new)?;
    let real_or =
        |attribute, default| Ok::<_, SiteError>(real(model, site, attribute)?.unwrap_or(default));
    let base_point = ProjectedPoint::new(
        real_or(BASE_POINT_EASTINGS, 0.0)?,
        real_or(BASE_POINT_NORTHINGS, 0.0)?,
        real_or(BASE_POINT_HEIGHT, 0.0)?,
    );
    let survey_point = match (
        real(model, site, SURVEY_POINT_EASTINGS)?,
        real(model, site, SURVEY_POINT_NORTHINGS)?,
        real(model, site, SURVEY_POINT_HEIGHT)?,
    ) {
        (Some(eastings), Some(northings), Some(height)) => {
            Some(ProjectedPoint::new(eastings, northings, height))
        }
        (None, None, None) => None,
        _ => return Err(SiteError::IncompleteSurveyPoint),
    };
    let georeference = Georeference::new(
        crs,
        base_point,
        real_or(GRID_ROTATION, 0.0)?,
        real_or(SCALE, 1.0)?,
        survey_point,
    )?;
    Ok(Some((site, georeference)))
}

/// The value of each georeference attribute for `georeference`, in the order of
/// [`ATTRIBUTES`]; with `None`, every attribute is unset.
#[must_use]
pub fn values(georeference: Option<&Georeference>) -> [(&'static str, Option<AttributeValue>); 9] {
    let Some(g) = georeference else {
        return ATTRIBUTES.map(|attribute| (attribute, None));
    };
    let real = |value: f64| Some(AttributeValue::Real(value));
    let base = g.base_point();
    let survey = g.survey_point();
    let survey_real = |coordinate: fn(ProjectedPoint) -> f64| {
        survey.map(|point| AttributeValue::Real(coordinate(point)))
    };
    [
        (
            CRS_EPSG,
            Some(AttributeValue::Integer(i64::from(g.crs().get()))),
        ),
        (BASE_POINT_EASTINGS, real(base.eastings)),
        (BASE_POINT_NORTHINGS, real(base.northings)),
        (BASE_POINT_HEIGHT, real(base.height)),
        (GRID_ROTATION, real(g.grid_rotation())),
        (SCALE, real(g.scale())),
        (SURVEY_POINT_EASTINGS, survey_real(|point| point.eastings)),
        (SURVEY_POINT_NORTHINGS, survey_real(|point| point.northings)),
        (SURVEY_POINT_HEIGHT, survey_real(|point| point.height)),
    ]
}

/// Writes `georeference` on the project's `Site`, which is created with the id `new_site` when
/// there is none, and returns the site's id. Every georeference attribute is set, so values left
/// from an earlier georeference, such as a survey point, are cleared.
///
/// # Errors
///
/// [`SiteError::MultipleSites`] when the model holds several sites, or [`SiteError::Model`]
/// when the model rejects a write. Through a [`Transaction`], a failure rolls the command back.
pub fn write(
    target: &mut impl SiteTarget,
    new_site: EntityId,
    georeference: &Georeference,
) -> Result<EntityId, SiteError> {
    let values = values(Some(georeference));
    if let Some(site) = find_site(target.model())? {
        set_all(target, site, values)?;
        return Ok(site);
    }
    let values = values
        .into_iter()
        .filter_map(|(attribute, value)| value.map(|value| (attribute, value)))
        .collect();
    target.insert(new_site, SITE, values)?;
    Ok(new_site)
}

/// Unsets every georeference attribute of the project's `Site`, which is kept, and returns its
/// id; returns `None`, changing nothing, when there is no `Site`.
///
/// # Errors
///
/// [`SiteError::MultipleSites`] when the model holds several sites, or [`SiteError::Model`]
/// when the model rejects a write.
pub fn clear(target: &mut impl SiteTarget) -> Result<Option<EntityId>, SiteError> {
    let Some(site) = find_site(target.model())? else {
        return Ok(None);
    };
    set_all(target, site, values(None))?;
    Ok(Some(site))
}

fn set_all(
    target: &mut impl SiteTarget,
    site: EntityId,
    values: [(&'static str, Option<AttributeValue>); 9],
) -> Result<(), SiteError> {
    for (attribute, value) in values {
        target.set(site, attribute, value)?;
    }
    Ok(())
}

fn integer(
    model: &Model,
    site: EntityId,
    attribute: &'static str,
) -> Result<Option<i64>, SiteError> {
    match model.get(site, attribute)? {
        None => Ok(None),
        Some(AttributeValue::Integer(value)) => Ok(Some(value)),
        Some(_) => Err(SiteError::WrongValueType { attribute }),
    }
}

fn real(model: &Model, site: EntityId, attribute: &'static str) -> Result<Option<f64>, SiteError> {
    match model.get(site, attribute)? {
        None => Ok(None),
        Some(AttributeValue::Real(value)) => Ok(Some(value)),
        Some(_) => Err(SiteError::WrongValueType { attribute }),
    }
}

#[cfg(test)]
mod tests;
