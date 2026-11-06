# Materials and build-ups — API contract

JSON-RPC methods registered in `crates/api` (API-01), version 1.0.0, exposed to the UI through `ui/src/api/` (UI-01). Commands run as one `model.batch`, so each is one undo step, records the caller's origin and emits a change event. Quantities are SI on the wire (metres, pascals, kg/m³, W/(m·K)); the UI converts. Errors use the existing `ApiError` shape with a stable `code` and never echo model values (SEC-03). The backend may refine field names in its plan; any change is written back here and told to `front` before T5/T8 start.

Shared shapes:

```ts
type Category = "concrete" | "reinforcement" | "steel" | "timber" | "masonry" | "insulation" | "finish" | "generic";
type Annex = "recommended" | "fr";
type Source = { document: string; clause: string } | { user: true };   // "EN 1992-1-1", "Table 3.1"
type PropertyValue = {
  key: string;            // "fck", "density", "thermal_conductivity" …
  quantity: "stress" | "modulus" | "density" | "strain" | "ratio" | "thermal_conductivity" | "specific_heat" | "length" | "none";
  value: number | null;   // SI; null = not given (optional property)
  source: Source | null;  // null when value is null
  library_value: number | null; // the grade's value, for "Reset"; null for a custom material
  annex?: Annex;          // only for annex-dependent values (partial factors)
};
type Law = { kind: "parabola_rectangle" | "bilinear" | "linear_elastic" | "elastic_plastic" | "elastic_brittle" | "none"; source: Source | null };
```

## Library (#86)

| Method | Params | Result |
| --- | --- | --- |
| `materials.library` (query) | `{ search?: string, category?: Category, offset?: number, limit?: number }` (limit ≤ `MAX_PAGE_SIZE`) | `{ items: { key: string, name: string, category: Category, design_code: string \| null }[], total: number }` — `key` is stable, e.g. `"en1992-1-1/C25/30"` |
| `materials.grade` (query) | `{ key: string }` | `{ key, name, category, design_code, law: Law, properties: PropertyValue[] }`; error `unknown_grade` |

## Project materials (#86)

| Method | Params | Result |
| --- | --- | --- |
| `materials.list` (query) | `{ search?: string, category?: Category }` | `{ items: { id: string, name: string, category: Category, grade: string \| null, used_by: number }[] }` |
| `materials.get` (query) | `{ material: string }` | `{ id, name, category, design_code, grade: string \| null, law: Law, properties: PropertyValue[], used_by: number }`; error `not_found` |
| `materials.add_from_library` (command) | `{ key: string, name?: string }` | `{ id: string }` — values copied from the grade; errors `unknown_grade`, `duplicate_name` |
| `materials.create` (command) | `{ name: string, category: Category, properties?: { [key: string]: number } }` | `{ id: string }` — custom material, every source `{ user: true }`; errors `invalid_name`, `duplicate_name`, `unknown_property`, `invalid_value` (with `key`) |
| `materials.update` (command) | `{ material: string, name?: string, properties?: { [key: string]: number \| null } }` — `null` resets to the library value (or clears an optional value on a custom material) | `{}`; errors `not_found`, `duplicate_name`, `unknown_property`, `invalid_value` (with `key`), `required_property` (with `key`) |
| `materials.delete` (command) | `{ material: string }` | `{}`; error `in_use` with `{ used_by: number }` |

`invalid_value` covers non-finite, negative where not allowed, and out-of-range values (for example ν outside [0, 0.5)). Names: trimmed, 1–120 characters, unique per project (case-insensitive).

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
