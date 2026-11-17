# Fix flaky e2e tests

Requirements: VP-01, VP-03 (3D view), UI-06 (keyboard, modal dialogs), F1 (project location dialog).
No GitHub issue yet. #1–#3 in the brief are the "attachment #1/#2/#3" lines of the Playwright output, not issues.

## The failures

A `scripts/check-all.sh e2e` run on `main` (CI, `ubuntu-24.04`, two Playwright workers, Chromium with SwiftShader) had:

| Test | Result | Symptom |
| --- | --- | --- |
| `georeferencing.spec.ts:88` the project can be placed on a coordinate system from the keyboard | flaky (passed on retry) | the second Ctrl+Shift+L opened no dialog: `Easting` not found |
| `section-visibility.spec.ts:152` clipping planes: arm a face pick, then cancel it with Esc | failed twice | the status read "3D view stopped". The page logged Dawn's `A valid external Instance reference no longer exists.` On the retry the view stopped before the View panel showed its controls. |

These are two independent causes.

### 1. Georeferencing: a shortcut dropped after a dialog closes (a real UI bug)

`GeorefProvider` installs its Ctrl+Shift+L listener in a `useEffect` that captures `modalOpen`. `Dialog` registers itself through `useEffect(registerModal)`. When Apply closes the dialog:

1. commit N unmounts the dialog and shows the toast. The test sees the toast and presses Ctrl+Shift+L.
2. the passive cleanup of `registerModal` runs only after paint and calls `setModals(count - 1)`.
3. commit N+1 re-runs the listener effect with `modalOpen = false`.

A key pressed between 1 and 3 reaches the old listener, which still sees `modalOpen = true` and ignores the key. A fast user hits the same gap. The same pattern guards the shell shortcuts (`Shell.tsx`, the `modalOpen` check in its keydown effect) and the viewport shortcuts and Esc (`ViewportView.tsx`, the two keydown effects).

**Fix.** The decision "is a modal open?" is read **when the key is pressed**, from a value updated **in the same commit** as the dialog's mount or unmount:

- `ShellState` keeps the open-modal count in a ref as well as in state, and exposes a stable `isModalOpen(): boolean` that reads the ref. `modalOpen` stays for consumers that render from it.
- `Dialog` registers with `useLayoutEffect`. Layout effects mount and clean up during the commit, before paint, so the count is already correct when the toast is painted.
- The keydown handlers of `GeorefContext`, `Shell` and `ViewportView` call `isModalOpen()` at event time instead of closing over `modalOpen`. Where `modalOpen` was in an effect's dependencies only for this guard, it leaves them.
- Regression unit test (Vitest + Testing Library): close the Location dialog, then fire Ctrl+Shift+L in the same act without waiting for passive effects. The dialog opens again. This test fails before the fix. The same kind of test covers one shell shortcut (for example Ctrl+Shift+K for the Coordinates tab) right after a dialog closes.
- The e2e spec stays as it is: it describes what a user does.

### 2. Section visibility: the WebGPU device is lost on CI's software renderer

Dawn sends `A valid external Instance reference no longer exists.` when the GPU process drops the last reference to the WebGPU instance it serves the page with. All the devices of that instance are then lost. The viewport reports the loss (`device_lost`) and shows the "3D view stopped" card, as designed. Two earlier fixes acted on the page's own `wgpu::Instance`: a792e61 kept it alive, and cebf4b3 shared one instance for the whole page. Neither cured it. wgpu's WebGPU backend does nothing when its handles drop, so the cause is outside our handles: the browser or GPU process, or the load on CI's software renderer.

The fix must be based on evidence, so this part starts with a plan (`plan_first`):

1. **Evidence.** Collect every e2e failure with a stopped view in the CI history: runs on `main` and on pull requests, through `api.github.com` job annotations and logs. Note for each one the tests, the timing (seconds after load), what the other worker was running, the runner and the Chromium version. Read `ui/playwright.config.ts`, `crates/viewport/src/web/` and the frame loop. Check whether the view redraws while idle: a continuous loop on SwiftShader keeps the CPU busy for both workers.
2. **Plan.** Name the most likely cause, backed by that evidence, and the change for it. Candidates to check, not to adopt blindly:
   - the GPU-heavy specs run one at a time (a Playwright project or `test.describe.configure({ mode: "serial" })` with one worker for the viewport specs) while the other specs stay parallel;
   - Chromium switches for the headless software path that keep the GPU process from tearing the WebGPU context down;
   - the viewport draws only when something changed (render on demand), if it redraws idle frames today;
   - anything the evidence points to instead.
3. **Implement**, with a test that fails before the change where one can (Rust unit test for render on demand, for example).
4. **Verify on CI.** The sandbox cannot run Chromium, so a local `--repeat-each` is impossible. The pull request's e2e job is the check. Done means three consecutive green e2e runs on the pull request, with no stopped view in the logs and no retry used by a GPU spec.

**Out of scope:** restarting the view by itself after a device loss. It would hide the CI fault without fixing it, and it would lose the section box, the clipping planes and the visibility state the user set. If the evidence shows the loss cannot be prevented on CI's software renderer, the back owner reports it and the lead asks the user before going further.

Not acceptable: more retries, longer timeouts that only hide the fault, skipping or marking the GPU specs `fixme`, or weakening their assertions (AGENTS.md rule 2).

## Definition of Done

- Both tests pass reliably: three consecutive green CI e2e runs on the pull request, with no retry used by these tests.
- Regression unit tests for the shortcut race. Coverage of `ui/src/` stays at or above 90 % lines, plus branches.
- Biome, `tsc --noEmit`, `cargo clippy -p viewport`, `cargo fmt` and `scripts/check-all.sh repo` are clean.
- User docs: N/A. Both are bug fixes with no change to documented behaviour: shortcuts already work after a dialog closes, and the 3D view already does not stop. The pull request says so.
- Each commit: a Conventional Commit, subject at most 100 characters, a requirement ID in it (for example `fix(ui): read the modal state when a key is pressed (UI-06)`), signed off with the noreply address only, never a `wip` subject.

## Tasks

| Node | Task | Files it owns |
| --- | --- | --- |
| `front` | Shortcut race (cause 1) | `ui/src/shell/ShellState.tsx`, `ui/src/ui/Dialog.tsx`, `ui/src/georef/GeorefContext.tsx`, `ui/src/shell/Shell.tsx`, `ui/src/viewport/ViewportView.tsx` (keydown guards only), their tests |
| `back` | Device loss on CI (cause 2), `plan_first`, Opus | `crates/viewport/`, `crates/render/` if needed, `ui/playwright.config.ts`, `ui/e2e/` |

The two tasks are independent and run in parallel. Neither edits the other's files.
