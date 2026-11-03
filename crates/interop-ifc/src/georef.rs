//! The georeference of a project in IFC 4.3: `IfcProjectedCRS` and `IfcMapConversion`, plus the
//! survey point in a `BIMCraft_SurveyPoint` property set on `IfcSite` (GEOREF-01, GEOREF-02,
//! ADR 0017).
//!
//! [`read_georeference`] finds the `IfcMapConversion` from the 3D `'Model'`
//! `IfcGeometricRepresentationContext` and returns the [`Georeference`] it describes, with the
//! ids of the instances it read, which the importer maps and [`Preserved`] does not keep.
//! [`write_georeference`] writes a [`Georeference`] back as new instances. Everything read is
//! validated: hostile input gives a [`GeorefImportError`], never a panic, and messages never
//! quote values from the file (SEC-03).
//!
//! Lengths in the file are in its length unit; [`Georeference`] holds metres. The caller gives
//! the size of the file's length unit in metres (1 for metres, 0.001 for millimetres).
//!
//! [`Preserved`]: crate::preserve::Preserved

use std::collections::{BTreeMap, BTreeSet};

use core_model::IdSource;
use georef::{
    EpsgCode, Georeference, GeoreferenceError, MapConversion, ProjectedPoint, XAxisError,
    rotation_from_x_axis,
};

use crate::ifc::decode::DecodeError;
use crate::ifc::{
    IfcCoordinateReferenceSystemSelect, IfcGeometricRepresentationContext, IfcMapConversion,
    IfcMapConversionScaled, IfcProjectedCRS, IfcPropertySet, IfcPropertySingleValue,
    IfcRelDefinesByProperties, IfcSIPrefix, IfcSIUnit, IfcSIUnitName, IfcSite, IfcUnitEnum,
    IfcValue,
};
use crate::step::{Instance, InstanceBody, Parameter, Record};

/// The name of the property set that holds the survey point.
pub const SURVEY_POINT_SET: &str = "BIMCraft_SurveyPoint";

/// The `ContextType` of the context a georeference starts from.
const MODEL_CONTEXT: &str = "Model";

/// A property of the survey point property set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurveyProperty {
    /// `Eastings`.
    Eastings,
    /// `Northings`.
    Northings,
    /// `OrthogonalHeight`.
    OrthogonalHeight,
}

impl SurveyProperty {
    /// Every property, in the order they are written.
    pub const ALL: [Self; 3] = [Self::Eastings, Self::Northings, Self::OrthogonalHeight];

    /// The property name, as written in the file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Eastings => "Eastings",
            Self::Northings => "Northings",
            Self::OrthogonalHeight => "OrthogonalHeight",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.name() == name)
    }
}

impl std::fmt::Display for SurveyProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Why the survey point property set could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SurveyPointError {
    /// More than one survey point property set, or one set attached more than once.
    #[error("the file holds more than one survey point")]
    MultipleSets,
    /// The set is not attached to exactly one `IfcSite`.
    #[error("the survey point is not attached to exactly one site")]
    NotOnSite,
    /// A reference points to no instance of the file.
    #[error("the survey point refers to a missing instance")]
    Dangling {
        /// The id of the missing instance.
        reference: u64,
    },
    /// A property is missing.
    #[error("the survey point has no {0} property")]
    Missing(SurveyProperty),
    /// A property is given more than once.
    #[error("the survey point has more than one {0} property")]
    Duplicate(SurveyProperty),
    /// A property holds no length measure, or has a unit of its own.
    #[error("the {0} property of the survey point is not a length")]
    NotALength(SurveyProperty),
    /// A property holds a number that is NaN or infinite.
    #[error("the {0} property of the survey point is not finite")]
    NonFinite(SurveyProperty),
    /// The set or one of its properties does not decode, or holds an unknown property.
    #[error("the survey point property set is malformed")]
    Malformed,
}

