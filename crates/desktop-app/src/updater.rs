//! The program's updates on Tauri's updater plugin (ENG-05): the HTTPS requests, the signature
//! check against the configured public key, and the platform installers, behind
//! `host_desktop::update`'s [`Updates`] and [`Package`].
//!
//! Only this glue touches the network and the installers, so `host-desktop`, where the update
//! state and commands live, is tested without them. Its checks, downloads and installs run in
//! release builds with an updater key only; what it decides (the offer it reads from an update
//! and the error code of each failure) is unit-tested here.

use std::time::Duration;

use host_desktop::update::{
    BoxFuture, ErrorCode, Found, Package, UpdateError, UpdateOffer, Updates, UpdatesSource,
    can_update,
};
use tauri::{AppHandle, Builder, Config, Runtime, Url};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

/// The longest an update check may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest a check or a download may stall (connecting, or between two chunks): a large
/// package on a slow line may take longer than [`CHECK_TIMEOUT`] as a whole.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// `builder` with the updater plugin when the program built with `config` can update itself,
/// and where its updates then come from. The window gets none of the plugin's own commands: the
/// capability grants none (`capabilities/main.json`).
pub(crate) fn register<R: Runtime>(
    builder: Builder<R>,
    config: &Config,
) -> (Builder<R>, Option<UpdatesSource<R>>) {
    if can_update(config) {
        let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        (builder, Some(updates::<R>))
    } else {
        (builder, None)
    }
}

/// The updates the updater plugin of `app` serves.
fn updates<R: Runtime>(app: &AppHandle<R>) -> Box<dyn Updates> {
    Box::new(PluginUpdates { app: app.clone() })
}

struct PluginUpdates<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Updates for PluginUpdates<R> {
    fn check(&self, endpoint: &'static str) -> BoxFuture<'_, Result<Option<Found>, UpdateError>> {
        Box::pin(async move {
            let endpoint = Url::parse(endpoint)
                .map_err(|parse| UpdateError::new(ErrorCode::Unavailable, parse.to_string()))?;
            let updater = self
                .app
                .updater_builder()
                .endpoints(vec![endpoint])
                .map_err(|failure| error(Phase::Check, &failure))?
                .timeout(CHECK_TIMEOUT)
                // Explicit, whatever the configuration says: only a newer version is an update.
                .version_comparator(|current, release| release.version > current)
                .configure_client(|client| {
                    client
                        .connect_timeout(STALL_TIMEOUT)
                        .read_timeout(STALL_TIMEOUT)
                })
                .build()
                .map_err(|failure| error(Phase::Check, &failure))?;
            let update = updater
                .check()
                .await
                .map_err(|failure| error(Phase::Check, &failure))?;
            // The plugin gives the update no overall timeout, so a long download is not cut
            // short; the client's stall timeouts still apply to it.
            Ok(update.map(|update| {
                let pub_date = update
                    .date
                    .and(update.raw_json.get("pub_date"))
                    .and_then(|date| date.as_str());
                Found {
                    offer: offer(&update.version, pub_date, update.body.as_deref()),
                    package: Box::new(PluginPackage {
                        update,
                        app: self.app.clone(),
                    }),
                }
            }))
        })
    }
}

struct PluginPackage<R: Runtime> {
    update: Update,
    app: AppHandle<R>,
}

impl<R: Runtime> Package for PluginPackage<R> {
    fn download<'a>(
        &'a self,
        on_chunk: &'a mut (dyn FnMut(usize, Option<u64>) + Send),
    ) -> BoxFuture<'a, Result<Vec<u8>, UpdateError>> {
        Box::pin(async move {
            self.update
                .download(on_chunk, || {})
                .await
                .map_err(|failure| error(Phase::Download, &failure))
        })
    }

    fn install(&self, bytes: &[u8]) -> Result<(), UpdateError> {
        self.update
            .install(bytes)
            .map_err(|failure| error(Phase::Install, &failure))?;
        // Windows never gets here: the installer has taken over and the program has exited.
        // `restart` would park this thread for good; this asks the event loop instead, which
        // restarts the program once the command has answered.
        self.app.request_restart();
        Ok(())
    }
}

/// The offer of an update at `version`, published at `pub_date` (RFC 3339, as the manifest has
/// it, when the plugin could read it) with release `notes`.
fn offer(version: &str, pub_date: Option<&str>, notes: Option<&str>) -> UpdateOffer {
    UpdateOffer {
        version: version.to_owned(),
        date: pub_date.map(ToOwned::to_owned),
        notes: notes.unwrap_or_default().to_owned(),
    }
}

/// The step of an update that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Check,
    Download,
    Install,
}

