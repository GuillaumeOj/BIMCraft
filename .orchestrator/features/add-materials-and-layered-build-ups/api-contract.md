# Materials and build-ups — API contract

JSON-RPC methods registered in `crates/api` (API-01), version 1.0.0, exposed to the UI through `ui/src/api/` (UI-01). Commands run as one `model.batch`, so each is one undo step, records the caller's origin and emits a change event. Quantities are SI on the wire (metres, pascals, kg/m³, W/(m·K)); the UI converts. Errors use the existing `ApiError` shape with a stable `code` and never echo model values (SEC-03). The backend may refine field names in its plan; any change is written back here and told to `front` before T5/T8 start.

Shared shapes (final, as implemented in T2; the generated types in `ui/src/api/protocol.gen.ts` are the reference):

```ts
type MaterialCategory = "concrete" | "reinforcement" | "steel" | "timber" | "masonry" | "insulation" | "finish" | "generic";
type MaterialAnnex = "recommended" | "fr";
type Source = { document: string; clause: string } | { entered_by_user: true };   // "EN 1992-1-1:2004", "Table 3.1"
type MaterialProperty = {
  key: string;            // "fck", "density", "thermal_conductivity", "gamma_c" …
  annex?: MaterialAnnex;  // only for the partial factors, which come once per annex
  quantity: "stress" | "modulus" | "density" | "strain" | "ratio" | "thermal_conductivity" | "specific_heat";
  value: number | null;   // SI; null = not given
  source: Source | null;  // null when value is null
  library_value: number | null; // what "Reset" restores: the grade's value; null for a custom material or a value the grade never gave
  range: { min: number | null; min_inclusive: boolean; max: number | null; max_inclusive: boolean };
  required: boolean;      // the category needs it (fck, fy, fm_k, fk)
};
type MaterialPropertyInput = { key: string; annex?: MaterialAnnex; value: number | null };
type MaterialLaw = {
  kind: "parabola_rectangle" | "bilinear" | "linear_elastic" | "elastic_plastic" | "elastic_brittle" | "none";
  source: Source | null;  // the clause that defines the law, when the backend is certain of it
  branch: "horizontal" | "inclined" | null;  // reinforcement top branch, steel hardening
};
```

A material lists **every property its category gives**, those it has (`value` set) and those it could have (`value: null`), so the UI can offer them. Partial factors appear once per annex.

## Library (#86)

| Method | Params | Result |
| --- | --- | --- |
| `materials.library` (query) | `{ search?: string, category?: MaterialCategory, offset?: number, limit?: number }` (limit 1 – `MAX_PAGE_SIZE`, default `DEFAULT_PAGE_SIZE`) | `{ items: { key, name, category, design_code: string \| null }[], total: number }` — sorted by `key`, which is stable, e.g. `"en1992-1-1/C25/30"`; `design_code` is the key `"en1992-1-1"`; `search` matches the name or the key, ignoring case |
| `materials.grade` (query) | `{ key: string }` | `{ key, name, category, design_code, law: MaterialLaw, properties: MaterialProperty[] }`; error `unknown_grade` |

## Project materials (#86)

Commands take the **id of the object they create** (as `model.create` does), so they can be replayed (CORE-11).

| Method | Params | Result |
| --- | --- | --- |
| `materials.list` (query) | `{ search?: string, category?: MaterialCategory }` | `{ items: { id, name, category, grade: string \| null, used_by: number }[] }`, sorted by name |
| `materials.get` (query) | `{ material: EntityId }` | `{ id, name, category, design_code, grade: string \| null, law, properties, used_by: number, edited: boolean }`; error `not_found`. `edited` says that a library value was changed, so a reset has something to restore |
| `materials.add_from_library` (command) | `{ id: EntityId, key: string, name?: string }` | `{ id }` — values and sources copied from the grade; errors `unknown_grade`, `duplicate_name`, `invalid_name` |
| `materials.create` (command) | `{ id: EntityId, name: string, category: MaterialCategory, properties?: MaterialPropertyInput[] }` | `{ id }` — custom material, every source `{ "entered_by_user": true }`. The law is the most specific one of the category the values allow (parabola-rectangle, bi-linear or elastic for concrete; bi-linear or elastic for steel; elastic-brittle or elastic for timber; elastic or none otherwise). Errors `invalid_name`, `duplicate_name`, `unknown_property`, `invalid_value`, `required_property` (with `key`) |
| `materials.update` (command) | `{ material: EntityId, name?: string, properties?: MaterialPropertyInput[] }` — `value: null` resets to the library value and source, or clears a value the user entered that the library never gave | `{}`; errors `not_found`, `invalid_name`, `duplicate_name`, `unknown_property`, `invalid_value`, `required_property` (with `key`). The values are checked together (fy and fu can be raised in one call); a refused call changes nothing; a call that changes nothing leaves no history entry |
| `materials.delete` (command) | `{ material: EntityId }` | `{}`; error `in_use` with `used_by` |

Details:

