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

The stored rotation is the grid rotation θ, counter-clockwise from the grid easting axis to the local x-axis. It is what `IfcMapConversion` holds: `XAxisAbscissa` = cos θ and `XAxisOrdinate` = sin θ. On import, θ = atan2(`XAxisOrdinate`, `XAxisAbscissa`), the direction need not be normalised, and a missing component takes the value of no rotation (1 for the abscissa, 0 for the ordinate). The angle read is always in (−π, π], so a rotation stored outside that range, such as 7 rad, comes back as the equivalent angle, 7 − 2π. The two place the model the same way, so this is accepted.

True north is not stored. It is derived from θ and the meridian convergence γ of the projection at the base point (sub-issue [#81](https://github.com/GuillaumeOj/BIMCraft/issues/81)). γ is the angle from true north to grid north, positive clockwise, so true north is at γ − θ counter-clockwise from the local y-axis. This is how GEOREF-02's "rotation (true north)" is met. The context's `TrueNorth` is read and written back as it was, never changed.

### Map conversion and scale

With s the scale and (E₀, N₀, H₀) the base point, a local point (x, y, z) maps to:

- E = s·(x·cos θ − y·sin θ) + E₀
- N = s·(x·sin θ + y·cos θ) + N₀
- H = s·z + H₀

The scale applies to all three axes. `IfcMapConversion.Scale` is the conversion between the units of the engineering frame and those of the map, so heights scale with it. IFC 4.3 ADD2 adds `IfcMapConversionScaled` for per-axis factors (`FactorX`, `FactorY`, `FactorZ`), for example a grid scale factor that applies to horizontal distances only. The EXPRESS schema in `crates/interop-ifc/schema/` carries no formula, and the buildingSMART documentation site could not be reached from the build environment to quote, so this reading rests on the attribute's role and on the existence of `IfcMapConversionScaled`. The inverse is exact up to rounding. Trigonometry uses `libm` (ENG-04).

### IFC mapping

`interop_ifc::georef` reads and writes the georeference:

- The project georeference is the `IfcMapConversion` whose `SourceCRS` is the 3D `IfcGeometricRepresentationContext` with `ContextType` `'Model'` (not a sub-context). More than one from the model context is an error. Only that conversion is decoded, strictly. The others, from other contexts or CRSs, are not the project's: they are left alone and kept by `Preserved`, however malformed. The context is recognised by its name, `ContextType` and dimension only, so a context that is malformed in another attribute does not stop the import.
- `TargetCRS` must be an `IfcProjectedCRS` named `EPSG:<code>` (prefix case-insensitive, spaces trimmed). Other names are the error `UnknownCrsName`, because BIMCraft cannot tell which CRS they mean.
- **Units.** In IFC, an `IfcLengthMeasure` with no unit of its own is in the project's length unit, and an absent `IfcProjectedCRS.MapUnit` is commonly read the same way. The caller gives the size of the project's length unit, f metres.
  - *Import.* The map unit is `MapUnit` when it is the metre or an SI-prefixed metre (any prefix), otherwise the import is refused (`UnsupportedMapUnit`). When `MapUnit` is absent, it is f. The base point is in the map unit, and the model's scale is `Scale` × map unit ÷ f. A survey point property with a `Unit` is in that unit (metre or SI-prefixed metre, otherwise `UnsupportedUnit`), and without one it is in f.
  - *Export.* The map unit and the unit of the three survey point properties are always written, as an `IfcSIUnit(LENGTHUNIT, $, METRE)`: the one the CRS used on import (`Targets::metre_unit`, which the import fills when the CRS's map unit is the plain metre), or a new instance. The base point and the survey point are written in metres, whatever f is, and `Scale` is the model's scale times f. A millimetre file therefore holds `Scale` 0.001 for a model scale of 1, and no tool reads a survey length in millimetres by mistake.
- **Scaled conversions.** An `IfcMapConversionScaled` from the model context is read only when its three factors are exactly 1. Otherwise the import of the georeference is refused (`UnsupportedScaledConversion`), and its instances stay in the file as read.
- On export, the georeference becomes new instances: an `IfcProjectedCRS`, an `IfcMapConversion` and, with a survey point, its property set. Instances that are kept and referred to the old CRS, such as an `IfcWellKnownText`, are pointed at the new one through `Preserved::restore` (INT-03).
- **What a round trip keeps.** The import records what the export writes back as it was read (`Kept`): the CRS's `Description`, `GeodeticDatum`, `VerticalDatum`, `MapProjection` and `MapZone`, while the CRS keeps its EPSG code, and the `GlobalId`s of the survey point set and of its relationship. Tools that track objects by `GlobalId` therefore see the same objects.
- **Validation.** Every value read is checked: dangling references, wrong entities, non-finite numbers, a scale that is not positive and a zero x-axis are typed errors, never panics. Messages never quote file content (SEC-03).

### Survey point: a `BIMCraft_SurveyPoint` property set on `IfcSite`

- **Where it goes.** The survey point is an `IfcPropertySet` named `BIMCraft_SurveyPoint`, attached to the `IfcSite` by an `IfcRelDefinesByProperties`.
- **Properties.** The set holds three `IfcPropertySingleValue`: `Eastings`, `Northings` and `OrthogonalHeight`. Each is an `IFCLENGTHMEASURE` whose `Unit` is the metre, as the map unit is: see Units above.
- **Identifiers.** The `GlobalId`s of a new set and relationship come from the model's `IdSource`, so a seeded source makes exports deterministic. Those read from a file are written back.
- **Name.** It has no `Pset_` prefix, which buildingSMART reserves for its own sets.
- **Reading.** The read is strict. One set, attached to exactly one `IfcSite`, holding exactly the three properties, is the survey point. Anything else is a typed `SurveyPointError`:
  - a second set, or a set attached twice
  - a set not on one site
  - a missing, duplicated, non-length or non-finite property
  - a unit that is not a metre
  - an unknown property, or a set that does not decode

  The error does not fail the import: `Imported::survey_error` holds it, and the georeference has no survey point. The set and its relationship are still in `Imported::mapped`, because `BIMCraft` owns a set of that name: an export replaces it, so a file never holds two. The properties of an invalid set are not mapped, as they may belong to other sets.
- **Writing.** Writing a survey point with no site to attach it to is the error `SurveyPointWithoutSite`. It is never dropped silently.

## The CRS registry

The CRS definitions (GEOREF-02) are data, not code (`crates/georef/epsg/`).

- **Embedded TSV.** `epsg.tsv` is extracted from PROJ's `proj.db` by `extract.py` and embedded with `include_str!`. `georef::Registry::embedded()` parses it once, on first use, into records that each carry a ready `Projection`. The crate reads no file (ARCH-02), so it builds for WebAssembly unchanged.
- **A subset.** Only the MVP set is in the file: Lambert-93, CC42 to CC50, ETRS89 and WGS 84 UTM zones. Adding a CRS is a change to `extract.py` and a regeneration, described in `epsg/README.md`, which also holds the IOGP attribution.
- **Deprecated codes.** EPSG deprecates some codes of the set, such as 25838. They stay in the data with their status (and replacement, if any). `Registry::projected` accepts them, so a project or an IFC file that names one still opens; `Registry::list_projected` leaves them out, so a user cannot pick one for a new project.
- **Hostile input.** Every number is checked on the way in (finite, within the bounds of real definitions: Helmert translations up to 10 km, rotations up to 60″, scale up to 1000 ppm), and an error names the line and the field, never a value (SEC-03).
- **True north.** `ProjectedCrs::true_north` and `factors_at_base_point` take a `Georeference` and refuse one on another CRS, so the projection is always the one of the georeference's `crs_epsg`.

## Consequences

- Undo/redo, change events, persistence, the generic `model.*` API and the IFC mapping table work for the georeference with no new core machinery. The georeference commands (sub-issue [#83](https://github.com/GuillaumeOj/BIMCraft/issues/83)) only call `georef::site::write` and `clear` in a transaction.
- The `Site` attribute names are now part of the schema history: renaming one needs a migration step ([schema versions](../schema-versions.md)).
- When the model-to-IFC export arrives, it must give `write_georeference` the export ids of the model context, of the `IfcSite` and, if it has one, of the metre unit.
- Other tools see the survey point as an ordinary property set. A tool that rewrites property sets may drop it, which loses only the survey point, not the georeference.
- Files whose CRS is not named after an EPSG code, whose map unit is not metric, or with per-axis scale factors, are not georeferenced on import. Their instances stay in the file.

## Alternatives considered

- **A project-settings store** outside the schema. It would need its own transactions, events, persistence and API, for one record.
- **The survey point as the `IfcSite` placement.** This moves the site's local frame for every other tool and mixes survey data with model geometry.
- **The survey point as a second `IfcMapConversion` or an `IfcRigidOperation`.** Other tools would apply it as a coordinate operation and misplace the model.
- **Storing true north instead of the grid rotation.** True north depends on the projection at the base point, and `IfcMapConversion` holds the grid rotation, so storing it would need the projection maths at every import and export, and would round-trip less exactly.
- **Reading the survey point in a mode chosen by the caller** (read, or ignore it). An ignored set stayed in the file, and a later export added a second one. The import now always finds the set and reports its state.
- **Writing no `MapUnit`, and survey lengths with no unit.** Both are read in the project's length unit by other tools, so a millimetre project put the survey point 1000 times off.
- **H = z + H₀, ignoring the scale for heights.** This contradicts `Scale` as a unit conversion: a model in millimetres would get heights a thousand times too large.

## References

- Specification §7 (GEOREF-01 to GEOREF-04).
- IFC 4.3 ADD2: `IfcMapConversion`, `IfcMapConversionScaled`, `IfcProjectedCRS`, `IfcCoordinateReferenceSystem`, `IfcSite` (`crates/interop-ifc/schema/IFC4X3_ADD2.exp`).
- [ADR 0001](0001-geometry-tolerance-policy.md) (model extent), [ADR 0002](0002-schema-definition-format.md) (schema format), [ADR 0003](0003-deterministic-math-library.md) (`libm`), [ADR 0015](0015-commands-and-transactions.md) (commands and transactions).

## Follow-ups

Not part of this decision; to schedule separately.

- A shared write trait in `core-model`, implemented by `Model` and `Transaction` in `core-txn`, to replace `georef::site::SiteTarget` and serve the next domain crate that writes through either.
- A generated trait for the IFC entity classes, with a `decode::entity::<T>()` helper, to replace the name and `from_record` pairs that `interop_ifc::georef` passes by hand.
- Fold `Preserved::split_ids` into `Preserved::split` by giving its predicate the instance id.
- Reading a file: index it in a `HashMap` and find the conversions and survey point sets in one pass, and give `core-model` a way to list the objects of one entity (`Model::ids_of`) so that `find_site` does not visit every object.
- One shared `proptest_config` for the integration tests of a crate.
