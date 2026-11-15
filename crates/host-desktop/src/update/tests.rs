//! The update state and commands, on fake updates: no network, no installer.

use std::collections::VecDeque;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::task::{Context, Poll, Waker};

use serde_json::json;
use tauri::async_runtime::block_on;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

use super::*;

/// What the fake package does when downloaded, and what it saw.
#[derive(Default)]
struct Script {
    /// Each chunk: its length, the total announced, and how long it takes to arrive.
    chunks: Vec<(usize, Option<u64>, Duration)>,
    /// The downloads' result, in order; the last one repeats.
    downloads: Mutex<VecDeque<Result<Vec<u8>, UpdateError>>>,
    /// Released when the download may finish; `None` finishes at once.
    gate: Option<Arc<AtomicBool>>,
    install: Option<UpdateError>,
    downloaded: AtomicU64,
    installed: Mutex<Vec<Vec<u8>>>,
}

/// A clock that only the fake package moves.
#[derive(Clone)]
struct Clock(Arc<Mutex<Instant>>);

impl Clock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }

    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

struct FakePackage {
    script: Arc<Script>,
    clock: Clock,
}

impl Package for FakePackage {
    fn download<'a>(
        &'a self,
        on_chunk: &'a mut (dyn FnMut(usize, Option<u64>) + Send),
    ) -> BoxFuture<'a, Result<Vec<u8>, UpdateError>> {
        Box::pin(async move {
            self.script.downloaded.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.script.gate {
                opened(gate).await;
            }
            for &(length, total, delay) in &self.script.chunks {
                self.clock.advance(delay);
                on_chunk(length, total);
            }
            let mut downloads = self.script.downloads.lock().unwrap();
            if downloads.len() > 1 {
                downloads.pop_front().unwrap()
            } else {
                downloads.front().unwrap().clone()
            }
        })
    }

    fn install(&self, bytes: &[u8]) -> Result<(), UpdateError> {
        self.script.installed.lock().unwrap().push(bytes.to_vec());
        self.script.install.clone().map_or(Ok(()), Err)
    }
}

