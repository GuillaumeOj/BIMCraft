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

1. **Tag push `v*`** → job `verify` (tag shape and version, D1) → jobs `build-macos` (`macos-15`) and `build-windows` (`windows-2025`); `tauri-cli` is pinned in `scripts/tool-versions.env` and installed with `taiki-e/install-action`:
   - **macOS**: build the UI (`ui/dist`) and `cargo tauri build --target universal-apple-darwin`. Tauri signs (Developer ID Application certificate of **Fusily**, imported into a temporary keychain, hardened runtime) and notarizes (App Store Connect API key) the app, then staples the ticket; Tauri signs the update package (`.app.tar.gz`) with the updater key (`TAURI_SIGNING_PRIVATE_KEY`) and writes its `.sig`.
   - **Windows**, in two signing rounds through SignPath (D3), because the program inside an installer must be signed before the installer is built:
     1. `cargo tauri build --no-bundle` builds `bimcraft.exe`; it is uploaded as a workflow artifact and submitted to SignPath (`signpath/github-action-submit-signing-request`, artifact configuration `program`); the signed file replaces the unsigned one in `target/release/`;
     2. `cargo tauri bundle` packs that signed program into the NSIS installer and the MSI; both go to SignPath (artifact configuration `installers`) and come back signed;
     3. `cargo tauri signer sign` signs the **signed** NSIS installer (the Windows update package) with the updater key and writes its `.sig` (Tauri's own updater signature would be computed before SignPath changes the file, so it is not used on Windows).
   - **Verify before upload**: `codesign --verify --deep --strict`, `spctl -a -vv` and `xcrun stapler validate` on macOS; `Get-AuthenticodeSignature` = `Valid` on the program, the installer and the MSI on Windows; `cargo tauri signer verify`-equivalent check of every `.sig` against `UPDATER_PUBLIC_KEY`. A failed check fails the job.
   - Job `sbom` (moved from today's `release: published` trigger) and job `release`: create a **draft** GitHub release for the tag (pre-release when beta) and attach the installers, the update packages and their `.sig`, `latest.json` (the Tauri static update manifest for this version, built by `scripts/release/update-manifest.ts` from the `.sig` files and the asset URLs; the universal macOS package is listed under both `darwin-aarch64` and `darwin-x86_64`, because the updater looks the platform up by architecture and a missing key is a `manifest` error), `SHA256SUMS.txt` and the SBOM.
2. **The maintainer reviews and publishes the draft.** Nothing reaches users before that.
3. **`release: published`** → job `channels`: `scripts/release/channel-manifests.ts` reads the published releases (`gh api`), picks the newest stable and the newest of all (stable or beta), and writes `updates/stable.json` and `updates/beta.json` (copies of the chosen releases' `latest.json`). They are deployed to **GitHub Pages** (`actions/upload-pages-artifact` + `actions/deploy-pages`). The beta channel is therefore never behind stable.
4. **Fail closed**: on a tag push, a missing signing secret fails the job. The workflow never publishes an unsigned artefact.
5. **Dry run**: `workflow_dispatch` and `pull_request` (paths: `.github/workflows/release.yml`, `scripts/release/**`, `.signpath/**`, `crates/desktop-app/**`, `crates/host-desktop/**`) run `build` without secrets: unsigned bundles with a throw-away updater key, uploaded as workflow artifacts, no release. Not a required check. Pull requests from forks get no secrets (`pull_request`, never `pull_request_target`).
6. Least-privilege permissions per job (`contents: write` only for `release` and `sbom`; `pages: write` + `id-token: write` only for `channels`). Secrets live in a GitHub **environment** `release`, limited to `v*` tags, used only by the jobs that sign. Actions pinned by SHA (AGENTS.md §4).

### D3. Windows code signing: SignPath Foundation (user decision)

BIMCraft applies to the **SignPath Foundation** programme (free code signing for open-source projects; the certificate is issued to the foundation, which checks that every signed file was built by this repository's GitHub Actions from its public source). Consequences for the workflow:

- Signing goes through SignPath's GitHub action as a **trusted build system**: the files to sign are uploaded as a workflow artifact, SignPath fetches them from GitHub, signs them according to an *artifact configuration* (which files inside the artifact to sign, with which options) and returns a signed artifact. The artifact configurations are kept in the repository too (`.signpath/artifact-configurations/program.xml` and `installers.xml`), so they are reviewed like code, and pasted into SignPath by the maintainer.
- The signing policy `release-signing` requires a **manual approval** by a project approver in SignPath for each request: the release job waits for it (`wait-for-completion`, timeout 2 h). The release guide says so.
- The dry run does not call SignPath (no secrets on pull requests): it produces unsigned files.
- Only the SignPath steps of `build-windows` and the secrets depend on the provider; switching later (an OV certificate in a cloud HSM, Azure Artifact Signing) replaces those steps only.

The NSIS uninstaller is generated by the installer on the user's computer and is not signed by SignPath; this is a known limit of external signing and is noted in the release guide.

### D4. Updater: channels, endpoints and security

- Endpoints are **fixed in the Rust code**, one per channel: `https://guillaumeoj.github.io/BIMCraft/updates/stable.json` and `…/beta.json`. The UI only names a channel (`"stable" | "beta"`); it can never pass a URL.
- The updater **public key** is injected at bundle time from the repository variable `UPDATER_PUBLIC_KEY` (`--config` merge in the workflow); the dry run uses a throw-away key pair. Update packages whose signature does not verify are refused by the updater, independently of HTTPS.
- Only a **higher** semver is offered: no downgrade. Moving from beta back to stable means waiting for the next stable version that is higher than the installed beta (documented).
- The update check sends nothing but the HTTPS request for a static file (no version, no identifier in the URL); the user guide says so.
- Debug builds and builds without an updater key report `updatable: false`; the UI then hides the update controls (development and CI windows never update themselves).
- Windows updates run the NSIS installer in passive mode (progress bar, no questions); macOS replaces the `.app` in place. Then the program restarts.

### D5. Where the code goes

- **`host-desktop`** (critical crate, ≥ 98 % lines): a new `update` module with the channel → endpoint table, the update state (none / available / downloaded), the command handlers of `api-contract.md` and their typed errors. The network and install calls go through a small trait implemented by `tauri-plugin-updater` and by a test fake, so every branch is tested without network. Registration next to `ipc::serve`.
- **`desktop-app`**: registers the updater plugin; `build.rs` declares the new commands; `capabilities/main.json` grants exactly them (no JavaScript updater or process plugin: the UI goes through our commands only). `tauri.conf.json` gets the `bundle` targets and options (`nsis`, `msi`, `dmg`, `app`; `createUpdaterArtifacts: true`, macOS `minimumSystemVersion`, hardened runtime, Windows WebView2 install mode).
- **New dependency**: `tauri-plugin-updater` (official Tauri plugin, MIT/Apache-2.0, maintained with Tauri; uses `reqwest` with rustls). Justified in the commit; `cargo deny check` stays clean (any advisory or duplicate handled the way `docs/dev/desktop.md` already does for Tauri).
- **`scripts/release/`**: `check-version.ts`, `update-manifest.ts`, `channel-manifests.ts` with `bun test` suites (`*.test.ts`, like the other scripts). **`.signpath/artifact-configurations/`**: the two SignPath artifact configurations.
- **UI**: `ui/src/api/updates.ts` (critical: ≥ 98 % lines and branches), the only code that calls the update commands; the update notice and the "Updates" dialog in a new `ui/src/updates/` folder; strings in both message catalogues (UI-04).

### D6. Tests

- Rust unit tests for `host-desktop::update` (every command, every error, the state transitions, the endpoint table) with the fake; an IPC test through Tauri's mock runtime that the window can call the update commands and nothing more.
- `bun test` for the three release scripts (tag/version mismatch, pre-release flag, manifest content, newest-release selection with mixed stable and beta, no release yet).
- Vitest for `ui/src/api/updates.ts` and the update components (all states of the UX brief, keyboard, labels).
- Playwright e2e on the stubbed Tauri IPC: start → notice → download progress → restart confirmation; check on demand with no update; channel change persisted; error state.
- The release workflow itself is verified by the dry run (`actionlint` in `repo` already lints it) and, once the secrets exist, by a first beta tag cut by the maintainer.

## Decisions taken with the user (spec review)

1. Windows signing: **SignPath Foundation** (D3).
2. macOS: the **Apple Developer Program account of Fusily**: the Developer ID certificate, and so the name macOS shows for the developer, is Fusily's.
3. Automatic update check on by default (at each start, never downloads without asking), with a switch to turn it off.
4. GitHub Pages hosts the channel manifests (`/updates/*.json`).
5. The user creates the secrets after merge, following the procedure below, which back-2 also writes into `docs/dev/release.md`. Until then only the dry run works; the acceptance criterion "release workflow produces signed artefacts" is proven by the first beta tag.

## How to produce the secrets

All secrets go into the GitHub **environment** `release` (Settings → Environments → New environment `release`; Deployment branches and tags → "Selected branches and tags" → add the tag rule `v*`; optionally yourself as required reviewer). Values marked *variable* are environment variables, not secrets (they are not confidential and are shown in logs).

### Updater key pair (Tauri)

The update packages are signed with this key; every installed copy trusts the public key it was built with.

1. Install the pinned Tauri CLI: `cargo install tauri-cli --version =<TAURI_CLI_VERSION from scripts/tool-versions.env> --locked`.
2. `cargo tauri signer generate -w ~/.tauri/bimcraft-updater.key` and enter a strong password when asked. It writes `bimcraft-updater.key` (private) and `bimcraft-updater.key.pub` (public).
3. Secret `TAURI_SIGNING_PRIVATE_KEY` = the content of `bimcraft-updater.key`; secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = the password; variable `UPDATER_PUBLIC_KEY` = the content of `bimcraft-updater.key.pub`.
4. Keep an offline backup of the private key and its password (password manager). **If it is lost, installed copies can no longer update**: users would have to reinstall by hand. Never commit it.

### Apple (Fusily's Developer Program account)

Creating a Developer ID certificate needs the *Account Holder* role of the Fusily team (or an admin with "Access to Certificates" for Developer ID).

1. **Certificate signing request**: on a Mac, Keychain Access → Keychain Access menu → Certificate Assistant → Request a Certificate From a Certificate Authority → your email, common name "Fusily", "Saved to disk".
2. **Certificate**: developer.apple.com → Account → Certificates, IDs & Profiles → Certificates → + → **Developer ID Application** (G2 Sub-CA) → upload the request → download the `.cer` and double-click it to add it to the login keychain.
3. **Export**: in Keychain Access, My Certificates → "Developer ID Application: Fusily (TEAMID)" → expand, select the certificate *and* its private key → Export 2 items… → `.p12` with a strong password.
4. Secret `APPLE_CERTIFICATE` = `base64 -i DeveloperID.p12 | pbcopy` (paste the clipboard); secret `APPLE_CERTIFICATE_PASSWORD` = the export password; variable `APPLE_SIGNING_IDENTITY` = the exact certificate name, `Developer ID Application: Fusily (TEAMID)` (`security find-identity -v -p codesigning` prints it).
5. **Notarization API key**: appstoreconnect.apple.com → Users and Access → Integrations → App Store Connect API → Team Keys → + → name "BIMCraft CI", access "Developer" → Generate → download the `.p8` (only possible once). Secret `APPLE_API_KEY_P8` = the content of the `.p8` file; variable `APPLE_API_KEY` = its Key ID; variable `APPLE_API_ISSUER` = the Issuer ID shown above the list. The workflow writes the `.p8` to a temporary file for Tauri (`APPLE_API_KEY_PATH`) and deletes it at the end.
6. The keychain password used in CI is generated at random in the job: nothing to create.

### SignPath Foundation

1. Apply at signpath.org → "Apply for Open Source code signing" with the repository URL, licence (MIT) and a short description. Approval takes some days; it may require that a first release exists (the unsigned dry-run output can be published as a pre-release for that if asked).
2. Once accepted, in app.signpath.io: install the **SignPath GitHub App** on the repository and add GitHub as trusted build system of the project; in the project `bimcraft`, create the artifact configurations `program` and `installers` from the files in `.signpath/artifact-configurations/`, and check the signing policy slug `release-signing` (manual approval by you).
3. Create a CI user (Users → Add CI user, submitter on `release-signing`) and generate its **API token**: secret `SIGNPATH_API_TOKEN`. Variable `SIGNPATH_ORGANIZATION_ID` = the organization ID shown in Settings; variable `SIGNPATH_PROJECT_SLUG` = `bimcraft`.
4. At each release, approve the two signing requests in SignPath (email notification); the workflow waits for them.

### GitHub Pages

Settings → Pages → Build and deployment → Source: **GitHub Actions**. The `channels` job deploys `updates/stable.json` and `updates/beta.json` at the next published release.

## Delivery

One pull request for #66, `Refs #66` and `ENG-05` in every commit; `Closes #66` in the PR. User docs: `docs/user/getting-started.md` ("Install BIMCraft", today a placeholder) becomes the install guide, with `getting-started.fr.md` ("Installer BIMCraft"); a new `update-bimcraft.md` ("Update BIMCraft") with `update-bimcraft.fr.md` ("Mettre à jour BIMCraft"). All four linked in `SUMMARY.md` (French ones under "Français"). `.orchestrator/` is removed in a final commit before the PR request, as in earlier features.

## Tasks (planned)

| Task | Node | Depends on | Content |
| --- | --- | --- | --- |
| back-1 | back | — | `host-desktop::update` + plugin registration, commands and capability per `api-contract.md`, tests ≥ 98 %, `docs/dev/desktop.md` |
| back-2 | back | back-1 | Bundle config, `release.yml` (SignPath two-round Windows signing, Fusily notarization), `scripts/release/*` + tests, `.signpath/` configurations, tool version, `docs/dev/release.md` (with the secrets procedure above) + `github-setup.md`, install guide EN + FR |
| ux-1 | ux | — | Update flow and states (`ux-brief.md` → wireframes) |
| ui-1 | ui | ux-1 | FigCraft boards (light + dark) on page "8 · Extensibility and system", exported to `design/` |
| front-1 | front | ui-1 | `ui/src/api/updates.ts`, update notice and Updates dialog, palette entry, i18n, Vitest + e2e, "Update BIMCraft" user page EN + FR |
