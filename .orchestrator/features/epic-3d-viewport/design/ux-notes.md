# 3D viewport — UX notes

Wireframes: boards **3.21–3.27** on page `3 · Architecture modelling` of the design file (light only; the UI node makes the high-fidelity light/dark boards). PNG exports are next to this file. Brief: `../ux-brief.md`. Contract: `../api-contract.md` §2.

| Board | Flow | PNG |
| --- | --- | --- |
| 3.21 | 1 Navigate | `3.21-navigate.png` |
| 3.22 | 2 Select | `3.22-select.png` |
| 3.23 | 3 Place a block | `3.23-place-block.png` |
| 3.24 | 4 Move | `3.24-move.png` |
| 3.25 | 5 Clipping planes | `3.25-clip-planes.png` |
| 3.26 | 6 Section box and visibility, keyboard paths (amends 3.1, 3.2) | `3.26-keyboard-paths.png` |
| 3.27 | 7 States: empty, loading, no WebGPU, device lost | `3.27-states.png` |

## Decisions

- **Preset views: a menu, not a view cube.** The "View ▾" button in the bottom-right cluster opens Top, Front, Back, Left, Right, four isometrics, then Perspective / Parallel. A cube is pointer-only, needs its own hit-test and cannot carry French labels; a menu is keyboard and screen-reader operable, and each entry is one `view_preset` / `projection` command that can join the command palette later.
- **Bottom-right cluster** (extends 3.1), left to right: Zoom fit, Zoom to selection, View ▾, Perspective|Parallel toggle, Orbit, Pan, Full screen. Every button has an `aria-label` and a tooltip with its shortcut.
- **Block tool** sits right after Select, before Wall, in the Architecture group (tooltip "Block B").
- **Canvas** is one focusable region labelled "3D view" with a visible focus ring; Tab leaves it. Typed values therefore never use Tab: digits, `.`, Enter (confirm, go to the next value), Esc (cancel).
- **No keyboard picking in the MVP** (documented gap): keyboard users select through the project tree or schedules, which send `select(ids)`.
- **Amended boards**: in 3.1 the "Extent X/Y/Z" rows become Min/Max numeric fields (typed equivalent of each face drag); in 3.2 the popover rows become a roving-tabindex list (Space hides, Shift+Space isolates) and the hint toast gains a typed alternative. Details on board 3.26. The UI node applies them to 3.1/3.2.
- **Clipping planes** get their own View panel section right under Section box: one row per plane (name, normal label, Flip, Remove), "Add plane on face…", "Clear all"; maximum 6.

## Flows

### 1 Navigate
Orbit: right-drag, arrows. Pan: middle-drag, Shift+drag, Shift+arrows, two-finger trackpad. Zoom: wheel, pinch, `+`/`-`. `F` zoom fit, `Shift+F` zoom to selection (disabled without selection). First-time hint card in the canvas lists these; "Got it" hides it for the session, "Don't show again" stores a preference. Zoom fit on an empty scene does nothing silently.

### 2 Select
Hover: amber outline. Selected: teal outline + tint, 3 px with a contrasting halo. Click selects, Shift+click adds, Ctrl/Cmd+click toggles, click on nothing or Esc clears. Hidden or clipped-away elements are never hovered. Status bar shows "N elements selected". Hover is not announced.

### 3 Place a block
`B` → first corner (snap marker + label chip) → second corner (dashed rectangle with live "6.00 × 4.00 m") → height (pointer up/down or typed; Enter at once = 3.00 m) → block placed and selected, toast "Block placed · … · Undo". Tool stays armed; Esc or `V` returns to Select. Esc cancels one step at a time. Alt suspends snapping.

### 4 Move
Direct: drag a selected element (unselected → selected first); ghost stays at the origin. Move tool (`M`, needs a selection): base point → target point with rubber-band line and distance chip; typed distance along the current direction (`2.5`, `2500mm`), Enter confirms. Snaps: endpoint, midpoint, edge, face, ground; Alt held suspends; `S` toggles. Result toast "Moved 1 element by 2.500 m · Undo". Refused move: toast "Can't move this element yet".

### 5 Clipping planes
"Add plane on face…" arms a pick mode ("Click a face to cut at it · Esc to cancel"); click adds, plane tints cut faces like the section box. Flip, Remove, Clear all (toast with Undo that re-adds, view state only). Delete removes the focused row.

### 6 Section box and visibility
Alt+B toggles the box; Min/Max fields in the panel (Enter applies, Esc restores, min ≥ max shows "Minimum must be below maximum" and is not sent). Visibility popover: Enter/Space opens, tablist ←→, rows ↑↓ Space / Shift+Space, Esc closes and returns focus. `H`, `I`, `Alt+H` global.

