# 3D viewport — API contract

Three boundaries: the core → viewport **scene channel** (binary, D7), the **viewport module's JavaScript interface** (wasm-bindgen, D2), and one new **JSON-RPC command** (`elements.move`). Owners: backend for the Rust side of all three, frontend for the TypeScript side. A change to anything here goes through the lead first, and both sides update this file in the same task.

All lengths are metres, angles radians, coordinates the project's world frame (z up), `f64` unless stated. Ids are `EntityId` (128-bit; in JSON the canonical string form of the API, in binary 16 bytes little-endian).

## 1. Scene channel (core → viewport)

### Tauri command `scene`

- Arguments: `{ since: number | null }`: the revision of the last delta the caller applied, or `null` for a full scene. Revisions stay below 2^53 so a JavaScript `number` holds them exactly (a `bigint` cannot cross the JSON-based Tauri arguments).
- Returns: raw bytes (`tauri::ipc::Response`), an `ArrayBuffer` in JavaScript: one **scene delta** (below).
- When `since` is unknown to the host (another document, a revision it no longer keeps), the host answers with a full scene (`flags.full = 1`); the viewport then drops everything it has. The host keeps a change log of the last 256 revisions (the ids of the elements and meshes each revision added, changed or removed, never geometry) and builds the delta from the current scene: `since` equal to the current revision gets an empty delta, a `since` inside the log gets the union of the changes after it, and any other `since` (older than the log, or unknown) gets a full scene. Without the log every edit would resend the whole scene, defeating GEO-05 and NFR-03.
- The UI calls it at start (`since: null`) and after each `model.changed` notification, serialised: one call in flight at a time; a notification during a call schedules one more call afterwards.
- Errors: a JSON-RPC-style error string for a closed document; never coordinates or model content.

### Binary format `scene-wire` version 1

Little-endian throughout. Every section length is checked against the buffer before reading; the decoder rejects trailing bytes, unknown versions, counts above the limits, non-finite floats, out-of-range indices, and values outside the ranges of ADR 0020 (`MAX_WORLD_COORDINATE` = 10⁹ m, unit normals, proper orthonormal rotations, mesh bounds that enclose every vertex of the mesh). Limits (constants in `scene-wire`, tested at the boundary): 256 MiB per delta, 2²⁴ vertices and 3·2²⁴ indices per mesh (as `render`), 2²⁰ meshes, 2²² elements, 2¹⁶ strings of at most 256 bytes (UTF-8).

```
Header (32 bytes)
  magic        [u8; 4]   "BCSW"
  version      u16       1
  flags        u16       bit 0: full (drop everything before applying)
  revision     u64       the host's scene revision after this delta
  base         u64       the revision this delta applies to (0 when full)
  reserved     u64       0

Strings section
  count        u32
  per string:  len u16, bytes [u8; len]          // category and discipline names

Meshes section (added meshes; a mesh is immutable once sent)
  count        u32
  per mesh:
    id         u64       content hash chosen by the host; unique per document
    origin     [f64; 3]  ADR 0020: vertices are relative to it
    vertex_count u32
    index_count  u32     multiple of 3
    positions  [f32; 3 * vertex_count]
    normals    [f32; 3 * vertex_count]   unit length (flat shading)
    indices    [u32; index_count]
    bounds_min [f32; 3]  local bounding box, for culling and LOD; the decoder checks it encloses every position
    bounds_max [f32; 3]

Removed meshes
  count        u32
  ids          [u64; count]                // no element refers to them after this delta

Elements section (added or changed elements)
  count        u32
  per element:
    id         [u8; 16]
    mesh       u64       a mesh id sent in this delta or earlier
    rotation   [f64; 9]  column-major, orthonormal, determinant +1 (a rigid placement, D3)
    translation [f64; 3]
    category   u32       index into the strings section
    discipline u32       index into the strings section

Removed elements
  count        u32
  ids          [[u8; 16]; count]
```

Order of application: strings, removed elements, removed meshes, meshes, elements. An element that failed to mesh (`SceneUpdate::failed`) is sent as removed; the failure itself is reported elsewhere later (not in this feature). Meshes are deduplicated by the host (D3); a mesh no element uses any more is removed in the same delta.

`scene-wire` exposes `SceneDelta` (owned, plain Rust types), `encode(&SceneDelta) -> Vec<u8>` and `decode(&[u8]) -> Result<SceneDelta, WireError>`. The fixtures for the UI tests and e2e (`ui/e2e/fixtures/scene-*.bin`, `ui/src/test-support/`) are written by a Rust test from known scenes and checked to be up to date (`UPDATE_SNAPSHOTS=1` regenerates them).

