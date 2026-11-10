# Materials and build-ups — UX notes (sub-issues #86 and #87, MAT-02, MAT-03)

For `ui` (T4: F1 boards 4.7+ on page `4 · Structure modelling`; T7: F2 boards 3.8+ on page `3 · Architecture modelling`) and then `front` (T5, T8). Read with `ux-brief.md`, `spec.md` (D3–D6) and `api-contract.md`. Text only: no wireframes were added to the design file, and the shipped boards (3.3, 4.6, 2.x) are untouched. Patterns are reused from `.orchestrator/features/georeferencing/design/ux-notes.md` (called "georef notes" below). `ui` does the contrast check (`ui/src/shell/contrast.ts`); `color/danger` already exists from georeferencing.

## Decisions `ui` and `front` must keep

1. **Two dialogs, two entry points.** "Materials…" and "Build-ups…" are separate modal dialogs opened from the title bar (next to "Project location…"). They are not tabs of one "Types and materials" area: the audiences differ (a structural engineer picks grades, an architect composes walls) and the build-up editor must move unchanged into the future wall-type editor (ARC-30).
2. **Materials: edit in place, no Apply.** Materials are a library you maintain; each edited value is saved when committed (Enter or leaving the field) with `materials.update` — one undo step each. The footer has a single **Done** button. **Build-ups: draft and Apply.** The editor holds a draft; **Apply** sends one `buildups.create` or `buildups.update` (one undo step); **Cancel** discards.
3. **Dialog shell as in georef notes**: modal, focus trapped, Esc closes the innermost open thing (popover, inline confirm, sub-view) first, then the dialog; returns focus to the opener; errors inline under the field with `aria-describedby`/`aria-invalid`, server errors in a `role="alert"` banner at the top, destructive confirmation inline replacing the footer with default focus on the safe button; never `confirm()`/`alert()`. Apply/Done stay enabled; clicking Apply with errors focuses the first invalid field and announces the count.
4. **Sizes.** Materials dialog 880 × up to 640 px (minimum window: 720 px wide, panes stack with the list above the detail and a "Back to the list" link). Build-ups list dialog 640 px; build-up editor opens in the same dialog frame, widened to 960 px. French is ~30 % longer: labels wrap to two lines above the field, buttons grow, table headers wrap (never truncate); material names truncate with an ellipsis and a tooltip only in the table, never in a field.
5. **Units and numbers.** UI shows mm (thickness), MPa (strengths and stresses), GPa (moduli), each fixed per property and never chosen by magnitude, kg/m³, W/(m·K), m²·K/W (thermal resistance). Locale decimal separator, input accepts `,` and `.` (georef rule 8). Thickness shown with up to 1 decimal (`12,5`), stresses/moduli with up to 3 significant decimals trimmed of trailing zeros, strains in ‰ (permille) with the ‰ suffix.
6. **Colour is never the only signal.** "Entered by user" uses a text badge plus a left rule (like the georef "Edited" badge); core layers use a text mark "Core" plus a hatch in the preview; in-use uses text.
7. **Plural messages need i18n support.** `translateWith` (`ui/src/i18n/i18n.ts`) only fills `{name}` placeholders; the ICU `{count, plural, one {…} other {…}}` strings in the copy tables (`delete.inUse`, `row.summary`, `delete.body`, `layers.removed`, `total.resistanceWhy`, `error.summary`) would be shown raw. `front` either extends `translateWith` with plural support (and tests it in EN and FR, where 0 and 1 are singular) or splits each into `…One` / `…Other` keys chosen with `Intl.PluralRules`.
8. **Toasts "with undo".** The existing toast (`GeorefContext.notify`) is a plain message with no action. An Undo button on the toast is new: it calls `history.undo`, which undoes the last step, so it is only offered while that step is still the one the toast announces (hide it on the next model change).

---

# F1 — Materials (#86)

## Entry points and shortcuts

- Title-bar ghost button **Materials…**, `aria-haspopup="dialog"`, `aria-keyshortcuts="Control+Shift+M Meta+Shift+M"`.
- Global shortcut **Ctrl/Cmd+Shift+M**, ignored while a modal dialog is open. Today `dialogOpen` only tracks the location dialog (`GeorefContext`), and the Ctrl/Cmd+Shift+L listener lives in `GeorefContext` with no guard at all (it opens the location dialog even over another dialog). `front` must hoist one shared "modal open" state (and the toast, also owned by `GeorefContext` today) into the shell, and apply it to every global shortcut, L included. The one allowed exception is "Manage materials…" from the build-up editor (stacked dialog).
- From the build-up editor's material picker: **Manage materials…** (stacked dialog, see F2-3).
- No command palette yet (UI-03): when it exists the entry is "Materials…".

