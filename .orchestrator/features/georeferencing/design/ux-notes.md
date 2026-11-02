# Georeferencing — UX notes (sub-issue #83, GEOREF-04)

Text twin of the FigCraft page "UX flows" in `georeferencing.figcraft`. Frames: F1-A…F1-E (dialog), F2-A…F2-C (panel), "Copy EN/FR", "Keyboard map", "Components". Tokens: collection "Tokens" (Light/Dark) copied from `ui/src/shell/theme.css`, plus **`color/danger`** (proposed: `#B3261E` light, `#F2867E` dark; ≥ 4.5:1 on surface — UI to verify with `contrast.ts`). The shell has no error colour yet.

## Decisions the UI designer must keep

1. Dialog is modal, 640 px wide (min width 480 at the minimum window size, then fields stack one per row), focus trapped, Esc cancels, Enter applies. Apply is one undo step.
2. Order and numbering: 1 Coordinate system → 2 Base point → 3 Orientation → "Advanced" (Scale, collapsed) → "Survey point" (optional, collapsed unless set) → Derived (read-only) → buttons. "Remove georeference" sits left, alone; Cancel and Apply right.
3. Remove confirmation is **inline in the dialog** (panel replacing the footer, form inert), default focus on "Keep georeference". Never `confirm()`.
4. Errors are inline under the field, linked with `aria-describedby`, `aria-invalid`, announced; server errors are a banner (`role="alert"`) at the top of the dialog. Never an alert box. Apply stays enabled; on click it focuses the first invalid field and announces the count.
5. True north is read-only, next to the rotation field, with a small north-arrow visual: grid north, true north, project north, with angles. It is not computed in the UI (`derived.true_north`); before Apply it shows "–".
6. Panel lives in the right tab group as a third tab "Coordinates". Three blocks always visible: Model, Map (CRS name), Geographic (datum name). Typing in a block makes it the source ("Edited" badge + 2 px verdigris left rule); the others show "Computed from …". Copy per block. Decimal/DMS toggle only affects the Geographic block (and the angle field in F1 has its own).
7. "Pick from view" button is present but disabled, with tooltip, in the panel header. No pointer picking now.
8. Numbers: lengths 3 decimals (m), angles 4 decimals (°) or DMS; thousands separator is a narrow no-break space, decimal separator by locale; input accepts `,` and `.` as the decimal separator: a single separator is the decimal one, when both appear the last one is, and spaces (including the narrow no-break space) are ignored as grouping.
9. Every field: visible label (never placeholder-only), units suffix inside field, help via an info button with a popover (keyboard reachable) and the same text as `aria-describedby`.
10. French strings are ~30 % longer: labels wrap to two lines above the field rather than truncate; buttons grow, never truncate.

## F1 — Project location dialog

Entry: title-bar/project menu "Project location…", shortcut **Ctrl/Cmd+Alt+L** (proposal, documented; Ctrl+Alt is AltGr on Windows AZERTY and other European layouts, so the UI must check that the final combination does not swallow typing in fields; no palette exists yet; if one exists the entry is "Project location…"). From the panel's button.

States (frames):
- **F1-A not georeferenced**: guidance sentence, CRS combobox focused, other sections disabled until a CRS is chosen (they enable on choice), Derived shows a placeholder, no "Remove". Apply stays enabled (decision 4): with no CRS or an unparsable number it focuses the first invalid field and shows the inline error.
- **F1-B georeferenced**: filled from `georef.get`; CRS row shows code, name and area of use; true north, derived lat/lon (named datum), convergence, scale factor, survey point in model coordinates. "Remove georeference" visible. Apply stays enabled (decision 4): when nothing changed it just closes, with no `georef.set` and no history entry.
- **F1-C errors**: unknown/empty CRS; non-numeric; scale ≤ 0 (Advanced auto-opens); outside CRS area/domain (`outside_domain`); server error banner. Apply focuses the first error.
- **F1-D remove confirmation**.
- **F1-E CRS list open**: combobox/listbox; loading, empty-result and error states; geographic CRSs not listed.
- Loading: while `georef.get` loads, fields show skeletons, buttons disabled. `multiple_sites`: dialog shows the banner "This project has several sites. Georeferencing needs exactly one." with only a Close button.

Flow: open → (get) → choose CRS → base point → rotation → [Advanced/Survey] → Apply → `georef.set` → ok: close, toast "Project location applied" (undo available) → error: stay, map `data.reason` to inline message. Remove → confirm → `georef.clear` → close, panel returns to F2-A.

Reason mapping: `unknown_crs`, `not_projected` → CRS field; `invalid_scale` → Scale; `outside_domain` → Base point group (error on the Easting/Northing group, not a single field; the message never repeats the typed values — SEC-03 — and the CRS row above it already shows the area of use); `not_georeferenced` n/a in F1; `multiple_sites` → banner; anything else → banner.