## 2. Viewport module (wasm-bindgen, `ui/src/viewport/pkg/`)

Generated by `scripts/build-viewport.sh`; the TypeScript side wraps it in `ui/src/viewport/` and never imports `pkg/` elsewhere. Names are fixed here; argument objects are plain JSON-compatible values validated on the Rust side (hostile input: a bad call returns an error, never panics).

```ts
// Loading
export default function init(): Promise<void>;          // wasm-bindgen's init, called once, lazily
export function webgpuAvailable(): Promise<boolean>;    // false → the UI shows the no-WebGPU state

export class Viewport {
  /** Attaches to the canvas (owns its pointer, wheel and key listeners and its frame loop). Rejects when no adapter. */
  static create(canvas: HTMLCanvasElement, options: ViewportOptions): Promise<Viewport>;
  /** Detaches listeners, stops the loop, frees GPU resources. */
  free(): void;

  /** Applies one scene delta (bytes from the `scene` command). Returns the revision now held. */
  applyScene(bytes: Uint8Array): number;
  /** The revision held, or null before the first delta (the `since` of the next `scene` call). */
  revision(): number | null;

  /** Canvas size changes (CSS pixels and device pixel ratio). */
  resize(width: number, height: number, devicePixelRatio: number): void;
  /** Theme colours (background, selection, hover, snap, cut tint) from the CSS tokens. */
  setTheme(theme: ViewportTheme): void;

  /** View commands, from buttons, menus and shortcuts outside the canvas. */
  command(command: ViewCommand): void;
  /** The tool the pointer drives in the canvas. */
  setTool(tool: Tool): void;

  /** Called with every event; one listener (the React wrapper fans out). */
  onEvent(listener: (event: ViewportEvent) => void): void;
}

type ViewportOptions = { theme: ViewportTheme };      // announcements are keys: the UI translates them
type ViewportTheme = { background: string; selection: string; hover: string; snap: string; preview: string; cut: string }; // "#RRGGBB"

type Tool = "select" | "move" | "place_block";

type ViewCommand =
  | { kind: "zoom_fit" } | { kind: "zoom_selection" }
  | { kind: "view_preset"; preset: "top" | "front" | "back" | "left" | "right" | "iso_se" | "iso_sw" | "iso_ne" | "iso_nw" }
  | { kind: "projection"; projection: "perspective" | "parallel" }
  | { kind: "select"; ids: string[] } | { kind: "clear_selection" }
  | { kind: "cancel" }                                     // Esc: cancels the tool step, then ends isolate, then clears selection
  | { kind: "typed_distance"; metres: number }             // during move / place_block
  | { kind: "snapping"; enabled: boolean }
  // #72 (B8):
  | { kind: "section_box"; enabled: boolean }
  | { kind: "section_box_extents"; min: [number, number, number]; max: [number, number, number] }
  | { kind: "section_box_fit_selection" } | { kind: "section_box_reset" }
  | { kind: "clip_plane_pick" }                           // arms a one-shot face pick; the viewport adds the plane itself; Esc cancels
  | { kind: "clip_plane_add"; point: [number, number, number]; normal: [number, number, number] }
  | { kind: "clip_plane_flip"; index: number }
  | { kind: "clip_plane_remove"; index: number } | { kind: "clip_planes_clear" }
  | { kind: "cut_faces"; style: "tinted" | "none" }
  | { kind: "hide"; target: VisibilityTarget } | { kind: "show"; target: VisibilityTarget }
  | { kind: "isolate"; target: VisibilityTarget } | { kind: "end_isolate" } | { kind: "reset_visibility" };

type VisibilityTarget =
  | { by: "category"; names: string[] } | { by: "discipline"; names: string[] } | { by: "selection" };

type ViewportEvent =
  | { kind: "selection"; ids: string[] }
  | { kind: "hover"; id: string | null }
  | { kind: "tool_state"; tool: Tool; step: "idle" | "base_point" | "target_point" | "first_corner" | "second_corner" | "height" | "pick_face" }
  | { kind: "announce"; key: AnnouncementKey; values: Record<string, string | number> } // UI translates the key (UI-04)
  | { kind: "intent"; intent: Intent }
  | { kind: "view_state"; state: ViewState }              // after every view command or handle drag (throttled)
  | { kind: "error"; key: "device_lost" | "scene_rejected" | "out_of_memory" };

type AnnouncementKey =
  | "snap.endpoint" | "snap.midpoint" | "snap.edge" | "snap.face" | "snap.ground"
  | "selection.count" | "selection.cleared"
  | "move.distance" | "move.cancelled" | "place.size" | "place.height"
  | "view.preset" | "view.projection"
  | "visibility.hidden" | "visibility.isolated" | "visibility.reset"
  | "clip_plane.added" | "clip_plane.flipped" | "clip_plane.removed" | "clip_plane.cleared"
  | "section_box.face_moved";
// Values are raw numbers (metres) and names; the UI formats them for the locale (FR "2,5").
// The viewport throttles announcements (one per 500 ms of frames; the snap kind only when it changes).
// EN/FR strings: design/ux-notes.md.

type Intent =
  | { kind: "move"; ids: string[]; translation: [number, number, number] }
  | { kind: "place_block"; location: [number, number, number]; rotation: number; size: [number, number, number] };

type ViewState = {
  projection: "perspective" | "parallel";
  snapping: boolean;                       // false after `S` or while Alt is held
  tool: Tool;
  categories: { name: string; count: number; hidden: boolean; isolated: boolean }[];
  disciplines: { name: string; count: number; hidden: boolean; isolated: boolean }[];
  isolating: boolean;
  hiddenSelection: number;                 // elements hidden by "hide selection"
  sectionBox: { enabled: boolean; min: [number, number, number]; max: [number, number, number] } | null;
  clipPlanes: { point: [number, number, number]; normal: [number, number, number] }[];
  cutFaces: "tinted" | "none";
};
```

