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
//! [`Georeference`] holds metres. The caller gives the size of the file's length unit in metres
//! (1 for metres, 0.001 for millimetres). On import, the base point is in the map unit of the
//! `IfcProjectedCRS`, which is the file's length unit when the CRS has none, and a survey point
//! property in its own `Unit`, or in the file's length unit when it has none, as IFC reads an
//! `IfcLengthMeasure`. On export, the map unit and the unit of every survey point property are
//! written as the metre, so no tool reads them in the project's unit by mistake.
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
    IfcGeometricRepresentationContext, IfcMapConversion, IfcMapConversionScaled, IfcProjectedCRS,
    IfcPropertySet, IfcPropertySingleValue, IfcRelDefinesByProperties, IfcSIPrefix, IfcSIUnit,
    IfcSIUnitName, IfcSite, IfcUnit, IfcUnitEnum, IfcValue,
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
    /// A property holds no length measure.
    #[error("the {0} property of the survey point is not a length")]
    NotALength(SurveyProperty),
    /// A property has a unit that is not the metre or a prefixed metre.
    #[error("the {0} property of the survey point has an unsupported unit")]
    UnsupportedUnit(SurveyProperty),
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
    /// The map unit is not the metre or an SI-prefixed metre.
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
}

/// A georeference read from a file, and the instances it was read from.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The georeference. Its survey point is the one read, if the file holds a valid one.
    pub georeference: Georeference,
    /// The ids of the instances read into the georeference: the map conversion, the projected
    /// CRS and the `BIMCraft_SurveyPoint` property set found, its relationship and, when valid,
    /// its properties. The importer maps them; [`Preserved`](crate::preserve::Preserved) keeps
    /// the others. A survey point set that is not valid is mapped too: `BIMCraft` owns that
    /// set, and an export replaces it instead of adding a second one.
    pub mapped: BTreeSet<u64>,
    /// The model context the map conversion starts from, the `IfcSite` that holds a valid
    /// survey point and the metre unit the CRS uses, by their ids in the file read. An export
    /// needs a site to write a survey point on, and takes it from here only if the file had a
    /// valid one.
    pub targets: Targets,
    /// The id of the projected CRS, which kept instances can refer to.
    pub crs: u64,
    /// Why the survey point set of the file could not be read, if it was there but not valid.
    /// The georeference then has no survey point; the caller can tell the user.
    pub survey_error: Option<SurveyPointError>,
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
/// Only the conversion from that context is read, and decoded strictly. Other map conversions
/// are not the project's georeference and are left alone, however malformed. An
/// `IfcMapConversionScaled` whose factors are all 1 is read as a map conversion.
///
/// A survey point set that is not valid does not fail the import: see
/// [`Imported::survey_error`].
///
/// # Errors
///
/// A [`GeorefImportError`] when the conversion or the CRS it uses does not decode, a reference
/// dangles, the CRS is not named after an EPSG code, the map unit is not supported, a value is
/// not finite, the scale is not positive, the x-axis is zero or more than one conversion starts
/// from the model context.
pub fn read_georeference(
    instances: &[Instance],
    length_unit: f64,
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
    // Without a map unit, the map is in the project's length unit.
    let (map_unit, metre_unit) = match crs.map_unit {
        Some(unit) => {
            let unit = unit.id();
            let (size, is_metre) = file.si_length_unit(unit)?;
            (size, is_metre.then_some(unit))
        }
        None => (length_unit, None),
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
    let SurveyRead {
        mapped: survey_ids,
        result,
    } = file.survey(length_unit);
    let mut mapped = BTreeSet::from([conversion, crs_id]);
    mapped.extend(survey_ids);
    let (survey, survey_error) = match result {
        Ok(survey) => (survey, None),
        Err(error) => (None, Some(error)),
    };
    let (survey_point, site, survey_global_ids) = match survey {
        Some(ReadSurvey {
            point,
            site,
            global_ids,
        }) => (Some(point), Some(site), Some(global_ids)),
        None => (None, None, None),
    };
    let georeference = Georeference::new(
        code,
        base_point,
        rotation,
        scale * map_unit / length_unit,
        survey_point,
    )?;
    let kept = Kept {
        crs: Some((
            code,
            std::array::from_fn(|index| crs_record.parameters[index + 1].clone()),
        )),
        survey_global_ids,
    };
    Ok(Some(Imported {
        georeference,
        mapped,
        targets: Targets {
            context,
            site,
            metre_unit,
        },
        crs: crs_id,
        survey_error,
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
    /// context). Conversions from other contexts are not decoded: they are not ours.
    fn model_conversion(&self) -> Result<Option<(u64, IfcMapConversion, u64)>, GeorefImportError> {
        let mut found = None;
        for (&id, instance) in &self.instances {
            let InstanceBody::Simple(record) = &instance.body else {
                continue;
            };
            if record.name != IfcMapConversion::STEP_NAME
                && record.name != IfcMapConversionScaled::STEP_NAME
            {
                continue;
            }
            // `SourceCRS` is the first attribute; a conversion whose source is not an instance
            // of the file is not from the model context either.
            let Some(Parameter::Reference(source)) = record.parameters.first() else {
                continue;
            };
            if !self.is_model_context(*source) {
                continue;
            }
            let (conversion, unit_factors) = decode_conversion(id, record)?;
            if !unit_factors {
                return Err(GeorefImportError::UnsupportedScaledConversion);
            }
            if found.is_some() {
                return Err(GeorefImportError::MultipleConversions);
            }
            found = Some((id, conversion, *source));
        }
        Ok(found)
    }

    /// Whether instance `id` is the 3D `'Model'` context, not a sub-context. Only its identity
    /// is read, so a context that is malformed otherwise is no reason to refuse the file.
    fn is_model_context(&self, id: u64) -> bool {
        let Some(Instance {
            body: InstanceBody::Simple(record),
            ..
        }) = self.instances.get(&id).copied()
        else {
            return false;
        };
        // `ContextType` and `CoordinateSpaceDimension` follow `ContextIdentifier`.
        record.name == IfcGeometricRepresentationContext::STEP_NAME
            && matches!(record.parameters.get(1), Some(Parameter::String(kind)) if kind == MODEL_CONTEXT)
            && record.parameters.get(2) == Some(&Parameter::Integer(3))
    }

    /// The projected CRS `id`, decoded and as read.
    fn projected_crs(&self, id: u64) -> Result<(IfcProjectedCRS, &'a Record), GeorefImportError> {
        let record = self
            .record(id)?
            .filter(|record| record.name == IfcProjectedCRS::STEP_NAME)
            .ok_or(GeorefImportError::UnexpectedEntity { instance: id })?;
        let crs =
            IfcProjectedCRS::from_record(record).map_err(|error| GeorefImportError::Decode {
                instance: id,
                error,
            })?;
        Ok((crs, record))
    }

    /// The size in metres of the length unit `id`, which must be the metre or an SI-prefixed
    /// metre, and whether it is the plain metre.
    fn si_length_unit(&self, id: u64) -> Result<(f64, bool), GeorefImportError> {
        let unit = self
            .typed(id, IfcSIUnit::STEP_NAME, IfcSIUnit::from_record)?
            .ok_or(GeorefImportError::UnsupportedMapUnit)?;
        if unit.unit_type != IfcUnitEnum::Lengthunit || unit.name != IfcSIUnitName::Metre {
            return Err(GeorefImportError::UnsupportedMapUnit);
        }
        Ok((prefix_size(unit.prefix), unit.prefix.is_none()))
    }

    /// The survey point of the file, with the ids of the set and relationship found whether
    /// the point is valid or not, and of the properties of a valid one. Properties without a
    /// unit are in the file's length unit, `length_unit` metres.
    fn survey(&self, length_unit: f64) -> SurveyRead {
        let sets: Vec<(u64, u64)> = self
            .instances
            .iter()
            .filter_map(|(&id, instance)| self.survey_set(instance).map(|set| (id, set)))
            .collect();
        let mut mapped: BTreeSet<u64> = sets.iter().flat_map(|&(rel, set)| [rel, set]).collect();
        let result = match sets.as_slice() {
            [] => Ok(None),
            &[(relationship, set)] => {
                self.read_survey(relationship, set, length_unit)
                    .map(|(survey, properties)| {
                        mapped.extend(properties);
                        Some(survey)
                    })
            }
            _ => Err(SurveyPointError::MultipleSets),
        };
        SurveyRead { mapped, result }
    }

    /// The survey point attached by the relationship `relationship` to the set `set`, and the
    /// ids of the properties of the set.
    fn read_survey(
        &self,
        relationship: u64,
        set: u64,
        length_unit: f64,
    ) -> Result<(ReadSurvey, Vec<u64>), SurveyPointError> {
        let (site, relationship_global_id) = self.survey_site(relationship)?;
        let (properties, point, set_global_id) = self.survey_values(set, length_unit)?;
        let survey = ReadSurvey {
            point,
            site,
            global_ids: [set_global_id, relationship_global_id],
        };
        Ok((survey, properties))
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
            && matches!(set_record.parameters.get(2), Some(Parameter::String(name)) if name == SURVEY_POINT_SET))
        .then_some(*set)
    }

    /// Instance `id` of the survey point, decoded with `from_record`; a missing instance
    /// dangles, anything else is malformed.
    fn survey_typed<T>(
        &self,
        id: u64,
        name: &str,
        from_record: fn(&Record) -> Result<T, DecodeError>,
    ) -> Result<T, SurveyPointError> {
        match self.typed(id, name, from_record) {
            Ok(Some(value)) => Ok(value),
            Err(GeorefImportError::Dangling { reference }) => {
                Err(SurveyPointError::Dangling { reference })
            }
            _ => Err(SurveyPointError::Malformed),
        }
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

    /// The ids of the properties of the set `id`, the point they give in metres, and the set's
    /// `GlobalId`.
    fn survey_values(
        &self,
        id: u64,
        length_unit: f64,
    ) -> Result<(Vec<u64>, ProjectedPoint, String), SurveyPointError> {
        let set = self.survey_typed(id, IfcPropertySet::STEP_NAME, IfcPropertySet::from_record)?;
        let ids: Vec<u64> = set
            .has_properties
            .iter()
            .map(|property| property.id())
            .collect();
        let mut values = [None; 3];
        for &property in &ids {
            let value = self.survey_typed(
                property,
                IfcPropertySingleValue::STEP_NAME,
                IfcPropertySingleValue::from_record,
            )?;
            let name =
                SurveyProperty::from_name(&value.name.0).ok_or(SurveyPointError::Malformed)?;
            let slot = &mut values[name as usize];
            if slot.is_some() {
                return Err(SurveyPointError::Duplicate(name));
            }
            let Some(IfcValue::IfcLengthMeasure(length)) = value.nominal_value else {
                return Err(SurveyPointError::NotALength(name));
            };
            // A length without a unit is in the project's length unit.
            let unit = match value.unit {
                None => length_unit,
                Some(IfcUnit::Instance(unit)) => match self.si_length_unit(unit) {
                    Ok((size, _)) => size,
                    Err(GeorefImportError::Dangling { reference }) => {
                        return Err(SurveyPointError::Dangling { reference });
                    }
                    Err(_) => return Err(SurveyPointError::UnsupportedUnit(name)),
                },
            };
            let metres = length.0 * unit;
            if !metres.is_finite() {
                return Err(SurveyPointError::NonFinite(name));
            }
            *slot = Some(metres);
        }
        let [Some(eastings), Some(northings), Some(height)] = values else {
            let missing = SurveyProperty::ALL
                .into_iter()
                .find(|&property| values[property as usize].is_none())
                .unwrap_or(SurveyProperty::Eastings);
            return Err(SurveyPointError::Missing(missing));
        };
        let point = ProjectedPoint::new(eastings, northings, height);
        Ok((ids, point, set.global_id.0))
    }
}

/// The size of an SI prefix, as a factor.
fn prefix_size(prefix: Option<IfcSIPrefix>) -> f64 {
    match prefix {
        None => 1.0,
        Some(IfcSIPrefix::Exa) => 1e18,
        Some(IfcSIPrefix::Peta) => 1e15,
        Some(IfcSIPrefix::Tera) => 1e12,
        Some(IfcSIPrefix::Giga) => 1e9,
        Some(IfcSIPrefix::Mega) => 1e6,
        Some(IfcSIPrefix::Kilo) => 1e3,
        Some(IfcSIPrefix::Hecto) => 1e2,
        Some(IfcSIPrefix::Deca) => 1e1,
        Some(IfcSIPrefix::Deci) => 1e-1,
        Some(IfcSIPrefix::Centi) => 1e-2,
        Some(IfcSIPrefix::Milli) => 1e-3,
        Some(IfcSIPrefix::Micro) => 1e-6,
        Some(IfcSIPrefix::Nano) => 1e-9,
        Some(IfcSIPrefix::Pico) => 1e-12,
        Some(IfcSIPrefix::Femto) => 1e-15,
        Some(IfcSIPrefix::Atto) => 1e-18,
    }
}

/// The map conversion of record `id`, an `IfcMapConversion` or an `IfcMapConversionScaled`, and
/// whether its per-axis factors are all 1: always for an `IfcMapConversion`, and for an
/// `IfcMapConversionScaled` only when each factor is exactly 1.
fn decode_conversion(
    id: u64,
    record: &Record,
) -> Result<(IfcMapConversion, bool), GeorefImportError> {
    let decode_error = |error| GeorefImportError::Decode {
        instance: id,
        error,
    };
    if record.name == IfcMapConversion::STEP_NAME {
        let conversion = IfcMapConversion::from_record(record).map_err(decode_error)?;
        return Ok((conversion, true));
    }
    let scaled = IfcMapConversionScaled::from_record(record).map_err(decode_error)?;
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
    Ok((conversion, unit_factors))
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

/// What the survey point search found: the ids of the instances to replace on export, and the
/// point or why it is not valid.
struct SurveyRead {
    mapped: BTreeSet<u64>,
    result: Result<Option<ReadSurvey>, SurveyPointError>,
}

/// The instances a georeference is written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Targets {
    /// The export id of the 3D `'Model'` `IfcGeometricRepresentationContext`.
    pub context: u64,
    /// The export id of the `IfcSite` that holds the survey point.
    pub site: Option<u64>,
    /// The export id of an `IfcSIUnit` for the metre that the export reuses as the map unit and
    /// as the unit of the survey point. When `None`, the export writes one.
    pub metre_unit: Option<u64>,
}

/// Why a georeference could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeorefExportError {
    /// The length unit given for the file is not a finite number greater than zero.
    #[error("the length unit must be a finite number greater than zero")]
    InvalidLengthUnit,
    /// The scale, times the file's length unit, is not a finite number greater than zero.
    #[error("the scale does not fit the length unit")]
    ScaleOutOfRange,
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
/// The map unit of the CRS and the unit of each survey point property are the metre
/// (`targets.metre_unit`, or an `IfcSIUnit` written with the rest), whatever the project's
/// length unit, so the base point and the survey point are written in metres. The scale of the
/// conversion takes the project's length unit into account: it converts the unit of the model
/// into the map unit.
///
/// # Errors
///
/// A [`GeorefExportError`] when `length_unit` is not valid, the scale does not fit it, there is
/// a survey point and no site, or the ids would overflow.
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
    let scale = georeference.scale() * length_unit;
    if !(scale.is_finite() && scale > 0.0) {
        return Err(GeorefExportError::ScaleOutOfRange);
    }
    let count = 2 + u64::from(targets.metre_unit.is_none()) + if survey.is_some() { 5 } else { 0 };
    first_id
        .checked_add(count - 1)
        .ok_or(GeorefExportError::IdOverflow)?;
    // The check above leaves room for every id taken; the counter after the last is not used.
    let mut next_id = first_id;
    let mut take_id = || {
        let id = next_id;
        next_id = next_id.wrapping_add(1);
        id
    };
    let crs = take_id();
    let conversion = take_id();
    let metre = targets.metre_unit.unwrap_or_else(&mut take_id);
    let metre_unit = targets.metre_unit.is_none().then(|| metre_unit(metre));
    let base = georeference.base_point();
    let (abscissa, ordinate) = MapConversion::new(georeference).x_axis();
    let attributes = match &kept.crs {
        Some((code, attributes)) if *code == georeference.crs() => attributes.clone(),
        _ => [const { Parameter::Null }; 5],
    };
    let name = Parameter::String(format!("EPSG:{}", georeference.crs().get()));
    let crs_parameters = std::iter::once(name)
        .chain(attributes)
        .chain([Parameter::Reference(metre)])
        .collect();
    let mut instances = vec![
        Instance::simple(crs, IfcProjectedCRS::STEP_NAME, crs_parameters),
        Instance::simple(
            conversion,
            IfcMapConversion::STEP_NAME,
            vec![
                Parameter::Reference(targets.context),
                Parameter::Reference(crs),
                Parameter::Real(base.eastings),
                Parameter::Real(base.northings),
                Parameter::Real(base.height),
                Parameter::Real(abscissa),
                Parameter::Real(ordinate),
                Parameter::Real(scale),
            ],
        ),
    ];
    instances.extend(metre_unit);
    if let Some((point, site)) = survey {
        let global_ids = kept
            .survey_global_ids
            .clone()
            .unwrap_or_else(|| [ids.next_id().to_global_id(), ids.next_id().to_global_id()]);
        let new_ids = [(); 5].map(|()| take_id());
        instances.extend(survey_instances(point, site, metre, new_ids, global_ids));
    }
    Ok(instances)
}

/// The `IfcSIUnit` for the metre, as instance `id`.
fn metre_unit(id: u64) -> Instance {
    Instance::simple(
        id,
        IfcSIUnit::STEP_NAME,
        vec![
            Parameter::Derived,
            Parameter::Enumeration("LENGTHUNIT".into()),
            Parameter::Null,
            Parameter::Enumeration("METRE".into()),
        ],
    )
}

/// The survey point property set on `site`: three properties in metres, the set and the
/// relationship, with the ids `[eastings, northings, height, set, relationship]` and the
/// `GlobalId`s `[set, relationship]`.
fn survey_instances(
    point: ProjectedPoint,
    site: u64,
    metre: u64,
    [eastings, northings, height, set, relationship]: [u64; 5],
    [set_global_id, relationship_global_id]: [String; 2],
) -> Vec<Instance> {
    let properties = [eastings, northings, height];
    let values = [point.eastings, point.northings, point.height];
    let mut instances: Vec<Instance> = properties
        .into_iter()
        .zip(SurveyProperty::ALL)
        .zip(values)
        .map(|((id, property), value)| {
            Instance::simple(
                id,
                IfcPropertySingleValue::STEP_NAME,
                vec![
                    Parameter::String(property.name().into()),
                    Parameter::Null,
                    Parameter::Typed {
                        type_name: "IFCLENGTHMEASURE".into(),
                        value: Box::new(Parameter::Real(value)),
                    },
                    Parameter::Reference(metre),
                ],
            )
        })
        .collect();
    instances.push(Instance::simple(
        set,
        IfcPropertySet::STEP_NAME,
        vec![
            Parameter::String(set_global_id),
            Parameter::Null,
            Parameter::String(SURVEY_POINT_SET.into()),
            Parameter::Null,
            Parameter::List(properties.map(Parameter::Reference).to_vec()),
        ],
    ));
    instances.push(Instance::simple(
        relationship,
        IfcRelDefinesByProperties::STEP_NAME,
        vec![
            Parameter::String(relationship_global_id),
            Parameter::Null,
            Parameter::Null,
            Parameter::Null,
            Parameter::List(vec![Parameter::Reference(site)]),
            Parameter::Reference(set),
        ],
    ));
    instances
}

#[cfg(test)]
mod tests;