/// Waits until `gate` is open.
async fn opened(gate: &AtomicBool) {
    poll_fn(|_| {
        if gate.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// A scripted check: a version and its package, no update, or a failure.
type CheckAnswer = Result<Option<(&'static str, Arc<Script>)>, UpdateError>;

/// Fake updates: each check answers the next scripted result.
struct FakeUpdates {
    checks: Mutex<VecDeque<CheckAnswer>>,
    endpoints: Arc<Mutex<Vec<&'static str>>>,
    clock: Clock,
    // The first check to come takes this gate and answers once it opens.
    gate: Mutex<Option<Arc<AtomicBool>>>,
}

impl Updates for FakeUpdates {
    fn check(&self, endpoint: &'static str) -> BoxFuture<'_, Result<Option<Found>, UpdateError>> {
        Box::pin(async move {
            self.endpoints.lock().unwrap().push(endpoint);
            let answer = self.checks.lock().unwrap().pop_front().unwrap();
            let gate = self.gate.lock().unwrap().take();
            if let Some(gate) = gate {
                opened(&gate).await;
            }
            let answer = answer?;
            Ok(answer.map(|(version, script)| Found {
                offer: offer(version),
                package: Box::new(FakePackage {
                    script,
                    clock: self.clock.clone(),
                }),
            }))
        })
    }
}

fn offer(version: &str) -> UpdateOffer {
    UpdateOffer {
        version: version.into(),
        date: Some("2026-11-01T10:00:00Z".into()),
        notes: format!("What is new in {version}"),
    }
}

fn script(downloads: Vec<Result<Vec<u8>, UpdateError>>) -> Arc<Script> {
    Arc::new(Script {
        downloads: Mutex::new(downloads.into()),
        ..Script::default()
    })
}

/// A script whose downloads all succeed with `b"package"`.
fn packaged() -> Script {
    Script {
        downloads: Mutex::new(vec![Ok(b"package".to_vec())].into()),
        ..Script::default()
    }
}

fn package() -> Arc<Script> {
    Arc::new(packaged())
}

fn failure(code: ErrorCode) -> UpdateError {
    UpdateError::new(code, "scripted")
}

struct Fixture {
    host: UpdateHost,
    endpoints: Arc<Mutex<Vec<&'static str>>>,
}

/// An updatable host at version 0.3.0 whose checks answer `checks` in turn.
fn updatable_host(checks: Vec<CheckAnswer>) -> Fixture {
    host_with_gate(checks, None)
}

/// [`updatable_host`], whose first check answers once `gate` opens.
fn host_with_gate(checks: Vec<CheckAnswer>, gate: Option<Arc<AtomicBool>>) -> Fixture {
    let clock = Clock::new();
    let endpoints = Arc::new(Mutex::new(Vec::new()));
    let updates = FakeUpdates {
        checks: Mutex::new(checks.into()),
        endpoints: Arc::clone(&endpoints),
        clock: clock.clone(),
        gate: Mutex::new(gate),
    };
    let mut host = UpdateHost::new("0.3.0", Some(Box::new(updates)));
    host.now = Box::new(move || clock.now());
    Fixture { host, endpoints }
}

fn fixed_host() -> UpdateHost {
    UpdateHost::new("0.3.0", None)
}

fn download(host: &UpdateHost) -> (Result<(), UpdateError>, Vec<UpdateProgress>) {
    let mut events = Vec::new();
    let result = block_on(host.download(&mut |progress| events.push(progress)));
    (result, events)
}

fn code<T: std::fmt::Debug>(result: Result<T, UpdateError>) -> ErrorCode {
    result.unwrap_err().code
}

// Channels and the contract's JSON.

#[test]
fn each_channel_has_its_fixed_manifest() {
    assert_eq!(
        Channel::Stable.endpoint(),
        "https://guillaumeoj.github.io/BIMCraft/updates/stable.json"
    );
    assert_eq!(
        Channel::Beta.endpoint(),
        "https://guillaumeoj.github.io/BIMCraft/updates/beta.json"
    );
}

#[test]
fn a_channel_is_named_and_nothing_else_is_one() {
    assert_eq!(
        serde_json::from_value::<Channel>(json!("stable")).unwrap(),
        Channel::Stable
    );
    assert_eq!(
        serde_json::from_value::<Channel>(json!("beta")).unwrap(),
        Channel::Beta
    );
    for other in [
        json!("nightly"),
        json!("Stable"),
        json!("https://example.com/x.json"),
    ] {
        assert!(serde_json::from_value::<Channel>(other).is_err());
    }
}

#[test]
fn the_results_have_the_contract_shape() {
    let info = AppInfo {
        version: "0.4.0-beta.1".into(),
        updatable: true,
    };
    assert_eq!(
        serde_json::to_value(info).unwrap(),
        json!({ "version": "0.4.0-beta.1", "updatable": true })
    );
    let offer = UpdateOffer {
        version: "0.4.0".into(),
        date: None,
        notes: String::new(),
    };
    assert_eq!(
        serde_json::to_value(offer).unwrap(),
        json!({ "version": "0.4.0", "date": null, "notes": "" })
    );
    let progress = UpdateProgress {
        downloaded: 10,
        total: None,
    };
    assert_eq!(
        serde_json::to_value(progress).unwrap(),
        json!({ "downloaded": 10, "total": null })
    );
}

#[test]
fn every_error_code_has_its_contract_name() {
    let codes = [
        (ErrorCode::Unavailable, "unavailable"),
        (ErrorCode::Network, "network"),
        (ErrorCode::Manifest, "manifest"),
        (ErrorCode::Signature, "signature"),
        (ErrorCode::NoUpdate, "no_update"),
        (ErrorCode::NotDownloaded, "not_downloaded"),
        (ErrorCode::Install, "install"),
    ];
    for (code, name) in codes {
        assert_eq!(
            serde_json::to_value(UpdateError::new(code, "why")).unwrap(),
            json!({ "code": name, "message": "why" })
        );
    }
    assert_eq!(
        UpdateError::new(ErrorCode::Network, "why").to_string(),
        "why"
    );
}

// Whether a build updates itself.

#[test]
fn only_release_builds_with_a_key_update_themselves() {
    assert!(updatable(false, Some("key")));
    assert!(!updatable(true, Some("key")));
    assert!(!updatable(false, None));
}

fn config_with(updater: Option<serde_json::Value>) -> tauri::Config {
    let mut config = mock_context::<MockRuntime, _>(noop_assets())
        .config()
        .clone();
    if let Some(updater) = updater {
        config.plugins.0.insert("updater".into(), updater);
    }
    config
}

#[test]
fn the_updater_key_is_read_from_the_plugin_configuration() {
    let config = config_with(Some(json!({ "pubkey": "dW50cnVzdGVk" })));
    assert_eq!(updater_key(&config), Some("dW50cnVzdGVk"));
}

#[test]
fn a_missing_or_blank_updater_key_is_no_key() {
    for updater in [
        None,
        Some(json!({})),
        Some(json!({ "pubkey": 42 })),
        Some(json!({ "pubkey": "" })),
        Some(json!({ "pubkey": "  \n" })),
    ] {
        assert_eq!(
            updater_key(&config_with(updater.clone())),
            None,
            "{updater:?}"
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn a_debug_build_never_updates_itself_even_with_a_key() {
    assert!(!can_update(&config_with(Some(
        json!({ "pubkey": "dW50cnVzdGVk" })
    ))));
}

fn app_info_of(source: Option<UpdatesSource<MockRuntime>>) -> AppInfo {
    let app = register(mock_builder(), source)
        .build(mock_context(noop_assets()))
        .unwrap();
    app.state::<UpdateHost>().app_info()
}

#[test]
fn the_application_gets_its_update_state_as_it_is_built() {
    fn no_checks(_: &AppHandle<MockRuntime>) -> Box<dyn Updates> {
        let Fixture { host, .. } = updatable_host(Vec::new());
        host.updates.unwrap()
    }
    let version = "0.1.0".to_owned();
    assert_eq!(
        app_info_of(None),
        AppInfo {
            version: version.clone(),
            updatable: false,
        }
    );
    assert_eq!(
        app_info_of(Some(no_checks)),
        AppInfo {
            version,
            updatable: true,
        }
    );
}

// app_info.

#[test]
fn app_info_says_whether_updates_are_possible() {
    assert_eq!(
        fixed_host().app_info(),
        AppInfo {
            version: "0.3.0".into(),
            updatable: false,
        }
    );
    assert!(updatable_host(Vec::new()).host.app_info().updatable);
}

// A build that cannot update.

#[test]
fn a_build_that_cannot_update_refuses_every_update_command() {
    let host = fixed_host();
    assert_eq!(
        code(block_on(host.check(Channel::Stable))),
        ErrorCode::Unavailable
    );
    assert_eq!(code(download(&host).0), ErrorCode::Unavailable);
    assert_eq!(code(host.install()), ErrorCode::Unavailable);
}

// update_check.

#[test]
fn a_check_asks_the_channel_manifest_and_returns_the_offer() {
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package()))), Ok(None)]);
    assert_eq!(
        block_on(fixture.host.check(Channel::Beta)),
        Ok(Some(offer("0.4.0")))
    );
    assert_eq!(block_on(fixture.host.check(Channel::Stable)), Ok(None));
    assert_eq!(
        *fixture.endpoints.lock().unwrap(),
        [Channel::Beta.endpoint(), Channel::Stable.endpoint()]
    );
}

#[test]
fn a_failed_check_reports_why() {
    for expected in [ErrorCode::Network, ErrorCode::Manifest] {
        let fixture = updatable_host(vec![Err(failure(expected))]);
        assert_eq!(
            code(block_on(fixture.host.check(Channel::Stable))),
            expected
        );
    }
}

#[test]
fn a_failed_check_keeps_the_previous_offer_and_its_download() {
    let package = package();
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", Arc::clone(&package)))),
        Err(failure(ErrorCode::Network)),
    ]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    code(block_on(fixture.host.check(Channel::Stable)));
    assert_eq!(fixture.host.install(), Ok(()));
}

#[test]
fn no_update_drops_the_previous_offer() {
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package()))), Ok(None)]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    assert_eq!(code(download(&fixture.host).0), ErrorCode::NoUpdate);
    assert_eq!(code(fixture.host.install()), ErrorCode::NotDownloaded);
}

