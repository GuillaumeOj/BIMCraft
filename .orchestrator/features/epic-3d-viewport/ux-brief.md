# 3D viewport — UX brief

For the UX node (flows and wireframes) and then the UI node (high-fidelity boards). Read `spec.md` (what users get, D1, D5, D6) and `api-contract.md` §2 (tools, commands, events, keyboard) first.

## Users and context

Architects and engineers working in the 3D view of a project: looking around a model, selecting elements, placing simple blocks (massing, placeholders) and moving them precisely, cutting the model to see inside, and hiding what is in the way. Mouse and trackpad first, keyboard always possible (UI-06), French and English (UI-04, French strings are longer).

## What exists

Page `3 · Architecture modelling` of the design file (`/Users/guillaume/Library/CloudStorage/GoogleDrive-guillaume.ojardias@gmail.com/My Drive/BIMCraft.figcraft`):

- **3.1 3D view — section box** (light/dark): box with face handles, View panel section "Section box" (Active, Cut faces, Extent X/Y/Z, Camera projection and Look from), buttons "Fit box to selection", "Reset box", "Turn off", status "Section box · 1 face moved", hint toast "drag a handle to move a face · Shift for 100 mm steps", view controls bottom right (scale, orbit, pan, full screen).
- **3.2 Visibility and isolate** (light/dark): Visibility popover with Category / Discipline tabs, eye toggles, "Isolated" and "Hidden" tags, "Isolate selection I", "Hide H"; View panel "Isolated" tag and "Visibility" section with "End isolate" and "Reset visibility"; hint "Isolate · only Structure is shown — Esc or End isolate to return".

Keep these; amend them only where the flows below need it, and say what changed.

## Flows to design (wireframes, then high fidelity)

1. **Navigate.** Orbit, pan, zoom with mouse, trackpad and keyboard; zoom fit; zoom to selection; preset views (a compact menu or view cube: pick one and justify it); perspective / parallel. Where the controls live (the bottom-right cluster of 3.1 is a start), their tooltips with shortcuts, and a first-time hint of the mouse and keyboard bindings that can be dismissed.
2. **Select.** Hover highlight, selected state, multi-select (Shift add, Ctrl/Cmd toggle), selection count in the status bar, Esc clears. What the screen reader hears.
3. **Place a block.** Tool in the bottom tool bar (the boards show Select, Wall, Slab…; add "Block" and say where it sits), first corner, second corner (rectangle preview with live sizes), height (drag or type), result selected. Typed values at each step. Snap markers and their labels.
4. **Move.** Drag a selected element directly, and the Move tool (base point → target point) for precision; rubber-band line with live distance; typed distance along the current direction; snap markers (endpoint, midpoint, edge, face, ground) and how snapping is suspended (Alt held); cancel with Esc; the toast after the move with Undo (as in board 3.20).
5. **Clipping planes.** Add a plane on a picked face, flip it, remove it, clear all; how it shows in the View panel next to the section box section.
6. **Section box and visibility.** As 3.1 and 3.2; check keyboard paths (every handle drag has a typed equivalent in the panel; the popover is fully keyboard operable).
7. **States of the view.** Empty project (no elements yet: what to do next, pointing to Place a block), loading the scene, no WebGPU on this computer (plain-language explanation, what the user can still do, link to the user guide's troubleshooting), device lost / scene rejected (recoverable error with "Reload view").

## Constraints

- The canvas is one focusable region labelled "3D view"; Tab enters and leaves it (never trapped); a visible focus ring. Inside it, keyboard bindings as in `api-contract.md` §2; propose changes there before the frontend starts, through the lead.
- Every pointer action has a keyboard or typed path; every icon-only control has a label and a tooltip with its shortcut; WCAG 2.2 AA contrast for overlays in both themes (snap markers and selection colours over any model colour: give them outlines).
- Announcements (live region) are short and throttled: the snap kind when it changes, the distance when a move ends, the selection count.
- The viewport colours (background, selection, hover, snap, preview ghost, cut tint) must be `Mode` variables so the frontend reads them from CSS tokens.
- No model state in the UI: panels show what the viewport reports (`ViewState`).
- Shortcuts go through the shell's global shortcut rules (quiet behind dialogs). No command palette yet: list each action with its shortcut so it can join the palette later.

## Deliverables

- **UX** (`ux` node): flows as annotated wireframes on page `3 · Architecture modelling` from **3.21** on, plus `design/ux-notes.md` in this folder: flows, states, keyboard map, announcements (EN and FR strings), and any change to the shortcuts of `api-contract.md`.
- **UI** (`ui` node): high-fidelity boards for every flow and state above, light and dark, reusing `Brand guide` components and `Mode`/`Core` variables (new variables for the viewport colours); amend 3.1 and 3.2 if the flows need it; PNG exports of every board and a `design/README.md` mapping boards to flows, in `.orchestrator/features/epic-3d-viewport/design/`.
- Both: change only your boards, re-read the file before changing it, commit nothing of it.
