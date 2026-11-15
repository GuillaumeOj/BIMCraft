//! Updates of the program itself, by channel (ENG-05): the Tauri commands `app_info`,
//! `update_check`, `update_download` and `update_install`, described in docs/dev/desktop.md.
//!
//! The window names a [`Channel`]; the endpoint of each channel is fixed here, so the window can
//! never make the program fetch another address. [`UpdateHost`] keeps the offer of the last check
//! and its downloaded package; the network and the installer sit behind the [`Updates`] and
//! [`Package`] traits, which `desktop-app` implements on Tauri's updater plugin in release builds
//! that carry an updater public key ([`can_update`]). The host never starts a check, a download
//! or an install by itself.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime, State};

/// The Tauri event that carries an [`UpdateProgress`] while a package downloads.
pub const PROGRESS: &str = "update://progress";

/// The shortest time between two [`PROGRESS`] events of one download, the final one apart.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// An update channel: which releases the program follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Stable releases only.
    Stable,
    /// Beta and stable releases, whichever is newer.
    Beta,
}

impl Channel {
    /// The address of the channel's update manifest.
    #[must_use]
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => "https://guillaumeoj.github.io/BIMCraft/updates/stable.json",
            Self::Beta => "https://guillaumeoj.github.io/BIMCraft/updates/beta.json",
        }
    }
}

/// The running program, as `app_info` reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// The program's version, semver.
    pub version: String,
    /// Whether this build can update itself: not in debug builds, nor without an updater key.
    pub updatable: bool,
}

/// A newer version found by a check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    /// The newer version, semver.
    pub version: String,
    /// Its publication date, RFC 3339, when the manifest has one.
    pub date: Option<String>,
    /// Its release notes, plain text, possibly empty.
    pub notes: String,
}

/// How far a download is: the payload of [`PROGRESS`] events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// Bytes received so far.
    pub downloaded: u64,
    /// Bytes expected, when the server says.
    pub total: Option<u64>,
}

/// Why an update command failed; the window shows its own text for each code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// This build cannot update itself.
    Unavailable,
    /// No connection, a timeout or an HTTP error.
    Network,
    /// The channel's manifest is malformed or has no package for this platform.
    Manifest,
    /// The downloaded package's signature does not verify.
    Signature,
    /// A download or an install was asked with no offer from the last check.
    NoUpdate,
    /// An install was asked before a completed download.
    NotDownloaded,
    /// The installer failed to start or to replace the program.
    Install,
}

/// A failed update command: its code, and an English message for logs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct UpdateError {
    /// What went wrong.
    pub code: ErrorCode,
    /// Details, in English, for logs only.
    pub message: String,
}

impl UpdateError {
    /// An error with `code` and `message`.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub use platform::BoxFuture;

/// Where updates come from: checks a channel's manifest.
pub trait Updates: Send + Sync {
    /// The newer version that the manifest at `endpoint` offers, if any. Never a downgrade.
    fn check(&self, endpoint: &'static str) -> BoxFuture<'_, Result<Option<Found>, UpdateError>>;
}

/// A newer version found by [`Updates::check`].
pub struct Found {
    /// What the window is told about it.
    pub offer: UpdateOffer,
    /// How to fetch and install it.
    pub package: Box<dyn Package>,
}

/// The package of an offered version.
pub trait Package: Send + Sync {
    /// Downloads the package and verifies its signature, calling `on_chunk` with the length of
    /// each chunk received and the expected total length when known.
    fn download<'a>(
        &'a self,
        on_chunk: &'a mut (dyn FnMut(usize, Option<u64>) + Send),
    ) -> BoxFuture<'a, Result<Vec<u8>, UpdateError>>;

    /// Installs the downloaded `bytes` and asks the program to restart, which it does once the
    /// command has answered.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Install`] when the installer cannot start or replace the program.
    fn install(&self, bytes: &[u8]) -> Result<(), UpdateError>;
}

/// The program's update state, shared by the update commands.
pub struct UpdateHost {
    version: String,
    updates: Option<Box<dyn Updates>>,
    current: Mutex<Current>,
    // Checks started so far: each check's number, so that the latest one asked wins.
    checks: AtomicU64,
    now: Box<dyn Fn() -> Instant + Send + Sync>,
}

#[derive(Default)]
struct Current {
    offer: Option<Arc<Offer>>,
    // The number of the check whose answer `offer` is.
    check: u64,
}

/// The offer of a check, and its download.
struct Offer {
    version: String,
    package: Box<dyn Package>,
    // Held for the whole download, so a second download waits for the first.
    download: AsyncMutex<Download>,
    // Downloads of this offer that finished, failed or not.
    finished: AtomicU64,
    installing: AtomicBool,
}

enum Download {
    NotStarted,
    Failed(UpdateError),
    Done(Vec<u8>),
}

impl Offer {
    fn new(version: String, package: Box<dyn Package>) -> Self {
        Self {
            version,
            package,
            download: AsyncMutex::new(Download::NotStarted),
            finished: AtomicU64::new(0),
            installing: AtomicBool::new(false),
        }
    }