/// The update error for `failure` during `phase`.
fn error(phase: Phase, failure: &Error) -> UpdateError {
    UpdateError::new(code(phase, failure), failure.to_string())
}

fn code(phase: Phase, failure: &Error) -> ErrorCode {
    match (phase, failure) {
        (Phase::Install, _) => ErrorCode::Install,
        // A response that is not JSON fails as it is decoded.
        (Phase::Check, Error::Reqwest(request)) if request.is_decode() => ErrorCode::Manifest,
        // Every endpoint answered with an HTTP error status.
        (_, Error::Reqwest(_) | Error::Network(_)) | (Phase::Check, Error::ReleaseNotFound) => {
            ErrorCode::Network
        }
        (
            Phase::Check,
            Error::Serialization(_)
            | Error::Semver(_)
            | Error::TargetNotFound(_)
            | Error::TargetsNotFound(_)
            | Error::UrlParse(_),
        ) => ErrorCode::Manifest,
        // The platform, the architecture or the install location cannot be updated.
        (Phase::Check, _) => ErrorCode::Unavailable,
        (
            Phase::Download,
            Error::Minisign(_)
            | Error::Base64(_)
            | Error::SignatureUtf8(_)
            | Error::SignedVersionMismatch { .. }
            | Error::MissingSignedVersion,
        ) => ErrorCode::Signature,
        (Phase::Download, _) => ErrorCode::Network,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offer_carries_the_manifest_date_and_notes_as_they_are() {
        assert_eq!(
            offer("0.4.0", Some("2026-11-01T10:00:00Z"), Some("Fixes <b>")),
            UpdateOffer {
                version: "0.4.0".into(),
                date: Some("2026-11-01T10:00:00Z".into()),
                notes: "Fixes <b>".into(),
            }
        );
    }

    #[test]
    fn an_offer_without_date_or_notes_has_none_and_empty_notes() {
        assert_eq!(
            offer("0.4.0-beta.1", None, None),
            UpdateOffer {
                version: "0.4.0-beta.1".into(),
                date: None,
                notes: String::new(),
            }
        );
    }

    fn json_error() -> Error {
        Error::Serialization(serde_json::from_str::<u8>("x").unwrap_err())
    }

    fn url_error() -> Error {
        Error::UrlParse(Url::parse("no scheme").unwrap_err())
    }

    fn assert_codes(phase: Phase, cases: Vec<(Error, ErrorCode)>) {
        for (failure, expected) in cases {
            let message = failure.to_string();
            assert_eq!(
                error(phase, &failure),
                UpdateError::new(expected, message),
                "{phase:?}"
            );
        }
    }

    #[test]
    fn check_failures_are_network_manifest_or_unavailable() {
        assert_codes(
            Phase::Check,
            vec![
                (Error::Network("timed out".into()), ErrorCode::Network),
                (Error::ReleaseNotFound, ErrorCode::Network),
                (json_error(), ErrorCode::Manifest),
                (url_error(), ErrorCode::Manifest),
                (Error::TargetNotFound("darwin".into()), ErrorCode::Manifest),
                (
                    Error::TargetsNotFound(vec!["darwin-aarch64".into()]),
                    ErrorCode::Manifest,
                ),
                (Error::UnsupportedOs, ErrorCode::Unavailable),
                (Error::UnsupportedArch, ErrorCode::Unavailable),
                (Error::EmptyEndpoints, ErrorCode::Unavailable),
            ],
        );
    }

    #[test]
    fn download_failures_are_network_or_signature() {
        assert_codes(
            Phase::Download,
            vec![
                (Error::Network("status 404".into()), ErrorCode::Network),
                (Error::SignatureUtf8("x".into()), ErrorCode::Signature),
                (
                    Error::SignedVersionMismatch {
                        signed: "0.3.0".into(),
                        announced: "0.4.0".into(),
                    },
                    ErrorCode::Signature,
                ),
                (Error::MissingSignedVersion, ErrorCode::Signature),
                (Error::ReleaseNotFound, ErrorCode::Network),
            ],
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    fn a_debug_build_gets_no_updater_plugin_nor_updates() {
        let mut config = Config::default();
        config.plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "dW50cnVzdGVk" }),
        );
        let (_, updates) = register(Builder::<tauri::Wry>::default(), &config);
        assert!(updates.is_none());
    }

    #[test]
    fn every_install_failure_is_an_install_error() {
        assert_codes(
            Phase::Install,
            vec![
                (Error::InvalidUpdaterFormat, ErrorCode::Install),
                (Error::MissingSignedVersion, ErrorCode::Install),
                (Error::Network("x".into()), ErrorCode::Install),
            ],
        );
    }
}