## Layout

Left pane (320 px): title "Materials", search field, category filter (select: All categories + the 8 categories), list of **project materials**, then two buttons at the bottom: **Add from library…** (primary) and **New material…**. Right pane: the **detail** of the selected material, or the library browser / new-material form (sub-views, below). Footer: **Done**.

List row: name; second line "category · grade" (for example "Concrete · C25/30", "Insulation · custom"); right aligned "Used by 3" (hidden when 0). Selected row marked by a left rule plus `aria-selected`.

## States

| State | What the user sees |
| --- | --- |
| **F1-A Empty** (new project) | Left list replaced by an explanation: "No materials yet. Add common grades such as C25/30 concrete or S355 steel from the library, or create your own." and the two buttons. Right pane: same explanation hidden; shows the library browser directly so a new user has one click less. |
| **F1-B Filled** | List with rows; first row selected on open; detail with tabs. |
| **F1-C Library browser** | Right pane sub-view (back link "Back to {material}" or "Back to the list"): search field, category chips, results list (name, category, design code), selecting a grade shows its values read-only (same table as the detail) and a name field prefilled with the grade name, **Add to project**. |
| **F1-D New material** | Right pane sub-view: Name, Category, then **Create**. After creation the detail opens on the Mechanical tab with every value empty and "Entered by user" marker on filled ones. |
| **F1-E Editing a value** | The value cell becomes a number field with the unit as suffix; Enter or blur saves; Esc cancels the edit. |
| **F1-F Field error** | Message under the row's field, row stays in edit mode, value not saved. |
| **F1-G Server error** | Banner at the top of the dialog; the list and detail stay usable. |
| **F1-H Delete confirmation** | Inline panel replacing the detail footer (see below). |
| **F1-I In use** | The confirmation never appears; the detail shows an info banner and Delete is explained. |
| **F1-J Loading** | Skeleton rows while `materials.list`/`materials.get` load; dialog buttons that need data are disabled. |
| **F1-K Library empty result** | "No grade matches “{query}”." |

## Detail pane

Header: material name (heading), the line "{category} · {design code or ‘No design code’}", and an overflow menu with **Rename…** and **Delete…**. Below: the **behaviour law** card, then the tabs.