#[test]
fn a_newer_offer_drops_the_package_downloaded_for_the_older() {
    let older = package();
    let newer = package();
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", Arc::clone(&older)))),
        Ok(Some(("0.5.0", Arc::clone(&newer)))),
    ]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    assert_eq!(
        block_on(fixture.host.check(Channel::Beta)),
        Ok(Some(offer("0.5.0")))
    );
    assert_eq!(code(fixture.host.install()), ErrorCode::NotDownloaded);
    download(&fixture.host).0.unwrap();
    assert_eq!(newer.downloaded.load(Ordering::SeqCst), 1);
    fixture.host.install().unwrap();
    assert!(older.installed.lock().unwrap().is_empty());
    assert_eq!(*newer.installed.lock().unwrap(), [b"package".to_vec()]);
}

#[test]
fn the_same_version_offered_again_keeps_its_download() {
    let first = package();
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", Arc::clone(&first)))),
        Ok(Some(("0.4.0", package()))),
    ]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    assert_eq!(
        block_on(fixture.host.check(Channel::Stable)),
        Ok(Some(offer("0.4.0")))
    );
    fixture.host.install().unwrap();
    assert_eq!(first.installed.lock().unwrap().len(), 1);
}

// update_download.

#[test]
fn a_download_without_an_offer_is_refused() {
    let fixture = updatable_host(Vec::new());
    assert_eq!(code(download(&fixture.host).0), ErrorCode::NoUpdate);
}

