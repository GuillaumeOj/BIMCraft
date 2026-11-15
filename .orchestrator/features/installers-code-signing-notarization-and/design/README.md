# Auto-update — boards (ENG-05)

Boards 8.7 to 8.19 on page **8 · Extensibility and system** of the FigCraft design file (`BIMCraft.figcraft`), drawn from `../ux-flow.md` and the wireframes in `wireframes/`. Light boards are in the top row, dark boards (Dark mode of the `Mode` collection) right below them. Every colour is a `Mode` token, so the dark boards differ only by mode. The text on the boards is the exact English copy of `ux-flow.md` section 6.

## Boards → files

| Board | Light | Dark |
| --- | --- | --- |
| 8.7 Title bar — update notice (all entry states, focus, French length check) | `8.7-title-bar-update-notice-light.png` | `8.7-title-bar-update-notice-dark.png` |
| 8.8 Updates — up to date | `8.8-updates-up-to-date-light.png` | `8.8-updates-up-to-date-dark.png` |
| 8.9 Updates — checking | `8.9-updates-checking-light.png` | `8.9-updates-checking-dark.png` |
| 8.10 Updates — available (release notes) | `8.10-updates-available-light.png` | `8.10-updates-available-dark.png` |
| 8.11 Updates — downloading (size known) | `8.11-updates-downloading-light.png` | `8.11-updates-downloading-dark.png` |
| 8.12 Updates — downloading, unknown size (indeterminate) | `8.12-updates-downloading-unknown-size-light.png` | `8.12-updates-downloading-unknown-size-dark.png` |
| 8.13 Updates — ready to restart | `8.13-updates-ready-to-restart-light.png` | `8.13-updates-ready-to-restart-dark.png` |
| 8.14 Restart confirmation | `8.14-restart-confirmation-light.png` | `8.14-restart-confirmation-dark.png` |
| 8.15 Updates — error, offline (`network`) | `8.15-updates-error-offline-light.png` | `8.15-updates-error-offline-dark.png` |
| 8.16 Updates — error, signature (`signature`) | `8.16-updates-error-signature-light.png` | `8.16-updates-error-signature-dark.png` |
| 8.17 Updates — error, could not install (`install`, generic) | `8.17-updates-error-install-light.png` | `8.17-updates-error-install-dark.png` |
| 8.18 Updates — Beta channel (+ "Stable chosen while running a beta" variant) | `8.18-updates-beta-channel-light.png` | `8.18-updates-beta-channel-dark.png` |
| 8.19 Command palette — Check for updates | `8.19-command-palette-check-for-updates-light.png` | `8.19-command-palette-check-for-updates-dark.png` |

New components: `components-progress-switch-update-button.png` (light) and `components-progress-switch-update-button-dark.png` (dark).

States without their own board:

- **Idle** = board 8.8 with the status text "Select Check now to look for a newer version." and no "Last checked" line; initial focus on Check now.
- **Restarting** = board 8.9 with "Restarting BIMCraft…", every control disabled, no Close.
- **Error — manifest** and **Error — not downloaded** = board 8.15 (or 8.16 for `not_downloaded`, on the Available screen) with their own Banner text and hint. **Error — other** = board 8.15 with "Something went wrong with the update. Check again to continue." and **Check now** as the only action.

## Layout of the Updates dialog

560 px wide (restart confirmation 480 px), radius 12, `color/surface`, 1 px `color/line` border, `Shadow/Raised`, over an 18 % black scrim. Top to bottom:

1. Header: "Updates" (15 px Semi Bold, h2) and the Close icon button (aria-label "Close").
2. Body, 20 px side padding, 16 px between groups: version line (13 px); channel fieldset (legend 12 px Semi Bold `color/ink2`, Segmented toggle Stable · Beta, help 12 px `color/muted`, optional note); the automatic-check switch with its label (13 px) and help (12 px `color/muted`); status area (`role="status"`, top rule `color/line`, headline 13 px Semi Bold, details 12 px `color/muted`); then the error Banner (`role="alert"`, **after** the status area, never inside) with its hint, or the release notes.
3. Release notes (Available only): label "What's new in {version}" + a box `color/raised` with a 1 px `color/field-line` border (3:1, it is a focusable scroll area), at most 132 px high (it shrinks to short notes), plain text with `pre-wrap`, 12/18 px.
4. Footer, top rule `color/line`: **Check now** (secondary) on the left when the state allows it, spacer, then the state's actions with the primary one last.

Channel locked (checking, downloading, ready, install error): the Segmented toggle is shown at 40 % opacity (disabled) and the line "The channel cannot change while an update is being checked, downloaded or waiting for a restart." appears under the help.

## Components used

From the Brand guide: `UI / Button primary`, `UI / Button secondary`, `UI / Button ghost` (Console), `UI / Icon button` (Close, with `Icons / UI / Close`), `UI / Segmented toggle`, `UI / Banner` (errors), `UI / Banner info` (beta notes, no action), `Icons / UI / Error`, `Icons / UI / Reset` and `Icons / UI / Info` (palette rows), effect styles `Shadow/Raised` and `Focus/Ring`. The dialog follows the `UI / Dialog` structure (header / body / footer) built from frames, like the Materials and Build-ups dialogs.