/// Why the georeference of a file could not be read.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum GeorefImportError {
    /// The length unit given for the file is not a finite number greater than zero.
    #[error("the length unit must be a finite number greater than zero")]
    InvalidLengthUnit,
    /// An instance the georeference reads does not decode.
    #[error("instance {instance}: {error}")]
    Decode {
        /// The id of the instance.
        instance: u64,
        /// What is wrong with it.
        error: DecodeError,
    },
    /// A reference points to no instance of the file.
    #[error("a georeference instance refers to a missing instance")]
    Dangling {
        /// The id of the missing instance.
        reference: u64,
    },
    /// An instance is of an entity the georeference does not accept there, such as an
    /// `IfcGeographicCRS` as the target of the map conversion.
    #[error("instance {instance} has an unexpected entity")]
    UnexpectedEntity {
        /// The id of the instance.
        instance: u64,
    },
    /// More than one map conversion starts from the model context.
    #[error("more than one map conversion starts from the model context")]
    MultipleConversions,
    /// The projected CRS is not named `EPSG:<code>`, or the code is out of range.
    #[error("the projected CRS is not named after an EPSG code")]
    UnknownCrsName,
    /// The map unit is not the metre or a decimal multiple of it.
    #[error("the map unit is not supported")]
    UnsupportedMapUnit,
    /// An `IfcMapConversionScaled` with factors other than 1, which a single scale cannot hold.
    #[error("map conversions with scale factors per axis are not supported")]
    UnsupportedScaledConversion,
    /// The x-axis direction of the map conversion is zero or not finite.
    #[error(transparent)]
    XAxis(#[from] XAxisError),
    /// The values do not make a valid georeference.
    #[error(transparent)]
    Invalid(#[from] GeoreferenceError),
    /// The survey point property set could not be read.
    #[error(transparent)]
    SurveyPoint(#[from] SurveyPointError),
}

/// Whether [`read_georeference`] reads the survey point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurveyPoint {
    /// Read the survey point property set; a malformed one is an error.
    Read,
    /// Leave the survey point property set alone, for example to import a file whose set is
    /// malformed without its survey point.
    Ignore,
}

/// A georeference read from a file, and the instances it was read from.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The georeference.
    pub georeference: Georeference,
    /// The ids of the instances read into the georeference: the map conversion, the projected
    /// CRS and, with a survey point, its property set, its properties and the relationship that
    /// attaches it. The importer maps them; [`Preserved`](crate::preserve::Preserved) keeps the
    /// others.
    pub mapped: BTreeSet<u64>,
    /// The id of the model context the map conversion starts from.
    pub context: u64,
    /// The id of the projected CRS, which kept instances can refer to.
    pub crs: u64,
    /// The id of the conversion.
    pub conversion: u64,
    /// The id of the `IfcSite` that holds the survey point.
    pub site: Option<u64>,
    /// What the export writes back as it was read.
    pub kept: Kept,
}

/// What an export writes back as the file had it, so that a round trip changes only what the
/// model changed (INT-03).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    /// The code of the CRS read and its `Description`, `GeodeticDatum`, `VerticalDatum`,
    /// `MapProjection` and `MapZone`, written back while the CRS keeps that code.
    pub crs: Option<(EpsgCode, [Parameter; 5])>,
    /// The `GlobalId`s of the survey point property set and of the relationship attaching it.
    pub survey_global_ids: Option<[String; 2]>,
}

