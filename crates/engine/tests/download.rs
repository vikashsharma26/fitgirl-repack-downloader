//! End-to-end tests against a local HTTP server that mimics real-world hosts:
//! correct ones, broken ones and hostile networks.

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use fitdl_engine::{Control, DownloadOptions, Engine, Error, Outcome, Progress, RateLimiter};
use futures_util::stream;
use rand::{Rng, RngCore, SeedableRng};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

const SIZE: usize = 24 * 1024 * 1024 + 12_345; // deliberately not aligned

struct Server {
    data: Bytes,
    requests: AtomicUsize,
    bytes_served: AtomicU64,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Well-behaved server with Range support.
    Normal,
    /// fuckingfast.co style: Content-Range always claims "to end of file".
    BogusEnd,
    /// Ignores Range and always sends the whole file with 200.
    NoRange,
    /// Random 500s and connections dropped mid-body.
    Flaky,
    /// Throttled, so a test can pause or kill the download midway.
    Slow,
}

async fn serve(
    State(srv): State<Arc<Server>>,
    UrlPath(mode): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    let mode = match mode.as_str() {
        "file" => Mode::Normal,
        "ff" => Mode::BogusEnd,
        "norange" => Mode::NoRange,
        "flaky" => Mode::Flaky,
        "slow" => Mode::Slow,
        "gone" => return status(StatusCode::GONE),
        _ => return status(StatusCode::NOT_FOUND),
    };
    let n = srv.requests.fetch_add(1, Ordering::Relaxed);
    if mode == Mode::Flaky && n % 3 == 2 {
        return status(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let total = srv.data.len() as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range)
        .filter(|_| mode != Mode::NoRange);

    let (start, end, code) = match range {
        Some((s, e)) => (
            s,
            e.unwrap_or(total - 1).min(total - 1),
            StatusCode::PARTIAL_CONTENT,
        ),
        None => (0, total - 1, StatusCode::OK),
    };
    let body = srv.data.slice(start as usize..=end as usize);

    let mut builder = Response::builder()
        .status(code)
        .header(header::CONTENT_LENGTH, body.len())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, "\"test-etag\"")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename*=UTF-8''test_--_file.part01.rar",
        );
    if code == StatusCode::PARTIAL_CONTENT {
        let reported_end = if mode == Mode::BogusEnd {
            total - 1
        } else {
            end
        };
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{reported_end}/{total}"),
        );
    }

    let drop_after =
        if mode == Mode::Flaky && body.len() > 64 * 1024 && rand::thread_rng().gen_bool(0.5) {
            Some(rand::thread_rng().gen_range(0..body.len() / 2))
        } else {
            None
        };
    let chunk = 64 * 1024;
    let delay = (mode == Mode::Slow).then(|| Duration::from_millis(20));
    let srv2 = srv.clone();
    let chunks = (0..body.len()).step_by(chunk).map(move |off| {
        let piece = body.slice(off..(off + chunk).min(body.len()));
        (off, piece)
    });
    let stream = stream::unfold(chunks, move |mut it| {
        let srv = srv2.clone();
        async move {
            let (off, piece) = it.next()?;
            if let Some(limit) = drop_after {
                if off >= limit {
                    return Some((Err(std::io::Error::other("simulated drop")), it));
                }
            }
            if let Some(d) = delay {
                tokio::time::sleep(d).await;
            }
            srv.bytes_served
                .fetch_add(piece.len() as u64, Ordering::Relaxed);
            Some((Ok::<_, std::io::Error>(piece), it))
        }
    });
    builder.body(Body::from_stream(stream)).unwrap()
}

fn status(code: StatusCode) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

fn parse_range(v: &str) -> Option<(u64, Option<u64>)> {
    let (s, e) = v.strip_prefix("bytes=")?.split_once('-')?;
    Some((s.parse().ok()?, e.parse().ok()))
}

async fn start_server() -> (String, Arc<Server>) {
    let mut data = vec![0u8; SIZE];
    rand::rngs::StdRng::seed_from_u64(7).fill_bytes(&mut data);
    let srv = Arc::new(Server {
        data: Bytes::from(data),
        requests: AtomicUsize::new(0),
        bytes_served: AtomicU64::new(0),
    });
    let app = Router::new()
        .route("/{mode}", get(serve))
        .with_state(srv.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), srv)
}

fn engine() -> Engine {
    Engine::new("fitdl-test", Arc::new(RateLimiter::unlimited())).unwrap()
}

fn opts() -> DownloadOptions {
    DownloadOptions {
        connections: 8,
        min_segment_size: 256 * 1024,
        buffer_size: 256 * 1024,
        max_retries: 20,
        retry_backoff: Duration::from_millis(10),
        state_save_interval: Duration::from_millis(100),
        progress_interval: Duration::from_millis(50),
    }
}

