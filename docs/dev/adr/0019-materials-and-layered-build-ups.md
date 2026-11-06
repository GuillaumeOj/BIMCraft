# 0019. Materials and layered build-ups

- Status: Accepted
- Requirements: MAT-01, MAT-02, MAT-03 (also REP-05, ENG-03, ENG-04, CORE-06, CORE-10, PERS-01, SEC-03)
- Issues: [#84](https://github.com/GuillaumeOj/BIMCraft/issues/84), [#85](https://github.com/GuillaumeOj/BIMCraft/issues/85), [#86](https://github.com/GuillaumeOj/BIMCraft/issues/86), [#87](https://github.com/GuillaumeOj/BIMCraft/issues/87), [#244](https://github.com/GuillaumeOj/BIMCraft/issues/244)

## Context

A project holds materials, such as C30/37 concrete or S355 steel, each with a behaviour law and properties per design code (MAT-01). A library ships the common European grades, and every value names the clause it comes from (MAT-02, REP-05). Walls and slabs will be built from layered build-ups that reference these materials (MAT-03).

Materials and build-ups are shared definitions. They are neither placed elements nor spatial structure. IFC calls them resources (`IfcMaterial`, `IfcMaterialLayerSet`, `IfcMaterialLayer`), which have no GlobalId and no placement. A material must survive saving, closing and reopening the project with every value and source. Property-set values are not saved in project files yet (ADR 0013, #35).

## Decision

**A `resource` entity kind.** Resources are schema entities that other objects refer to but that are never placed. They take part in no relationship by kind and have no type and no representation. As schema entities they get undo and redo (CORE-06), change events, persistence (ADR 0016) and the generic `model.*` API with no new core machinery. ADR 0002 lists the kind.

**The `Material` entity** (IFC `IfcMaterial`) has these attributes:

| Attribute | Value | Meaning |
| --- | --- | --- |
| `name` | text, required | The grade's name, such as `C30/37` |
| `category` | `enum:MaterialCategory`, required | `concrete`, `reinforcement`, `steel`, `timber`, `masonry`, `insulation`, `finish`, `generic` |
| `behaviour_law` | `enum:BehaviourLaw`, required | `linear_elastic`, `concrete_parabola_rectangle`, `concrete_bilinear`, `steel_bilinear`, `timber_elastic_brittle`, `none` |
| `design_code` | text | The code the values follow, such as `EN 1992-1-1:2004` |
| `library_key` | text | The library grade the material was created from; unset when created from scratch |
| one per property | sourced real or quantity, optional | The property catalogue below |

**Property values are sourced attributes, not property sets.** Each value is stored with its document and clause (ADR 0018). Attributes are saved in the project file today, as their value column plus `@document` and `@clause` columns. Property-set values are not, so the property sets that #244 started with are gone. A save, close and reopen test in `host-desktop` checks every value and source on the embedded core schema, and a property test does the same for random materials.

**The property catalogue.** Values are in SI units (ADR 0004). The `material` crate says which properties each category gives and which ones it requires.

| Category | Properties | Required |
| --- | --- | --- |
| every category | `density`, `e_modulus`, `shear_modulus`, `poisson_ratio`, `thermal_expansion`, `thermal_conductivity` (λ), `specific_heat` | — |
| concrete | `fck`, `fck_cube`, `fcm`, `fctm`, `eps_c2`, `eps_cu2`, `n`, `eps_c3`, `eps_cu3`; `gamma_c`, `alpha_cc` | `fck` |
| reinforcement | `fy` (fyk), `fu` (ftk), `k`, `eps_u` (εuk); `gamma_s` | `fy` |
| steel | `fy`, `fu`, `k`, `eps_u`; `gamma_m0`, `gamma_m1`, `gamma_m2` | `fy` |
| timber | `fm_k`, `ft0_k`, `fc0_k`, `fv_k`, `e0_mean`, `e0_05`, `g_mean`, `rho_k`, `rho_mean`; `gamma_m` | `fm_k` |
| masonry | `fk`; `gamma_m` | `fk` |
| insulation, finish, generic | the common properties only | — |

For concrete, `e_modulus` holds Ecm. Reinforcement and structural steel share `fy`, `fu`, `k` and `eps_u`, so one steel law serves both. Each property has a physical range, such as strains in (0, 1), ν in [0, 0.5), and k and the partial factors at least 1. Ordered pairs are checked too: fck ≤ fcm and fy ≤ fu.

**National annexes and partial factors.** The values that differ by annex are γc, αcc, γs, γM0, γM1, γM2 and γM. They are stored per material, because the factor that applies depends on the material's code. Each factor has an attribute for the recommended values (`gamma_c`) and one for the French annex (`gamma_c_fr`). The suffix is internal: the API names a factor by its key and its annex (`{ key, annex }`). `Material::partial_factor` returns the value for one annex and never falls back to another annex (`MissingAnnexValue`). `Material::design_law` divides the law's strength by the category's factors: αcc fck / γc, fyk / γs, fy / γM0, and f / γM.

**Behaviour laws** are pure functions in the `material` crate. Each gives the stress for a strain, with tension positive, both characteristic and design. Each also gives its limit strains and its tangent modulus at zero, and uses `libm` for its powers (ENG-04).

| Law | Parameters | Limits |
| --- | --- | --- |
| linear elastic | E | none |
| concrete parabola-rectangle (EN 1992-1-1, 3.1.7) | fc, εc2, εcu2, n; no tension | εcu2 in compression |
| concrete bi-linear (EN 1992-1-1, 3.1.7(2)) | fc, εc3, εcu3; no tension | εcu3 in compression |
| steel bi-linear (EN 1992-1-1, 3.2.7; EN 1993-1-1, 5.4.3) | fy, E, optional k and εu (inclined branch) | εu both ways when it hardens |
| timber elastic-brittle | E0,mean, ft,0, fc,0: brittle in tension, plastic in compression | ft,0 / E0,mean in tension |
| none | no structural use; `stress` fails with `NoStructuralLaw` | — |

Concrete may follow either concrete law, reinforcement and steel the steel law, and timber the timber law. Masonry and the non-structural categories may follow `none`. Every category may follow linear elasticity. ENG-03: every law has hand calculations in its tests, with the arithmetic in comments, plus property tests for monotonicity, continuity at branch ends, odd symmetry where it applies, and the limits and initial modulus.

**Sources and overrides.** A material created from the library copies its values with their library sources, as types copy component defaults (ADR 0014). A library update therefore never changes a project silently. When the user edits a value, it takes the source "Entered by user" (`Source::user()`, ADR 0018). The first time a cited value is replaced, a `MaterialOriginal` resource (IFC `IfcMaterialProperties`) keeps it. That resource holds the material, the attribute's key and the library value with its source. Resetting the value restores the value and its source and deletes the original, in one undoable command. A value the library never gave has nothing to reset (`NothingToReset`). Deleting a material deletes its originals in the same step: `material::delete` is one command that removes the originals, then the material. The generic `model.delete` refuses instead: `Model::remove` fails with `StillReferenced { count }` when other objects hold a required `ref` or `list:ref` to the object (the API answers with the reason `in_use` and the count), so no required reference is ever left dangling, and nothing is deleted behind the caller's back. Build-ups follow the same rule: a layer set's layers are removed first, in the same command. Originals are attributes, so they persist like any object.

**Build-ups (#87, to come).** `MaterialLayerSet` (IFC `IfcMaterialLayerSet`) and `MaterialLayer` (IFC `IfcMaterialLayer`) are resources too. A layer references a `Material` and holds a thickness, a function (`structure`, `substrate`, `insulation`, `air_gap`, `membrane`, `finish`), a join priority from 1 (highest) to 5 and a core flag. Default priorities by function: structure 1, substrate 2, insulation 3, air gap 3, membrane 4, finish 5. The layer's object id is its stable LayerId (ADR 0010).

**IFC mapping**, declared now and implemented with the element export (INT-07):

| BIMCraft | IFC 4.3 |
| --- | --- |
| `Material` | `IfcMaterial` (`Name`, `Category` from the category) |
| common properties | `Pset_MaterialCommon` (`MassDensity`), `Pset_MaterialMechanical` (`YoungModulus`, `ShearModulus`, `PoissonRatio`, `ThermalExpansionCoefficient`), `Pset_MaterialThermal` (`ThermalConductivity`, `SpecificHeatCapacity`) |
| concrete, steel, timber properties | `Pset_MaterialConcrete`, `Pset_MaterialSteel`, `Pset_MaterialWood`, and `BIMCraft_Material*` sets for values IFC has no property for |
| each source | the property's `Specification` (ADR 0018) |
| `MaterialOriginal` | `IfcMaterialProperties` named `BIMCraft_LibraryValues` |
| `MaterialLayerSet` / `MaterialLayer` | `IfcMaterialLayerSet` / `IfcMaterialLayer` (`LayerThickness`, `IsVentilated` for an air gap, `Category` from the function) |
| layer priority 1 … 5 | `IfcMaterialLayer.Priority` 0 … 100: (5 − p) × 25, so 1 → 100 and 5 → 0; on import, p = 5 − round(Priority / 25) |

## Consequences

- Materials need no new storage path. They are saved, undone and served by the API like any object, and their values persist now.
- The `Material` entity has many optional columns, most of them empty for any one material. A column costs little, and the category catalogue keeps a material to its own properties.
- Property sets become usable for materials once they persist (#35). Moving the values there would then be a schema migration.
- Partial factors are flat attributes, which two annexes (recommended and France) are enough for in the MVP. A new annex is a schema version: seven attributes and a migration.
- The schema history does not check enumeration items between versions, so removing an item of `MaterialCategory` or `BehaviourLaw` in a later version needs a migration step.
- `analysis` and the discipline crates read laws through `material`, which sits in layer 3 beside `georef` (ARCH-04).

## Alternatives considered

- **Property sets for the values** (the #244 port): they match IFC better but are not saved in project files yet, so a reopened material lost everything. Making property sets persist belongs to #35.
- **Elements or spatial entities for materials**: they would bring a GlobalId, placement and relationships that a definition does not have.
- **A list of overridden keys on the material**, with the library value read back from the library on reset: a library update would change what a reset restores, which D4 forbids.
- **The original value in a second column per property**: it doubles every column for values that are rarely edited.
- **Partial factors as code constants per annex**: they are data with sources (REP-05), and plugins will add annexes.

## References

- [Specification §8.1](../../specifications.md): MAT-01, MAT-02, MAT-03; REP-05, ENG-03, ENG-04.
- [ADR 0002](0002-schema-definition-format.md), [ADR 0004](0004-units-and-quantities.md), [ADR 0010](0010-location-lines.md), [ADR 0013](0013-property-sets.md), [ADR 0014](0014-components-types-instances.md), [ADR 0016](0016-project-file-layout.md), [ADR 0018](0018-sourced-values.md).
