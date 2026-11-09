# Materials and build-ups — UI design (sub-issues #86 and #87, MAT-02, MAT-03)

High-fidelity boards for the Materials dialog (flow F1 of `ux-notes.md`). They live in the shared project design file `/Users/guillaume/Library/CloudStorage/GoogleDrive-guillaume.ojardias@gmail.com/My Drive/BIMCraft.figcraft` (outside the repository), page **"4 · Structure modelling"**, after the concept board 4.6, which is unchanged. Each board is the full 1440 × 900 shell, in a light board (top row, y = 0) and a dark board (bottom row, y = 1100, `Mode` = Dark). `ux-notes.md` (from `ux`) is the text twin, with the English copy and the behaviour. The strings in the boards are French.

The build-up boards (F2, page 3, 3.8–3.20) are described in [Build-ups](#build-ups-sub-issue-87-mat-03) below.

## Boards → images

Images are 2× PNG exports of the light boards, plus two dark samples.

| Board (page "4 · Structure modelling", `… · light` / `… · dark`) | Image | Notes |
| --- | --- | --- |
| 4.7 Materials — empty project | `materials-dialog-empty.png` | F1-A. The list is replaced by the explanation; focus on "Ajouter depuis la bibliothèque…". The right pane opens the library browser directly, with no back link |
| 4.8 Materials — filled, Mechanical tab | `materials-dialog-filled.png`, `materials-dialog-filled-dark.png` | F1-B. First row selected; header, behaviour law card, tabs (the Graphics and Environmental tabs disabled), property table with the partial factor on two lines (Recommandée / France). The toast with undo ("Béton C25/30 ajouté") is shown after an add |
| 4.9 Materials — Thermal tab, value entered by the user | `materials-dialog-thermal-user-value.png` | λ edited: "Saisie par l’utilisateur" badge, 2 px `color/acc` rule, "Rétablir la valeur de la bibliothèque" focused with its tooltip "Valeur de la bibliothèque : 2,3 W/(m·K)" |
| 4.10 Materials — add from library, grade preview | `materials-dialog-library.png` | F1-C. Back link, library search, category chips (Béton selected), results with "Dans le projet", selected grade C35/45 previewed read-only, "Nom dans ce projet" renamed, "Ajouter au projet" |
| 4.11 Materials — new material | `materials-dialog-new-material.png` | F1-D. Nom (focused), Catégorie, Créer |
| 4.12 Materials — editing, field error, server error | `materials-dialog-errors.png`, `materials-dialog-errors-dark.png` | F1-E/F/G on Acier S355: ν = 0,6 refused, error and range help under the field, focus kept in the field; server error banner (`role="alert"`) at the top of the dialog, list and detail still usable |
| 4.13 Materials — delete confirmation | `materials-dialog-delete-confirm.png` | F1-H on Béton C30/37 (not in use). Inline confirmation replaces the footer; body inert at 45 % opacity; default focus on "Conserver le matériau"; solid danger "Supprimer le matériau" |
| 4.14 Materials — in use, delete explained | `materials-dialog-in-use.png` | F1-I on Béton C25/30 (used by 3). Info banner under the detail header with "Afficher les compositions…"; no confirmation |
| Brand guide › 8 · Form and dialog components | `ui-components-materials.png` | The new components, all variants (1×) |

The dialog sits on the structure shell of board 4.6 with its 18 % black scrim. It is 880 × 640 at (280, 130): header, body (list pane 320 px + detail pane), footer. Focus uses the **Focus/Ring** effect style of the georeferencing boards: a 2 px `color/acc` ring outside a 2 px `color/surface` gap, as `:focus-visible` in `theme.css`. The title bar has a new ghost button **Matériaux…** before the command search.

## Variables

The boards use only the existing `Mode` (Light/Dark) and `Core` variables: `color/surface`, `raised`, `paper`, `ink`, `ink2`, `muted`, `line`, `field-line`, `acc`, `acc-ink`, `acc-tint`, `on-acc`, `err`, `on-err`. No variable was added. Every colour of the new components and boards is bound, so the dark boards are the light ones with `Mode` = Dark.

Contrast of the new pairs, computed with `ui/src/shell/contrast.ts` (≥ 4.5:1 for text, ≥ 3:1 for borders and the selection rule):

| Pair | Light | Dark |
| --- | --- | --- |
| ink on acc-tint (selected row name) | 13.88 | 9.31 |
| ink2 on acc-tint (selected row category and "Utilisé par") | 7.10 | 6.11 |
| acc-ink on acc-tint (selected chip) | 7.47 | 7.11 |
| acc on surface (Rétablir, Retour, Ajouter une valeur, badge) | 6.39 | 6.00 |
| ink2 on surface (chip label, confirmation text) | 8.63 | 7.83 |
| muted on raised (behaviour law note) | 5.37 | 4.96 |
| ink2 on raised (tag) | 7.91 | 6.93 |
| surface on ink (toast text) | 16.88 | 11.93 |
| acc-tint on ink (toast "Annuler" label and border) | 13.88 | 9.31 |
| field-line on surface (search, select, chip borders) | 3.43 | 3.71 |
| acc on acc-tint (selected row rule) | 5.26 | 4.68 |
| err on surface (field error) | 6.57 | 6.18 |

`muted` on `acc-tint` is only 4.37:1 in dark mode, so the selected row uses `ink2` for its second line and its count: do not use `muted` on a selected row.

## Components

New components are in **Brand guide › 8 · Form and dialog components**. The boards also use `UI / Button primary`, `secondary`, `ghost`, `UI / Button danger`, `UI / Text field`, `UI / Banner`, `UI / Badge`, `UI / Tooltip` and the `Icons / UI / Search`, `Expand`, `More`, `Info`, `Error`, `Check` icons.

| Component | Variants and properties | Use |
| --- | --- | --- |
| UI / Material row | State = Default, Selected; Name, Meta, Used by, Show used by | Project list and library results. Selected = `acc-tint` fill + 3 px `acc` left rule (`aria-selected`). The name truncates with an ellipsis (tooltip in code) |
| UI / Tab | State = Selected, Default, Disabled; Label | Mechanical, Thermal, Graphics and Environmental tabs in a `raised` tab list. Disabled is `aria-disabled`, reachable by arrows, tooltip "Pas encore disponible." |
| UI / Material property row | Source = Library, User, Editing, Error, Empty; Property, Value, Source text, Unit, Message, Help | Property (name · symbol, 200 px, wraps), value (110 px, right-aligned, a button that turns into a number field with the unit as suffix), source. User adds the badge, the 2 px rule and Rétablir; Error shows the message and the range under the field; Empty shows "—" and "Ajouter une valeur" (custom materials) |
| UI / Select | State = Default, Focus; Label, Value, Show label | Category select (left pane, new material) |
| UI / Filter chip | Selected = false, true; Label | Library categories. Selected adds a check icon, not only colour |
| UI / Banner info | Message, Show action | `in_use` explanation with "Afficher les compositions…" |
| UI / Tag | Label | Neutral text tag ("Dans le projet") for wider layouts |
| UI / Toast with undo | Text | "{name} ajouté" / "{name} supprimé" with "Annuler" (UX decision 8) |
| Icons / UI / Reset, Back, Plus | – | 16 px icons in `Icons (georeferencing)` |

## Deviations from the UX notes, and points for front

1. **Search and category labels.** The left-pane search field and the category select show no visible label to keep the list tall: the search field has a search icon and the placeholder "Rechercher par nom", the select shows "Toutes les catégories". Give them `aria-label` "Rechercher des matériaux" and "Catégorie" (`materials.search.label`, `materials.category.label`). In the new-material form the select keeps its visible label.
2. **"Dans le projet"** is written in the result's second line ("EN 1992-1-1 · Dans le projet") rather than as a separate tag, because the 210 px results column cannot hold both. `UI / Tag` exists for wider layouts.
3. **Grade preview** shows the grade name, its law and four key values as read-only text, not the full property table, to fit beside the results. The full table is on the material once added.
4. **Delete confirmation** replaces the dialog footer ("Terminé"): the detail pane has no footer of its own. The body is inert (45 %), as in board 2.9.
5. **In-use banner** goes under the detail header; focus moves to it (`role="status"` or focus on its button) when Supprimer… is activated.
6. **Partial factors** sit in one table row with two lines (Recommandée 1,5 / France 1,5) and two sources; in code each line is its own value button (`annex` in `materials.update`, amendment 1).
7. **Rétablir la valeur de la bibliothèque** wraps on two lines in the source column; the tooltip with the library value shows on hover and on focus.
8. **Toast with undo** follows UX decision 8 and differs from the georeferencing toast, which had only a "Ctrl+Z pour annuler" hint. Hide "Annuler" as soon as the model changes again.
9. **Dialog height** is the 640 px maximum on the boards: the list and the detail pane scroll, the header and footer stay visible.
10. **Behaviour law card** is one card with "Loi de comportement : {law}" and the line "{source} — Utilisée pour le dimensionnement des sections. Non modifiable."
11. **States not drawn** (details in `ux-notes.md`, built from the same components): F1-J loading, F1-K (no grade matches the query), rename in the header (Renommer… turns the name into a `UI / Text field`), the overflow menu open, the narrow window (720 px: panes stacked with "Retour à la liste"), and the `Editing` variant of the property row in a board (it is in the Brand guide; 4.12 shows the same field in error).
12. **Example values** are illustrative grades from the cited tables; the UI never stores them, it shows what `materials.get` and `materials.grade` return.
13. **The ν symbol** looks like a "v" in IBM Plex Sans at 13 px; check it in the app font (`system-ui`, IBM Plex Sans is not bundled).
14. The shell around the dialog is the structure shell of board 4.6, whose panels are in English; only the dialog and the title-bar button are French.

## French strings

The French strings are in the boards (and in the images). `front` receives the full French table for `fr.json` by message; it follows the keys of `ux-notes.md` › Copy — F1. French typography: a narrow no-break space before « ? » and « : » and inside « » quotes; numbers use a decimal comma and a narrow no-break space between thousands (2 500 kg/m³).

---

# Build-ups (sub-issue #87, MAT-03)

High-fidelity boards for flow F2 of `ux-notes.md` (with its "Lead decisions" and `api-contract.md` amendments 6–8), in the same shared design file, page **"3 · Architecture modelling"**, boards **3.8–3.20** after the concept boards 3.1–3.7. Board 3.3 "Wall type — layers and location line" is unchanged. Light boards are on the top row (y = 0, from x = 10640, one every 1520), dark boards below them (y = 1100, `Mode` = Dark). The shell is the architecture shell of board 3.3 (plan view, wall selected, 18 % black scrim); its panels stay in English, the dialogs and the title-bar buttons are French.

## Boards → images

2× PNG exports of the light boards, plus three dark samples.

| Board (page "3 · Architecture modelling", `… · light` / `… · dark`) | Image | Notes |
| --- | --- | --- |
| 3.8 Build-ups — empty project | `buildups-empty.png` | F2-A. "Aucune composition pour l'instant…" with **Nouvelle composition de mur** (focused, as UX step 1) and **Nouvelle composition de plancher** |
| 3.9 Build-ups — list, filled, row menu | `buildups-list-row-menu.png`, `buildups-list-row-menu-dark.png` | F2-B. Usage filter (Toutes / Murs / Planchers), **Nouvelle composition…** menu button, five rows (name, "{n} couches · {total} mm", usage tag), first row selected, its row menu open with Modifier… (Entrée), Dupliquer…, Renommer… (F2), Supprimer… (Suppr) |
| 3.10 Build-ups — duplicate inline | `buildups-duplicate.png` | F2-C. Inline form under the row: "Nom de la copie" focused, "Mur béton 200 + ITE (copie)", Dupliquer / Annuler |
| 3.11 Build-ups — rename inline | `buildups-rename.png` | F2-D. The row turns into a field (Entrée : enregistrer · Échap : annuler) |
| 3.12 Build-ups — delete confirmation | `buildups-delete-confirm.png` | F2-E. Inline in the row area, `role="alertdialog"`, default focus on **Conserver la composition**, other rows inert at 45 % |
| 3.13 Build-up editor — new wall build-up | `buildup-editor-new-wall.png` | One starter layer (Structure, "Choisir un matériau", no thickness, 1 (par défaut), core checked); Name focused; every row action disabled with one layer; totals "0 mm" and "—" with the reason |
| 3.14 Build-up editor — wall, filled, join priority help | `buildup-editor-wall.png`, `buildup-editor-wall-dark.png` | The reference editor: layer table, layer 3 selected, layer 5 priority overridden ("4" + badge **Modifiée**), air gap with R "—", section preview, totals with "1 couche sans données thermiques…", and the join priority help popover open from the column header |
| 3.15 Build-up editor — slab, priority reset | `buildup-editor-slab.png` | Slab faces Dessus / Dessous, vertical section preview (top to bottom), membrane at 0 mm, all layers with λ so no partial note; the priority list of layer 3 is open with **Utiliser la valeur par défaut de la fonction (4)** |
| 3.16 Build-up editor — errors | `buildup-editor-errors.png`, `buildup-editor-errors-dark.png` | Server banner (`role="alert"`); `invalid_thickness` under the thickness of layer 2; `core_not_contiguous` as a group message above the table plus a 3 px `err` rule on the core rows (3, 5) and the gap row (4), error checkboxes, row message "Cette couche est entre deux couches du noyau."; `invalid_material` on the air gap of layer 4 ("Ce matériau ne peut pas être utilisé pour une lame d'air…"); footer "3 problèmes à corriger." |
| 3.17 Build-up editor — keyboard reordering | `buildup-editor-reorder.png` | After Alt+↑ on Laine de roche: the moved layer is selected, focus stays on its **Monter la couche** button with its visible label "Monter la couche 2 (Alt+↑)"; the dashed annotation under the dialog gives the live-region text (not drawn in the UI) |
| 3.18 Build-up editor — discard confirmation | `buildup-editor-discard.png` | Cancel / Esc with changes: the footer becomes "Abandonner vos modifications ?", default focus on **Continuer la modification**, danger **Abandonner les modifications**; body inert at 45 % |
| 3.19 Build-up editor — material picker | `buildup-editor-material-picker.png` | Searchable listbox over the project materials (name + category, selected one checked, active one highlighted), separator, **Gérer les matériaux…** (opens F1 stacked, Ctrl+Maj+M) |
| 3.20 Build-ups — after Apply, toast with undo | `buildups-applied-toast.png` | Back to the list, applied row selected, `UI / Toast with undo` "Composition appliquée" + Annuler |
| Brand guide › 8 · Form and dialog components | `ui-components-buildups.png` | The new components (1×) |

## Layout

- **List dialog**: 640 px at (400, 130), same header/footer metrics as the Materials dialog (title 15 semibold, padding 18/20/12/20, footer with **Fermer**).
- **Editor**: the same dialog frame widened to 960 px at (240, 64) (3.16 at y = 16 to fit the banner and messages). Header (title, subtitle with the face order), **Body (editor component)**, footer owned by the host (note, Annuler, Appliquer). The body is the reusable editor of UX Part 2: Nom (320 px) and Description, an **empty, hidden `Slot / extra (ARC-30 location line)` frame** above the layers where the wall-type editor adds the location line, the **Couches** group (table, Ajouter une couche, Ctrl+Entrée hint), then the section preview and the totals side by side.
- **Table columns** (918 px): # 64 (number + face label on the first and last rows), Fonction 136, Matériau 180, Épaisseur (mm) 84 (right-aligned), Priorité de jonction 150 (+ ⓘ help), Noyau 72 (+ ⓘ help), R (m²·K/W) 64 (right-aligned, tabular figures), actions 94 (Monter, Descendre, Supprimer). Rows 44 px; selected row = `acc-tint` + 3 px `acc` left rule.
- **Join priority**: default shows "{n} (par défaut)"; overridden shows the number alone and the `UI / Badge` **Modifiée** (text, not only colour). The list offers 1–5 and, after a separator, **Utiliser la valeur par défaut de la fonction ({n})**.
- **Section preview** (`aria-hidden`): wall = horizontal strip from Extérieur to Intérieur, slab = vertical stack from Dessus to Dessous. Widths (heights) follow the thickness with a 16 px (3 px for a 0 mm membrane) minimum; fills by function (finish `raised`, insulation `warn-tint`, structure `acc-tint`, air gap dashed outline, membrane `ink2`); core layers carry a hatch and a **Noyau** bracket; the selected layer has a 2 px `acc` outline and an `acc` number; caption "Schématique, pas à l'échelle.". An empty `Overlay / location line (ARC-30)` frame sits over the strip for the future location line.
- **Totals** (`raised` card): "Épaisseur totale : {mm} mm", "Résistance thermique : {value} m²·K/W" (or "—"), then the partial note "{n} couche(s) sans données thermiques … n'est pas comptée." with an info icon (amendment 7). Values come from `buildups.preview` (amendment 6); the UI does no material maths.

## Components

New, in **Brand guide › 8 · Form and dialog components** (the section description lists them):

| Component | Variants and properties | Use |
| --- | --- | --- |
| UI / Build-up row | State = Default, Selected; Name, Summary; exposed Usage tag (`UI / Tag`) and row menu (`UI / Icon button` with More) | List rows. Selected = `acc-tint` + 3 px `acc` rule, summary in `ink2` (never `muted` on a selected row) |
| UI / Checkbox | State = Checked, Unchecked, Checked error, Unchecked error | Core flag; error variants for `core_not_contiguous` |
| UI / Icon button | State = Default, Focus (Focus/Ring), Disabled (40 %); nested icon swappable | Row actions and the row menu. The label is in `aria-label` and shown as a `UI / Tooltip` on focus and hover (3.17) |
| Icons / UI / Move up, Move down, Remove | 16 px, `ink2` stroke | Monter / Descendre / Supprimer la couche |

Reused: `UI / Button primary`, `secondary`, `ghost`, `UI / Button danger`, `UI / Text field`, `UI / Select` (Default, Focus), `UI / Compact field` (Default, Error), `UI / Banner`, `UI / Badge`, `UI / Tag`, `UI / Tooltip`, `UI / Toast with undo`, Icons Info, Error, Check, More, Plus, Reset, Search.

## Contrast

Every colour is bound to the `Mode` / `Core` variables (no new variable). New pairs, computed with `ui/src/shell/contrast.ts`:

| Pair | Light | Dark |
| --- | --- | --- |
| muted on surface (face labels, footer note, categories, hints) | 5.86 | 5.60 |
| ink on hover (active menu and list item) | 14.51 | 9.24 |
| ink2 on hover (shortcut and category in the active item) | 7.42 | 6.06 |
| ink on acc-tint (selected layer row) | 13.88 | 9.31 |
| on-acc on acc (checkbox tick) | 6.39 | 7.52 |
| acc on surface (checked box, selected number, Gérer les matériaux…) | 6.39 | 6.00 |
| field-line on surface (unchecked box, 3:1) | 3.43 | 3.71 |
| err on surface (messages, error rule and checkboxes) | 6.57 | 6.18 |
| ink2 on raised (table header, totals note) | 7.91 | 6.93 |
| ink2 on warn-tint / acc-tint (preview outlines, 3:1) | 7.42 / 7.10 | 7.21 / 6.11 |

`muted` on `hover` is only 4.34:1 in dark mode, so text in an active (hovered or keyboard-active) item uses `ink2`, never `muted`.

## Deviations from the UX notes, and points for front

1. **Board numbering** differs from the UX mapping because each list state got its own board: 3.8 empty, 3.9 filled, 3.10 duplicate, 3.11 rename, 3.12 delete, 3.13 new wall, 3.14 wall, 3.15 slab, 3.16 errors, 3.17 reordering, 3.18 discard, 3.19 picker, 3.20 after Apply.
2. **Preview below the table**, not in a right column: at 960 px the eight columns need the full width, so preview and totals sit side by side under the table (UX allowed "below the table otherwise").
3. **Slab preview is vertical** (top to bottom), which reads like a slab section; the wall preview stays horizontal.
4. **The segmented filter has three options**, drawn with the `UI / Segmented toggle` tokens (that component has two); `role="radiogroup"` or a tablist in code.
5. **"Nouvelle composition…"** is a menu button (`aria-haspopup="menu"`) with the items "Composition de mur" / "Composition de plancher"; the menu is not drawn, it is the row-menu style of 3.9.
6. **Row messages** sit in a full-width message row under the layer (aligned under the field in error), not inside the cell, so the 44 px rows keep their height; `aria-describedby` points the field at it.
7. **Material error look**: `UI / Select` has no error variant; 3.16 shows a 2 px `err` border on the select box. Add an Error variant to the select in code.
8. **Core column header and priority column header** each carry an ⓘ button; the core help text is `buildups.core.help`.
9. **Live R and totals** in the boards are illustrative values from `buildups.preview`; the material values are examples (the UI never stores them).
10. **Live region** text is shown in a dashed annotation on 3.17; it is not part of the UI.
11. **States not drawn**, built from the same parts: F2-F loading (skeleton rows) and `buildups.list` banner with **Réessayer**, filtered empty "Aucune composition de plancher.", `too_many_layers` (Ajouter une couche disabled with tooltip), `unknown_layer` / `not_found` banners, `duplicate_name` under the copy name, the stacked Materials dialog over the editor.
12. French words chosen: build-up = **composition**, core = **noyau**, substrate = **support**, air gap = **lame d'air**, join priority = **priorité de jonction**; keyboard keys **Entrée**, **Échap**, **Suppr**, **Maj**.
