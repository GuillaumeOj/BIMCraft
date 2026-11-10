# Design review — Materials and Build-ups dialogs (T9)

Review by `ui` of the dialogs as merged on `orch/add-materials-and-layered-build-ups/integration` (commit e2693e0): `ui/src/materials/` (T5, #86) against boards 4.7–4.14, and `ui/src/buildups/` (T8, #87) against boards 3.8–3.20 (images and notes in this folder, `README.md`). The review is from the code and CSS: the sandbox blocks the dev server, so nothing was seen running. Strings were checked in `ui/src/i18n/messages/fr.json`.

**must-fix**: a defect a user meets (wrong behaviour, unreadable content, an accessibility failure). **nice-to-have**: a gap with the boards that can wait, or a design-side update.

Overall the two dialogs follow the boards closely: sizes (880 × 640, 640 → 960), panes, the inline confirmations with the safe button focused, the server banner, inline errors with `aria-invalid` / `aria-describedby`, the live region for reordering, the ARC-30 `extra` slot, and the French copy. The deviations front listed are accepted unless a finding below says otherwise.

## must-fix

| # | Where | Finding | Suggested fix |
| --- | --- | --- | --- |
| 1 | `ui/src/buildups/LayerRow.tsx`, material `<select>` (`MANAGE` option) | "Gérer les matériaux…" is an option of the native select, and choosing it opens the Materials dialog. On a closed native select, ↑/↓ change the value at once in Chrome on Windows and Linux, and in Firefox. A keyboard user stepping past the last material opens a modal dialog without asking for it: a change of context on input (WCAG 3.2.2). Board 3.19 has the entry outside the options list, after a separator. | Take "Gérer les matériaux…" out of the select: a small link button next to the select (or under the table), or the searchable listbox of board 3.19 where the item is activated with Enter only. Keep the stacked dialog and the "select the added material" behaviour. |
| 2 | `ui/src/buildups/buildups.css`, `.preview-layer[data-function="membrane"]` and `.preview-number` | The membrane band is filled with `--bc-ink-2` and its number is also `--bc-ink-2`: 1:1, the number disappears whenever the band is wide enough to show it. The preview is `aria-hidden`, but the numbers are what ties it to the table for sighted users. | Give the membrane number `--bc-surface` (or put the number outside a band thinner than the text). |

## nice-to-have — Build-ups (boards 3.8–3.20)

| # | Where | Finding | Suggested fix |
| --- | --- | --- | --- |
| 3 | `BuildUpList.tsx`, `ActionMenu` "Nouvelle composition…" | Primary button, and the footer **Fermer** is primary too: two primary buttons in one view. Board 3.9 has the menu button secondary. | `className="button"` for the menu button; keep **Fermer** primary, as **Terminé** in Materials. |
| 4 | `BuildUpList.tsx`, `DeleteConfirmation` | The other rows stay fully active while the inline confirmation is open; board 3.12 shows them inert at 45 % (as the Materials confirmation does). | `inert` + `opacity: 0.45` on the other rows while `inline.kind === "delete"`. |
| 5 | `buildups.css`, `.tag` | Pill (`border-radius: 999px`, no border). `UI / Tag` on the boards is a 4 px radius with a `--bc-line` border. | `border: 1px solid var(--bc-line); border-radius: 4px; font-size: 11px`. |
| 6 | `LayerRow.tsx` / `buildups.css`, material select in error | `aria-invalid` is set but the select has no error look, while the thickness field turns red (board 3.16: 2 px `err` border). The message under the row is there, so this is consistency, not a missing cue. | A `select[aria-invalid="true"]` rule in `forms.css` matching `.field-input[data-invalid="true"]`. |
| 7 | `LayerRow.tsx`, core checkbox | Native checkbox with the browser's default colour (blue) and no error look; board `UI / Checkbox` uses `color/acc` and an `err` ring for `core_not_contiguous`. The row rule already marks the error. | `accent-color: var(--bc-verdigris)` and an `outline: 2px solid var(--bc-danger)` on `input[type=checkbox][aria-invalid="true"]`. |
| 8 | `LayerRow.tsx` row actions | Glyphs ↑ ↓ × (front deviation 2) and no visible label on focus. Board 3.17: `UI / Icon button` with the Move up / Move down / Remove icons and the label as a tooltip on focus and hover. Accessible names are correct. | Inline the three 16 px SVG icons from Brand guide › 8 and show the `aria-label` text as a tooltip on `:focus-visible` / `:hover`. |
| 9 | `BuildUpEditorView.tsx`, `.footer-note` | With errors, "3 problèmes à corriger." keeps the `muted` colour; board 3.16 shows it in `err` with the error icon. | `color: var(--bc-danger)` and `ErrorIcon` when `errors.count > 0`. |
| 10 | `buildups.css`, `.preview-layer[data-function="insulation"]` | Insulation is a 12 % mix of `--bc-danger`: a red tint next to red error states reads as an error. The boards use `color/warn-tint` (light #FCEBDC, dark #3D2A1A, already a design variable). | Add `--bc-warn-tint` to `theme.css` from the design variable and use it here. |
| 11 | `SectionPreview.tsx`, `.preview-core` | One "Noyau" caption under the whole strip (front deviation 9); boards 3.14–3.16 put a bracket under the core layers only, so a non-contiguous core is visible in the preview. | Bracket spanning the first to the last core layer (a positioned element using the cumulated sizes); the hatch already marks each core layer. |
| 12 | `Totals.tsx` / `useBuildUpPreview.ts` | While a layer has no material the preview is not asked, so "Épaisseur totale : —" even when every thickness is typed (front deviation 5); board 3.13 shows "0 mm" and UX says the totals follow the draft at once. | Ask `back` whether `buildups.preview` can answer with a `material_required` error and still return `total_thickness`; otherwise keep "—" and say why ("Choisissez un matériau pour chaque couche."). |
| 13 | `LayerRow.tsx`, R cell | "—" explains itself only through `title`, which keyboard and touch users never see (board: tooltip). | Visually hidden text after the dash ("Pas de conductivité thermique pour ce matériau."), or `aria-describedby` to a tooltip shown on focus of the row. |
| 14 | `fr.json`, `buildups.priority.help.defaults` and `buildups.layers.moved` | A normal space before « : » where French typography wants U+202F (the other strings have it). | Replace the space before « : » with U+202F in both strings ("Par défaut :", "sur {total} :"). |
| 15 | `LayerRow.tsx`, material select | Native select instead of the searchable listbox of board 3.19 (front deviation 1): no search and no category column. Acceptable for a first version with a few materials. | Reuse the CRS combobox pattern (`UI / Search select`) when the list grows; it also fixes finding 1 for good. |

## nice-to-have — Materials (boards 4.7–4.14)

| # | Where | Finding | Suggested fix |
| --- | --- | --- | --- |
| 16 | `MaterialsDialog.tsx` / `LibraryBrowser.tsx`, empty project | Focus goes to the library search field; UX F1 step 1 and board 4.7 put it on **Ajouter depuis la bibliothèque…**. The search field is a reasonable first stop. | Either keep it and `ux` updates the notes, or focus the left-pane button when the project is empty. |
| 17 | `ProjectPane.tsx`, `.search` | No search icon in the field (board 4.8 shows `Icons / UI / Search`). | Background SVG or an inline icon with `padding-left`. |
| 18 | `MaterialDetail.tsx`, in-use banner | Neutral border (`banner-info`, front deviation 7); board 4.14 `UI / Banner info` has a `color/acc` left rule. | `border-left: 4px solid var(--bc-verdigris)` on `.banner-info`. |
| 19 | `forms.css`, `.toast` | Surface toast with an outlined "Annuler" (front deviation 4) instead of the dark `UI / Toast with undo` of the boards. The code matches the georeferencing toast, which is the better rule. | No code change: `ui` updates the boards and the Brand guide toast to the surface style. |
| 20 | `theme.css`, `--bc-selected` | New token (light #d6ece7, dark #1f3a36) for the design's `color/acc-tint` (light #D6EEE9, dark #1D3F39): two names and slightly different values for one colour. | No code change: `ui` aligns the design variable to the code values and notes the name mapping in the Brand guide. |
| 21 | `LibraryBrowser.tsx`, grade preview | Full read-only property table (front deviation 1) instead of four key values; better for the user, and the 4.10 note already allowed it. | Accepted; no change. |

Resolved since the request: the in-use banner's **Afficher les compositions…** button (front deviation 2) is wired in `shell/Shell.tsx` now that the Build-ups dialog exists.

## For `ui` (design side)

Findings 16, 19 and 20 change the boards, not the code: update 4.7 (focus note), the toast in 4.8 / 3.20 and Brand guide › 8, and the `color/acc-tint` values. This review does not touch the design file; those updates are a separate task.
