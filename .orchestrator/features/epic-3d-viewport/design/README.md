# 3D viewport — high-fidelity boards (UI)

These are the high-fidelity boards for the 3D viewport epic. Start with the brief (`../ux-brief.md`), the UX notes (`ux-notes.md`, with flows, keyboard map and EN/FR strings) and the contract (`../api-contract.md` §2). The UX wireframes are the `3.2x-*.png` files with no `-light`/`-dark` suffix.

The boards are in the project design file, on page `3 · Architecture modelling`, under the UX wireframes:

- light boards: row `y = 2200`, from `x = 30400`, 1520 apart;
- dark boards: row `y = 3300`, same columns. They are copies of the light boards with the `Mode` collection set to **Dark**, so every colour comes from a variable.

All strings in the new boards are French (UI-04). The French strings come from `ux-notes.md`, and the strings it doesn't list are in the [French strings](#french-strings-added-by-the-ui-boards) table below. Boards 3.1 and 3.2 keep their English strings and are amended in place.

## Boards → flows → files

Each board is exported twice: `<file>-light.png` and `<file>-dark.png`.

| Board | Flow (brief) | What it shows | File |
| --- | --- | --- | --- |
| 3.21 | 1 Navigate | Canvas focus ring ("Vue 3D"), view-controls cluster, **View presets** menu open (Iso sud-ouest has keyboard focus; Perspective is ticked; P is shown on both projection items), tooltip "Tout cadrer F", first-time hint card ("J’ai compris" / "Ne plus afficher") | `3.21-navigate` |
| 3.22 | 2 Select | Two blocks selected (outline, halo and tint), one hovered (amber outline), hover name chip, status "2 éléments sélectionnés · Échap pour effacer" | `3.22-select` |
| 3.23a | 3 Place a block | Block tool active: first corner placed, footprint preview, **Sol** (ground) snap marker and chip, live size "6,00 × 4,00 m", typed field "Longueur 6 m ↵" | `3.23a-place-block-corners` |
| 3.23b | 3 Place a block | Height step: ghost box, height handle, live chip, typed field "Hauteur 3 m ↵" | `3.23b-place-block-height` |
| 3.23c | 3 Place a block | Block placed and selected, toast with undo "Bloc placé · 6,00 × 4,00 × 3,00 m · Annuler"; the tool stays armed | `3.23c-place-block-placed` |
| 3.24a | 4 Move | Move tool: base point → target point, rubber band, origin ghost, typed field "Distance 2,5 m ↵", live chip "2,500 m" | `3.24a-move-tool-typed` |
| 3.24b | 4 Move | Direct drag with Select: **Milieu** (midpoint) snap marker and chip, origin ghost, live distance "4,000 m" | `3.24b-move-drag-snap` |
| 3.24c | 4 Move | Result: toast with undo "1 élément déplacé de 2,500 m · Annuler" | `3.24c-move-done-undo` |
| 3.25a | 5 Clipping planes | Pick mode: hint "Cliquez sur une face pour couper à cet endroit · Échap pour annuler", face preview with **Face** marker, pressed pick button (label superseded by rule 7), plane 1 already cutting (tinted cut face, dashed plane outline, arrow showing the normal) | `3.25a-clip-plane-pick` |
| 3.25b | 5 Clipping planes | Two planes in the View panel ("2 sur 6"), one row per plane (Flip, Remove), keyboard focus on Plan 2, "Ajouter un plan sur une face", "Tout supprimer" | `3.25b-clip-planes-panel` |
| 3.26 | 6 Section box and visibility | Section box with handles (+X moved, value chip), **Min/Max fields** in the View panel (Max X focused, Min Z in error: "Le minimum doit être inférieur au maximum"), visibility popover operable from the keyboard (focused row, tablist, shortcut footer) | `3.26-section-box-visibility-keyboard` |
| 3.27a | 7 States | Empty project: grid, axes, "Rien à afficher pour l’instant", "Placer un bloc B", link to the guide | `3.27a-state-empty` |
| 3.27b | 7 States | Loading, slow (after 10 s): spinner, "Toujours en cours…", indeterminate bar, "Annuler", controls disabled | `3.27b-state-loading` |
| 3.27c | 7 States | No WebGPU: plain-language card with only "Ouvrir le guide de dépannage" (no plan view, section or schedule mentioned); tool bar disabled | `3.27c-state-no-webgpu` |
| 3.27d | 7 States | Device lost: alert card, "Recharger la vue" (focused), "Copier les détails", and the second-failure line "Si le problème persiste, redémarrez BIMCraft."; tool bar disabled | `3.27d-state-device-lost` |
| 3.28 | All (spec) | Overlay spec: the 5 snap markers on light, dark, dark-material and saturated backgrounds; selection, hover, ghost and cut samples; chips; typed field; every toast (move refused, move without selection, planes cleared with no Undo, 6-plane limit); swatches of the viewport tokens | `3.28-overlays-tokens` |
| 3.1 (amended) | 6 Section box | See [Changes to 3.1 and 3.2](#changes-to-31-and-32) | `3.1-section-box-amended` |
| 3.2 (amended) | 6 Visibility | See [Changes to 3.1 and 3.2](#changes-to-31-and-32) | `3.2-visibility-isolate-amended` |

## New `Mode` variables (viewport colours)

They are in the `Mode` collection, so each one has a Light and a Dark value. The CSS names are those of `variable.export` (format `css`); the frontend reads them as tokens. Only six of them fit `setTheme` today (see [Open points](#open-points-for-the-frontend-not-blocking)).

| Variable | CSS custom property | Light | Dark | Use |
| --- | --- | --- | --- | --- |
| `color/vp-bg` | `--color-vp-bg` | `#F4F5F6` | `#16181B` | Canvas background (clear colour) |
| `color/vp-shade-1` | `--color-vp-shade-1` | `#FFFFFF` | `#4A4F57` | Default material, faces lit most (tops) |
| `color/vp-shade-2` | `--color-vp-shade-2` | `#DCDFE3` | `#383C43` | Default material, faces lit less |
| `color/vp-shade-3` | `--color-vp-shade-3` | `#C3C8CE` | `#2B2F35` | Default material, faces lit least |
| `color/vp-select` | `--color-vp-select` | `#0F6B5E` | `#4DB6A3` | Selection outline (3 px), selected rows, plane outline, section box |
| `color/vp-select-tint` | `--color-vp-select-tint` | `#0F6B5E` at 20 % (`rgba(15,107,94,0.20)`) | `#4DB6A3` at 25 % | Fill over selected elements |
| `color/vp-hover` | `--color-vp-hover` | `#B25E00` | `#F5B04A` | Hover outline (3 px), face picked for a clipping plane |
| `color/vp-snap` | `--color-vp-snap` | `#B4137B` | `#FF7AD0` | Snap markers (3 px stroke) |
| `color/vp-halo` | `--color-vp-halo` | `#FFFFFF` | `#0B0C0E` | Halo under every overlay line and marker (7 px under a 3 px line) and marker fill |
| `color/vp-ghost` | `--color-vp-ghost` | `#5C6470` | `#9AA3AE` | Dashed preview outline (1.5 px, dashes 6/4), origin ghost while moving |
| `color/vp-ghost-tint` | `--color-vp-ghost-tint` | `#0F6B5E` at 12 % | `#4DB6A3` at 15 % | Fill of the preview ghost and of the footprint |
| `color/vp-cut` | `--color-vp-cut` | `#B4410A` | `#FDBA74` | Solid fill of cut faces (section box and clipping planes); a warm hue so a cut face never reads as selected |
| `color/vp-chip` | `--color-vp-chip` | `#1B1D21` | `#E6E7E9` | Background of snap-label, live-size and hover-name chips |
| `color/vp-chip-ink` | `--color-vp-chip-ink` | `#FFFFFF` | `#1B1D21` | Chip text (12 px, Medium) |

The ground grid uses the existing `color/grid` variable at 45 % opacity. The canvas focus ring is a 2 px inside stroke in `color/acc`, and the fields, rows and buttons use the shared `Focus/Ring` effect style.

### Contrast (WCAG 2.2 AA)

| Pair | Light | Dark |
| --- | --- | --- |
| Selection line on halo | 6.4:1 | 8.0:1 |
| Hover line on halo | 4.7:1 | 10.4:1 |
| Snap marker on halo | 6.3:1 | 8.3:1 |
| Ghost line on halo | 6.0:1 | 7.7:1 |
| Chip text on chip | 16.9:1 | 13.6:1 |
| Cut fill on the closest model shade | 3.4:1 (`vp-shade-3`) | 4.9:1 (`vp-shade-1`) |

Every line and marker sits on its halo, so it keeps at least 4.7:1 against its adjacent colour (target 3:1, SC 1.4.11) whatever the model colour. Board 3.28 shows this on a dark material (`#2B2F35`) and a saturated one (`#C2410C`). Meaning never depends on colour alone: snap markers differ by shape (endpoint = square, midpoint = triangle, edge = short line, face = diamond, ground = cross), and every marker has a label chip.

## New icon masters (page `Icons`)

| Component | Size | Used for |
| --- | --- | --- |
| `Icons / Tool / Block` | 20 | Block tool |
| `Icons / Tool / Move` | 20 | Move tool |
| `Icons / UI / Zoom to selection` | 16 | View controls (Shift+F) |
| `Icons / UI / Full screen` | 16 | View controls |
| `Icons / UI / View presets` | 16 | View presets menu button |
| `Icons / UI / Flip` | 16 | Flip a clipping plane |
| `Icons / UI / Clip plane` | 16 | Clipping-plane rows, "Ajouter un plan sur une face" |
| `Icons / UI / Warning` | 16 | No-WebGPU card |
| `Icons / UI / Snap` | 16 | Snapping state (for the status bar and the future command palette) |

The boards reuse the existing Brand guide components: `UI / Toast with undo`, `UI / Segmented toggle` (projection, visibility tablist), `UI / Checkbox`, and the icons `Check`, `Copy`, `Error`, `Plus`, `Remove`, `Expand`, `Collapsed`, `Visibility`, `Enter key`, `Orbit`, `Pan`, `Zoom to fit`.

## Changes to 3.1 and 3.2

3.1 (section box), light and dark:

1. "Shaded with edges" → **"Shaded"** (spec, Out of scope: no edges in this feature).
2. The "Extent X/Y/Z" rows are replaced by **Min/Max numeric fields** (X, Y, Z; the board shows mm, superseded by rule 1: metres), the typed equivalent of each handle drag. Max X has focus, and the help line reads "Enter applies · Esc restores · min must be below max".
3. Hint toast → "Section box · drag a handle or type values in the View panel · Shift for 100 mm steps".
4. Tool bar: **Move** and **Block** added right after Select (see the decisions below).
5. View controls: the new cluster replaces the old one (see the decisions below).
6. The cut face takes `vp-cut` (it used `acc-tint`, the selection colour).

3.2 (visibility and isolate), light and dark:

1. "Shaded with edges" → **"Shaded"**.
2. Popover: the **Columns** row has the keyboard focus ring (roving tabindex), and a footer gives the keys (full list in rule 8).
3. Tool bar: **Move** and **Block** added after Select.
4. View controls: new cluster.

## UI decisions beyond the UX notes

- **Move tool in the tool bar.** UX gives Move a shortcut (`M`) but no place. Select, Move and Block now open the tool bar, followed by the Architecture tools (Wall, Slab…). UX asked for Block right after Select; it now comes right after Move, still before Wall, and Move sits with Select because both apply in every discipline. Block is therefore outside the Architecture group.
- **View-controls cluster** (bottom right), left to right: Zoom fit `F`, Zoom to selection `Shift+F`, View presets ▾ (icon only, tooltip and `aria-label` "Vues prédéfinies"), Perspective | Parallèle segmented toggle `P`, Orbit, Pan, Full screen. The old "1:100" scale is gone, because a perspective 3D view has no drawing scale. Zoom fit and Zoom to selection are disabled on an empty scene (3.27a, see rule 4).
- **View panel order:** Caméra (Projection, Vue depuis), Boîte de coupe, Plans de coupe, Visibilité. A collapsed section shows its state on the right ("Désactivée", "Aucun", "2 sur 6", "Tout est visible"), and the title tag names the active cut ("Boîte de coupe active", "Plans de coupe").
- **Hover name chip:** the hovered element's name is shown next to the pointer (3.22), in the chip style. The screen reader still doesn't hear hover (UX).
- **Clipping planes on the canvas:** each plane is drawn as a dashed outline in `vp-select`, with an arrow showing its normal; the face it cuts takes the `vp-cut` fill. The Flip and Remove buttons are icon-only and labelled "Inverser" and "Supprimer".
- **Loading:** the spinner stops spinning when `prefers-reduced-motion` is set (the text and the bar remain). The model behind it is dimmed to 20 %.
- **Device lost:** no last frame is kept (rule 3); the card has `role="alert"` and focus moves to "Recharger la vue".

## French strings added by the UI boards

| Where | EN | FR |
| --- | --- | --- |
| Hint card title | Moving around the model | Se déplacer dans le modèle |
| Hint card rows | Orbit / Pan / Zoom / Fit all / Fit selection / Trackpad | Orbite / Panoramique / Zoom / Tout cadrer / Cadrer la sélection / Pavé tactile |
| Hint card buttons | Got it · Don’t show again | J’ai compris · Ne plus afficher |
| Presets menu | View presets · Top, Front, Back, Left, Right, Iso south-east… | Vues prédéfinies · Dessus, Avant, Arrière, Gauche, Droite, Iso sud-est, Iso sud-ouest, Iso nord-est, Iso nord-ouest |
| Projection | Perspective · Parallel | Perspective · Parallèle |
| Tool bar | Select · Move · Block | Sélection · Déplacer · Bloc |
| Display style | Shaded | Ombré |
| Snap chips | Endpoint · Midpoint · Edge · Face · Ground | Extrémité · Milieu · Arête · Face · Sol |
| Typed field labels | Length · Height · Distance | Longueur · Hauteur · Distance |
| Place hints | Block · click the second corner or type the length, Enter · Alt: no snapping · Esc: previous step | Bloc · cliquez le second angle ou saisissez la longueur, Entrée · Alt : sans accroche · Échap : étape précédente |
| | Block · move the pointer up or type the height, Enter (3.00 m by default) · Esc: previous step | Bloc · déplacez le pointeur vers le haut ou saisissez la hauteur, Entrée (3,00 m par défaut) · Échap : étape précédente |
| Move hints | Move · click the target point or type a distance, Enter · Alt: no snapping · Esc: cancel | Déplacer · cliquez le point d’arrivée ou saisissez une distance, Entrée · Alt : sans accroche · Échap : annuler |
| | Drag to move · hold Alt: no snapping · Esc: cancel | Glisser pour déplacer · Alt maintenu : sans accroche · Échap : annuler |
| Clip planes panel | Clipping planes · Plane 1 · Normal +X · Add plane on face · Clear all · 2 of 6 | Plans de coupe · Plan 1 · Normale +X · Ajouter un plan sur une face · Tout supprimer · 2 sur 6 |
| Section box panel | Active · Cut faces · Tinted · Fit to selection · Reset · Turn off | Active · Faces coupées · Teintées · Ajuster à la sélection · Réinitialiser · Désactiver |
| Section box help | Metres · Enter applies · Esc restores · Tab: next field | Mètres · Entrée applique · Échap rétablit · Tab : champ suivant |
| Popover footer | ↑ ↓ Home End move · ← → tabs · Space hide · Shift+Space isolate · Esc close | ↑ ↓ Home End parcourir · ← → onglets · Espace masquer · Maj+Espace isoler · Échap fermer |
| Popover tags | Isolated · Hidden | Isolé · Masqué |
| Status bar | No selection | Aucune sélection |
| | No elements | Aucun élément |
| | Loading… | Chargement… |
| | 3D view unavailable | Vue 3D indisponible |
| | 3D view stopped | Vue 3D arrêtée |
| | 2 clipping planes active | 2 plans de coupe actifs |
| | Block · second corner · Snap on | Bloc · second angle · Accroche activée |
| | Block · height · footprint {w} × {d} m | Bloc · hauteur · emprise {w} × {d} m |
| | Clipping plane · choose a face | Plan de coupe · choisir une face |
| | Section box · 1 face moved | Boîte de coupe · 1 face déplacée |
| | 1 element selected · Snap on | 1 élément sélectionné · Accroche activée |
| Empty state link | Guide: place a block → | Guide : placer un bloc → |
| Canvas chips | Height {h} m | Hauteur {h} m |
| | Face · horizontal plane at {z} m | Face · plan horizontal à {z} m |
| | Max X {value} m | Max X {value} m |
| Clip plane row detail | Normal +X · x = {value} m | Normale +X · x = {value} m |
| Clip pick cancel | Cancel Esc | Annuler Échap |
| View panel title tags | Section box on · Clipping planes · Isolated | Boîte de coupe active · Plans de coupe · Isolé |
| View panel collapsed states | Off · None · {n} of 6 · Everything visible | Désactivée · Aucun · {n} sur 6 · Tout est visible |

## Rules for the frontend (lead decisions; they override the boards)

1. **Units.** The viewport UI shows metres everywhere, with 3 decimals and the locale decimal separator, including the section-box Min/Max fields. The mm values in 3.1 are superseded.
2. **Reference board.** 3.26 is the reference for the View panel layout and the projection label. 3.1 (amended) only illustrates the section box itself.
3. **Device lost (3.27d).** No dimmed last frame: the canvas shows only the background and grid behind the card.
4. **Empty scene (3.27a).** Zoom fit (`F`) is disabled (`aria-disabled`), like Zoom to selection; this supersedes the UX note.
5. **Typed fields (3.24a).** The hint bar never covers the typed Distance field: lay the field out above or beside the hint.
6. **Snapping while moving (3.24b).** The snap target on the board is illustrative. The viewport never snaps to the elements being moved.
7. **Pick button (3.25a).** One label in both states, "Ajouter un plan sur une face" / "Add plane on face"; the pressed state is `aria-pressed="true"` plus the pressed styling. The board's "Choisir une face…" label is superseded.
8. **Popover keys (3.26).** The footer also lists Home/End and ←/→ for the tabs: "↑ ↓ Home End parcourir · ← → onglets · Espace masquer · Maj+Espace isoler · Échap fermer" / "↑ ↓ Home End move · ← → tabs · Space hide · Shift+Space isolate · Esc close".
9. **Tool bar without a 3D view.** No modelling tools without a viewport: the tool bar is disabled while loading, with no WebGPU, and after the device is lost (3.27b–d).
10. **Single-key shortcuts can be turned off (WCAG 2.1.4, AA).** Add a checkbox "Single-key shortcuts" / "Raccourcis à une touche", checked by default, next to the existing language choice. It persists like the language override (`localStorage`, same `bimcraft.` key prefix, guarded with try/catch, default on when storage is unavailable). When it is off, V, M, B, H, I, P and S do nothing outside the focused canvas. Modified shortcuts (Alt+H, Alt+B) and the keys inside the focused canvas still work, because those are active only on focus. Tooltips and the shortcut footer show a single-key shortcut only while it is on. The user docs say how to turn them off.
11. **Block values.** Rule 1 applies to block sizes too: "6,000 × 4,000 × 3,000 m", "Hauteur 3,000 m". The boards' 2 decimals are superseded.
12. **Block footprint labels.** The typed value is named from `preview_label.key` (api-contract.md §2): `place.size` → "Width" / "Largeur", `place.depth` → "Depth" / "Profondeur", `place.height` → "Height" / "Hauteur". The hint bar uses the same word.

`typos` flags the French text in this folder when pointed at it directly; that is accepted (CI skips `.orchestrator/`).

## Open points for the frontend (not blocking)

- **No keyboard picking** in the MVP (UX gap): keyboard users select from the project tree. Nothing in the canvas shows a keyboard cursor.
- The status bar shows the snapping state ("Accroche activée", 3.23a, 3.24a, 3.24b) from `view_state.snapping`, which is in `api-contract.md` §2; the `Snap` icon is ready for a status-bar snapping toggle.
- **`setTheme` carries fewer colours than the boards use.** `ViewportTheme` has six `"#RRGGBB"` fields: `background` ← `vp-bg`, `selection` ← `vp-select`, `hover` ← `vp-hover`, `snap` ← `vp-snap`, `preview` ← `vp-ghost`, `cut` ← `vp-cut`. It has no slot for `vp-shade-1/2/3` (default material, which the dark boards change), `vp-halo`, `vp-select-tint` or `vp-ghost-tint`, and no alpha for the tints (`vp-select-tint`, `vp-ghost-tint`; `vp-cut` is now solid). Until the lead extends the contract, the dark boards' material and halo cannot be rendered as drawn.
- **Block footprint labels.** Settled by rule 12.
