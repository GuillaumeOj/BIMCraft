# Installers, code signing, notarization and auto-update — feature spec

Issue [#66](https://github.com/GuillaumeOj/BIMCraft/issues/66), epic [#61](https://github.com/GuillaumeOj/BIMCraft/issues/61). Requirement: **ENG-05** (`docs/specifications.md` §14: installers, code signing on Windows, notarization on macOS and auto-update channels). Related: SEC-05 (SBOM with each release), UI-01, UI-04, UI-06. Milestone: Phase 1 – MVP.

## Goal

Anyone can download BIMCraft from the GitHub release page, install it on Windows or macOS without a security warning, and keep it up to date from inside the program, on a stable or a beta channel. A release is made by pushing a version tag: CI builds, signs, notarizes and attaches every artefact to a draft release, and publishing that release makes it reach the users of its channel.

## Scope

In scope:

- **Windows**: an NSIS installer (`BIMCraft_<version>_x64-setup.exe`, per-user install, no administrator rights needed) and an MSI package (`BIMCraft_<version>_x64_en-US.msi`, per-machine, for IT departments). Both Authenticode-signed, the program inside them too. x64 only (Windows on ARM runs it emulated). Windows 10 1809 or later; WebView2 is installed by the installer when missing (embedded bootstrapper).
- **macOS**: a universal (Apple silicon + Intel) `BIMCraft.app`, signed with a Developer ID Application certificate, hardened runtime, notarized and stapled, shipped in `BIMCraft_<version>_universal.dmg`. macOS 11 or later.
- **Auto-update**: Tauri's updater (`tauri-plugin-updater`), with update packages signed by a separate updater key, two channels (stable, beta), a check at start-up and on demand, download with progress, then install and restart when the user says so.
- **Release workflow**: tag-driven build, sign, notarize, draft release with SBOM and checksums; channel manifests published when the release is published; an unsigned dry run on demand and on pull requests that touch the release files.
- **Docs**: install guide and update guide (EN then FR) in `docs/user/`, a maintainer release guide in `docs/dev/release.md`, the secrets in `docs/dev/github-setup.md`.

Out of scope (later issues): Linux packages (the Linux desktop is V1, spec §15), Windows on ARM native builds, the Microsoft Store and Mac App Store, delta updates, silent/managed-deployment policies (turning updates off by MDM or group policy), crash reporting (SEC-03), release cadence and LTS (ENG-07).

## Decisions

### D1. Versions and tags

- The program's version is the `version` of `crates/desktop-app/Cargo.toml` (Tauri reads it; `tauri.conf.json` has no `version`). It is semver.
- A **stable** release is tagged `vX.Y.Z`; a **beta** release `vX.Y.Z-beta.N`. No other tag shape starts a release.
- The workflow fails when the tag does not match the package version, or when a beta tag is not marked pre-release / a stable tag is (checked again at publication).
- The maintainer bumps the version in a normal pull request, then tags the merge commit on `main` (documented in `docs/dev/release.md`). Agents never push tags or publish releases (AGENTS.md §1).

### D2. Release flow (`.github/workflows/release.yml`)

1. **Tag push `v*`** → job `verify` (tag shape and version, D1) → matrix `build` on `windows-2025` and `macos-15`:
   - build the UI (`ui/dist`) and bundle with the Tauri CLI (`cargo tauri build`, `tauri-cli` pinned in `scripts/tool-versions.env`, installed with `taiki-e/install-action`); macOS with `--target universal-apple-darwin`;
   - sign: macOS through Tauri's built-in signing and notarization (Developer ID certificate imported into a temporary keychain; notarization with an App Store Connect API key); Windows through `bundle.windows.signCommand`, which calls a script in `scripts/release/` (D3);
   - Tauri signs each update package with the updater private key (`TAURI_SIGNING_PRIVATE_KEY`) and writes the `.sig` files;
   - verify before upload: `codesign --verify --deep --strict`, `spctl -a -vv`, `xcrun stapler validate` on macOS; `Get-AuthenticodeSignature` = `Valid` on every `.exe`/`.msi` on Windows. A failed check fails the job.
   - job `sbom` (moved from today's `release: published` trigger) and job `release`: create a **draft** GitHub release for the tag (pre-release when beta) and attach the installers, the update packages and their `.sig`, `latest.json` (the Tauri static update manifest for this version, built by `scripts/release/update-manifest.ts` from the `.sig` files and the asset URLs), `SHA256SUMS.txt` and the SBOM.
2. **The maintainer reviews and publishes the draft.** Nothing reaches users before that.
3. **`release: published`** → job `channels`: `scripts/release/channel-manifests.ts` reads the published releases (`gh api`), picks the newest stable and the newest of all (stable or beta), and writes `updates/stable.json` and `updates/beta.json` (copies of the chosen releases' `latest.json`). They are deployed to **GitHub Pages** (`actions/upload-pages-artifact` + `actions/deploy-pages`). The beta channel is therefore never behind stable.
4. **Fail closed**: on a tag push, a missing signing secret fails the job. The workflow never publishes an unsigned artefact.
5. **Dry run**: `workflow_dispatch` and `pull_request` (paths: `.github/workflows/release.yml`, `scripts/release/**`, `crates/desktop-app/**`) run `build` without secrets: unsigned bundles with a throw-away updater key, uploaded as workflow artifacts, no release. Not a required check. Pull requests from forks get no secrets (`pull_request`, never `pull_request_target`).
6. Least-privilege permissions per job (`contents: write` only for `release` and `sbom`; `pages: write` + `id-token: write` only for `channels`). Secrets live in a GitHub **environment** `release`, limited to `v*` tags, used only by the jobs that sign. Actions pinned by SHA (AGENTS.md §4).

### D3. Windows code signing: Azure Artifact Signing (decision to confirm)

`bundle.windows.signCommand` runs `scripts/release/sign-windows.ps1 "%1"` on each binary Tauri produces (program, installer, uninstaller, MSI). The script signs with **Azure Artifact Signing** (formerly Trusted Signing; Microsoft-managed certificate, SHA-256, RFC 3161 timestamp), authenticated by OIDC federation from GitHub Actions to an Azure app registration, so no long-lived Azure secret is stored. It fails when it cannot sign. Keeping the provider behind one script means a switch (SignPath Foundation, an OV certificate in a cloud HSM) changes only that script and the secrets.

Costs and eligibility are the user's call; see "Decisions for the user".

### D4. Updater: channels, endpoints and security

- Endpoints are **fixed in the Rust code**, one per channel: `https://guillaumeoj.github.io/BIMCraft/updates/stable.json` and `…/beta.json`. The UI only names a channel (`"stable" | "beta"`); it can never pass a URL.
- The updater **public key** is injected at bundle time from the repository variable `UPDATER_PUBLIC_KEY` (`--config` merge in the workflow); the dry run uses a throw-away key pair. Update packages whose signature does not verify are refused by the updater, independently of HTTPS.
- Only a **higher** semver is offered: no downgrade. Moving from beta back to stable means waiting for the next stable version that is higher than the installed beta (documented).
- The update check sends nothing but the HTTPS request for a static file (no version, no identifier in the URL); the user guide says so.
- Debug builds and builds without an updater key report `updatable: false`; the UI then hides the update controls (development and CI windows never update themselves).
- Windows updates run the NSIS installer in passive mode (progress bar, no questions); macOS replaces the `.app` in place. Then the program restarts.

### D5. Where the code goes

- **`host-desktop`** (critical crate, ≥ 98 % lines): a new `update` module with the channel → endpoint table, the update state (none / available / downloaded), the command handlers of `api-contract.md` and their typed errors. The network and install calls go through a small trait implemented by `tauri-plugin-updater` and by a test fake, so every branch is tested without network. Registration next to `ipc::serve`.
- **`desktop-app`**: registers the updater plugin; `build.rs` declares the new commands; `capabilities/main.json` grants exactly them (no JavaScript updater or process plugin: the UI goes through our commands only). `tauri.conf.json` gets the `bundle` targets and options (`nsis`, `msi`, `dmg`, `app`; `createUpdaterArtifacts: true`, macOS `minimumSystemVersion`, hardened runtime, Windows WebView2 install mode and `signCommand`).
- **New dependency**: `tauri-plugin-updater` (official Tauri plugin, MIT/Apache-2.0, maintained with Tauri; uses `reqwest` with rustls). Justified in the commit; `cargo deny check` stays clean (any advisory or duplicate handled the way `docs/dev/desktop.md` already does for Tauri).
- **`scripts/release/`**: `check-version.ts`, `update-manifest.ts`, `channel-manifests.ts` with `bun test` suites (`*.test.ts`, like the other scripts), and `sign-windows.ps1`.
- **UI**: `ui/src/api/updates.ts` (critical: ≥ 98 % lines and branches), the only code that calls the update commands; the update notice and the "Updates" dialog in a new `ui/src/updates/` folder; strings in both message catalogues (UI-04).

### D6. Tests

- Rust unit tests for `host-desktop::update` (every command, every error, the state transitions, the endpoint table) with the fake; an IPC test through Tauri's mock runtime that the window can call the update commands and nothing more.
- `bun test` for the three release scripts (tag/version mismatch, pre-release flag, manifest content, newest-release selection with mixed stable and beta, no release yet).
- Vitest for `ui/src/api/updates.ts` and the update components (all states of the UX brief, keyboard, labels).
- Playwright e2e on the stubbed Tauri IPC: start → notice → download progress → restart confirmation; check on demand with no update; channel change persisted; error state.
- The release workflow itself is verified by the dry run (`actionlint` in `repo` already lints it) and, once the secrets exist, by a first beta tag cut by the maintainer.

## Decisions for the user (asked at spec approval)

1. **Windows signing provider**: Azure Artifact Signing (about US$10 a month; Microsoft validates the identity; open to organisations with three years of verifiable history in the EU, UK, US or Canada, and to individuals only in the US and Canada) — **recommended if Fusily or you are eligible**. Alternative: SignPath Foundation (free for open-source projects, certificate issued to the foundation, approval process, signing through their GitHub action). Only `sign-windows.ps1` and the secrets differ.
2. **Apple Developer Program** membership (US$99 a year) for the Developer ID certificate and notarization.
3. **Update check default**: on (check at each start, never download without asking), with a switch to turn it off. Recommended.
4. **GitHub Pages** hosts the two channel manifests (`/updates/*.json`); you enable Pages with "GitHub Actions" as source.
5. You create the secrets after merge (listed in `docs/dev/release.md`): the updater key pair (`cargo tauri signer generate`), the Apple certificate and API key, the Azure (or SignPath) credentials. Until then only the dry run works; the acceptance criterion "release workflow produces signed artefacts" is proven by your first beta tag.

## Delivery

One pull request for #66, `Refs #66` and `ENG-05` in every commit; `Closes #66` in the PR. User docs: `docs/user/getting-started.md` ("Install BIMCraft", today a placeholder) becomes the install guide, with `getting-started.fr.md` ("Installer BIMCraft"); a new `update-bimcraft.md` ("Update BIMCraft") with `update-bimcraft.fr.md` ("Mettre à jour BIMCraft"). All four linked in `SUMMARY.md` (French ones under "Français"). `.orchestrator/` is removed in a final commit before the PR request, as in earlier features.

## Tasks (planned)

| Task | Node | Depends on | Content |
| --- | --- | --- | --- |
| back-1 | back | — | `host-desktop::update` + plugin registration, commands and capability per `api-contract.md`, tests ≥ 98 %, `docs/dev/desktop.md` |
| back-2 | back | back-1 | Bundle config, `release.yml`, `scripts/release/*` + tests, tool version, `docs/dev/release.md` + `github-setup.md`, install guide EN + FR |
| ux-1 | ux | — | Update flow and states (`ux-brief.md` → wireframes) |
| ui-1 | ui | ux-1 | FigCraft boards (light + dark) on page "8 · Extensibility and system", exported to `design/` |
| front-1 | front | ui-1 | `ui/src/api/updates.ts`, update notice and Updates dialog, palette entry, i18n, Vitest + e2e, "Update BIMCraft" user page EN + FR |