- **Errors** are JSON-RPC errors with code `-32000` and `data: { reason, key?, used_by? }`. `reason` is one of `unknown_grade`, `not_found`, `duplicate_name`, `invalid_name`, `unknown_property`, `invalid_value`, `required_property`, `in_use`; `key` names the property at fault when there is one; `used_by` comes with `in_use`. The generic `model.delete` of a material that is still referenced is refused the same way (`reason: "in_use"`, `count`).
- `invalid_value` covers non-finite, out-of-range and inconsistent values (ν outside [0, 0.5), fy above fu), a missing value in `materials.create`, an annex given to a property that takes none, a partial factor without annex, and a property named twice. The `range` of each property says what the server accepts.
- **Names**: trimmed, 1–120 characters, no control character, unique per project ignoring case.
- **`used_by`** counts the objects that use the material; the library values kept for edited properties are not users.
- A `MaterialOriginal` keeps the library value of an edited property. Its id derives from the material's id and the property (the UI never draws it), so a replayed `update` gives the same objects.

## Build-ups (#87)

```ts
type Usage = "wall" | "slab";
type LayerFunction = "structure" | "substrate" | "insulation" | "air_gap" | "membrane" | "finish";
type LayerInput = {
  id?: string;              // existing layer: keeps its LayerId (ADR 0010); absent = new layer
  material: string;         // project material id
  thickness: number;        // metres
  function: LayerFunction;
  priority?: 1 | 2 | 3 | 4 | 5;  // default from function: structure 1, substrate 2, insulation 3, air_gap 3, membrane 4, finish 5
  core?: boolean;           // default: function === "structure"
  name?: string;
};
type Layer = Required<Omit<LayerInput, "name">> & { name: string | null; material_name: string; thermal_resistance: number | null };
```

| Method | Params | Result |
| --- | --- | --- |
| `buildups.list` (query) | `{ usage?: Usage }` | `{ items: { id, name, usage: Usage, layer_count: number, total_thickness: number }[] }` |
| `buildups.get` (query) | `{ buildup: string }` | `{ id, name, usage, description: string \| null, layers: Layer[], total_thickness: number, thermal_resistance: number \| null }` (layers exterior → interior, or top → bottom) |
| `buildups.create` (command) | `{ name: string, usage: Usage, description?: string, layers: LayerInput[] }` | `{ id: string, layers: string[] }` |
| `buildups.update` (command) | `{ buildup: string, name?: string, description?: string \| null, layers?: LayerInput[] }` — `layers` replaces the whole list in order; layers left out are deleted | `{ layers: string[] }` |
| `buildups.duplicate` (command) | `{ buildup: string, name: string }` | `{ id: string }` (new layer ids) |
| `buildups.delete` (command) | `{ buildup: string }` | `{}` (its layers go with it) |

Errors (with `index` of the offending layer where it applies): `not_found`, `invalid_name`, `duplicate_name`, `no_layers`, `too_many_layers` (> 32), `invalid_thickness`, `core_not_contiguous`, `unknown_material`, `unknown_layer` (an `id` not in this build-up), `invalid_priority`.

## Generated types

The backend adds the methods to the protocol schema (`schema/api/protocol.schema.json`) and regenerates `ui/src/api/protocol.gen.ts`. Frontend writes the typed helpers (`ui/src/api/materials.ts`, `ui/src/api/buildups.ts`) on top of them, as `ui/src/api/georef.ts` does.

## Amendments (lead, after the UX notes)

These answer the open questions of `design/ux-notes.md` and supersede the tables above where they differ.

1. **Annex-dependent values are addressable.** Done as written in the tables above (`MaterialPropertyInput`): `annex` is required for partial factors and refused for the others (`invalid_value` with `key`).
2. **Range hints.** Done: `MaterialProperty` has `range: { min: number | null, min_inclusive: boolean, max: number | null, max_inclusive: boolean }`, so the UI can say "Between 0 and 0.5." The server still enforces it.
3. **Law branch.** Done: `MaterialLaw` has `branch: "horizontal" | "inclined" | null` (reinforcement top branch, steel hardening).
4. **Design codes on the wire** (done) are keys (`"en1992-1-1"`); the UI shows the label.
5. **No stress–strain curve query** in this feature. The detail pane shows the law name and its source.
6. **`buildups.preview` (query)**, so the UI never recomputes material maths (ENG-04): params `{ usage: Usage, layers: LayerInput[] }`; result `{ layers: { priority, core, thermal_resistance: number | null }[], total_thickness, thermal_resistance, layers_without_thermal_data: number, errors: { code, index? }[] }`. It validates without saving and returns every error at once. The editor calls it on each draft change (debounced) for the live totals and the inline errors; Apply still calls `create`/`update`.
7. **Partial thermal resistance.** `thermal_resistance` (in `get`, `preview`) is the sum over the layers whose material has λ, and `layers_without_thermal_data` counts the others (air gaps included: conventional air-layer resistances belong to the thermal module). It is `null` only when no layer has λ. The UI shows the sum with "N layers without thermal data".
8. **Air gaps.** New error `invalid_material` (with `index`): an `air_gap` layer whose material is not of category `generic`. The picker offers only `generic` materials for an air gap.
