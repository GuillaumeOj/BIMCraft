# 0017. Georeference storage and IFC mapping

- Status: Accepted
- Requirements: GEOREF-01, GEOREF-02 (also CORE-06, CORE-10, INT-03, ENG-04, SEC-03)
- Issues: [#80](https://github.com/GuillaumeOj/BIMCraft/issues/80) (epic [#79](https://github.com/GuillaumeOj/BIMCraft/issues/79))

## Context

A project is placed on Earth through a projected coordinate reference system (CRS), identified by its EPSG code (GEOREF-01), and its local engineering frame is linked to that CRS by a translation, a rotation and a scale (GEOREF-02). Modelling stays in the local frame near the origin, because projected coordinates of millions of metres would eat the precision the geometry kernel relies on ([ADR 0001](0001-geometry-tolerance-policy.md)).

IFC 4.3 describes this link with `IfcMapConversion` from the model's `IfcGeometricRepresentationContext` to an `IfcProjectedCRS`. The georeference must survive an IFC round trip, both ours (BIMCraft → IFC → BIMCraft) and other tools' (their file → BIMCraft → file), without changing what the file means to them.

Forces:

- The schema defined no entity yet, and no project setting goes through transactions, so undo/redo (CORE-06), change events (CORE-07), persistence ([ADR 0016](0016-project-file-layout.md)) and the API would each need new machinery for a setting stored anywhere else.
- IFC 4.3 has no attribute for a survey point (a known point, such as a benchmark).
- Files are hostile input (AGENTS.md §8).

## Decision

### Storage: a `Site` entity

The georeference is stored as attributes of a spatial schema entity, `Site` (`schema/core/entities/site.toml`, IFC mapping `IfcSite`), the first entity of the core schema (version 2). A project has at most one `Site`; reading the georeference of a model with several is the typed error `SiteError::MultipleSites`.

| Attribute | Value | Default | Meaning |
| --- | --- | --- | --- |
| `crs_epsg` | `integer` | unset | EPSG code of the projected CRS. Unset: not georeferenced |
| `base_point_eastings`, `_northings`, `_height` | `quantity:length` | 0 | Projected coordinates of the local origin (the project base point) |
| `grid_rotation` | `quantity:plane_angle` | 0 | Angle from the CRS grid easting axis to the local x-axis, counter-clockwise |
| `scale` | `real` | 1 | Map conversion scale, finite and greater than 0 |
| `survey_point_eastings`, `_northings`, `_height` | `quantity:length` | unset | The survey point, in projected coordinates; all three or none |

The `georef` crate (layer 3) turns these attributes into a validated `Georeference` and back (`georef::site`). Writes go through a `SiteTarget`: a `Model` for an import, or a `Transaction`, so a command sets or clears the whole georeference as one undo step.

### Rotation: grid rotation stored, true north derived

The stored rotation is the grid rotation θ, counter-clockwise from the grid easting axis to the local x-axis. It is what `IfcMapConversion` holds: `XAxisAbscissa` = cos θ and `XAxisOrdinate` = sin θ. On import, θ = atan2(`XAxisOrdinate`, `XAxisAbscissa`), the direction need not be normalised, and a missing component takes the value of no rotation (1 for the abscissa, 0 for the ordinate).

True north is not stored. It is derived from θ and the meridian convergence γ of the projection at the base point (sub-issue [#81](https://github.com/GuillaumeOj/BIMCraft/issues/81)). γ is the angle from true north to grid north, positive clockwise, so true north is at γ − θ counter-clockwise from the local y-axis. This is how GEOREF-02's "rotation (true north)" is met. The context's `TrueNorth` is read and written back as it was, never changed.

### Map conversion and scale

With s the scale and (E₀, N₀, H₀) the base point, a local point (x, y, z) maps to:

- E = s·(x·cos θ − y·sin θ) + E₀
- N = s·(x·sin θ + y·cos θ) + N₀
- H = s·z + H₀

The scale applies to all three axes. `IfcMapConversion.Scale` is the conversion between the units of the engineering frame and those of the map, so heights scale with it. IFC 4.3 ADD2 adds `IfcMapConversionScaled` for per-axis factors (`FactorX`, `FactorY`, `FactorZ`), for example a grid scale factor that applies to horizontal distances only. The EXPRESS schema in `crates/interop-ifc/schema/` carries no formula, and the buildingSMART documentation site could not be reached from the build environment to quote, so this reading rests on the attribute's role and on the existence of `IfcMapConversionScaled`. The inverse is exact up to rounding. Trigonometry uses `libm` (ENG-04).

### IFC mapping

`interop_ifc::georef` reads and writes the georeference:

- The project georeference is the `IfcMapConversion` whose `SourceCRS` is the 3D `IfcGeometricRepresentationContext` with `ContextType` `'Model'` (not a sub-context). Conversions from other contexts or CRSs are left alone. More than one from the model context is an error.
- `TargetCRS` must be an `IfcProjectedCRS` named `EPSG:<code>` (prefix case-insensitive, spaces trimmed). Other names are the error `UnknownCrsName`, because BIMCraft cannot tell which CRS they mean.
- **Units.** Lengths in the file are in the project length unit, which the caller gives. The map unit is `MapUnit` when it is the metre or a decimal multiple of it, otherwise the import is refused. When `MapUnit` is absent, the map unit is the project length unit. The model's scale is `Scale` × map unit ÷ length unit. The export writes no `MapUnit` and writes lengths in the file's unit, so `Scale` stays the georeference's scale.
- **Scaled conversions.** An `IfcMapConversionScaled` is read only when its three factors are exactly 1. Otherwise the import of the georeference is refused (`UnsupportedScaledConversion`), and its instances stay in the file as read.
- On export, the georeference becomes new instances: an `IfcProjectedCRS`, an `IfcMapConversion` and, with a survey point, its property set. Instances that are kept and referred to the old CRS, such as an `IfcWellKnownText`, are pointed at the new one through `Preserved::restore` (INT-03).
- **What a round trip keeps.** The import records what the export writes back as it was read (`Kept`): the CRS's `Description`, `GeodeticDatum`, `VerticalDatum`, `MapProjection` and `MapZone`, while the CRS keeps its EPSG code, and the `GlobalId`s of the survey point set and of its relationship. Tools that track objects by `GlobalId` therefore see the same objects.
- **Validation.** Every value read is checked: dangling references, wrong entities, non-finite numbers, a scale that is not positive and a zero x-axis are typed errors, never panics. Messages never quote file content (SEC-03).

### Survey point: a `BIMCraft_SurveyPoint` property set on `IfcSite`

- **Where it goes.** The survey point is an `IfcPropertySet` named `BIMCraft_SurveyPoint`, attached to the `IfcSite` by an `IfcRelDefinesByProperties`.
- **Properties.** The set holds three `IfcPropertySingleValue`: `Eastings`, `Northings` and `OrthogonalHeight`. Each is an `IFCLENGTHMEASURE` with no unit of its own, in the map unit, like the base point in `IfcMapConversion`.
- **Identifiers.** The `GlobalId`s of a new set and relationship come from the model's `IdSource`, so a seeded source makes exports deterministic. Those read from a file are written back.
- **Name.** It has no `Pset_` prefix, which buildingSMART reserves for its own sets.
- **Reading.** The read is strict. One set, attached to exactly one `IfcSite`, holding exactly the three properties, is the survey point. Anything else is a typed `SurveyPointError`:
  - a second set, or a set attached twice
  - a set not on one site
  - a missing, duplicated, non-length or non-finite property
  - an unknown property, or a set that does not decode

  None of these instances is taken from the file then, so nothing is lost on export. The caller can import without the survey point (`SurveyPoint::Ignore`).
- **Writing.** Writing a survey point with no site to attach it to is the error `SurveyPointWithoutSite`. It is never dropped silently.

## Consequences

- Undo/redo, change events, persistence, the generic `model.*` API and the IFC mapping table work for the georeference with no new core machinery. The georeference commands (sub-issue [#83](https://github.com/GuillaumeOj/BIMCraft/issues/83)) only call `georef::site::write` and `clear` in a transaction.
- The `Site` attribute names are now part of the schema history: renaming one needs a migration step ([schema versions](../schema-versions.md)).
- When the model-to-IFC export arrives, it must give `write_georeference` the export ids of the model context and of the `IfcSite`.
- Other tools see the survey point as an ordinary property set. A tool that rewrites property sets may drop it, which loses only the survey point, not the georeference.
- A file imported with `SurveyPoint::Ignore` keeps its unread survey point set. An export that then writes a survey point adds a second set, which the next strict read reports as `MultipleSets`. The importer has to drop the old set first, or keep the survey point unset.
- Files whose CRS is not named after an EPSG code, whose map unit is not metric, or with per-axis scale factors, are not georeferenced on import. Their instances stay in the file.

## Alternatives considered

- **A project-settings store** outside the schema. It would need its own transactions, events, persistence and API, for one record.
- **The survey point as the `IfcSite` placement.** This moves the site's local frame for every other tool and mixes survey data with model geometry.
- **The survey point as a second `IfcMapConversion` or an `IfcRigidOperation`.** Other tools would apply it as a coordinate operation and misplace the model.
- **Storing true north instead of the grid rotation.** True north depends on the projection at the base point, and `IfcMapConversion` holds the grid rotation, so storing it would need the projection maths at every import and export, and would round-trip less exactly.
- **H = z + H₀, ignoring the scale for heights.** This contradicts `Scale` as a unit conversion: a model in millimetres would get heights a thousand times too large.

## References

- Specification §7 (GEOREF-01 to GEOREF-04).
- IFC 4.3 ADD2: `IfcMapConversion`, `IfcMapConversionScaled`, `IfcProjectedCRS`, `IfcCoordinateReferenceSystem`, `IfcSite` (`crates/interop-ifc/schema/IFC4X3_ADD2.exp`).
- [ADR 0001](0001-geometry-tolerance-policy.md) (model extent), [ADR 0002](0002-schema-definition-format.md) (schema format), [ADR 0003](0003-deterministic-math-library.md) (`libm`), [ADR 0015](0015-commands-and-transactions.md) (commands and transactions).