fn sha(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

fn assert_same(path: &Path, srv: &Server) {
    let got = std::fs::read(path).unwrap();
    assert_eq!(got.len(), srv.data.len(), "size differs");
    assert_eq!(sha(&got), sha(&srv.data), "content differs");
    assert!(!fitdl_engine::part_path(path).exists(), ".part left behind");
    assert!(
        !fitdl_engine::state_path(path).exists(),
        ".state left behind"
    );
}

async fn run(url: &str, dest: &Path, opts: &DownloadOptions) -> fitdl_engine::Result<Outcome> {
    let (_ctl_tx, ctl_rx) = watch::channel(Control::Run);
    let (prog_tx, _prog_rx) = watch::channel(Progress::default());
    engine().download(url, dest, opts, ctl_rx, &prog_tx).await
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_with_many_connections() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let outcome = run(&format!("{base}/file"), &dest, &opts()).await.unwrap();
    assert!(matches!(outcome, Outcome::Completed { size, .. } if size == SIZE as u64));
    assert_same(&dest, &srv);
    assert!(
        srv.requests.load(Ordering::Relaxed) >= 8,
        "should use several connections"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tolerates_bogus_content_range_end() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    run(&format!("{base}/ff"), &dest, &opts()).await.unwrap();
    assert_same(&dest, &srv);
}

#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_single_connection_without_ranges() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let (_c, rx) = watch::channel(Control::Run);
    let (ptx, prx) = watch::channel(Progress::default());
    engine()
        .download(&format!("{base}/norange"), &dest, &opts(), rx, &ptx)
        .await
        .unwrap();
    assert!(!prx.borrow().resumable);
    assert_same(&dest, &srv);
}

#[tokio::test(flavor = "multi_thread")]
async fn survives_errors_and_dropped_connections() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    run(&format!("{base}/flaky"), &dest, &opts()).await.unwrap();
    assert_same(&dest, &srv);
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_then_resume() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let url = format!("{base}/slow");

    let (ctl_tx, ctl_rx) = watch::channel(Control::Run);
    let (prog_tx, mut prog_rx) = watch::channel(Progress::default());
    let eng = engine();
    let o = opts();
    let task = {
        let (dest, url) = (dest.clone(), url.clone());
        tokio::spawn(async move { eng.download(&url, &dest, &o, ctl_rx, &prog_tx).await })
    };
    prog_rx
        .wait_for(|p| p.downloaded > SIZE as u64 / 3)
        .await
        .unwrap();
    ctl_tx.send(Control::Pause).unwrap();
    assert!(matches!(task.await.unwrap().unwrap(), Outcome::Paused));
    assert!(fitdl_engine::part_path(&dest).exists());
    assert!(fitdl_engine::state_path(&dest).exists());
    assert!(!dest.exists());

    let served_before = srv.bytes_served.load(Ordering::Relaxed);
    run(&url, &dest, &opts()).await.unwrap();
    let served_after = srv.bytes_served.load(Ordering::Relaxed) - served_before;
    assert!(
        served_after < SIZE as u64 * 3 / 4,
        "resume re-downloaded too much: {served_after}"
    );
    assert_same(&dest, &srv);
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_after_crash() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    let url = format!("{base}/slow");

    // Kill the download abruptly (no pause, no cleanup), like a crash.
    let (_c, rx) = watch::channel(Control::Run);
    let (ptx, _prx) = watch::channel(Progress::default());
    let killed = tokio::time::timeout(
        Duration::from_millis(400),
        engine().download(&url, &dest, &opts(), rx, &ptx),
    )
    .await;
    assert!(killed.is_err(), "download should still have been running");
    let saved = fitdl_engine::SavedState::load(&fitdl_engine::state_path(&dest))
        .expect("state file should exist");
    assert!(saved.downloaded() > 0);

    run(&url, &dest, &opts()).await.unwrap();
    assert_same(&dest, &srv);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_removes_partial_files() {
    let (base, _srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");

    let (ctl_tx, ctl_rx) = watch::channel(Control::Run);
    let (prog_tx, mut prog_rx) = watch::channel(Progress::default());
    let task = {
        let (dest, url) = (dest.clone(), format!("{base}/slow"));
        tokio::spawn(async move {
            engine()
                .download(&url, &dest, &opts(), ctl_rx, &prog_tx)
                .await
        })
    };
    prog_rx.wait_for(|p| p.downloaded > 0).await.unwrap();
    ctl_tx.send(Control::Cancel).unwrap();
    assert!(matches!(task.await.unwrap().unwrap(), Outcome::Cancelled));
    assert!(!fitdl_engine::part_path(&dest).exists());
    assert!(!fitdl_engine::state_path(&dest).exists());
    assert!(!dest.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_link_is_reported() {
    let (base, _srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let err = run(&format!("{base}/gone"), &dir.path().join("x"), &opts())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::LinkExpired(410)), "{err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn skips_already_downloaded_file() {
    let (base, srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.bin");
    run(&format!("{base}/file"), &dest, &opts()).await.unwrap();
    let before = srv.bytes_served.load(Ordering::Relaxed);
    run(&format!("{base}/file"), &dest, &opts()).await.unwrap();
    assert!(srv.bytes_served.load(Ordering::Relaxed) - before < 1024);
}

#[tokio::test(flavor = "multi_thread")]
async fn speed_limit_is_respected() {
    let (base, _srv) = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let limiter = Arc::new(RateLimiter::new(16 * 1024 * 1024)); // 16 MB/s
    let eng = Engine::new("fitdl-test", limiter).unwrap();
    let (_c, rx) = watch::channel(Control::Run);
    let (ptx, _p) = watch::channel(Progress::default());
    let started = std::time::Instant::now();
    eng.download(
        &format!("{base}/file"),
        &dir.path().join("x"),
        &opts(),
        rx,
        &ptx,
    )
    .await
    .unwrap();
    // 24 MB at 16 MB/s, minus the one-second burst allowance.
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
}
