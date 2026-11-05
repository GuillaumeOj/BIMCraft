# Georeferencing — UI design (sub-issue #83, GEOREF-04)

High-fidelity boards for the project location dialog (F1) and the Coordinates panel (F2). They live in the project design file `docs/BIMCraft.figcraft` (Git LFS), page **"2 · Site and georeferencing"**, after the existing boards 2.1–2.3. Each board is the full 1440 × 900 shell, in a light board (top row) and a dark board (bottom row, `Mode` = Dark). `ux-notes.md` is the text twin of the UX wireframes, with the English copy. The strings in the boards are French (about 30 % longer than English).

Open the file in FigCraft or over MCP: see `docs/dev/ui-design.md` on main.

## Boards → images

Images are 2× PNG exports of the light boards, plus two dark samples.

| Board (page "2 · Site and georeferencing", `… · light` / `… · dark`) | Image | Notes |
| --- | --- | --- |
| 2.4 Project location — not georeferenced | `location-dialog-empty.png` | Intro, CRS combobox focused (empty), sections 2–3, Advanced and Survey point disabled until a CRS is chosen, true north "–", derived info placeholder, no "Remove" |
| 2.5 Project location — georeferenced | `location-dialog-filled.png`, `location-dialog-filled-dark.png` | Filled from `georef.get`; Advanced closed, survey point open with its model position |
| 2.6 Project location — field errors | `location-dialog-field-error.png` | Non-numeric easting and scale ≤ 0; Easting has the focus (first invalid field); Advanced opened by the scale error |
| 2.7 Project location — outside the CRS area | `location-dialog-outside-area.png` | `outside_domain`: group error on Easting/Northing, focus on Easting; the message never repeats the values (SEC-03) |
| 2.8 Project location — server error | `location-dialog-server-error.png` | Banner `role="alert"` at the top of the body; focus stays on Apply |
| 2.9 Project location — remove confirmation | `location-dialog-remove-confirm.png` | Inline confirmation replaces the footer; form inert (45 % opacity); default focus on "Conserver la géoréférence" |
| 2.10 Coordinates panel — not georeferenced | `coordinates-panel-not-georeferenced.png` | Explanation + "Set project location…" button (`georef.panel.setLocation`, opens F1) |
| 2.11 Coordinates panel — georeferenced | `coordinates-panel-filled.png` | Origin (0, 0, 0) on open; tooltip of the disabled "Pick from view" shown |
| 2.12 Coordinates panel — editing | `coordinates-panel-editing.png`, `coordinates-panel-editing-dark.png` | Map block is the source: "Modifié" badge, 2 px accent rule, focus; other blocks "Calculé depuis Carte" |
| 2.13 Coordinates panel — outside the CRS area | `coordinates-panel-outside-area.png` | `outside_domain` from `georef.convert`: the other blocks show "—" and the message |
| Brand guide › 8 · Form and dialog components | `ui-components.png` | The new components, all variants (1×) |

The dialogs sit on the shell with the 18 % black scrim of board 2.2. The panel replaces the Properties panel in the right dock (296 px). Focus uses the **Focus/Ring** effect style: a 2 px `color/acc` ring outside a 2 px `color/surface` gap, as `:focus-visible` in `theme.css`.

## Variables

The boards use the file's `Mode` (Light/Dark) and `Core` collections: `color/paper`, `surface`, `raised`, `hover`, `line`, `ink`, `ink2`, `muted`, `acc`, `on-acc`, `err`. Errors and destructive actions use the existing **`color/err`** (`#B42318` / `#F28B82`); UX's proposed `color/danger` is not added. Added to `Mode`:

| Variable | Light | Dark | Why |
| --- | --- | --- | --- |
| `color/field-line` | `#858B94` | `#7A8089` | Borders of text fields, the select and the selected toggle segment: `line` is only 1.46:1, below the 3:1 of WCAG 1.4.11 |
| `color/on-err` | `#FFFFFF` | `#1C1E21` | Text on the solid danger button |

Contrast, computed with `ui/src/shell/contrast.ts` (≥ 4.5:1 for text, ≥ 3:1 for field borders):

| Pair | Light | Dark |
| --- | --- | --- |
| err on surface / raised / paper | 6.57 / 6.02 / 6.35 | 6.18 / 5.47 / 6.99 |
| on-err on err | 6.57 | 6.99 |
| on-acc on acc | 6.39 | 7.52 |
| muted on surface / raised | 5.86 / 5.37 | 5.60 / 4.96 |
| field-line on surface / raised | 3.43 / 3.14 | 3.71 / 3.28 |

Do not put `muted` text on `hover` in dark mode (4.34:1). Effect styles added: **Shadow/Raised** (0 4 16, 10 % black, as `--bc-shadow`) and **Focus/Ring**.

## Components

New components are in **Brand guide › 8 · Form and dialog components**. The boards also use the file's `UI / Button primary`, `UI / Button secondary`, `UI / Button ghost` and the `Icons / UI / Expand`, `Collapsed`, `Search` and `More` icons.

