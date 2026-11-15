# Auto-update — UX brief

For ux-1 (flow and wireframes), ui-1 (FigCraft boards) and front-1 (implementation). Commands and states come from `api-contract.md`; product rules from `spec.md` D4.

## Users and situation

Architects and engineers, often in the middle of a project, sometimes offline (site visits) or behind an office proxy. An update must never interrupt work, never start a restart by itself, and never lose work. Some offices want to stay on a known version: they turn automatic checks off.

## Surfaces

1. **Update notice** — a small, non-modal notice that appears when a check finds a newer version. It does not steal focus. Proposed home: the right end of the title bar (an "Update available" pill/button next to the Language list) that opens the Updates dialog; or a dismissible banner under the title bar. UX picks one and justifies it; existing patterns: `ui/src/ui/Banner.tsx`, the title-bar ghost buttons in `ui/src/shell/Shell.tsx`, toasts (board 3.20).
2. **Updates dialog** (reuse `ui/src/ui/Dialog.tsx`), reachable from the notice, from the command palette ("Check for updates", "About BIMCraft" may share it) and from a title-bar entry. It shows:
   - the running version (`app_info.version`);
   - the channel: **Stable** (recommended) or **Beta** (earlier access, may have bugs), with one line explaining each (`SegmentedToggle` or radio group);
   - a switch **Check for updates automatically** (default on);
   - **Check now**, the result of the last check, and the offer (version, date, release notes as plain text, scrollable);
   - the action area that follows the state machine below.

## States (each needs a board, light + dark)

| State | What the user sees | Actions |
| --- | --- | --- |
| Up to date | "BIMCraft 0.3.0 is up to date" + time of the check | Check now |
| Checking | Spinner/progress text, Check now disabled | — |
| Available | "BIMCraft 0.4.0 is available" + date + notes | Download and install · Later |
| Downloading | Progress bar with percentage (or indeterminate when total is unknown) and size | (dialog can be closed; download continues; the notice shows progress) |
| Ready to restart | "Ready: restart BIMCraft to finish" | Restart now · Later (installs at next "Restart now") |
| Restart confirmation | Confirmation dialog: the program closes and reopens; open dialogs and unapplied edits (e.g. a build-up draft) are lost; the model itself is kept by the recovery copy (link to the recovery page). Saving from the UI does not exist yet, so the wording must not promise "Save". | Restart now (primary) · Cancel |
| Error — offline | "Could not reach the update server. Check your connection." | Try again |
| Error — signature | "The downloaded update could not be verified and was not installed." (security message, calm) | Try again · close |
| Error — manifest / install | Generic "The update could not be installed" + what to do (download from the website) | Try again |
| Beta selected | The channel line shows the beta warning; switching back to Stable explains that a newer beta stays installed until the next stable version | — |

Updates unavailable (`updatable: false`, development build or browser): no notice, no title-bar entry, the palette command is absent.

## Behaviour rules

- At start-up, when "check automatically" is on, the UI checks the chosen channel once, a few seconds after the window is ready; a failure at start-up is silent (no error notice), only an on-demand check shows errors.
- "Later" hides the notice until the next start; nothing is skipped forever.
- Preferences (channel, automatic check) are remembered like the language (per computer).
- The restart is always the user's explicit choice, through the confirmation.

## Accessibility and language (UI-04, UI-06)

- Keyboard: every action reachable by Tab; the notice is announced politely (`role="status"`), not as an alert; the progress bar has `aria-valuenow`/`aria-valuetext` ("12 MB of 40 MB"); focus returns to the opener when the dialog closes.
- French strings are ~30 % longer: buttons must wrap or grow ("Télécharger et installer").
- Release notes are plain text from the network: shown as text, never as HTML.

## Deliverables

- ux-1: the flow (diagram), the states table above refined, wireframes, and the exact English copy of every string; handed off to ui.
- ui-1: boards on page "8 · Extensibility and system", numbered `8.7 …` onward, each in light and dark, using the existing components and tokens; exported images (PNG) in `.orchestrator/features/installers-code-signing-notarization-and/design/`.
