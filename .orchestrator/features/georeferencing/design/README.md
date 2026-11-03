# Georeferencing — UI design (sub-issue #83, GEOREF-04)

High-fidelity frames for the project location dialog (F1) and the Coordinates panel (F2). Source file: `georeferencing.figcraft`, page **"UI"** (the page "UX flows" holds the wireframes, text twin in `ux-notes.md`). All strings are the French ones (about 30 % longer than English); the English copy is in `ux-notes.md`.

Open the file in the FigCraft app, or through the FigCraft MCP server (`figcraft-app` when the app is running, else the headless `figcraft`): `open_file` with this path, then `inspect_document` / `screenshot`. See `docs/dev/ui-design.md`.

## Frames → images

Every image is a 2× PNG export of one frame on the page "UI".

| Frame (page "UI") | Image | Notes |
| --- | --- | --- |
| F1-A Dialog · not georeferenced (empty) | `location-dialog-empty.png` | Intro, CRS combobox focused (empty), sections 2–3 disabled until a CRS is chosen, true north "–", derived info placeholder, no "Remove" |
| F1-B Dialog · georeferenced (filled) | `location-dialog-filled.png` | Filled from `georef.get`; Advanced closed, survey point open with its model position |
| F1-C Dialog · field errors (non-numeric, scale ≤ 0) | `location-dialog-field-error.png` | Client errors; Easting has the focus (first invalid field); Advanced opened by the scale error |
| F1-C Dialog · outside the CRS area (server reason outside_domain) | `location-dialog-outside-area.png` | Group error on Easting/Northing, focus on Easting; message never repeats the values (SEC-03) |
| F1-C Dialog · server error | `location-dialog-server-error.png` | Banner `role="alert"` at the top of the body; focus stays on Apply |
| F1-D Dialog · remove confirmation | `location-dialog-remove-confirm.png` | Inline confirmation replaces the footer; form inert (shown at 45 % opacity); default focus on "Keep georeference" |
| F2-A Coordinates panel · not georeferenced | `coordinates-panel-not-georeferenced.png` | Explanation + "Set project location…" (opens F1) |
| F2-B Coordinates panel · georeferenced | `coordinates-panel-filled.png` | Origin (0, 0, 0) on open; tooltip of the disabled "Pick from view" shown |
| F2-C Coordinates panel · editing (Map is the source) | `coordinates-panel-editing.png` | Map block: "Modifié" badge + 2 px verdigris rule + focus; other blocks "Calculé depuis Carte" |
| F2-C Coordinates panel · editing, outside the CRS area | `coordinates-panel-outside-area.png` | `outside_domain` from `georef.convert`: other blocks show "—" and the message |
| (component sheet) | `ui-components.png` | Every component below, all variants |

Focus is shown with the **Focus/Ring** effect style (2 px verdigris ring, 2 px gap, as `:focus-visible` in `theme.css`).

## Variables

Collection **Tokens** (modes Light, Dark): `color/paper`, `surface`, `raised`, `hover`, `line`, `ink`, `ink-2`, `muted`, `verdigris`, `on-verdigris` are the `--bc-*` values of `ui/src/shell/theme.css`. New:

| Variable | Light | Dark | Why |
| --- | --- | --- | --- |
| `color/danger` | `#B3261E` | `#F2867E` | Error text, error borders, destructive actions (proposed by UX, checked) |
| `color/on-danger` | `#FFFFFF` | `#1C1E21` | Text on a solid danger button |
| `color/field-line` | `#858B94` | `#7A8089` | Borders of text fields, the select and the selected toggle segment (WCAG 1.4.11, see below) |
| `color/shadow` | `#0000001A` | `#00000066` | Colour of the `--bc-shadow` effect (bound in **Shadow/Raised**) |

Contrast, computed with `ui/src/shell/contrast.ts` (needs ≥ 4.5:1 for text, ≥ 3:1 for field borders):

| Pair | Light | Dark |
| --- | --- | --- |
| danger on surface / raised / paper | 6.54 / 5.99 / 6.31 | 5.97 / 5.28 / 6.75 |
| on-verdigris on verdigris | 6.39 | 7.52 |
| verdigris on surface / raised | 6.39 / 5.86 | 6.00 / 5.31 |
| muted on surface / raised | 5.86 / 5.37 | 5.60 / 4.96 |
| field-line on surface / raised | 3.43 / 3.14 | 3.71 / 3.28 |
| line on surface (for comparison) | 1.46 | 1.37 |

Do not put `muted` text on `hover` in dark mode (4.34:1).

Collection **Size** (mode Default), from `shell.css` and the new controls: `radius/control` 6, `radius/tab` 8, `radius/tile` 7, `radius/panel` 12, `space/4…20`, `size/title-bar` 52, `size/panel-header` 40, `size/control` 32 (dialog fields; panel fields are 28), `size/panel-right` 296, `size/focus-ring` 2, `font/size` 13, `font/family` "IBM Plex Sans".

## Styles