    /// Whether a check that offers `version` again keeps this offer: when its package is
    /// downloaded or downloading. Otherwise the new answer replaces it, so that a manifest
    /// corrected for the same version (a new signature, say) is used.
    fn kept_for(&self, version: &str) -> bool {
        self.version == version
            && self
                .download
                .try_lock()
                .map_or(true, |download| matches!(*download, Download::Done(_)))
    }
}

impl UpdateHost {
    /// The update state of a program at `version`, whose updates come from `updates`, or which
    /// cannot update itself when that is `None`.
    #[must_use]
    pub fn new(version: impl Into<String>, updates: Option<Box<dyn Updates>>) -> Self {
        Self {
            version: version.into(),
            updates,
            current: Mutex::new(Current::default()),
            checks: AtomicU64::new(0),
            now: Box::new(Instant::now),
        }
    }

    /// The running program.
    #[must_use]
    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            version: self.version.clone(),
            updatable: self.updates.is_some(),
        }
    }

    /// Checks `channel` for a newer version and keeps it as the offer, replacing the previous
    /// one; a package downloaded for another version is dropped. A failed check keeps the
    /// previous offer, and so does a check answered after a check asked later.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Unavailable`], [`ErrorCode::Network`] or [`ErrorCode::Manifest`].
    pub async fn check(&self, channel: Channel) -> Result<Option<UpdateOffer>, UpdateError> {
        let updates = self.updates()?;
        let number = self.checks.fetch_add(1, Ordering::AcqRel) + 1;
        let found = updates.check(channel.endpoint()).await?;
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        if number < current.check {
            return Ok(found.map(|found| found.offer));
        }
        current.check = number;
        let Some(found) = found else {
            current.offer = None;
            return Ok(None);
        };
        let kept = current
            .offer
            .as_ref()
            .is_some_and(|offer| offer.kept_for(&found.offer.version));
        if !kept {
            let offer = Offer::new(found.offer.version.clone(), found.package);
            current.offer = Some(Arc::new(offer));
        }
        Ok(Some(found.offer))
    }

    /// Downloads and verifies the package of the current offer, reporting its progress to
    /// `progress` at most every [`PROGRESS_INTERVAL`] and once at the end. Returns at once when
    /// it is already downloaded; a call while another runs waits for that download and shares
    /// its result.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Unavailable`], [`ErrorCode::NoUpdate`] (also when a check replaces the
    /// offer during the download), [`ErrorCode::Network`] or [`ErrorCode::Signature`].
    pub async fn download(
        &self,
        progress: &mut (dyn FnMut(UpdateProgress) + Send),
    ) -> Result<(), UpdateError> {
        self.updates()?;
        let offer = self.current_offer().ok_or_else(no_update)?;
        let finished_before = offer.finished.load(Ordering::Acquire);
        let mut download = offer.download.lock().await;
        let result = match &*download {
            Download::Done(_) => Ok(()),
            Download::Failed(error) if offer.finished.load(Ordering::Acquire) > finished_before => {
                Err(error.clone())
            }
            Download::NotStarted | Download::Failed(_) => {
                let result = self.fetch(&offer, progress).await;
                offer.finished.fetch_add(1, Ordering::AcqRel);
                let (state, result) = match result {
                    Ok(bytes) => (Download::Done(bytes), Ok(())),
                    Err(error) => (Download::Failed(error.clone()), Err(error)),
                };
                *download = state;
                result
            }
        };
        if self.is_current(&offer) {
            result
        } else {
            Err(replaced())
        }
    }

    /// Installs the downloaded package and asks the program to restart. A second call while
    /// the first installs does nothing.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Unavailable`], [`ErrorCode::NotDownloaded`] or [`ErrorCode::Install`].
    pub fn install(&self) -> Result<(), UpdateError> {
        self.updates()?;
        let offer = self.current_offer().ok_or_else(not_downloaded)?;
        if offer.installing.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        // A download still running holds the lock: the package is not there yet.
        let result = match offer.download.try_lock().as_deref() {
            Ok(Download::Done(bytes)) => offer.package.install(bytes),
            _ => Err(not_downloaded()),
        };
        if result.is_err() {
            offer.installing.store(false, Ordering::Release);
        }
        result
    }

    fn updates(&self) -> Result<&dyn Updates, UpdateError> {
        self.updates.as_deref().ok_or_else(|| {
            UpdateError::new(
                ErrorCode::Unavailable,
                "this build cannot update itself (debug build or no updater key)",
            )
        })
    }

    fn current_offer(&self) -> Option<Arc<Offer>> {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .offer
            .clone()
    }

    fn is_current(&self, offer: &Arc<Offer>) -> bool {
        self.current_offer()
            .is_some_and(|current| Arc::ptr_eq(&current, offer))
    }

    /// Downloads the package of `offer`. Progress is reported while `offer` is current only, so
    /// that the events of a download a check made obsolete never mix with the next one's.
    async fn fetch(
        &self,
        offer: &Arc<Offer>,
        progress: &mut (dyn FnMut(UpdateProgress) + Send),
    ) -> Result<Vec<u8>, UpdateError> {
        let mut status = UpdateProgress {
            downloaded: 0,
            total: None,
        };
        let mut sent = None;
        let mut throttle = Throttle::default();
        let bytes = {
            let mut on_chunk = |length: usize, total: Option<u64>| {
                let length = u64::try_from(length).unwrap_or(u64::MAX);
                status.downloaded = status.downloaded.saturating_add(length);
                status.total = total;
                if throttle.ready((self.now)()) && self.is_current(offer) {
                    progress(status);
                    sent = Some(status);
                }
            };
            offer.package.download(&mut on_chunk).await?
        };
        if sent != Some(status) && self.is_current(offer) {
            progress(status);
        }
        Ok(bytes)
    }
}