New, added to Brand guide card "8 · Form and dialog components":

- **`UI / Progress bar`** — `Kind=Determinate` (indicator width = percent) and `Kind=Indeterminate` (a 30 % segment that slides; static under `prefers-reduced-motion`). 8 px high, radius 4, track `color/soft` with a 1 px `color/field-line` border, indicator `color/acc`. Fills the dialog width. `role="progressbar"`, `aria-label` "Download progress", `aria-valuenow`/`aria-valuetext` ("17 MB of 40 MB"); indeterminate: no `aria-valuenow`, `aria-valuetext` "17 MB downloaded".
- **`UI / Switch`** — `On=true` (track `color/acc`, knob `color/on-acc`) and `On=false` (track `color/surface`, 1 px `color/field-line` border, knob `color/muted`). 32 × 18 px; the label sits to its right and is part of the hit area. `role="switch"` + `aria-checked`, or a checkbox styled as a switch.
- **`UI / Update button`** — the title-bar entry: `State=Idle` (ghost "Updates"), `Available` and `Ready` (pill, `color/acc` + `color/on-acc`, 6 px dot), `Downloading` (pill, `color/acc-tint` + `color/acc-ink`, 1 px `color/acc` border), `Failed` (pill, `color/err-tint` + `color/err`, 1 px `color/err` border, Error icon). Height 28 px, radius 14, 13 px Semi Bold label, hugs its text. The dot and the icon make the state readable without colour.

## Accessibility check (UI-06)

Contrast, computed from the `Mode` token values (WCAG 2.2; text needs 4.5:1, UI parts 3:1):

| Pair | Light | Dark |
| --- | --- | --- |
| Body text `ink` / `surface` | 16.9 | 11.9 |
| Help text `muted` / `surface` | 5.9 | 5.6 |
| Notes text `ink` / `raised` | 15.5 | 10.6 |
| Pill and primary label `on-acc` / `acc` | 6.4 | 7.5 |
| Downloading pill `acc-ink` / `acc-tint` | 7.5 | 7.1 |
| Failed pill `err` / `err-tint` | 5.4 | 6.1 |
| Banner border and icon `err` / `surface` | 6.6 | 6.2 |
| Focus ring, switch on `acc` / `surface` | 6.4 | 6.0 |
| Switch off, progress track, notes box border `field-line` / `surface` | 3.4 | 3.7 |
| Progress indicator `acc` / track `soft` | 5.3 | 4.9 |

Disabled controls (Check now while checking, the channel toggle while locked) are 40 % opacity and exempt; the locked reason is written next to them.

Focus: each board shows the state's initial focus with `Focus/Ring` (2 px `color/acc` ring, 2 px gap): Close (up to date, downloading), Download and install (available), Restart now (ready), **Cancel** (restart confirmation), Try again (errors), Close (8.18: choosing Beta starts a check, which disables the channel control and moves focus to Close), and the pill's focus look when the user tabs to it (8.7; the pill never takes focus by itself). Checking has focus on the dialog container (no ring drawn). Tab order: Close (X) → channel → switch → release notes box → footer actions.

## French length (UI-04)

Checked on board 8.7 with indicative French: "Download and install" 146 px → "Télécharger et installer" 156 px; "Restart now" → its French label 161 px (shown on the board); widest pill "Restart to update" → "Redémarrer pour mettre à jour" 210 px. All buttons and pills hug their label (no fixed widths); the French Available footer needs about 390 of 520 px and stays on one line. In the title bar the open-view tabs shrink first, so the pill never truncates.

## Notes for front

- The title bar on these boards uses the design's header (menu, breadcrumb, Search commands, Export) plus the shell's controls in their real order (`ui/src/shell/Shell.tsx`): **Materials**, **Build-ups**, **Location**, **Console**, the **Updates** entry, the **Language** list. The Updates entry goes between Console and the Language list, as in `wireframes/01-title-bar-entry.svg`.
- Sizes and dates on the boards are sample values formatted with the rules of `ux-flow.md` section 5: "17 MB of 40 MB" (one decimal only under 10) and `Intl.DateTimeFormat` in the interface language (English: "Nov 15, 2026, 9:42 AM", "Nov 12, 2026").
- The percentage at the right of the progress text (8.11) has no key in `ux-flow.md` yet: it needs one (for example `updates.progress.percent` = "{percent}%"), asked of lead; it is hidden when the total is unknown (8.12).
- New colour tokens: `color/acc-tint`, `color/acc-ink`, `color/err-tint` and `color/soft` are not in `ui/src/shell/theme.css` yet; take their Light and Dark values from the `Mode` collection and add their pairs (table above) to `contrast.test.ts`. `color/acc` is `--bc-verdigris`, `color/on-acc` is `--bc-on-verdigris`, `color/err` is `--bc-danger`.
- The restart confirmation shows only the body sentence: the recovery-copy paragraph (`updates.confirm.recovery`) stays hidden until the interface really autosaves (`ux-flow.md` section 6).
- Release notes are plain text; the bullets on board 8.10 are the manifest's own "- " characters, not HTML lists.
- The spinner (8.9) and the indeterminate bar (8.12) stop moving under `prefers-reduced-motion`; the text carries the state.