## F2 — Coordinates panel

States: **F2-A** not georeferenced (explanation + "Set project location…" opens F1); **F2-B** georeferenced (three blocks, origin shown on open: local 0,0,0); **F2-C** editing with source marker, per-block errors. Conversion on change, debounced 200 ms, `georef.convert`; outdated responses ignored. `outside_domain` → other blocks show "—" with a message; `not_georeferenced` → switch to F2-A. After F1 applies or removes, the panel refreshes and keeps its current local point. "Reset" (header menu) sets the point back to local 0,0,0. Header: CRS name, "Change…" link opens F1.

Reserved slot: header right, "Pick from view" disabled button (tooltip "Available when the 3D view is ready"); when enabled it fills the source block, no layout change.

Copy formats (plain text, tab-separated so it pastes into a spreadsheet cells; locale decimal separator, no thousands separator): Model `x⇥y⇥z`; Map `E⇥N⇥H`; Geographic decimal `lat⇥lon⇥h`, or DMS `48°51'24.0"N⇥2°21'07.9"E⇥h`. Toast "Copied" + live region.

## Copy (EN / FR) — keys under `georef.*`

| Key | EN | FR |
| --- | --- | --- |
| `location.title` | Project location | Localisation du projet |
| `location.intro` | This project is not placed on Earth yet. Choose a coordinate system, then say where the model origin (0, 0, 0) is on the map. | Ce projet n'est pas encore placé sur la Terre. Choisissez un système de coordonnées, puis indiquez où se trouve l'origine du modèle (0, 0, 0) sur la carte. |
| `crs.label` | Coordinate system | Système de coordonnées |
| `crs.help` | The map system used by your survey, for example Lambert 93 (EPSG 2154) or a CC zone (CC42–CC50). Search by code or name. | Le système cartographique de votre relevé, par exemple Lambert 93 (EPSG 2154) ou une zone CC (CC42–CC50). Recherchez par code ou par nom. |
| `crs.placeholder` | Search by code or name, e.g. 2154 or Lambert | Rechercher par code ou nom, p. ex. 2154 ou Lambert |
| `crs.empty` | No coordinate system matches “{query}”. | Aucun système de coordonnées ne correspond à « {query} ». |
| `crs.loading` | Loading coordinate systems… | Chargement des systèmes de coordonnées… |
| `base.title` | Project base point | Point de base du projet |
| `base.help` | The map coordinates of the model's origin (0, 0, 0), in metres. | Les coordonnées sur la carte de l'origine du modèle (0, 0, 0), en mètres. |
| `base.easting` / `northing` / `height` | Easting / Northing / Height | Est / Nord / Hauteur |
| `rotation.label` | Grid rotation | Rotation de la grille |
| `rotation.help` | The angle between the map grid and your project grid, counter-clockwise. 0 means the project axes follow the map. | L'angle entre la grille de la carte et la grille du projet, dans le sens trigonométrique. 0 signifie que les axes du projet suivent la carte. |
| `trueNorth.label` | True north | Nord géographique |
| `trueNorth.help` | Computed from the meridian convergence at the base point and the grid rotation. | Calculé à partir de la convergence des méridiens au point de base et de la rotation de la grille. |
| `scale.label` | Scale | Échelle |
| `scale.help` | Leave at 1 unless your surveyor gave you a project scale. | Laissez 1 sauf si votre géomètre vous a donné une échelle de projet. |
| `survey.title` | Survey point (optional) | Point de repère topographique (facultatif) |
| `survey.local` | Position in the model | Position dans le modèle |
| `derived.title` | Derived information | Informations calculées |
| `derived.convergence` | Meridian convergence | Convergence des méridiens |
| `derived.scaleFactor` | Point scale factor | Coefficient d'échelle au point |
| `action.apply` / `cancel` | Apply / Cancel | Appliquer / Annuler |
| `action.remove` | Remove georeference | Supprimer la géoréférence |
| `remove.title` | Remove the georeference? | Supprimer la géoréférence ? |
| `remove.body` | The model stays as it is; only its place on Earth is forgotten. You can undo this. | Le modèle reste inchangé ; seule sa position sur la Terre est oubliée. Vous pouvez annuler cette action. |
| `remove.keep` | Keep georeference | Conserver la géoréférence |
| `applied` | Project location applied | Localisation du projet appliquée |
| `error.required` | Choose a coordinate system. | Choisissez un système de coordonnées. |
| `error.number` | Enter a number, e.g. 652000.000. | Saisissez un nombre, p. ex. 652 000,000. |
| `error.unknown_crs` | This coordinate system is not available. | Ce système de coordonnées n'est pas disponible. |
| `error.not_projected` | Choose a projected system (map coordinates in metres), not latitude/longitude. | Choisissez un système projeté (coordonnées en mètres), pas latitude/longitude. |
| `error.invalid_scale` | The scale must be greater than 0. | L'échelle doit être supérieure à 0. |
| `error.outside_domain` | This point is outside the area where this coordinate system is valid. Check the numbers or the system. | Ce point est hors de la zone de validité de ce système de coordonnées. Vérifiez les nombres ou le système. |
| `error.multiple_sites` | This project has several sites. Georeferencing needs exactly one. | Ce projet contient plusieurs sites. La géoréférence en exige un seul. |
| `error.not_georeferenced` | The project has no location yet. | Le projet n'a pas encore de localisation. |
| `error.generic` | Something went wrong. Try again. | Un problème est survenu. Réessayez. |
| `error.summary` | {count} fields need attention. | {count} champs demandent votre attention. |
| `panel.title` | Coordinates | Coordonnées |
| `panel.empty` | This project has no location yet. Place it on a coordinate system to read and enter map and latitude/longitude coordinates. | Ce projet n'a pas encore de localisation. Placez-le dans un système de coordonnées pour lire et saisir des coordonnées cartographiques et géographiques. |
| `panel.setLocation` | Set project location… | Définir la localisation du projet… |
| `block.local` / `map` / `geographic` | Model (local) / Map / Geographic | Modèle (local) / Carte / Géographique |
| `block.edited` | Edited | Modifié |
| `block.computed` | Computed from {source} | Calculé depuis {source} |
| `format.decimal` / `dms` | Decimal degrees / DMS | Degrés décimaux / DMS |
| `copy` / `copied` | Copy / Copied | Copier / Copié |
| `pick.disabled` | Available when the 3D view is ready. | Disponible quand la vue 3D sera prête. |
| `convert.outside` | This point is too far from the projection to convert. | Ce point est trop éloigné de la projection pour être converti. |
| `convert.failed` | Conversion failed. Try again. | La conversion a échoué. Réessayez. |