#[test]
fn a_download_reports_progress_at_most_every_100_ms_and_at_the_end() {
    let ms = Duration::from_millis;
    let package = Arc::new(Script {
        chunks: vec![
            (100, Some(1000), ms(0)),
            (100, Some(1000), ms(50)),
            (100, Some(1000), ms(49)),
            (100, Some(1000), ms(1)),
            (100, Some(1000), ms(300)),
            (500, Some(1000), ms(10)),
        ],
        ..packaged()
    });
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package)))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    let (result, events) = download(&fixture.host);
    assert_eq!(result, Ok(()));
    let progress = |downloaded| UpdateProgress {
        downloaded,
        total: Some(1000),
    };
    assert_eq!(
        events,
        [progress(100), progress(400), progress(500), progress(1000)]
    );
}

#[test]
fn a_download_without_chunks_still_reports_its_end() {
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package())))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    let (result, events) = download(&fixture.host);
    assert_eq!(result, Ok(()));
    assert_eq!(
        events,
        [UpdateProgress {
            downloaded: 0,
            total: None
        }]
    );
}

#[test]
fn a_finished_download_resolves_at_once_without_downloading_again() {
    let package = package();
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    let (result, events) = download(&fixture.host);
    assert_eq!(result, Ok(()));
    assert!(events.is_empty());
    assert_eq!(package.downloaded.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_download_reports_why_and_can_be_tried_again() {
    for expected in [ErrorCode::Network, ErrorCode::Signature] {
        let package = script(vec![Err(failure(expected)), Ok(b"package".to_vec())]);
        let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
        block_on(fixture.host.check(Channel::Stable)).unwrap();
        let (result, events) = download(&fixture.host);
        assert_eq!(code(result), expected);
        assert!(events.is_empty(), "no final progress after a failure");
        assert_eq!(code(fixture.host.install()), ErrorCode::NotDownloaded);
        assert_eq!(download(&fixture.host).0, Ok(()));
        assert_eq!(package.downloaded.load(Ordering::SeqCst), 2);
        fixture.host.install().unwrap();
    }
}

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// Two downloads of `result`, the second started while the first waits for the network.
fn concurrent_downloads(
    result: Result<Vec<u8>, UpdateError>,
) -> (
    Result<(), UpdateError>,
    Result<(), UpdateError>,
    Arc<Script>,
    UpdateHost,
) {
    let gate = Arc::new(AtomicBool::new(false));
    let package = Arc::new(Script {
        downloads: Mutex::new(vec![result, Ok(b"package".to_vec())].into()),
        gate: Some(Arc::clone(&gate)),
        ..Script::default()
    });
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    let host = fixture.host;
    block_on(host.check(Channel::Stable)).unwrap();
    let (first, second) = {
        let mut ignore = |_| {};
        let mut ignore_too = |_| {};
        let mut first = std::pin::pin!(host.download(&mut ignore));
        let mut second = std::pin::pin!(host.download(&mut ignore_too));
        assert!(poll_once(first.as_mut()).is_pending());
        assert!(poll_once(second.as_mut()).is_pending());
        assert_eq!(
            code(host.install()),
            ErrorCode::NotDownloaded,
            "still downloading"
        );
        gate.store(true, Ordering::SeqCst);
        (block_on(first), block_on(second))
    };
    (first, second, package, host)
}

#[test]
fn a_download_asked_while_one_runs_waits_for_it() {
    let (first, second, package, host) = concurrent_downloads(Ok(b"package".to_vec()));
    assert_eq!((first, second), (Ok(()), Ok(())));
    assert_eq!(package.downloaded.load(Ordering::SeqCst), 1);
    host.install().unwrap();
}

#[test]
fn a_download_asked_while_one_runs_shares_its_failure() {
    let (first, second, package, _) = concurrent_downloads(Err(failure(ErrorCode::Network)));
    assert_eq!(code(first), ErrorCode::Network);
    assert_eq!(code(second), ErrorCode::Network);
    assert_eq!(package.downloaded.load(Ordering::SeqCst), 1);
}

// update_install.

#[test]
fn an_install_before_any_check_or_download_is_refused() {
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package())))]);
    assert_eq!(code(fixture.host.install()), ErrorCode::NotDownloaded);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    assert_eq!(code(fixture.host.install()), ErrorCode::NotDownloaded);
}