/// Reads the georeference of a file from its `instances`, whose length unit is `length_unit`
/// metres. Returns `None` when no map conversion starts from the 3D `'Model'` context.
///
/// Map conversions from other contexts or CRSs are not the project's georeference and are left
/// alone. An `IfcMapConversionScaled` whose factors are all 1 is read as a map conversion.
///
/// # Errors
///
/// A [`GeorefImportError`] when an instance it reads does not decode, a reference dangles, the
/// CRS is not named after an EPSG code, the map unit is not supported, a value is not finite,
/// the scale is not positive, the x-axis is zero, more than one conversion starts from the
/// model context or, with [`SurveyPoint::Read`], the survey point property set is malformed.
pub fn read_georeference(
    instances: &[Instance],
    length_unit: f64,
    survey: SurveyPoint,
) -> Result<Option<Imported>, GeorefImportError> {
    if !(length_unit.is_finite() && length_unit > 0.0) {
        return Err(GeorefImportError::InvalidLengthUnit);
    }
    let file = File::new(instances);
    let Some((conversion, map_conversion, context)) = file.model_conversion()? else {
        return Ok(None);
    };
    let crs_id = map_conversion.target_crs.id();
    let (crs, crs_record) = file.projected_crs(crs_id)?;
    let code = crs
        .name
        .as_ref()
        .and_then(|name| epsg_code(&name.0))
        .ok_or(GeorefImportError::UnknownCrsName)?;
    let map_unit = match crs.map_unit {
        Some(unit) => file.map_unit(unit.id())?,
        None => length_unit,
    };
    let base_point = ProjectedPoint::new(
        map_conversion.eastings.0 * map_unit,
        map_conversion.northings.0 * map_unit,
        map_conversion.orthogonal_height.0 * map_unit,
    );
    let rotation = rotation_from_x_axis(
        map_conversion.x_axis_abscissa.map_or(1.0, |value| value.0),
        map_conversion.x_axis_ordinate.map_or(0.0, |value| value.0),
    )?;
    let scale = map_conversion.scale.map_or(1.0, |value| value.0);
    if scale.is_finite() && scale <= 0.0 {
        return Err(GeoreferenceError::InvalidScale.into());
    }
    let mut mapped = BTreeSet::from([conversion, crs_id]);
    let survey_point = match survey {
        SurveyPoint::Read => file.survey_point(map_unit, &mut mapped)?,
        SurveyPoint::Ignore => None,
    };
    let georeference = Georeference::new(
        code,
        base_point,
        rotation,
        scale * map_unit / length_unit,
        survey_point.as_ref().map(|survey| survey.point),
    )?;
    let kept = Kept {
        crs: Some((
            code,
            std::array::from_fn(|index| crs_record.parameters[index + 1].clone()),
        )),
        survey_global_ids: survey_point
            .as_ref()
            .map(|survey| survey.global_ids.clone()),
    };
    Ok(Some(Imported {
        georeference,
        mapped,
        context,
        crs: crs_id,
        conversion,
        site: survey_point.map(|survey| survey.site),
        kept,
    }))
}

/// The EPSG code of a CRS named `EPSG:<code>`; the prefix is case-insensitive and surrounding
/// spaces are ignored.
fn epsg_code(name: &str) -> Option<EpsgCode> {
    let name = name.trim();
    let (prefix, digits) = name.split_at_checked(5)?;
    if !prefix.eq_ignore_ascii_case("EPSG:")
        || digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    EpsgCode::new(digits.parse().ok()?).ok()
}

/// The instances of a file, by id.
struct File<'a> {
    instances: BTreeMap<u64, &'a Instance>,
}

impl<'a> File<'a> {
    fn new(instances: &'a [Instance]) -> Self {
        let mut by_id = BTreeMap::new();
        for instance in instances {
            by_id.entry(instance.id).or_insert(instance);
        }
        Self { instances: by_id }
    }

