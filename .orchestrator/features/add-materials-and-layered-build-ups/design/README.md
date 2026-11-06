# Materials and build-ups — UI design (sub-issue #86, MAT-02)

High-fidelity boards for the Materials dialog (flow F1 of `ux-notes.md`). They live in the shared project design file `/Users/guillaume/Library/CloudStorage/GoogleDrive-guillaume.ojardias@gmail.com/My Drive/BIMCraft.figcraft` (outside the repository), page **"4 · Structure modelling"**, after the concept board 4.6, which is unchanged. Each board is the full 1440 × 900 shell, in a light board (top row, y = 0) and a dark board (bottom row, y = 1100, `Mode` = Dark). `ux-notes.md` (from `ux`) is the text twin, with the English copy and the behaviour. The strings in the boards are French.

The build-up boards (F2, page 3, 3.8+) are task T7 and will be added to this file.

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