#[test]
fn an_install_installs_the_downloaded_package() {
    let package = package();
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    assert_eq!(fixture.host.install(), Ok(()));
    assert_eq!(*package.installed.lock().unwrap(), [b"package".to_vec()]);
}

#[test]
fn a_check_answered_after_a_later_one_is_returned_but_not_kept() {
    let gate = Arc::new(AtomicBool::new(false));
    let beta = package();
    let stable = package();
    let fixture = host_with_gate(
        vec![
            Ok(Some(("0.5.0-beta.1", Arc::clone(&beta)))),
            Ok(Some(("0.4.0", Arc::clone(&stable)))),
        ],
        Some(Arc::clone(&gate)),
    );
    let host = &fixture.host;
    let mut slow = std::pin::pin!(host.check(Channel::Beta));
    assert!(poll_once(slow.as_mut()).is_pending());
    assert_eq!(
        block_on(host.check(Channel::Stable)),
        Ok(Some(offer("0.4.0")))
    );
    gate.store(true, Ordering::SeqCst);
    assert_eq!(block_on(slow), Ok(Some(offer("0.5.0-beta.1"))));
    download(host).0.unwrap();
    host.install().unwrap();
    assert_eq!(stable.installed.lock().unwrap().len(), 1);
    assert_eq!(beta.downloaded.load(Ordering::SeqCst), 0);
}

#[test]
fn the_same_version_offered_again_after_a_failed_download_uses_the_new_package() {
    let broken = script(vec![Err(failure(ErrorCode::Signature))]);
    let resigned = package();
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", Arc::clone(&broken)))),
        Ok(Some(("0.4.0", Arc::clone(&resigned)))),
    ]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    assert_eq!(code(download(&fixture.host).0), ErrorCode::Signature);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    assert_eq!(download(&fixture.host).0, Ok(()));
    assert_eq!(broken.downloaded.load(Ordering::SeqCst), 1);
    assert_eq!(resigned.downloaded.load(Ordering::SeqCst), 1);
}

#[test]
fn the_same_version_offered_again_while_downloading_keeps_that_download() {
    let gate = Arc::new(AtomicBool::new(false));
    let first = Arc::new(Script {
        gate: Some(Arc::clone(&gate)),
        ..packaged()
    });
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", Arc::clone(&first)))),
        Ok(Some(("0.4.0", package()))),
    ]);
    let host = &fixture.host;
    block_on(host.check(Channel::Stable)).unwrap();
    let mut ignore = |_| {};
    let mut running = std::pin::pin!(host.download(&mut ignore));
    assert!(poll_once(running.as_mut()).is_pending());
    block_on(host.check(Channel::Stable)).unwrap();
    gate.store(true, Ordering::SeqCst);
    assert_eq!(block_on(running), Ok(()));
    host.install().unwrap();
    assert_eq!(first.installed.lock().unwrap().len(), 1);
}

#[test]
fn a_download_whose_offer_a_check_replaces_fails_and_reports_no_progress() {
    let gate = Arc::new(AtomicBool::new(false));
    let older = Arc::new(Script {
        chunks: vec![(5, Some(5), Duration::ZERO)],
        downloads: Mutex::new(vec![Ok(b"older".to_vec())].into()),
        gate: Some(Arc::clone(&gate)),
        ..Script::default()
    });
    let fixture = updatable_host(vec![
        Ok(Some(("0.4.0", older))),
        Ok(Some(("0.4.1", package()))),
    ]);
    let host = &fixture.host;
    block_on(host.check(Channel::Stable)).unwrap();
    let mut events = Vec::new();
    let result = {
        let mut record = |progress| events.push(progress);
        let mut running = std::pin::pin!(host.download(&mut record));
        assert!(poll_once(running.as_mut()).is_pending());
        block_on(host.check(Channel::Stable)).unwrap();
        gate.store(true, Ordering::SeqCst);
        block_on(running)
    };
    assert_eq!(code(result), ErrorCode::NoUpdate);
    assert!(events.is_empty());
    assert_eq!(code(host.install()), ErrorCode::NotDownloaded);
}