    /// The simple record of instance `id`, or `None` for a complex instance.
    fn record(&self, id: u64) -> Result<Option<&'a Record>, GeorefImportError> {
        let instance = self
            .instances
            .get(&id)
            .ok_or(GeorefImportError::Dangling { reference: id })?;
        Ok(match &instance.body {
            InstanceBody::Simple(record) => Some(record),
            InstanceBody::Complex(_) => None,
        })
    }

    /// Instance `id` decoded with `from_record`, or `None` when it is not a simple record named
    /// `name`.
    fn typed<T>(
        &self,
        id: u64,
        name: &str,
        from_record: fn(&Record) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, GeorefImportError> {
        match self.record(id)? {
            Some(record) if record.name == name => {
                from_record(record)
                    .map(Some)
                    .map_err(|error| GeorefImportError::Decode {
                        instance: id,
                        error,
                    })
            }
            _ => Ok(None),
        }
    }

    /// The map conversion from the 3D `'Model'` context, as (its id, its values, the id of the
    /// context).
    fn model_conversion(&self) -> Result<Option<(u64, IfcMapConversion, u64)>, GeorefImportError> {
        let mut found = None;
        for &id in self.instances.keys() {
            let Some((conversion, unit_factors)) = self.map_conversion(id)? else {
                continue;
            };
            let IfcCoordinateReferenceSystemSelect::Instance(source) = conversion.source_crs;
            // Conversions from other contexts or CRSs are left alone, whatever their factors.
            if !self.is_model_context(source)? {
                continue;
            }
            if !unit_factors {
                return Err(GeorefImportError::UnsupportedScaledConversion);
            }
            if found.is_some() {
                return Err(GeorefImportError::MultipleConversions);
            }
            found = Some((id, conversion, source));
        }
        Ok(found)
    }

    /// The map conversion of instance `id`, when it is one, and whether its per-axis factors
    /// are all 1: always for an `IfcMapConversion`, and for an `IfcMapConversionScaled` only
    /// when each factor is exactly 1.
    fn map_conversion(
        &self,
        id: u64,
    ) -> Result<Option<(IfcMapConversion, bool)>, GeorefImportError> {
        if let Some(conversion) = self.typed(
            id,
            IfcMapConversion::STEP_NAME,
            IfcMapConversion::from_record,
        )? {
            return Ok(Some((conversion, true)));
        }
        let Some(scaled) = self.typed(
            id,
            IfcMapConversionScaled::STEP_NAME,
            IfcMapConversionScaled::from_record,
        )?
        else {
            return Ok(None);
        };
        let unit_factors = [scaled.factor_x, scaled.factor_y, scaled.factor_z]
            .iter()
            // Exactly 1: any other factor changes the conversion.
            .all(|factor| factor.0.to_bits() == 1.0_f64.to_bits());
        let conversion = IfcMapConversion {
            source_crs: scaled.source_crs,
            target_crs: scaled.target_crs,
            eastings: scaled.eastings,
            northings: scaled.northings,
            orthogonal_height: scaled.orthogonal_height,
            x_axis_abscissa: scaled.x_axis_abscissa,
            x_axis_ordinate: scaled.x_axis_ordinate,
            scale: scaled.scale,
        };
        Ok(Some((conversion, unit_factors)))
    }

    /// Whether instance `id` is the 3D `'Model'` context, not a sub-context.
    fn is_model_context(&self, id: u64) -> Result<bool, GeorefImportError> {
        let context = self.typed(
            id,
            IfcGeometricRepresentationContext::STEP_NAME,
            IfcGeometricRepresentationContext::from_record,
        )?;
        Ok(context.is_some_and(|context| {
            context.coordinate_space_dimension.0 == 3
                && context
                    .context_type
                    .is_some_and(|context_type| context_type.0 == MODEL_CONTEXT)
        }))
    }

    /// The projected CRS `id`, decoded and as read.
    fn projected_crs(&self, id: u64) -> Result<(IfcProjectedCRS, &'a Record), GeorefImportError> {
        let crs = self
            .typed(id, IfcProjectedCRS::STEP_NAME, IfcProjectedCRS::from_record)?
            .ok_or(GeorefImportError::UnexpectedEntity { instance: id })?;
        // Decoding succeeded, so the instance is a simple record.
        let record = self
            .record(id)?
            .ok_or(GeorefImportError::UnexpectedEntity { instance: id })?;
        Ok((crs, record))
    }

    /// The size in metres of the map unit `id`: the metre or a decimal multiple of it.
    fn map_unit(&self, id: u64) -> Result<f64, GeorefImportError> {
        let unit = self
            .typed(id, IfcSIUnit::STEP_NAME, IfcSIUnit::from_record)?
            .ok_or(GeorefImportError::UnsupportedMapUnit)?;
        if unit.unit_type != IfcUnitEnum::Lengthunit || unit.name != IfcSIUnitName::Metre {
            return Err(GeorefImportError::UnsupportedMapUnit);
        }
        match unit.prefix {
            None => Ok(1.0),
            Some(IfcSIPrefix::Kilo) => Ok(1e3),
            Some(IfcSIPrefix::Deci) => Ok(1e-1),
            Some(IfcSIPrefix::Centi) => Ok(1e-2),
            Some(IfcSIPrefix::Milli) => Ok(1e-3),
            Some(_) => Err(GeorefImportError::UnsupportedMapUnit),
        }
    }

    /// The survey point, adding the instances read to `mapped`. Lengths in the set are in the
    /// map unit, `map_unit` metres.
    fn survey_point(
        &self,
        map_unit: f64,
        mapped: &mut BTreeSet<u64>,
    ) -> Result<Option<ReadSurvey>, SurveyPointError> {
        let mut found = None;
        for (&id, instance) in &self.instances {
            let Some(set) = self.survey_set(instance) else {
                continue;
            };
            if found.is_some() {
                return Err(SurveyPointError::MultipleSets);
            }
            found = Some((id, set));
        }
        let Some((relationship, set)) = found else {
            return Ok(None);
        };
        let (site, relationship_global_id) = self.survey_site(relationship)?;
        let (properties, values, set_global_id) = self.survey_values(set)?;
        let [eastings, northings, height] = values.map(|value| value * map_unit);
        mapped.extend([relationship, set]);
        mapped.extend(properties);
        Ok(Some(ReadSurvey {
            point: ProjectedPoint::new(eastings, northings, height),
            site,
            global_ids: [set_global_id, relationship_global_id],
        }))
    }

    /// The id of the survey point property set that `instance` attaches, when it is an
    /// `IfcRelDefinesByProperties` attaching a set named [`SURVEY_POINT_SET`].
    fn survey_set(&self, instance: &Instance) -> Option<u64> {
        let InstanceBody::Simple(record) = &instance.body else {
            return None;
        };
        if record.name != IfcRelDefinesByProperties::STEP_NAME {
            return None;
        }
        let Some(Parameter::Reference(set)) = record.parameters.get(5) else {
            return None;
        };
        let set_record = self.record(*set).ok()??;
        (set_record.name == IfcPropertySet::STEP_NAME
            && set_record.parameters.get(2) == Some(&Parameter::String(SURVEY_POINT_SET.into())))
        .then_some(*set)
    }

    /// Instance `id` of the survey point, decoded with `from_record`; anything else is
    /// malformed.
    fn survey_typed<T>(
        &self,
        id: u64,
        name: &str,
        from_record: fn(&Record) -> Result<T, DecodeError>,
    ) -> Result<T, SurveyPointError> {
        self.typed(id, name, from_record)
            .ok()
            .flatten()
            .ok_or(SurveyPointError::Malformed)
    }

    /// The `IfcSite` the relationship `id` attaches the survey point to, and the relationship's
    /// `GlobalId`.
    fn survey_site(&self, id: u64) -> Result<(u64, String), SurveyPointError> {
        let relationship = self.survey_typed(
            id,
            IfcRelDefinesByProperties::STEP_NAME,
            IfcRelDefinesByProperties::from_record,
        )?;
        let [site] = relationship.related_objects.as_slice() else {
            return Err(SurveyPointError::NotOnSite);
        };
        let site = site.id();
        match self.record(site) {
            Ok(Some(record)) if record.name == IfcSite::STEP_NAME => {
                Ok((site, relationship.global_id.0))
            }
            Ok(_) => Err(SurveyPointError::NotOnSite),
            Err(_) => Err(SurveyPointError::Dangling { reference: site }),
        }
    }

    /// The ids of the properties of the set `id`, their values in the order of
    /// [`SurveyProperty::ALL`], and the set's `GlobalId`.
    fn survey_values(&self, id: u64) -> Result<(Vec<u64>, [f64; 3], String), SurveyPointError> {
        let set = self.survey_typed(id, IfcPropertySet::STEP_NAME, IfcPropertySet::from_record)?;
        let mut values = [None; 3];
        let mut ids = Vec::with_capacity(set.has_properties.len());
        for property in &set.has_properties {
            let property = property.id();
            let value = match self.typed(
                property,
                IfcPropertySingleValue::STEP_NAME,
                IfcPropertySingleValue::from_record,
            ) {
                Ok(Some(value)) => value,
                Err(GeorefImportError::Dangling { reference }) => {
                    return Err(SurveyPointError::Dangling { reference });
                }
                _ => return Err(SurveyPointError::Malformed),
            };
            let name =
                SurveyProperty::from_name(&value.name.0).ok_or(SurveyPointError::Malformed)?;
            let slot = &mut values[name as usize];
            if slot.is_some() {
                return Err(SurveyPointError::Duplicate(name));
            }
            let number = match (value.nominal_value, value.unit) {
                (Some(IfcValue::IfcLengthMeasure(length)), None) => length.0,
                _ => return Err(SurveyPointError::NotALength(name)),
            };
            if !number.is_finite() {
                return Err(SurveyPointError::NonFinite(name));
            }
            *slot = Some(number);
            ids.push(property);
        }
        let mut result = [0.0; 3];
        for (property, (slot, value)) in SurveyProperty::ALL
            .into_iter()
            .zip(result.iter_mut().zip(values))
        {
            *slot = value.ok_or(SurveyPointError::Missing(property))?;
        }
        Ok((ids, result, set.global_id.0))
    }
}