### 7 States
- **Empty**: grid and axes, "Nothing to see yet — Place a block to start your model", button "Place a block B", link to the guide.
- **Loading**: spinner, "Loading the 3D view…", controls disabled; after 10 s "Still working…" + Cancel.
- **No WebGPU**: "The 3D view isn't available on this computer"; what still works (plan, sections, schedules, editing); "Open the troubleshooting guide", "Go to the plan view".
- **Device lost / scene rejected / out of memory**: "The 3D view stopped" + a one-line cause per `error.key`, "Reload view" (focused, `role="alert"`), "Copy details" (no model content). Reload = new `Viewport.create` + `scene(since: null)`. Second failure within 30 s adds "If it keeps happening, restart BIMCraft."

## Keyboard map (confirmed unless marked NEW)

| Action | Shortcut | Where |
| --- | --- | --- |
| Orbit | Arrows | canvas focus |
| Pan | Shift+Arrows | canvas focus |
| Zoom in / out | `+` / `-` | canvas focus |
| Zoom fit | `F` | canvas focus |
| Zoom to selection | `Shift+F` | canvas focus |
| Typed value | digits, `.`; Enter confirms; Esc cancels | canvas focus, during move / place |
| Leave the canvas | Tab / Shift+Tab | canvas focus |
| Cancel | Esc (step → isolate → selection) | canvas focus |
| Select tool | `V` | global |
| Move tool | `M` | global |
| Place block tool | `B` | global |
| Hide selection | `H` | global |
| Isolate selection | `I` | global |
| Reset visibility | `Alt+H` | global |
| Section box on/off | `Alt+B` | global |
| Toggle projection | `P` — **NEW** | global |
| Toggle snapping | `S` — **NEW** | global |
| Visibility popover rows | ↑ ↓ Home End, Space hide, Shift+Space isolate | popover |

Global shortcuts are quiet behind dialogs and inside text fields. `Alt+B` / `Alt+H` types a symbol on some macOS layouts inside text fields only; inside fields the shell already ignores shortcuts, so no change is proposed. Presets and Clear planes have no shortcut (menu / panel only) to keep the letter space free.

## Proposed changes to `api-contract.md` §2 (lead decides; I do not edit it)

1. **Add `P` (toggle projection) and `S` (toggle snapping)** to the global shortcuts; both map to existing `ViewCommand`s (`projection`, `snapping`). The UI keeps the current value from `ViewState.projection` and a local snapping flag shown in the status bar.
2. **Clip plane on a picked face is missing.** `clip_plane_add` needs a point and a normal that only the viewport knows (it picks on the CPU). Proposed: `ViewCommand { kind: "clip_plane_pick" }` arms a one-shot pick; the viewport adds the plane itself and reports it in `ViewState.clipPlanes`; Esc cancels. (Alternative: a `clip_plane_picked` event the UI turns into `clip_plane_add`. The first is simpler and keeps interaction in Rust, D5.) `tool_state.step` needs a `"pick_face"` value, or a new `Tool = "clip_plane"`.
3. **Add a `ViewCommand { kind: "clip_plane_flip"; index: number }`**: Flip must not be remove + add (the row would move).
4. **Add `view_state.snapping: boolean`** so the status bar shows the snap state after `S` / Alt (UI holds no state).
5. **Announcement keys to add**: `view.preset`, `view.projection`, `selection.cleared`, `visibility.hidden`, `visibility.isolated`, `visibility.reset`, `clip_plane.added`, `clip_plane.flipped`, `clip_plane.removed`, `clip_plane.cleared`, `move.cancelled`, `place.height`. Strings below.
6. **`typed_distance` for place_block** carries only one number: width, depth and height are three successive values, each confirmed with Enter (not Tab). Confirm the Rust side treats each Enter as the next step.
7. **`error.key`** gets no change; the UI maps the three keys to the strings below.

## Live-region announcements (polite, throttled to one per 500 ms; the snap kind only when it changes)

| Key | EN | FR |
| --- | --- | --- |
| `snap.endpoint` | Snap: endpoint | Accroche : extrémité |
| `snap.midpoint` | Snap: midpoint | Accroche : milieu |
| `snap.edge` | Snap: edge | Accroche : arête |
| `snap.face` | Snap: face | Accroche : face |
| `snap.ground` | Snap: ground | Accroche : sol |
| `selection.count` (0) | Selection cleared | Sélection effacée |
| `selection.count` (1) | 1 element selected | 1 élément sélectionné |
| `selection.count` (n) | {count} elements selected | {count} éléments sélectionnés |
| `move.distance` | Moved by {distance} metres | Déplacé de {distance} mètres |
| `move.cancelled` | Move cancelled | Déplacement annulé |
| `place.size` | Footprint {width} by {depth} metres | Emprise {width} par {depth} mètres |
| `place.height` | Block placed, height {height} metres | Bloc placé, hauteur {height} mètres |
| `section_box.face_moved` | Section box face moved to {value} metres | Face de la boîte de coupe déplacée à {value} mètres |
| `view.preset` | {name} view | Vue {name} |
| `view.projection` | {projection} projection | Projection {projection} |
| `visibility.hidden` | {count} elements hidden | {count} éléments masqués |
| `visibility.isolated` | {name} isolated | {name} isolé |
| `visibility.reset` | Visibility reset | Visibilité réinitialisée |
| `clip_plane.added` | Clipping plane added | Plan de coupe ajouté |
| `clip_plane.flipped` | Clipping plane flipped | Plan de coupe inversé |
| `clip_plane.removed` | Clipping plane removed | Plan de coupe supprimé |
| `clip_plane.cleared` | All clipping planes cleared | Tous les plans de coupe supprimés |