#[test]
fn the_final_progress_is_not_repeated_when_the_last_chunk_reported_it() {
    let package = Arc::new(Script {
        chunks: vec![
            (3, Some(7), Duration::ZERO),
            (4, Some(7), PROGRESS_INTERVAL),
        ],
        ..packaged()
    });
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", package)))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    let (result, events) = download(&fixture.host);
    assert_eq!(result, Ok(()));
    let progress = |downloaded| UpdateProgress {
        downloaded,
        total: Some(7),
    };
    assert_eq!(events, [progress(3), progress(7)]);
}

#[test]
fn a_second_install_while_the_first_runs_does_nothing() {
    let package = package();
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    fixture.host.install().unwrap();
    assert_eq!(fixture.host.install(), Ok(()));
    assert_eq!(package.installed.lock().unwrap().len(), 1);
}

#[test]
fn an_install_can_be_tried_again_after_a_failure() {
    let package = Arc::new(Script {
        install: Some(failure(ErrorCode::Install)),
        ..packaged()
    });
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    block_on(fixture.host.check(Channel::Stable)).unwrap();
    download(&fixture.host).0.unwrap();
    assert_eq!(code(fixture.host.install()), ErrorCode::Install);
    assert_eq!(code(fixture.host.install()), ErrorCode::Install);
    assert_eq!(package.installed.lock().unwrap().len(), 2);
}

// The throttle.

#[test]
fn the_throttle_lets_the_first_event_through_then_one_per_interval() {
    let start = Instant::now();
    let mut throttle = Throttle::default();
    assert!(throttle.ready(start));
    assert!(!throttle.ready(start + Duration::from_millis(99)));
    assert!(throttle.ready(start + PROGRESS_INTERVAL));
    assert!(!throttle.ready(start + Duration::from_millis(150)));
    // A clock read out of order never panics nor lets an event through early.
    assert!(!throttle.ready(start));
}

// The commands, through Tauri's IPC.

const APP_URL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

struct Window {
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    progress: Arc<Mutex<Vec<UpdateProgress>>>,
}

fn window(host: UpdateHost) -> Window {
    let app = mock_builder()
        .manage(host)
        .invoke_handler(tauri::generate_handler![
            app_info,
            update_check,
            update_download,
            update_install
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&progress);
    app.listen_any(PROGRESS, move |event| {
        let progress = serde_json::from_str(event.payload()).unwrap();
        received.lock().unwrap().push(progress);
    });
    let webview = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    Window {
        _app: app,
        webview,
        progress,
    }
}

impl Window {
    fn call(
        &self,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        let request = InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: APP_URL.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize().unwrap())
    }
}

#[test]
fn the_window_runs_a_whole_update() {
    let package = Arc::new(Script {
        chunks: vec![(3, Some(7), Duration::ZERO), (4, Some(7), Duration::ZERO)],
        ..packaged()
    });
    let fixture = updatable_host(vec![Ok(Some(("0.4.0", Arc::clone(&package))))]);
    let window = window(fixture.host);
    assert_eq!(
        window.call("app_info", json!({})),
        Ok(json!({ "version": "0.3.0", "updatable": true }))
    );
    assert_eq!(
        window.call("update_check", json!({ "channel": "beta" })),
        Ok(json!({
            "version": "0.4.0",
            "date": "2026-11-01T10:00:00Z",
            "notes": "What is new in 0.4.0"
        }))
    );
    assert_eq!(window.call("update_download", json!({})), Ok(json!(null)));
    let progress = |downloaded| UpdateProgress {
        downloaded,
        total: Some(7),
    };
    assert_eq!(*window.progress.lock().unwrap(), [progress(3), progress(7)]);
    assert_eq!(window.call("update_install", json!({})), Ok(json!(null)));
    assert_eq!(package.installed.lock().unwrap().len(), 1);
    assert_eq!(
        *fixture.endpoints.lock().unwrap(),
        [Channel::Beta.endpoint()]
    );
}

#[test]
fn the_window_cannot_name_another_channel_or_an_address() {
    let fixture = updatable_host(Vec::new());
    let window = window(fixture.host);
    for channel in [
        json!("nightly"),
        json!("https://example.com/x.json"),
        json!(null),
    ] {
        assert!(
            window
                .call("update_check", json!({ "channel": channel }))
                .is_err()
        );
    }
    assert!(window.call("update_check", json!({})).is_err());
    assert!(fixture.endpoints.lock().unwrap().is_empty());
}
