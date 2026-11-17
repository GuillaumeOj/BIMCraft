# Make the desktop app present itself as Fabrilo (#385)

Part of epic #380. Follows [ADR 0028](../../../docs/dev/adr/0028-rename-to-fabrilo.md) (the brief says "ADR 0025"; the rename record landed as 0028). Requirement IDs: none.

## Goal

The desktop program shows itself as **Fabrilo**: window title, app and bundle name, bundle identifier, executable, Liquid Glass icon file and the start-up error message.

## Changes

### A. Program and bundle (task `back`)

| File | Old | New |
| --- | --- | --- |
| `crates/desktop-app/tauri.conf.json` `productName` | `BIMCraft` | `Fabrilo` |
| `crates/desktop-app/tauri.conf.json` `identifier` | `org.bimcraft.desktop` | `org.fabrilo.desktop` |
| `crates/desktop-app/tauri.conf.json` window `title` | `BIMCraft` | `Fabrilo` |
| `crates/desktop-app/Cargo.toml` `[[bin]] name` | `bimcraft` | `fabrilo` |
| `crates/desktop-app/Cargo.toml` `description` | "The BIMCraft desktop program…" | "The Fabrilo desktop program…" |
| `crates/desktop-app/src/main.rs` error line and its test | `BIMCraft could not start: …` | `Fabrilo could not start: …` |
| `docs/dev/desktop.md` line 3 | program `bimcraft` | program `fabrilo` |

The crate keeps its name `desktop-app` (`cargo run -p desktop-app` is unchanged). Crate renames belong to #382.

### B. Icons (task `front`)

- `crates/desktop-app/icons/source/logo.svg` and `logo-small.svg`: `<title>Fabrilo</title>` and "Fabrilo app icon" in the header comments. The drawing does not change (the tile shows a building, not a letter).
- `scripts/build-icons.ts`: header comment names Fabrilo; `GLASS_ICON = "Fabrilo.icon"`; temp dir prefix `fabrilo-icons-`. `scripts/build-icons.test.ts` still passes (it reads `GLASS_ICON`).
- `crates/desktop-app/icons/BIMCraft.icon/` becomes `crates/desktop-app/icons/Fabrilo.icon/`, produced by `bun scripts/build-icons.ts` (rebuild the whole set; the raster outputs should be byte-identical or visually identical, since only titles and comments change). If the rebuild cannot run in the sandbox, `git mv` the bundle and say so in the report.
- `crates/desktop-app/tauri.conf.json` `bundle.icon`: `icons/BIMCraft.icon` → `icons/Fabrilo.icon` (only this line; task A owns the other lines).
- `docs/dev/desktop.md` line 27: fix the stale description of the icons ("B" on verdigris) only if it mentions the old name; otherwise leave it.

### C. User docs (task `front`)

- `docs/user/getting-started.md` (the install page; there is no French version yet): "Install BIMCraft" → "Install Fabrilo", and the body. The rest of the user guide is #387.

## Kept on purpose

- `crates/desktop-app/capabilities/main.json` keeps the opener scope `https://github.com/GuillaumeOj/BIMCraft/blob/main/docs/user/*`. It is the repository's address, and ADR 0028 keeps GitHub links until the owner renames the repository (#388, #389). This is the one BIMCraft left in `crates/desktop-app/`; the pull request says so. Changing it now would break the help links (GitHub redirects old → new, not new → old).
- No compatibility code for the old identifier (ADR 0028: nothing released).

## Where the app keeps its data

- **Recovery copies**: next to the project file (`persist::autosave`); the identifier does not affect them. Nothing moves.
- **Settings** (language, single-key shortcuts, viewport hint): webview `localStorage`, which Tauri keeps in a folder named after the identifier: macOS `~/Library/WebKit/org.bimcraft.desktop/`, Windows `%LOCALAPPDATA%\org.bimcraft.desktop\EBWebView\`, Linux `~/.local/share/org.bimcraft.desktop/`. With `org.fabrilo.desktop` they start empty once (developers only; no release). The keys themselves are renamed by #386 anyway.
- **Logs**: the program writes none to disk (stderr only).
- **Updater** (PR #378, not merged): it adopts `org.fabrilo.desktop` and the `fabrilo` executable when it rebases. This change lands before the first signed release.

## Acceptance

- `git grep -i bimcraft -- crates/desktop-app scripts/build-icons.ts scripts/build-icons.test.ts` finds only the capability URL.
- `cargo test -p desktop-app`, `cargo clippy -p desktop-app --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, `bun test scripts/build-icons.test.ts`, `scripts/check-all.sh repo` pass.
- CI green including `desktop` (`scripts/check-all.sh desktop` builds the bundled program).
- Manual (owner, outside the sandbox): `cargo run -p desktop-app` shows a window titled "Fabrilo"; the dock shows "Fabrilo" with the icon.
- Commits: Conventional Commits, signed off, `Refs #385`; no `wip` subjects.

## Tasks

1. `back`: section A.
2. `front`: sections B and C. Independent of task 1 (separate lines of `tauri.conf.json`).