**Tabs**: Mechanical, Thermal enabled; Graphics and Environmental are **shown disabled** with the tooltip/description "Not available yet." Reason: showing them tells the user what the product will cover and keeps the tab order stable when they arrive (MAT-04, documentation work); a disabled tab stays reachable by arrow keys but not activatable, `aria-disabled="true"`. Tabs follow the WAI-ARIA tab pattern (←/→, Home/End; as in the shell's right panel).

**Property table** per tab, one row per property: columns *Property* (name + symbol, for example "Characteristic compressive strength · fck"), *Value* (number + unit), *Source*. The Mechanical tab shows the category's mechanical properties and density; Thermal shows λ, specific heat (density is shown in Mechanical only). Optional properties with no value show "—" and in a custom material a button "Add value". A property that is required and missing cannot exist (the API refuses to clear it).

- **Source column**: "EN 1992-1-1 · Table 3.1", or the badge **Entered by user**. Library sources are plain text, not links.
- **Partial factors** (γc, γs, γM0…) are annex-dependent: the row shows two sub-values on two lines: "Recommended 1.5 · EN 1992-1-1 · 2.4.2.4" and "France 1.5 · NF EN 1992-1-1/NA · 2.4.2.4". Both are editable; neither is "selected" because the project-wide annex choice is out of scope.
- **Edited value**: the row gets the **Entered by user** badge and a left rule, and a **Reset to library value** button (icon + label, visible only for a changed value of a library-based material; tooltip shows the library value: "Library value: 25 MPa"). Reset sends `null`; the badge disappears and the source returns. On a custom material there is no reset; a value cleared (optional only) sends `null`.
- **Behaviour law card** (read-only): "Behaviour law: Parabola-rectangle" with the source line, and the sentence "Used for section design. Not editable." The stress–strain sketch is **not part of this feature**: there is no curve API and inventing one in the UI would duplicate the maths of `materials` (ENG-04). Open question 1 asks whether to add `materials.curve`.

## Flow F1 — numbered steps

1. User presses Ctrl/Cmd+Shift+M or clicks **Materials…**. Dialog opens (F1-J then F1-A or F1-B). Focus goes to the search field (F1-B) or to **Add from library…** (F1-A).
2. **Browse.** Typing in the search field filters by name (`materials.list` with `search`, debounced 200 ms, outdated answers ignored); the category select filters by category. ↑/↓ in the list moves the selection, which updates the detail; Tab leaves the list.
3. **Add from library.** Press **Add from library…** → F1-C. Search/filter (`materials.library`, paged); ↑/↓ moves through grades; the right side shows the grade values (`materials.grade`). The name field is prefilled; the user may rename. **Add to project** → `materials.add_from_library`. Success: back to the list with the new material selected, toast "{name} added" with undo, focus on the new row. Error `duplicate_name`: field error "A material with this name already exists. Choose another name." (the name field is focused). `unknown_grade`: banner "This grade is no longer in the library."
4. **Create a custom material.** Press **New material…** → F1-D: Name (required, 1–120), Category (required select). **Create** → `materials.create`. Errors: `invalid_name` "Enter a name between 1 and 120 characters." / `duplicate_name` (as above). On success the detail opens on the Mechanical tab.
5. **Read.** Select a row; the detail shows tabs, table and law (above).
6. **Edit a value.** Activate the value (click, or Enter/F2 on the row's value button). Edit, Enter saves via `materials.update { properties: { key: value } }`. Result: the row shows **Entered by user**. Errors from the API map to the row: `invalid_value` → "Enter a valid value for this property." (ν example: help text under the field shows the allowed range when known: "Between 0 and 0.5."), `unknown_property` → banner (should not occur), `required_property` → "This value is required." Any other error → server banner.
7. **Reset.** Press **Reset to library value** → `materials.update` with `null`. Live region: "{property} reset to the library value."
8. **Rename.** Menu → **Rename…**: the name in the header becomes a text field (Enter saves, Esc cancels) → `materials.update { name }`; `duplicate_name` / `invalid_name` as above, under the field.
9. **Delete.** Menu → **Delete…**:
   - not in use (`used_by` = 0): inline confirmation in the detail footer: "Delete {name}?" "This removes the material from the project. You can undo this." Buttons **Keep material** (default focus) and **Delete material**. → `materials.delete`; the list selects the next row (or the previous, or returns to F1-A); toast "{name} deleted" with undo.
   - in use (`used_by` > 0, known from `materials.list`): the menu item stays enabled; activating it shows an **info banner** in the detail: "{count} build-up layers use {name}. Change their material in the build-ups, then delete it." with a **Show build-ups…** button that opens the build-up list (F2). If the server nevertheless answers `in_use` (race), the same banner appears, using the returned `used_by`.
10. **Close.** **Done** or Esc closes; edits were already saved. Focus returns to the opener.

## Focus order (F1)

Dialog title → search field → category select → project list (one tab stop, arrow keys inside) → **Add from library…** → **New material…** → detail header (name heading is not a stop) → overflow menu → tabs (one tab stop) → property table: each row's **value button** then **Reset** (when shown) → **Done**. In sub-views: back link → fields in visual order → primary button. Opening a sub-view focuses its first field; going back focuses the control that opened it.

## Keyboard (F1)

| Where | Keys | Action |
| --- | --- | --- |
| Global | Ctrl/Cmd+Shift+M | Open Materials |
| List | ↑ ↓ Home End | Move selection |
| List | Delete | Start the delete flow (confirmation or in-use banner) |
| List | F2 | Rename the selected material |
| Tabs | ← → Home End | Move between tabs (disabled ones are skipped for activation) |
| Value | Enter / F2 / click | Edit; Enter saves, Esc cancels |
| Library | ↑ ↓ Enter | Move, choose a grade (Enter in the name field adds) |
| Dialog | Esc | Innermost first: edit → confirmation → sub-view → dialog |

## Copy — F1 (English; keys under `materials.*`)

| Key | EN |
| --- | --- |
| `open` | Materials… |
| `title` | Materials |
| `search.label` | Search materials |
| `search.placeholder` | Search by name |
| `category.label` | Category |
| `category.all` | All categories |
| `category.concrete` / `reinforcement` / `steel` / `timber` / `masonry` / `insulation` / `finish` / `generic` | Concrete / Reinforcement / Steel / Timber / Masonry / Insulation / Finish / Generic |
| `list.label` | Project materials |
| `list.usedBy` | Used by {count} |
| `list.customGrade` | custom |
| `list.noMatch` | No material matches “{query}”. |
| `empty` | No materials yet. Add common grades such as C25/30 concrete or S355 steel from the library, or create your own. |
| `action.addFromLibrary` | Add from library… |
| `action.new` | New material… |
| `action.done` | Done |
| `action.rename` | Rename… |
| `action.delete` | Delete… |
| `library.title` | Add from library |
| `library.search.label` | Search the library |
| `library.empty` | No grade matches “{query}”. |
| `library.name` | Name in this project |
| `library.nameHelp` | Values are copied into your project. Later library updates never change your materials. |
| `library.add` | Add to project |
| `library.back` | Back to the list |
| `new.title` | New material |
| `new.name` | Name |
| `new.category` | Category |
| `new.create` | Create |
| `detail.designCode` | {category} · {code} |
| `detail.noCode` | No design code |
| `tab.mechanical` / `thermal` / `graphics` / `environmental` | Mechanical / Thermal / Graphics / Environmental |
| `tab.unavailable` | Not available yet. |
| `law.title` | Behaviour law |
| `law.note` | Used for section design. Not editable. |
| `law.parabola_rectangle` / `bilinear` / `linear_elastic` / `elastic_plastic` / `elastic_brittle` / `none` | Parabola-rectangle / Bi-linear / Linear elastic / Elastic–perfectly plastic / Elastic, brittle in tension / None (no structural use) |
| `table.property` / `value` / `source` | Property / Value / Source |
| `value.none` | — |
| `value.add` | Add value |
| `value.edit` | Edit {property} |
| `source.user` | Entered by user |
| `source.clause` | {document} · {clause} |
| `annex.recommended` / `annex.fr` | Recommended / France |
| `reset` | Reset to library value |
| `reset.tooltip` | Library value: {value} |
| `reset.done` | {property} reset to the library value. |
| `rename.label` | Material name |
| `delete.title` | Delete {name}? |
| `delete.body` | This removes the material from the project. You can undo this. |
| `delete.keep` | Keep material |
| `delete.confirm` | Delete material |
| `delete.inUse` | {count, plural, one {# build-up layer uses} other {# build-up layers use}} {name}. Change their material in the build-ups, then delete it. |
| `delete.showBuildups` | Show build-ups… |
| `toast.added` | {name} added |
| `toast.deleted` | {name} deleted |
| `error.invalid_name` | Enter a name between 1 and 120 characters. |
| `error.duplicate_name` | A material with this name already exists. Choose another name. |
| `error.invalid_value` | Enter a valid value for this property. |
| `error.range` | Between {min} and {max}. |
| `error.required_property` | This value is required. |
| `error.unknown_grade` | This grade is no longer in the library. |
| `error.not_found` | This material no longer exists. |
| `error.generic` | Something went wrong. Try again. |
| `error.summary` | {count} fields need attention. |
| `loading` | Loading materials… |

(Messages never repeat model values, SEC-03; `list.noMatch` and `library.empty` echo only the user's own search text. Property names and symbols come from the property catalogue and are translated in `materials.property.*`; symbols — fck, Ecm, λ — are not translated.)

---

# F2 — Build-ups (#87)

## Entry points and shortcuts

- Title-bar ghost button **Build-ups…**, `aria-keyshortcuts="Control+Shift+B Meta+Shift+B"`; global **Ctrl/Cmd+Shift+B**, same guards.
- From F1's in-use banner: **Show build-ups…**.
- Existing shortcuts in `ui/src/shell/Shell.tsx` (checked): Ctrl+` (console), Ctrl/Cmd+Shift+K (Coordinates), Ctrl/Cmd+Shift+L (location, in `aria-keyshortcuts`), F6 (regions). No Ctrl+Alt (AltGr on AZERTY). M and B are free. Caveat: in a regular browser (the Playwright runs) Ctrl+Shift+B toggles the bookmarks bar and Ctrl+Shift+M switches profile; in the Tauri webview and with `preventDefault` they are ours. Open question 2.

## Part 1 — the list

Dialog "Build-ups", 640 px. Header: filter (segmented: All / Wall / Slab) and a **New build-up…** menu button with the two items "Wall build-up" and "Slab build-up". The list: row = name, usage tag (Wall/Slab), "{n} layers · {total} mm", and a row menu (**Edit…**, **Duplicate…**, **Rename…**, **Delete…**). Enter on a row edits it. Footer: **Close**.

| State | What the user sees |
| --- | --- |
| **F2-A Empty** | "No build-ups yet. A build-up lists the layers of a wall or a slab, from one face to the other." + **New wall build-up** and **New slab build-up** buttons. With the filter set and no match: "No {usage} build-ups." |
| **F2-B Filled** | Rows as above. |
| **F2-C Duplicate** | Row menu → inline mini-form in the row: Name prefilled "{name} copy", **Duplicate** / Cancel → `buildups.duplicate`; `duplicate_name` / `invalid_name` as field errors. New row selected and focused. |
| **F2-D Rename** | Row name becomes a field (Enter saves, Esc cancels) → `buildups.update { name }`. |
| **F2-E Delete confirmation** | Inline in the row area: "Delete {name}?" "Its {count} layers are deleted with it. You can undo this." **Keep build-up** (default) / **Delete build-up** → `buildups.delete`. Nothing references build-ups yet (D5), so there is no in-use state; when wall types arrive (epic #88) a refusal will be added here, and the row menu is built to take a message. |
| **F2-F Loading / server error** | Skeleton rows; banner on `buildups.list` failure with **Try again**. |

Creating: **New wall/slab build-up** opens the editor (Part 2) with usage fixed, an empty name and one starter layer (function Structure, no material, thickness empty) so the table is never empty, and the Name field focused. The usage cannot be changed afterwards (the layer order meaning differs).

## Part 2 — the editor (reusable component)

The editor is a self-contained form component with **inputs**: the draft (name, description, usage, layers) and the list of project materials; **outputs**: the draft changes and validation state; **slots**: an `extra` slot above the table where the wall-type editor adds the location line (ARC-30), and a footer owned by the host dialog (Apply/Cancel). It does not call the API itself; the host does. This lets board 3.3's wall-type editor embed it.

Layout, top to bottom: Name (required) and Description (optional) → **Layers** group → section preview + totals (right column ≥ 960 px; below the table otherwise) → footer **Cancel** / **Apply**.

**Layer table** — columns:

| # | Column | Control | Notes |
| --- | --- | --- | --- |
| 1 | Position | number, read-only, "1…n" | with the face label on first/last: wall "Exterior" … "Interior", slab "Top" … "Bottom" |
| 2 | Function | select (Structure, Substrate, Insulation, Air gap, Membrane, Finish) | changing it updates priority and core **only if they are still the defaults** |
| 3 | Material | select over project materials (searchable listbox) | last item "Manage materials…" opens F1 stacked; on close the picker refreshes and keeps the draft |
| 4 | Thickness | number, mm | membrane may be 0 |
| 5 | Join priority | select 1–5, shows “2 (default)” until overridden | header has an info button: help popover "When two walls meet, layers with a higher priority (1) run through layers with a lower priority (5). Layers with the same priority meet." Reset item "Use the default for this function" in the select |
| 6 | Core | checkbox | default checked for Structure |
| 7 | R | read-only, m²·K/W | `thermal_resistance`, "—" with tooltip when the material has no λ: "No thermal conductivity for this material." Computed by the server, so it appears after the draft is saved or refreshed: **while editing it shows the value from the last saved state and “—” for new/changed layers** (open question 3: ask for a draft preview call or compute in the UI) |
| 8 | Actions | **Move up**, **Move down**, **Remove** icon buttons with labels | Move up disabled on the first row, Move down on the last, Remove disabled with a single layer |

Under the table: **Add layer** (adds after the selected layer, or at the end; the new row's Function select is focused). Footer of the table: "Total thickness: {mm} mm" and "Thermal resistance: {value} m²·K/W" or "—" with the reason "Every material needs a thermal conductivity." (for example "2 layers have no thermal conductivity.").

**Section preview** (**schematic**): a horizontal strip of rectangles, one per layer, widths proportional to thickness but clamped to a minimum width for legibility, each filled by a function pattern, labelled by its position number; core layers carry a bracket "Core" below; the selected layer (focus or hover in the table) is outlined; faces labelled "Exterior" / "Interior" (wall) or "Top" / "Bottom" (slab) at the ends; caption "Schematic, not to scale." The preview is decorative for assistive tech (`aria-hidden`); the table is the accessible source. The location line of ARC-30 goes into the same preview later (a vertical line marker), hence the `extra` slot and an empty overlay layer in the design.

## Reordering with the keyboard (UI-06)

- Each row is a `role="row"`; **Move up / Move down** buttons do the job with no pointer, and are in the tab order.
- Shortcut on a focused button or checkbox inside a row: **Alt+↑ / Alt+↓** moves the layer up or down (not AltGr composition on AZERTY). It is **not** bound on text or number fields and selects, where Alt+↑/↓ belongs to the control (it opens a native select on Windows and Linux and moves the caret by paragraph on macOS); from those, use the Move buttons.
- After a move, focus **stays on the moved layer's** control (the same button or field), not on the row index.
- Live region (`aria-live="polite"`): "Layer moved to position {n} of {total}: {material}, {thickness} mm." At the ends (button disabled but Alt+arrow pressed): "Already the first layer." / "Already the last layer."
- Pointer drag handle is **optional** and not required by any flow; if `ui` draws one, it is a visual handle on the position cell with `aria-hidden` and the buttons stay.
- Removing a layer moves focus to the next row's Remove-equivalent (the same control) or the previous one if it was last; announce "Layer removed. {n} layers left."

## Flow F2 — numbered steps

1. User presses Ctrl/Cmd+Shift+B or **Build-ups…**; the list opens (`buildups.list`), focus on the first row, or on **New wall build-up** if empty.
2. **Filter** by usage with the segmented control (`buildups.list { usage }`).
3. **Create**: **New build-up…** → Wall / Slab → editor. The user enters a name, fills the starter layer, adds layers. Needing a material that does not exist: the picker's **Manage materials…** → F1 stacked → add → close → the picker lists it and selects it for this layer.
4. **Edit**: Enter or **Edit…** on a row → `buildups.get` → editor with the saved state. Title "Edit build-up — {name}".
5. **Layer edits** change the draft only. Totals update immediately in the UI from the draft's thicknesses; thermal values as in the table notes.
6. **Apply** → `buildups.create` (new) or `buildups.update` with the full layer list, ids kept for existing layers (reordering never changes them). Success: back to the list, selected row focused, toast "Build-up applied" with undo. Nothing was sent if nothing changed: Apply just closes.
7. **Cancel** or Esc with a changed draft: inline confirmation replaces the footer: "Discard your changes?" **Keep editing** (default) / **Discard changes**. With no changes it closes directly.
8. **Duplicate**, **Rename**, **Delete**: Part 1.

## Errors (F2)

All shown inline at the row or field concerned, with `aria-describedby`, summarised by the Apply announcement (“{count} problems to fix.”); `index` from the API is mapped to the row.

| API code | Where | Message (EN) |
| --- | --- | --- |
| client-side empty/invalid | Name | “Enter a name between 1 and 120 characters.” |
| `duplicate_name` | Name | “A build-up with this name already exists. Choose another name.” |
| `no_layers` | Layers group (above the table) | “Add at least one layer.” (the client also prevents removing the last layer) |
| `too_many_layers` | Layers group | “A build-up can have at most 32 layers.” (Add layer is disabled at 32 with the same text as a tooltip) |
| `invalid_thickness` | Thickness field of row `index` | “Enter a thickness greater than 0 mm. Only a membrane can be 0 mm.” |
| `core_not_contiguous` | Core checkboxes: **all core rows and the gap rows** get a left error rule and the group message | Group: “Core layers must be next to each other.” Row: “This layer is between core layers.” |
| `unknown_material` | Material select of row `index` | “This material no longer exists in the project. Choose another one.” |
| `invalid_priority` | Priority select of row `index` | “Choose a priority from 1 to 5.” |
| `unknown_layer` | banner | “This build-up changed since you opened it. Close and open it again.” |
| `not_found` | banner, then back to the list | “This build-up no longer exists.” |
| other | banner | “Something went wrong. Try again.” |

The air-gap rule of D5 (an air gap references an air or generic material) is not in the API error list: `unknown_material` is reused as `invalid_material` by the backend or the editor filters the picker for air gaps: open question 4.

## Focus order (F2)

List: filter segments → **New build-up…** → rows (arrow keys, one stop) → each row menu → **Close**. Editor: Name → Description → table (row by row: Function → Material → Thickness → Priority → Core → Move up → Move down → Remove; a row is entered with Tab, arrows do not leave cells) → **Add layer** → priority help button is reached from the header before the first row → Cancel → Apply. Opening the editor focuses Name (new) or the first layer's Function (edit). Closing returns focus to the row that opened it (or to the list button).

## Keyboard (F2)

| Where | Keys | Action |
| --- | --- | --- |
| Global | Ctrl/Cmd+Shift+B | Open Build-ups |
| List | ↑ ↓ Enter | Move, edit |
| List | Delete / F2 | Delete flow / rename |
| Table (buttons, checkbox) | Alt+↑ / Alt+↓ | Move the layer |
| Table | Ctrl+Enter | Add a layer after the current one |
| Dialog | Esc | Innermost first, then discard confirmation if changed |
| Dialog | Enter | Apply (not inside a select/listbox/button) |

## Copy — F2 (English; keys under `buildups.*`)

| Key | EN |
| --- | --- |
| `open` | Build-ups… |
| `title` | Build-ups |
| `filter.all` / `wall` / `slab` | All / Wall / Slab |
| `usage.wall` / `slab` | Wall / Slab |
| `action.new` | New build-up… |
| `action.newWall` / `newSlab` | New wall build-up / New slab build-up |
| `action.edit` / `duplicate` / `rename` / `delete` | Edit… / Duplicate… / Rename… / Delete… |
| `action.close` | Close |
| `action.apply` / `cancel` | Apply / Cancel |
| `row.summary` | {count, plural, one {# layer} other {# layers}} · {total} mm |
| `empty` | No build-ups yet. A build-up lists the layers of a wall or a slab, from one face to the other. |
| `empty.filtered` | No {usage} build-ups. |
| `duplicate.name` | Name of the copy |
| `duplicate.suggest` | {name} copy |
| `duplicate.confirm` | Duplicate |
| `delete.title` | Delete {name}? |
| `delete.body` | Its {count, plural, one {# layer is} other {# layers are}} deleted with it. You can undo this. |
| `delete.keep` / `confirm` | Keep build-up / Delete build-up |
| `editor.titleNew` | New {usage} build-up |
| `editor.titleEdit` | Edit build-up — {name} |
| `editor.name` | Name |
| `editor.description` | Description (optional) |
| `layers.title` | Layers |
| `layers.face.wall.start` / `end` | Exterior / Interior |
| `layers.face.slab.start` / `end` | Top / Bottom |
| `layers.col.position` | # |
| `layers.col.function` | Function |
| `layers.col.material` | Material |
| `layers.col.thickness` | Thickness (mm) |
| `layers.col.priority` | Join priority |
| `layers.col.core` | Core |
| `layers.col.resistance` | R (m²·K/W) |
| `function.structure` / `substrate` / `insulation` / `air_gap` / `membrane` / `finish` | Structure / Substrate / Insulation / Air gap / Membrane / Finish |
| `material.choose` | Choose a material |
| `material.manage` | Manage materials… |
| `priority.default` | {n} (default) |
| `priority.useDefault` | Use the default for this function |
| `priority.help.title` | About join priority |
| `priority.help` | When two walls meet, layers with a higher priority (1) run through layers with a lower priority (5). Layers with the same priority meet. |
| `core.help` | The core is the load-bearing part of the build-up. Core layers must be next to each other. |
| `resistance.none` | — |
| `resistance.why` | No thermal conductivity for this material. |
| `layers.add` | Add layer |
| `layers.moveUp` / `moveDown` / `remove` | Move layer {n} up / Move layer {n} down / Remove layer {n} |
| `layers.moved` | Layer moved to position {n} of {total}: {material}, {thickness} mm. |
| `layers.alreadyFirst` / `alreadyLast` | Already the first layer. / Already the last layer. |
| `layers.removed` | Layer removed. {count, plural, one {# layer} other {# layers}} left. |
| `layers.max` | A build-up can have at most 32 layers. |
| `total.thickness` | Total thickness: {total} mm |
| `total.resistance` | Thermal resistance: {value} m²·K/W |
| `total.resistanceNone` | Thermal resistance: — |
| `total.resistanceWhy` | {count, plural, one {# layer has} other {# layers have}} no thermal conductivity. |
| `preview.title` | Section preview |
| `preview.note` | Schematic, not to scale. |
| `preview.core` | Core |
| `discard.title` | Discard your changes? |
| `discard.keep` / `confirm` | Keep editing / Discard changes |
| `toast.applied` | Build-up applied |
| `toast.deleted` | {name} deleted |
| `error.*` | the messages of the table “Errors (F2)” |
| `error.summary` | {count, plural, one {# problem} other {# problems}} to fix. |
| `loading` | Loading build-ups… |

---

## Reuse of georeferencing components and what is new

Reuse as is: Dialog, TextField (+ units suffix), Disclosure, Banner (error and info), InlineConfirm, Toast, InfoPopover, SegmentedToggle (filters). **New**, to be added to `Brand guide › 8 · Form and dialog components` by `ui`: Select, SearchSelect/listbox (shared with the CRS combobox), Tabs (the shell's tab style, in-dialog), Badge ("Entered by user", usage tags), DataTable with an editable cell and a row action group, IconButton with a visible label on focus, ListRow (selectable), SectionPreview (layer strip), Skeleton row.

## Frames for `ui` (mapping)

| Flow part | Boards | Content |
| --- | --- | --- |
| F1 (T4, page 4, **4.7+**, light and dark) | 4.7 Materials — empty (F1-A); 4.8 filled, Mechanical tab (F1-B); 4.9 Thermal tab with a user-entered value (F1-B/E); 4.10 library browser (F1-C, F1-K); 4.11 new material (F1-D); 4.12 editing + field error + server error banner (F1-E/F/G); 4.13 delete confirmation (F1-H); 4.14 in use (F1-I) |
| F2 (T7, page 3, **3.8+**, light and dark) | 3.8 Build-ups — empty (F2-A); 3.9 list filled (F2-B/C/D/E); 3.10 editor, wall, filled (with preview, totals); 3.11 editor, slab; 3.12 editor — errors (`invalid_thickness`, `core_not_contiguous`, `unknown_material`, banner); 3.13 reordering (focus on the moved layer, live-region caption); 3.14 discard confirmation; 3.15 material picker with “Manage materials…” |

Not a board: keyboard-only reordering is a behaviour; show the focus ring on a moved row in 3.13.

## Open questions

1. **Stress–strain sketch.** Worth a `materials.curve { material, law }` returning sampled (strain, stress) points computed by `materials`, so the UI draws and never recomputes (ENG-04)? Proposed: **not in this feature**; the detail shows the law name and source only. `lead`/`back` to decide.
2. **Shortcuts.** Ctrl/Cmd+Shift+M and Ctrl/Cmd+Shift+B, consistent with Shift-letter shortcuts K and L, but collide with browser shortcuts in non-desktop runs (Playwright, web). Accept, or choose others (for example Ctrl/Cmd+Shift+U for build-Ups)? Needs `lead`'s confirmation like the L shortcut.
3. **Live thermal resistance in the draft.** The API returns `thermal_resistance` only for saved build-ups. For layers edited in the draft, either `front` computes d/λ in the UI from `materials.get` (a duplicate of a one-line formula; λ is already needed), or the backend adds a pure `buildups.preview { layers }` query. Proposed: the UI computes d/λ for the live total (trivial, exact division, no libm) and the saved value from the server replaces it after Apply.
4. **Air gaps and materials** (D5 rule) have no matching error code: add `invalid_material` (with `index`) to the contract, or filter the picker. Proposed: both — the picker offers only air/generic materials for the Air gap function, and the API refuses the rest with `invalid_material`. When the user changes a layer's function to Air gap while its material is not air/generic, the material select shows the error `unknown_material` copy variant: “This material cannot be used for an air gap.” (`back`/`front`).
5. **Min/max hints.** Whether `PropertyValue` carries the allowed range (min/max) so the field error can say “Between 0 and 0.5.” A hint only; without it the generic `invalid_value` message is used (`back`).
6. **Library "already in project".** Should the library list mark grades already added ("In project")? Proposed: yes, a text tag; adding again is allowed with a distinct name (brief step 3). Needs `materials.list` `grade` keys (already in the contract) — no API change.
7. **Material units choice** (for example kN/cm² instead of MPa) — out of scope; fixed units as above.
8. **French copy.** English only here, as asked. `ui` writes the French strings in the boards and `front` in `fr.json`; the longest risks are the table headers (“Priorité de jonction”, “Conductivité thermique”) and the in-use sentence.
9. **Annex-dependent values cannot be addressed.** `PropertyValue` carries `annex`, so a partial factor appears twice with the same `key` (recommended and France), but `materials.update { properties: { [key]: number | null } }` and `materials.create` address values by `key` only. The "both are editable" rule of the detail pane needs a way to name the annex (for example `{ key, annex, value }` entries). `back` to amend `api-contract.md` before T5.
10. **Thermal resistance of air gaps.** An air gap has a conventional resistance, not a d/λ, so a build-up with an air gap or a generic material will rarely sum to a thermal resistance ("every layer has one"). Decide whether air-gap materials carry an equivalent λ or the sum is shown partially with the reason (`total.resistanceWhy` already counts the layers without λ). `back`/`lead`.
11. **Live R duplicates maths.** Question 3 proposes computing d/λ in the UI while the stress–strain sketch was refused to avoid recomputing `materials` maths in the UI (ENG-04). Pick one rule; the consistent one is `buildups.preview`.
12. **Stacked dialogs.** The material picker's “Manage materials…” opens F1 over the editor (nested modal with a restored focus). If `ui`/`front` find the nested modal too heavy, the fallback is a small “New material…” popover that creates a custom material by name and category only, and leaves values to F1.

## Lead decisions on the open questions

1. No curve query; law name and source only.
2. Shortcuts accepted: Ctrl/Cmd+Shift+M (Materials) and Ctrl/Cmd+Shift+B (Build-ups). The desktop window gets them; e2e drives the page directly, as for K and L.
3, 11. One rule: the UI does no material maths. Live totals, thermal resistance and inline errors come from the new `buildups.preview` query (api-contract.md, amendment 6).
4. Both: the picker filters, the API refuses with `invalid_material` (amendment 8).
5. Yes: `PropertyValue.range` (amendment 2).
6. Yes: "In project" tag; adding again needs a distinct name.
7. Fixed units, as proposed.
8. As proposed.
9. Fixed in the contract (amendment 1).
10. Partial sum with the count of layers without thermal data (amendment 7).
12. Nested modal as proposed; `ui`/`front` may fall back to the popover if it proves too heavy, and say so in the PR.
