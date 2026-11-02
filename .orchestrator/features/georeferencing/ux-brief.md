# Georeferencing — UX brief (sub-issue #83, GEOREF-04)

## Users and their situation

Architects and structural engineers, mostly in France at first. They receive a site survey or a cadastral or topographic plan, usually in Lambert 93 or a CC zone (CC42–CC50), sometimes in UTM. It gives coordinates of known points (benchmarks, property corners) and sometimes north arrows. They need to:

1. **Place the project on Earth once**, near the start: choose the CRS, say where the project origin (base point) is in map coordinates, set how the building grid is rotated relative to the map grid, and optionally record the survey benchmark.
2. **Check and exchange coordinates** throughout the project: "what are the Lambert 93 coordinates of this column corner?", "where is this survey peg in my model?", or give coordinates to a surveyor or a planning-permit form, sometimes as latitude and longitude.

Most of them are not geodesists. Terms like CRS, easting/northing, grid north vs true north and scale factor must be explained in plain words in the UI (tooltips or help text) and in the user guide.

## What exists today

The UI is only the shell (`ui/src/shell/Shell.tsx`): a title bar, a left "Project" panel area, the viewport area (no renderer yet), right tabs (Properties and History) and a console. The design system lives in the Penpot file (`docs/dev/ui-design.md`). No dialogs or forms exist yet, so this feature introduces the first form controls: text and number fields, a searchable select, validation messages. Design them as reusable components.

## Flows to design

### F1. Set up the project location (dialog)

Entry points: the title-bar/project menu "Project location…", a keyboard shortcut and the command palette entry if a palette exists. The dialog is modal, focus is trapped, Esc cancels and Enter applies.

1. **Coordinate system**: a searchable list (by EPSG code or name, e.g. "2154" or "Lambert"), showing code, name and area of use. Data: `georef.list_crs`.
2. **Project base point**: Easting, Northing and Height in metres. Explain it: "the map coordinates of the model's origin (0, 0, 0)".
3. **Orientation**: the angle between the map grid and the project grid (degrees or DMS). Show the **true north** angle, derived and read-only, next to it, with a small north-arrow visual: grid north, true north and project north.
4. **Scale**: hidden under "Advanced", default 1. Most users never touch it.
5. **Survey point (optional)**: E, N and H of a known benchmark. Once applied, show its position in model coordinates (`survey_point_local` from `georef.get`).
6. **Derived information** (read-only): latitude and longitude of the base point on the CRS datum (named, e.g. "RGF93 v1"), meridian convergence and point scale factor.
7. **Apply / Cancel**, and **Remove georeference**, which needs a confirmation in the same dialog, not a browser `confirm()`. Apply is one undo step.

Validation states to design: unknown or empty CRS, non-numeric input, scale ≤ 0, base point outside the CRS's area or domain (error returned by `georef.set`, reasons in `api-contract.md`), and server error. Errors appear inline at the field, are announced to screen readers, and never as an alert box.

State: "not georeferenced", the first time, an empty form with guidance; "georeferenced", the form filled from `georef.get`.

### F2. Read and enter coordinates (Coordinates panel)

A dockable panel, proposed in the right tab group next to Properties and History.

- One point at a time, shown in three systems at once: **Model (local)** x/y/z, **Map (projected, CRS name)** E/N/H, **Geographic** latitude, longitude and height.
- The user types into any one of the three blocks, and the other two update (`georef.convert`). Make it clear which block is the source, for example a subtle "edited" state.
- Copy buttons for each block, with a format suitable for pasting into a spreadsheet or a form.
- Geographic display format: decimal degrees or DMS, as a toggle.
- **Not georeferenced**: the panel shows a short explanation and a button that opens F1.
- **Later**: the viewport will push the hovered or clicked point into the panel ("pick"). Leave a place for "pick from view" (a disabled state, or no button for now), so it can be added without a redesign. No pointer picking exists in this feature.

## Constraints

- UI-04: every string goes into `en.json`/`fr.json`. Design with French strings, which are about 30 % longer.
- UI-06: every field has a visible label, full keyboard order, WCAG 2.2 AA contrast (`ui/src/shell/contrast.ts`), focus visible, and errors linked to their fields.
- Numbers: metres with mm precision (3 decimals) for lengths, and angles in degrees with 4 decimals or DMS. Locale decimal separators (a comma in French) must be accepted on input.
- The dialog must work at the minimum supported window size, and the panel at the width of the right dock.

## Deliverables

- **UX (T4)**: flows F1 and F2 as wireframes or annotated flows in the Penpot file (a new page "Georeferencing"), states and error copy (EN and FR), keyboard map and a list of reusable components. Hand off to `ui`.
- **UI (T5)**: high-fidelity boards in Penpot for the dialog (empty, filled, error, remove-confirmation) and the panel (not georeferenced, filled, editing), using and extending the design system. Export each board as PNG to `.orchestrator/features/georeferencing/design/` (e.g. `location-dialog-filled.png`), with a `design/README.md` listing the boards, their Penpot page and board names, and the new components. Commit them on your task branch.