Keyboard inside the focused canvas (handled in Rust; the UI documents them and shows them in tooltips): arrows orbit, Shift+arrows pan, `+`/`-` zoom, `F` zoom fit, `Shift+F` zoom to selection, `Esc` cancel, digits and `.` start a typed distance, `Enter` confirms, `Tab`/`Shift+Tab` leave the canvas (never trapped). Placing a block takes three successive typed values (width, depth, height), each confirmed with `Enter` (each `typed_distance` or Enter advances one step; never Tab). Global shortcuts (outside the canvas too, handled by the UI and sent as commands): `V` select tool, `M` move, `B` place block, `H` hide selection, `I` isolate selection, `Alt+H` reset visibility, `Alt+B` section box on/off, `P` toggle projection, `S` toggle snapping. Confirmed by the UX notes (`design/ux-notes.md`, keyboard map); the shell's shortcut rules (quiet behind dialogs) apply.

## 3. JSON-RPC command `elements.move`

Registered in `crates/api` (descriptor, `api.schema.json`, `protocol.gen.ts` regenerated, conformance tests).

```
elements.move
  params: { ids: EntityId[], translation: [number, number, number] }
  result: null
  errors: refused { reason: "unknown_element" | "not_movable" | "out_of_range" | "empty" | "too_many" }
```

- One transaction (undoable as one step, CORE-06), one `model.changed` notification. Its history name is `batch`: core-txn names a transaction after its command type, and `elements.move` is a batch of `set` commands, as `model.create` is `create` ("Place block" has no name of its own either). The UI words its own undo toast.
- `ids`: 1 to 10 000 elements (more: refused `too_many`; the UI shows its own string, it never splits a move), each with a placement the command knows how to translate (`Block` now; walls and the rest join as they arrive). Any invalid id refuses the whole command, nothing changes. **Duplicate ids are invalid params** (JSON-RPC -32602), not a refusal.
- `translation`: finite, each component within ±10⁶ m; the resulting location within `MAX_WORLD_COORDINATE`. Either violation is refused `out_of_range`. A refusal is a JSON-RPC error with code -32000 and `data: { reason }`.
- Placing a block uses the existing `model.create` with entity `Block` and its attributes, named by the caller's `id`. Attribute encoding (schema version 4): `location` is a list of exactly three lengths `[x, y, z]` in metres, the **centre of the base**, in the project's world frame (values go through `model.create`'s `values` as a JSON list of numbers); `rotation` a plane angle in radians about z (counter-clockwise from above, default 0); `size_x`, `size_y`, `size_z` positive lengths in metres (x and y along the block's own axes, z the height). The schema cannot say "exactly three" or "positive": the generator draws nothing (the element fails) otherwise. The `Block`'s representation is context `body`, lod 300.
