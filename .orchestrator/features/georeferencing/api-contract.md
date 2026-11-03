# Georeferencing — API contract

These are the commands and queries that task T6 adds to the `api` crate and that task T7 calls from `ui/src/api/`. They follow `docs/dev/api.md`: dotted names, version `1.0.0`, signatures written by hand, a schema snapshot in `schema/api/protocol.schema.json`, and TypeScript types generated into `ui/src/api/protocol.gen.ts` (`bun scripts/generate-api-types.ts`).

**Units.** Values are SI everywhere, as in the rest of the API: lengths in metres, angles in **radians**, geographic latitude and longitude in radians too. Conversion to degrees or DMS for display happens in the UI. Numbers are JSON numbers (f64).

**Points.** `[x, y, z]` arrays of 3 numbers.

## Shared definitions (`$defs`)

```jsonc
// Georeference: what is stored on the Site (spec D1)
"Georeference": {
  "crs_epsg": 2154,                       // integer, a code from georef.list_crs
  "base_point": [652000.0, 6862000.0, 35.0], // E, N, orthogonal height of the local origin
  "grid_rotation": 0.0,                   // rad, CRS easting axis -> local x-axis, counter-clockwise
  "scale": 1.0,                           // > 0
  "survey_point": null                    // or [E, N, H]
}

"CrsSummary": {
  "epsg": 2154,
  "name": "RGF93 v1 / Lambert-93",
  "kind": "projected",                     // "projected" | "geographic"
  "geographic_name": "RGF93 v1",           // name of the base geodetic CRS
  "area": "France - onshore and offshore, mainland and Corsica" // EPSG area of use, short text
}
```

## Queries

### `georef.list_crs` v1

Lists the CRSs of the embedded registry that can georeference a project. Only projected CRSs are listed by default.

- params: `{ "include_geographic"?: bool }` (default `false`)
- Deprecated CRSs are omitted from the list. `georef.get`, `georef.convert` and `georef.set` accept a deprecated code (such as 25838), so a project or IFC file that names one still opens.
- result: `{ "crs": CrsSummary[] }`, sorted by EPSG code. The list is small (≈ 140 entries), so it is not paged.

### `georef.get` v1

Returns the project's georeference and the values derived from it.

- params: `{}`
- result:

```jsonc
{
  "georeference": null | {
    "site": EntityId,
    "value": Georeference,
    "crs": CrsSummary,
    "derived": {
      "true_north": 0.0123,               // rad: angle from the local y-axis to true north at the base point, counter-clockwise (= convergence - grid_rotation)
      "convergence": 0.0123,              // rad: meridian convergence at the base point, angle from true north to grid north, positive clockwise (east of the central meridian)
      "point_scale_factor": 0.99995,      // projection scale factor at the base point
      "base_point_geographic": [0.8527, 0.0409, 35.0], // latitude, longitude (rad), ellipsoidal height approx = orthogonal height
      "survey_point_local": null | [x, y, z]
    }
  }
}
```

- Errors: `-32000` with `data.reason = "multiple_sites"` when the model has more than one `Site`.

### `georef.convert` v1

Converts points from one system to the other two. The UI uses it for the coordinates panel.

- params: `{ "from": "local" | "projected" | "geographic", "points": Point[] }`. At most 1000 points, the same bound as `MAX_PAGE_SIZE`. A geographic point is `[lat, lon, h]` in rad, rad and m, on the CRS's base geodetic CRS.
- result: `{ "points": [{ "local": Point, "projected": Point, "geographic": Point }] }`, in the input order.
- Heights: no vertical datum or geoid yet (GEOREF-05), so the geographic height equals the projected `H` and the local `z` plus `H₀`; the UI labels it as approximate.
- Errors: `-32000` `data.reason = "not_georeferenced"`; `-32602` for non-finite numbers or too many points; `-32000` `data.reason = "outside_domain"` (with the index of the first bad point) when a point cannot be projected, for example a latitude out of range or a point too far from the central meridian for the method.

## Commands (undoable, recorded with their origin, API-05)

### `georef.set` v1

Sets the whole georeference in one transaction, so one undo step reverts it. It creates the `Site` if there is none, otherwise it sets the attributes of the existing one.

- params: `Georeference`
- result: `{ "site": EntityId }`, plus the usual `ChangeEvent`, which also goes out as the `model.changed` notification
- Validation, before anything is changed:
  - `crs_epsg` must be in the registry and projected. Otherwise `-32000` `data.reason = "unknown_crs"` or `"not_projected"`.
  - Every number must be finite (`-32602`), and `scale` > 0 (`data.reason = "invalid_scale"`).
  - The base point must lie in the projection's valid domain, which is checked by an inverse projection (`outside_domain`).
  - More than one `Site` gives `multiple_sites`.

### `georef.clear` v1

Removes the georeference: it unsets every georeference attribute of the `Site` (it does not delete the `Site`). It can be undone. When the project is not georeferenced, it succeeds and changes nothing (empty change event, no history entry).

- params: `{}`
- result: `{}`

## Error data

Every georef error uses the existing codes. `data.reason` is a stable snake_case string that the UI maps to an i18n message (`georef.error.<reason>`). Messages never repeat model values (SEC-03).

| `data.reason` | Code | When |
| --- | --- | --- |
| `unknown_crs` | -32000 | EPSG code not in the registry |
| `not_projected` | -32000 | Code is a geographic CRS |
| `invalid_scale` | -32000 | scale ≤ 0 |
| `outside_domain` | -32000 | Point out of the projection's domain; `data.index` names the first bad point |
| `not_georeferenced` | -32000 | `georef.convert` before `georef.set` |
| `multiple_sites` | -32000 | Model holds more than one `Site` |

If the API has no `data.reason` convention yet, T6 adds it in a backward-compatible way and documents it in `docs/dev/api.md`.

## Changes to this contract

T6 may refine names or shapes during implementation, for example if the registry's area-of-use text is too long or the Site lookup works differently. It must tell `front` and the lead before T7 starts. The generated `protocol.gen.ts` is the final reference.