| Style | Value | CSS |
| --- | --- | --- |
| Body/Regular | 13 px, 400, line height 120 % | `body` font |
| Body/Strong | 13 px, 600 | titles, selected tab, labels of buttons |
| Body/Paragraph | 13 px, 400, 18 px | intro and confirmation text |
| Caption/Regular | 12 px, 400, 16 px | help, errors, area of use, captions |
| Caption/Strong | 12 px, 600, 16 px | badge |
| Heading/Dialog | 15 px, 600, 20 px | dialog title |
| Shadow/Raised (effect) | 0 4 16, `color/shadow` | `--bc-shadow` |
| Focus/Ring (effect) | 2 px `color/verdigris` ring outside a 2 px `color/surface` gap | `:focus-visible` outline 2 px, offset 2 |

Field labels are 13 px Medium (500). Numbers use tabular figures (`font-variant-numeric: tabular-nums`).

## Components

| Component | Variants | Use |
| --- | --- | --- |
| Icon/Info, Error, Copy, Chevron down, Chevron right, Search, Pick, Check, More | – | 16 px icons (Info is a 24 px target) |
| Button | Kind = Primary, Secondary, Danger, Danger quiet, Ghost | Apply, Cancel, confirm Remove, "Remove georeference", "Modifier…" |
| TextField | State = Default, Focus, Error, Disabled, Read-only | Label above (wraps, never truncates), info button, unit suffix inside, inline error with icon (`aria-describedby`, `aria-invalid`) |
| SearchSelect | State = Filled, Empty, Focus, Error, Open | CRS combobox; the area of use under it; Open shows the listbox (a popover in code) with a result count |
| ListOption | State = Default, Active, Selected | Row: code, name, area of use; Active = `aria-activedescendant` |
| SectionHeader | – | Numbered section title + info button; in F1 it is the visible label of the CRS combobox |
| Disclosure | Open = false, true | "Avancé", "Point de repère topographique (facultatif)" |
| SegmentedToggle | Selected = First, Second | Decimal / DMS |
| NorthArrow | – | Project, grid and true north with their angles (true north = convergence − grid rotation, counter-clockwise, as in `api-contract.md`) |
| Banner | Kind = Error | Server errors, `multiple_sites` |
| Badge | – | "Modifié" source marker |
| CopyButton | State = Default, Copied | Per-block copy |
| CompactField | State = Default, Focus, Error, Unavailable | Panel rows: label left (84 px), value right-aligned with unit |
| CoordinateBlock | Role = Idle, Computed, Source | Panel block: title, badge, Copy, CRS/datum line, three CompactFields, "Calculé depuis …" caption, source rule |
| Dialog | – | Modal shell: title, body, footer (Remove left, Cancel and Apply right). The F1 frames are detached copies of it |
| Tooltip, Toast | – | Tooltip of the disabled Pick button; "Project location applied" toast (`georef.applied`) |

Colours of the components are bound to the Tokens variables; set a frame's Tokens mode to Dark to see a component in dark mode.

## Deviations from the UX notes, and points for front

1. **Font.** IBM Plex Sans is neither installed here nor bundled in `ui/` (the app falls back to `system-ui`). The frames are drawn in Inter at the same sizes; widths are close but not exact, so check the French strings in the running app.
2. **Field borders.** `--bc-line` (1.46:1) fails WCAG 1.4.11 for input boundaries, so fields use the new `color/field-line`. `line` stays for dividers, panel borders, the listbox and the Copy button (identified by its text).
3. **Panel width.** The frames use the shipped right dock, 296 px (`.panel-right`), not 320 px. At 296 px the three French tabs ("Propriétés", "Historique", "Coordonnées") do not fit with the shipped tab padding (12 px): about 295 px for 268 px available. The frames use 8 px tab padding and 2 px list padding (about 271 px, still tight). Front: reduce `.panel-tab` padding, or let the tab list scroll horizontally; never truncate.
4. **Geographic precision.** Latitude and longitude show 8 decimals in decimal degrees (about 1 mm) and seconds with 4 decimals in DMS (about 3 mm). 4 decimals of a degree (the UX rule for angles) is about 11 m, too coarse for coordinates. Rotation, true north and convergence keep 4 decimals. To be confirmed by lead/UX.
5. **True north** is shown in the NorthArrow legend (highlighted), not as a separate read-only field. Before Apply / without a CRS it shows "–".
6. **Labels and help in F1.** The section header labels the CRS combobox (`aria-labelledby`), no duplicate label. Easting/Northing/Height (base point and survey point) use the section's help sentence as their description; info buttons stay on the section headers, the rotation and the scale.
7. **F1-C** is split into three frames (client field errors, `outside_domain`, server error banner) to keep each state readable.
8. **Pick from view** must be `aria-disabled="true"` (focusable, not `disabled`) so its tooltip is reachable from the keyboard.
9. **Toast** has no Undo button (only the hint "Ctrl+Z pour annuler"): interactive toasts are hard to reach. Its colours are fixed (dark), not bound.
10. **Dialog height.** With the survey point open the dialog is about 880 px high: the body scrolls (max height 100vh − 64 px), header and footer stay visible.
11. **Remove confirmation** uses a solid danger button for "Supprimer la géoréférence" next to the focused "Conserver la géoréférence".
12. In the panel, "Calculé depuis …" sits under the fields of each computed block; a conversion error replaces it in danger colour.
13. Only the components are bound to variables; text added directly in the frames uses the Light hex values, so the frames themselves are Light only.
14. Example values (a Paris site in Lambert-93, grid rotation 30°) are illustrative and only approximately computed.