| Component | Variants | Use |
| --- | --- | --- |
| UI / Text field | State = Default, Focus, Error, Disabled, Read-only | Label above (wraps, never truncates), info button, unit suffix inside, inline error with icon (`aria-describedby`, `aria-invalid`) |
| UI / Search select | State = Filled, Empty, Focus, Error, Open | CRS combobox, area of use under it; Open shows the listbox (a popover in code) with a result count |
| UI / List option | State = Default, Active, Selected | Code, name, area of use; Active = `aria-activedescendant` |
| UI / Button danger | Kind = Danger, Danger quiet | Confirm Remove; "Supprimer la géoréférence" in the footer |
| UI / Section header | – | Numbered section title + info button; labels the CRS combobox |
| UI / Disclosure | Open = false, true | "Avancé", "Point de repère topographique (facultatif)" |
| UI / Segmented toggle | Selected = First, Second | Decimal / DMS |
| UI / North arrow | – | Project, grid and true north with their angles (true north = convergence − grid rotation, counter-clockwise, as in `api-contract.md`) |
| UI / Banner | – | Server errors, `multiple_sites` |
| UI / Badge | – | "Modifié" source marker |
| UI / Copy button | State = Default, Copied | Per-block copy |
| UI / Compact field | State = Default, Focus, Error, Unavailable | Panel rows: label left, value right-aligned with unit |
| UI / Coordinate block | Role = Idle, Computed, Source | Panel block: title, badge, Copy, CRS/datum line, three compact fields, "Calculé depuis …" caption, source rule |
| UI / Dialog | – | Modal shell: title, body, footer (Remove left, Cancel and Apply right); the board dialogs are detached copies |
| UI / Tooltip, UI / Toast | – | Tooltip of the disabled Pick button; "Project location applied" toast (`georef.applied`) |
| Icons / UI / Info, Error, Copy, Pick, Check | – | 16 px icons (Info is a 24 px target) |

## Deviations from the UX notes, and points for front

1. **Field borders.** Inputs use `color/field-line` (3:1); `line` stays for dividers, panel borders, the listbox and the Copy button (identified by its text).
2. **Panel width.** The panel is the 296 px dock of the shell boards, 268 px for its tab list. Shipped CSS renders 310 px (no `box-sizing: border-box` reset), so 282 px for the tabs. The three French tabs need about 295 px with the shipped 12 px tab padding and about 271 px with the 8 px used here. Front: shrink `.panel-tab` padding or let the tab list scroll; never truncate.
3. **Geographic precision.** Latitude and longitude show 8 decimals in decimal degrees (about 1 mm) and seconds with 4 decimals in DMS (about 3 mm); 4 decimals of a degree is about 11 m. Rotation, true north and convergence keep 4 decimals. The copy format in `ux-notes.md` has 1 decimal of a second. To be confirmed by lead/UX.
4. **True north** is shown in the north-arrow legend (highlighted), not as a separate read-only field; "–" before a CRS is chosen.
5. **Labels and help in F1.** Easting/Northing/Height (base point and survey point) use the section's help sentence as their description; info buttons stay on the section headers, the rotation and the scale.
6. **Pick from view** must be `aria-disabled="true"` (focusable, not `disabled`) so its tooltip is reachable from the keyboard.
7. **Toast** has no Undo button, only the hint "Ctrl+Z pour annuler" ("Cmd+Z" on macOS): interactive toasts are hard to reach.
8. **Dialog height.** Taller dialogs are capped at 836 px (900 − 64) on the boards: the body scrolls, header and footer stay visible.
9. **Remove confirmation** uses a solid danger button next to the focused "Conserver la géoréférence".
10. In the panel, "Calculé depuis …" sits under the fields of each computed block; a conversion error replaces it in `color/err`.
11. **Example values** are a Paris site in Lambert-93 with a 30° grid rotation, illustrative and only approximately computed. The shell around them (2.1–2.3) shows a Lyon site; the `outside_domain` boards use an easting 2 000 km off, and the backend decides what is outside.
12. **Geographic height** is labelled "Hauteur (h)" only. `api-contract.md` says the UI marks it as approximate (no geoid yet, GEOREF-05): front adds that marker.
13. **States not drawn** (details in `ux-notes.md`, built from the same components): F1-E list states, dialog loading, `multiple_sites`, Hover of fields and buttons, the "Copié" toast.
14. **Shortcuts in strings** are written per platform: Ctrl on Windows and Linux, Cmd on macOS.
15. The boards keep the file's font, IBM Plex Sans. It is not bundled with the app (which falls back to `system-ui`), so check long French strings in the running app.
16. **Board 2.2 is superseded.** Board 2.2 (an older coordinate-system picker, a table in its own dialog) stays in the file, but 2.4–2.9 supersede it for the CRS picker. Front implements from 2.4–2.13.