(Messages never repeat model values — SEC-03; `crs.empty` echoes only the search text the user just typed. French typographic spaces before `:`, `?`, `!`, `»` are narrow no-break spaces.)

## Keyboard map

| Where | Keys | Action |
| --- | --- | --- |
| Global | Ctrl/Cmd+Alt+L | Open Project location (F1) |
| Global | Ctrl/Cmd+Alt+K | Focus the Coordinates panel (opens the tab) |
| Dialog | Tab / Shift+Tab | Fields in visual order: CRS, E, N, H, rotation (+ DMS toggle), Advanced toggle, scale, survey toggle/E/N/H, Remove, Cancel, Apply; trapped |
| Dialog | Enter | Apply (not inside the open CRS list or a button) |
| Dialog | Esc | Closes an open list/popover first, then cancels the dialog; in the remove confirmation it keeps the georeference |
| CRS combobox | ↑ ↓ Home End Enter Esc, type to filter | Move, select, close, search |
| Fields | ↑ ↓ | Nudge by 1 m (Shift: 0.1 m… lengths) / 1° for rotation — optional |
| Panel | Tab order | Header (Pick, menu), Model x y z, Copy, Map E N H, Copy, Geographic format toggle, lat lon h, Copy |
| Panel | Enter in a field | Convert now (skips debounce) |
| Panel | Ctrl/Cmd+C in a focused block's Copy button | Copies that block (button Space/Enter) |

Focus: opening F1 focuses the CRS combobox (empty) or the first field (filled); closing returns focus to the opener. Focus ring: 2 px verdigris, offset 2.

## Reusable components (first form controls in the app)

1. **Dialog** (modal shell: title, body, footer, focus trap, Esc/Enter) — also for later dialogs.
2. **TextField** (label, suffix unit, help button, error message, states: default, hover, focus, disabled, read-only, error).
3. **NumberField** (TextField + locale parsing, decimals, ↑↓ nudge; variants: length m, angle °, scale).
4. **AngleField** (NumberField degrees ⇄ DMS toggle).
5. **Combobox / SearchSelect** (input + listbox, row = code, name, area; loading/empty/error).
6. **FieldGroup** (numbered section header + help + group error).
7. **Disclosure** ("Advanced", "Survey point").
8. **InlineConfirm** (confirmation panel inside a dialog).
9. **Banner** (error / info, `role="alert"` for errors).
10. **NorthArrow** (grid / true / project north diagram, angle labels).
11. **CoordinateBlock** (title, source badge, three NumberFields, Copy button, "Computed from" caption, error).
12. **SegmentedToggle** (Decimal/DMS).
13. **Toast** ("Applied", "Copied") with live region.
14. **Tooltip / InfoPopover** (keyboard reachable).
15. **PanelEmptyState** (text + button).