Numbers are formatted by the UI with the locale's decimal separator (FR: "2,5").

## Other strings (EN / FR)

| Where | EN | FR |
| --- | --- | --- |
| Canvas label | 3D view | Vue 3D |
| Empty title | Nothing to see yet | Rien à afficher pour l'instant |
| Empty body | Place a block to start your model. | Placez un bloc pour commencer votre modèle. |
| Empty button | Place a block | Placer un bloc |
| Loading | Loading the 3D view… | Chargement de la vue 3D… |
| Loading, slow | Still working… | Toujours en cours… |
| No WebGPU title | The 3D view isn't available on this computer | La vue 3D n'est pas disponible sur cet ordinateur |
| No WebGPU body | BIMCraft draws 3D with your graphics card through a feature called WebGPU, and this computer doesn't offer it. Plan views, sections, schedules and editing still work. | BIMCraft affiche la 3D avec votre carte graphique grâce à WebGPU, que cet ordinateur ne propose pas. Les plans, coupes, nomenclatures et l'édition restent disponibles. |
| No WebGPU buttons | Open the troubleshooting guide · Go to the plan view | Ouvrir le guide de dépannage · Aller au plan |
| Error title | The 3D view stopped | La vue 3D s'est arrêtée |
| `device_lost` | The graphics card stopped responding. Your model is safe. | La carte graphique ne répond plus. Votre modèle est intact. |
| `scene_rejected` | The view couldn't read the model data. Your model is safe. | La vue n'a pas pu lire les données du modèle. Votre modèle est intact. |
| `out_of_memory` | The model is too big for this graphics card. | Le modèle est trop volumineux pour cette carte graphique. |
| Error buttons | Reload view · Copy details | Recharger la vue · Copier les détails |
| Repeat error | If it keeps happening, restart BIMCraft. | Si le problème persiste, redémarrez BIMCraft. |
| Move toast | Moved {count} element(s) by {distance} m · Undo | {count} élément(s) déplacé(s) de {distance} m · Annuler |
| Place toast | Block placed · {w} × {d} × {h} m · Undo | Bloc placé · {w} × {d} × {h} m · Annuler |
| Move refused | Can't move this element yet | Cet élément ne peut pas encore être déplacé |
| Move with no selection | Select something to move | Sélectionnez d'abord un élément à déplacer |
| Clip pick hint | Click a face to cut at it · Esc to cancel | Cliquez sur une face pour couper à cet endroit · Échap pour annuler |
| Clip max | Maximum 6 planes | 6 plans au maximum |
| Planes cleared toast | Planes cleared · Undo | Plans supprimés · Annuler |
| Box field error | Minimum must be below maximum | Le minimum doit être inférieur au maximum |
| Section box hint | drag a handle or type values in the View panel · Shift for 100 mm steps | faites glisser une poignée ou saisissez des valeurs dans le panneau Vue · Maj pour des pas de 100 mm |
| Snap off chip | Snap off | Accroche désactivée |
| Status, selection | N elements selected · Esc to clear | N éléments sélectionnés · Échap pour effacer |

French strings run about 25 % longer than English: toasts, chips and the View panel rows must wrap or truncate with a tooltip, never clip.

## Actions list for the future command palette

Zoom fit (F), Zoom to selection (Shift+F), View: Top / Front / Back / Left / Right / Iso SE / SW / NE / NW, Projection: Perspective / Parallel (P), Select tool (V), Move tool (M), Place block tool (B), Toggle snapping (S), Hide selection (H), Isolate selection (I), End isolate (Esc), Reset visibility (Alt+H), Section box on/off (Alt+B), Fit section box to selection, Reset section box, Add clipping plane on face, Clear clipping planes.

## Accessibility checklist for the UI node

- Overlays (selection, hover, snap marker, label chips, ghost): 3:1 against any model colour via outline and halo; label chips 4.5:1 text. Do not rely on colour alone: markers differ by shape (square, triangle, line, diamond, cross).
- Viewport colours are `Mode` variables: background, selection, hover, snap, preview ghost, cut tint (sent to `setTheme` from CSS tokens).
- Reduced motion: no animated view transitions when `prefers-reduced-motion`; presets jump.
- Focus: canvas ring, panel fields, popover rows, and the "Reload view" button all show the shared `Focus/Ring` style.