/// A survey point read from a file.
struct ReadSurvey {
    /// The point, in metres.
    point: ProjectedPoint,
    /// The id of its `IfcSite`.
    site: u64,
    /// The `GlobalId`s of the property set and of the relationship.
    global_ids: [String; 2],
}

/// The instances a georeference is written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Targets {
    /// The export id of the 3D `'Model'` `IfcGeometricRepresentationContext`.
    pub context: u64,
    /// The export id of the `IfcSite` that holds the survey point.
    pub site: Option<u64>,
}

/// Why a georeference could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeorefExportError {
    /// The length unit given for the file is not a finite number greater than zero.
    #[error("the length unit must be a finite number greater than zero")]
    InvalidLengthUnit,
    /// A value does not fit the file's length unit.
    #[error("a georeference value is too large for the length unit")]
    NonFinite,
    /// The georeference has a survey point but no site was given to attach it to.
    #[error("the survey point needs a site")]
    SurveyPointWithoutSite,
    /// The instance ids would run past the largest id.
    #[error("no instance ids are left")]
    IdOverflow,
}

/// Writes `georeference` as new instances numbered from `first_id`, in a file whose length unit
/// is `length_unit` metres: an `IfcProjectedCRS` named `EPSG:<code>`, an `IfcMapConversion`
/// from `targets.context` to it and, with a survey point, three `IfcPropertySingleValue`, the
/// `BIMCraft_SurveyPoint` `IfcPropertySet` and the `IfcRelDefinesByProperties` attaching it to
/// `targets.site`. What `kept` holds is written back as read: the other attributes of the CRS
/// while it keeps its code, and the `GlobalId`s of the survey point instances; new `GlobalId`s
/// come from `ids`.
///
/// The map unit is left to the project's length unit, so lengths are written in the file's
/// unit, and the scale stays the one of the georeference.
///
/// # Errors
///
/// A [`GeorefExportError`] when `length_unit` is not valid, a value overflows in it, there is a
/// survey point and no site, or the ids would overflow.
pub fn write_georeference(
    georeference: &Georeference,
    targets: Targets,
    kept: &Kept,
    length_unit: f64,
    first_id: u64,
    ids: &mut impl IdSource,
) -> Result<Vec<Instance>, GeorefExportError> {
    if !(length_unit.is_finite() && length_unit > 0.0) {
        return Err(GeorefExportError::InvalidLengthUnit);
    }
    let survey = match (georeference.survey_point(), targets.site) {
        (Some(point), Some(site)) => Some((point, site)),
        (Some(_), None) => return Err(GeorefExportError::SurveyPointWithoutSite),
        (None, _) => None,
    };
    let count = if survey.is_some() { 7 } else { 2 };
    first_id
        .checked_add(count - 1)
        .ok_or(GeorefExportError::IdOverflow)?;
    let length = |value: f64| {
        let value = value / length_unit;
        if value.is_finite() {
            Ok(Parameter::Real(value))
        } else {
            Err(GeorefExportError::NonFinite)
        }
    };
    let crs = first_id;
    let conversion = first_id + 1;
    let base = georeference.base_point();
    let (abscissa, ordinate) = MapConversion::new(georeference).x_axis();
    let mut crs_parameters = vec![Parameter::String(format!(
        "EPSG:{}",
        georeference.crs().get()
    ))];
    match &kept.crs {
        Some((code, attributes)) if *code == georeference.crs() => {
            crs_parameters.extend(attributes.iter().cloned());
        }
        _ => crs_parameters.extend([const { Parameter::Null }; 5]),
    }
    // The map unit is the project's length unit, in which every length is written.
    crs_parameters.push(Parameter::Null);
    let mut instances = vec![
        simple(crs, "IFCPROJECTEDCRS", crs_parameters),
        simple(
            conversion,
            "IFCMAPCONVERSION",
            vec![
                Parameter::Reference(targets.context),
                Parameter::Reference(crs),
                length(base.eastings)?,
                length(base.northings)?,
                length(base.height)?,
                Parameter::Real(abscissa),
                Parameter::Real(ordinate),
                Parameter::Real(georeference.scale()),
            ],
        ),
    ];
    if let Some((point, site)) = survey {
        let values = [point.eastings, point.northings, point.height];
        let properties = [first_id + 2, first_id + 3, first_id + 4];
        for ((id, property), value) in properties.into_iter().zip(SurveyProperty::ALL).zip(values) {
            instances.push(simple(
                id,
                "IFCPROPERTYSINGLEVALUE",
                vec![
                    Parameter::String(property.name().into()),
                    Parameter::Null,
                    Parameter::Typed {
                        type_name: "IFCLENGTHMEASURE".into(),
                        value: Box::new(length(value)?),
                    },
                    Parameter::Null,
                ],
            ));
        }
        let [set_global_id, relationship_global_id] = kept
            .survey_global_ids
            .clone()
            .unwrap_or_else(|| [ids.next_id().to_global_id(), ids.next_id().to_global_id()]);
        let set = first_id + 5;
        instances.push(simple(
            set,
            "IFCPROPERTYSET",
            vec![
                Parameter::String(set_global_id),
                Parameter::Null,
                Parameter::String(SURVEY_POINT_SET.into()),
                Parameter::Null,
                Parameter::List(properties.map(Parameter::Reference).to_vec()),
            ],
        ));
        instances.push(simple(
            first_id + 6,
            "IFCRELDEFINESBYPROPERTIES",
            vec![
                Parameter::String(relationship_global_id),
                Parameter::Null,
                Parameter::Null,
                Parameter::Null,
                Parameter::List(vec![Parameter::Reference(site)]),
                Parameter::Reference(set),
            ],
        ));
    }
    Ok(instances)
}

fn simple(id: u64, name: &str, parameters: Vec<Parameter>) -> Instance {
    Instance {
        id,
        body: InstanceBody::Simple(Record {
            name: name.into(),
            parameters,
        }),
    }
}

#[cfg(test)]
mod tests;