fn replaced() -> UpdateError {
    UpdateError::new(
        ErrorCode::NoUpdate,
        "a later check replaced the offer while its package downloaded",
    )
}

fn no_update() -> UpdateError {
    UpdateError::new(
        ErrorCode::NoUpdate,
        "no update was offered by the last check",
    )
}

fn not_downloaded() -> UpdateError {
    UpdateError::new(
        ErrorCode::NotDownloaded,
        "the update's package is not downloaded",
    )
}

/// Lets an event through at most every [`PROGRESS_INTERVAL`].
#[derive(Default)]
struct Throttle {
    last: Option<Instant>,
}

impl Throttle {
    /// Whether an event may go out at `now`; if so, it counts as sent.
    fn ready(&mut self, now: Instant) -> bool {
        let ready = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= PROGRESS_INTERVAL);
        if ready {
            self.last = Some(now);
        }
        ready
    }
}

/// The updater public key of `config` (`plugins.updater.pubkey`), when it has a non-empty one.
fn updater_key(config: &tauri::Config) -> Option<&str> {
    config
        .plugins
        .0
        .get("updater")?
        .get("pubkey")?
        .as_str()
        .filter(|key| !key.trim().is_empty())
}

/// Whether the program built with `config` can update itself: release builds with an updater
/// public key (`plugins.updater.pubkey`) only, so that development and CI windows never replace
/// themselves.
#[must_use]
pub fn can_update(config: &tauri::Config) -> bool {
    updatable(cfg!(debug_assertions), updater_key(config))
}

fn updatable(debug: bool, key: Option<&str>) -> bool {
    !debug && key.is_some()
}

/// `app_info`: the running program. `async`, like every update command, so it never runs on the
/// main thread.
#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri hands commands their arguments by value.
pub(crate) fn app_info(host: State<'_, UpdateHost>) -> AppInfo {
    host.app_info()
}

/// `update_check { channel }`: the newer version of `channel`, or `null`.
#[tauri::command]
pub(crate) async fn update_check(
    host: State<'_, UpdateHost>,
    channel: Channel,
) -> Result<Option<UpdateOffer>, UpdateError> {
    host.check(channel).await
}

/// `update_download`: downloads the offered package, emitting [`PROGRESS`] events.
#[tauri::command]
pub(crate) async fn update_download<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, UpdateHost>,
) -> Result<(), UpdateError> {
    host.download(&mut |progress| {
        // Emitting fails only when no window can receive events, so nobody misses it.
        let _ = app.emit(PROGRESS, progress);
    })
    .await
}

/// `update_install`: installs the downloaded package and restarts the program.
#[tauri::command]
pub(crate) async fn update_install(host: State<'_, UpdateHost>) -> Result<(), UpdateError> {
    host.install()
}

/// Makes the [`Updates`] of an application from its handle.
pub type UpdatesSource<R> = fn(&AppHandle<R>) -> Box<dyn Updates>;

/// `builder` with the program's update state, whose updates come from `source`, or which cannot
/// update itself when that is `None`.
pub(crate) fn register<R: Runtime>(
    builder: Builder<R>,
    source: Option<UpdatesSource<R>>,
) -> Builder<R> {
    // A plugin's setup runs as the application is built, with its handle, before any window
    // can call a command; the builder's own setup hook waits for the event loop.
    builder.plugin(
        tauri::plugin::Builder::<R>::new("bimcraft-updates")
            .setup(move |app, _| {
                let version = app.package_info().version.to_string();
                app.manage(UpdateHost::new(version, source.map(|source| source(app))));
                Ok(())
            })
            .build(),
    )
}

#[cfg(test)]
mod tests;
