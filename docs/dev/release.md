# Releasing Fabrilo

How a version of Fabrilo reaches users (ENG-05): the maintainer tags a commit on `main`, `.github/workflows/release.yml` builds, signs and notarizes the installers into a **draft** release, and publishing that draft updates the update channels. Nothing reaches users before the draft is published. Agents never push tags or publish releases (AGENTS.md §1).

The secrets the workflow needs are described in [How to produce the secrets](#how-to-produce-the-secrets). Until they exist, only the [dry run](#the-dry-run) works.

## What a release contains

| File | What it is |
| --- | --- |
| `Fabrilo_<version>_x64-setup.exe` | Windows installer (NSIS), per-user, no administrator rights. Also the Windows update package. |
| `Fabrilo_<version>_x64-setup.exe.sig` | Its updater signature. |
| `Fabrilo_<version>_x64_en-US.msi` | Windows MSI package, per-machine, for IT departments. **Stable releases only**: WiX accepts numeric versions only. |
| `Fabrilo_<version>_universal.dmg` | macOS disk image with the universal (Apple silicon + Intel) app. |
| `Fabrilo_<version>_universal.app.tar.gz` and `.sig` | macOS update package and its updater signature. |
| `latest.json` | Tauri static update manifest of this version (`scripts/release/update-manifest.ts`). |
| `SHA256SUMS.txt` | SHA-256 of every other file. |
| `*.cdx.json` | CycloneDX SBOM, one per workspace crate (SEC-05, `scripts/sbom.sh`). |

Every Windows file is Authenticode-signed through SignPath Foundation, the program inside the installers too. The macOS app is signed with the Developer ID Application certificate of **Fusily**, with the hardened runtime, notarized and stapled. The update packages are signed with the updater key, for their version (`requireSignedVersion`).

## Cut a release

1. **Bump the version** in a normal pull request: `version` in `crates/desktop-app/Cargo.toml` (Tauri reads it; `tauri.conf.json` has none), then `cargo update -w` so `Cargo.lock` follows. A stable version is `X.Y.Z`, a beta `X.Y.Z-beta.N`.
2. **Tag the merge commit on `main`** and push the tag:

   ```sh
   git switch main && git pull --ff-only
   git tag -s v0.4.0-beta.1 -m "Fabrilo 0.4.0-beta.1"
   git push origin v0.4.0-beta.1
   ```

   A stable tag is `vX.Y.Z`, a beta tag `vX.Y.Z-beta.N`; no other shape starts a release. The `verify` job fails when the tag does not match the version.
3. **Approve the two SignPath requests.** `build-windows` submits the program, then the installers, to SignPath's `release-signing` policy and waits up to 2 hours for each. SignPath sends an email for each request; approve it in app.signpath.io. Meanwhile `build-macos` waits for Apple's notarization, usually a few minutes.
4. **Review the draft release** (Releases on GitHub). Check the files listed above, and edit the notes: the generated list of pull requests is a start, but the notes are what users read in the Updates window ("What's new"), so write them for users. A beta tag gives a pre-release; leave that flag as it is.
5. **Publish the draft.** The `channels` job then writes `updates/stable.json` and `updates/beta.json` on GitHub Pages: users of the channel are offered the version at their next check.

If a build fails, fix the cause on `main`, delete the tag (and the draft release if one was created), bump the version if the fix changes the program, and tag again. A re-run of the same tag fails at `release` when its draft already exists: delete the draft first.

### Withdraw a release

Unpublish it (edit → "Save as draft") or delete it. Every release event recomputes the channels from the published releases, so the channel goes back to the previous version. Installed copies that already updated keep the withdrawn version: copies never go down a version, so fix forward with a higher one.

### Channels

`stable.json` is the `latest.json` of the newest published stable release; `beta.json` that of the newest published release, stable or beta, so the beta channel is never behind stable. Each gets the release notes. A channel with no release yet gets a manifest whose version is `0.0.0`, which never offers an update. Drafts, tags of another shape and releases whose pre-release flag disagrees with their tag are ignored; publishing such a release fails the `channels` job.

## The dry run

Pull requests that touch the release files (`.github/workflows/release.yml`, `.signpath/**`, `scripts/release/**`, `crates/desktop-app/**`, `crates/host-desktop/**`) and a manual run (Actions → Release → Run workflow) run the build without any secret: unsigned bundles, update packages signed with a throw-away key that nobody keeps, every check that does not need a secret, and the files as workflow artifacts (`release-macos`, `release-windows`, `release-sbom`). No release is created. The dry run is not a required check. Pull requests from forks never get secrets (`pull_request`, never `pull_request_target`).

## How the workflow works

- **Fail closed.** On a tag push, a step at the start of each build job fails when any secret or variable it needs is missing (`scripts/release/require-env.sh`). The workflow never publishes an unsigned file.
- **Secrets** live in the `release` environment, limited to `v*` tags, and are used only by `build-macos` and `build-windows` on tag pushes. Each job has least-privilege permissions: `contents: write` only for `release`, `pages: write` and `id-token: write` only for `channels`, `actions: read` for `build-windows` (SignPath downloads its artifacts). Untrusted values (tag names, release fields) reach scripts only through `env:`.
- **macOS.** `cargo tauri build --target universal-apple-darwin`. Tauri imports the certificate into a temporary keychain with a random password and deletes it afterwards; the `.p8` API key is written to a temporary file and deleted at the end of the job. Then `codesign --verify --deep --strict`, `spctl --assess` and `xcrun stapler validate` check the app.
- **Windows, two SignPath rounds.** The program inside an installer must be signed before the installer is built:
  1. `cargo tauri build --no-bundle` builds `fabrilo.exe`; it goes to SignPath (artifact configuration `program`) and the signed file replaces the unsigned one;
  2. `cargo tauri bundle --no-binary-patching` packs it into the installers, which go to SignPath (artifact configuration `installers`);
  3. `cargo tauri signer sign --app-version` signs the **signed** NSIS installer with the updater key. Tauri's own updater signature would be computed before SignPath changes the file, so it is turned off on Windows.

  `Get-AuthenticodeSignature` must then answer `Valid` for the program and each installer. The artifact configurations are in `.signpath/artifact-configurations/`: review a change like code, then paste it into the SignPath project.
- **Updater signatures.** The Tauri CLI signs but does not verify, so `scripts/release/verify-signature.ts` checks every `.sig` against `UPDATER_PUBLIC_KEY`, including the signed version, before upload.
- **Updater public key.** Merged into `tauri.conf.json` at build time from the variable `UPDATER_PUBLIC_KEY` (`--config`), never committed. A build without it cannot update itself.

## Known limits

- **MSI installations do not update themselves.** Tauri writes the bundle type into the program as it packs each installer, which would break SignPath's signature, so the release turns that off (`--no-binary-patching`). The updater then cannot tell an MSI copy from an NSIS one and only reads the generic `windows-x86_64` entry, which names the per-user NSIS installer; running it over a per-machine installation would add a second copy. So a program running from under a Program Files folder reports `updatable: false` and the update controls are hidden. IT departments deploy each new MSI themselves; users who want automatic updates install with the `.exe`.
- **The NSIS uninstaller is not signed.** The installer writes it on the user's computer, after SignPath has signed the installer. Windows may show "Unknown publisher" when uninstalling.
- **Beta versions have no MSI.** WiX accepts numeric versions only.
- **Moving from beta back to stable** means waiting for a stable version higher than the installed beta, or reinstalling by hand: copies never go down a version.

## How to produce the secrets

All secrets go into the GitHub **environment** `release`: Settings → Environments → New environment `release`; Deployment branches and tags → "Selected branches and tags" → add the tag rule `v*`; optionally add yourself as required reviewer. Values marked *variable* are environment variables, not secrets: they are not confidential and appear in logs.

| Name | Kind | Used by |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | both builds: signs the update packages |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | both builds |
| `UPDATER_PUBLIC_KEY` | variable | both builds: embedded in the program, checks the `.sig` files |
| `APPLE_CERTIFICATE` | secret | macOS: the Developer ID Application `.p12`, in base64 |
| `APPLE_CERTIFICATE_PASSWORD` | secret | macOS |
| `APPLE_SIGNING_IDENTITY` | variable | macOS |
| `APPLE_API_KEY_P8` | secret | macOS: notarization |
| `APPLE_API_KEY` | variable | macOS: the key ID |
| `APPLE_API_ISSUER` | variable | macOS: the issuer ID |
| `SIGNPATH_API_TOKEN` | secret | Windows |
| `SIGNPATH_ORGANIZATION_ID` | variable | Windows |
| `SIGNPATH_PROJECT_SLUG` | variable | Windows: `fabrilo` |

### Updater key pair (Tauri)

The update packages are signed with this key; every installed copy trusts the public key it was built with.

1. Install the pinned Tauri CLI: `cargo install tauri-cli --version =<TAURI_CLI_VERSION from scripts/tool-versions.env> --locked`.
2. `cargo tauri signer generate -w ~/.tauri/fabrilo-updater.key` and enter a strong password when asked. It writes `fabrilo-updater.key` (private) and `fabrilo-updater.key.pub` (public).
3. Secret `TAURI_SIGNING_PRIVATE_KEY` = the content of `fabrilo-updater.key`; secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = the password; variable `UPDATER_PUBLIC_KEY` = the content of `fabrilo-updater.key.pub`.
4. Keep an offline backup of the private key and its password (password manager). **If it is lost, installed copies can no longer update**: users would have to reinstall by hand. Never commit it.

### Apple (Fusily's Developer Program account)

Creating a Developer ID certificate needs the *Account Holder* role of the Fusily team (or an admin with "Access to Certificates" for Developer ID).

1. **Certificate signing request**: on a Mac, Keychain Access → Keychain Access menu → Certificate Assistant → Request a Certificate From a Certificate Authority → your email, common name "Fusily", "Saved to disk".
2. **Certificate**: developer.apple.com → Account → Certificates, IDs & Profiles → Certificates → + → **Developer ID Application** (G2 Sub-CA) → upload the request → download the `.cer` and double-click it to add it to the login keychain.
3. **Export**: in Keychain Access, My Certificates → "Developer ID Application: Fusily (TEAMID)" → expand, select the certificate *and* its private key → Export 2 items… → `.p12` with a strong password.
4. Secret `APPLE_CERTIFICATE` = `base64 -i DeveloperID.p12 | pbcopy` (paste the clipboard); secret `APPLE_CERTIFICATE_PASSWORD` = the export password; variable `APPLE_SIGNING_IDENTITY` = the exact certificate name, `Developer ID Application: Fusily (TEAMID)` (`security find-identity -v -p codesigning` prints it).
5. **Notarization API key**: appstoreconnect.apple.com → Users and Access → Integrations → App Store Connect API → Team Keys → + → name "Fabrilo CI", access "Developer" → Generate → download the `.p8` (only possible once). Secret `APPLE_API_KEY_P8` = the content of the `.p8` file; variable `APPLE_API_KEY` = its Key ID; variable `APPLE_API_ISSUER` = the Issuer ID shown above the list. The workflow writes the `.p8` to a temporary file for Tauri (`APPLE_API_KEY_PATH`) and deletes it at the end.
6. The keychain password used in CI is generated at random by Tauri: nothing to create.

### SignPath Foundation

1. Apply at signpath.org → "Apply for Open Source code signing" with the repository URL, licence (MIT) and a short description. Approval takes some days; it may require that a first release exists (the unsigned dry-run output can be published as a pre-release for that if asked).
2. Once accepted, in app.signpath.io: install the **SignPath GitHub App** on the repository and add GitHub as trusted build system of the project; in the project `fabrilo`, create the artifact configurations `program` and `installers` from the files in `.signpath/artifact-configurations/`, and check the signing policy slug `release-signing` (manual approval by you).
3. Create a CI user (Users → Add CI user, submitter on `release-signing`) and generate its **API token**: secret `SIGNPATH_API_TOKEN`. Variable `SIGNPATH_ORGANIZATION_ID` = the organization ID shown in Settings; variable `SIGNPATH_PROJECT_SLUG` = `fabrilo`.
4. At each release, approve the two signing requests in SignPath (email notification); the workflow waits for them.

### GitHub Pages

Settings → Pages → Build and deployment → Source: **GitHub Actions**. The `channels` job deploys `updates/stable.json` and `updates/beta.json` at the next published release.

## Troubleshooting

**`verify` fails with "does not match version".** The tag is not the version of `crates/desktop-app/Cargo.toml`: delete the tag, bump the version in a pull request, tag again.

**A build fails at "Check the signing secrets".** The error names each missing secret or variable of the `release` environment; add it (see above) and re-run the job.

**`build-windows` times out at a SignPath step.** A request was not approved within 2 hours. Re-run the job and approve the new request.

**`channels` fails with "must be a pre-release" or "must not be a pre-release".** The pre-release flag of the published release disagrees with its tag. Fix the flag on GitHub: the edit triggers the job again.
