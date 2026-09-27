use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::fileio::{make_sparse, write_all_at};
use crate::http::HttpClient;
use crate::limiter::RateLimiter;
use crate::probe::{check_status, content_range, probe, RemoteInfo};
use crate::state::{split, SavedState, Segment};

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// Parallel connections for one file.
    pub connections: usize,
    /// Segments are never split below this size.
    pub min_segment_size: u64,
    /// Per-connection write buffer; data hits the disk in chunks of this size.
    pub buffer_size: usize,
    /// Consecutive failures (without progress) tolerated per segment.
    pub max_retries: u32,
    /// Base delay for exponential backoff between retries.
    pub retry_backoff: Duration,
    /// How often progress is flushed to disk and the state file saved.
    pub state_save_interval: Duration,
    /// How often `Progress` is published.
    pub progress_interval: Duration,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            connections: 8,
            min_segment_size: 4 * 1024 * 1024,
            buffer_size: 1024 * 1024,
            max_retries: 10,
            retry_backoff: Duration::from_secs(2),
            state_save_interval: Duration::from_secs(2),
            progress_interval: Duration::from_millis(250),
        }
    }
}

/// Sent by the caller to steer a running download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Run,
    /// Stop and keep the partial file so `download` can resume later.
    Pause,
    /// Stop and delete the partial file.
    Cancel,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    pub total: Option<u64>,
    pub downloaded: u64,
    /// Bytes per second, smoothed.
    pub speed: u64,
    pub connections: usize,
    pub resumable: bool,
    /// Name suggested by the server (Content-Disposition), if any.
    pub filename: Option<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Completed { path: PathBuf, size: u64 },
    Paused,
    Cancelled,
}

/// `<dest>.part`: the file being written while the download runs.
pub fn part_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part")
}

/// `<dest>.part.state`: segment progress used to resume.
pub fn state_path(dest: &Path) -> PathBuf {
    with_suffix(dest, ".part.state")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Download engine. Cheap to clone; clones share the connection pool and
/// the global speed limiter.
#[derive(Clone)]
pub struct Engine {
    http: HttpClient,
    limiter: Arc<RateLimiter>,
}

impl Engine {
    pub fn new(user_agent: &str, limiter: Arc<RateLimiter>) -> Result<Self> {
        Ok(Self::with_client(HttpClient::new(user_agent)?, limiter))
    }

    pub fn with_client(http: HttpClient, limiter: Arc<RateLimiter>) -> Self {
        Self { http, limiter }
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn limiter(&self) -> &Arc<RateLimiter> {
        &self.limiter
    }

    pub async fn probe(&self, url: &str) -> Result<RemoteInfo> {
        probe(&self.http, url).await
    }

    /// Download `url` to `dest`, resuming from `<dest>.part.state` if present.
    ///
    /// The file is written to `<dest>.part` and only renamed to `dest` once
    /// every byte is on disk and the size matches, so `dest` is never a
    /// broken file.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        opts: &DownloadOptions,
        control: watch::Receiver<Control>,
        progress: &watch::Sender<Progress>,
    ) -> Result<Outcome> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = part_path(dest);
        let state_file = state_path(dest);

        let info = self.probe(url).await?;
        progress.send_modify(|p| {
            p.total = info.size;
            p.resumable = info.supports_ranges;
            p.filename = info.filename.clone();
        });

        if let Some(size) = info.size {
            let existing = std::fs::metadata(dest).ok().map(|m| m.len());
            if existing == Some(size) && !part.exists() {
                progress.send_modify(|p| p.downloaded = size);
                return Ok(Outcome::Completed {
                    path: dest.to_owned(),
                    size,
                });
            }
        }

        match info.size {
            Some(total) if info.supports_ranges => {
                self.download_segmented(
                    url,
                    dest,
                    &part,
                    &state_file,
                    total,
                    &info,
                    opts,
                    control,
                    progress,
                )
                .await
            }
            _ => {
                self.download_single(
                    url,
                    dest,
                    &part,
                    &state_file,
                    info.size,
                    opts,
                    control,
                    progress,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_segmented(
        &self,
        url: &str,
        dest: &Path,
        part: &Path,
        state_file: &Path,
        total: u64,
        info: &RemoteInfo,
        opts: &DownloadOptions,
        mut control: watch::Receiver<Control>,
        progress: &watch::Sender<Progress>,
    ) -> Result<Outcome> {
        let part_len = std::fs::metadata(part).ok().map(|m| m.len());
        let resumed = SavedState::load(state_file).filter(|s| {
            s.total == total
                && part_len == Some(total)
                && (s.etag.is_none() || info.etag.is_none() || s.etag == info.etag)
        });

        let segments = match resumed {
            Some(state) => {
                debug!(
                    downloaded = state.downloaded(),
                    "resuming {}",
                    dest.display()
                );
                state.segments
            }
            None => {
                let file = File::create(part)?;
                if let Err(e) = make_sparse(&file) {
                    debug!("could not mark file sparse: {e}");
                }
                file.set_len(total)?;
                split(total, opts.connections, opts.min_segment_size)
            }
        };

        let ctx = Arc::new(Ctx {
            http: self.http.clone(),
            limiter: self.limiter.clone(),
            url: url.to_owned(),
            total,
            part: part.to_owned(),
            opts: opts.clone(),
            slots: Mutex::new(
                segments
                    .into_iter()
                    .map(|seg| Slot { seg, busy: false })
                    .collect(),
            ),
            received: AtomicU64::new(0),
            active: AtomicUsize::new(0),
        });
        let etag = info.etag.clone();
        let sync_handle = Arc::new(OpenOptions::new().write(true).open(part)?);

        // Internal stop signal: mirrors the caller's control, but we can also
        // pull it ourselves when one segment fails for good.
        let (stop_tx, _) = watch::channel(*control.borrow());
        let mut workers = JoinSet::new();
        for _ in 0..opts.connections.max(1) {
            workers.spawn(worker(ctx.clone(), stop_tx.subscribe()));
        }

        let mut tick = tokio::time::interval(opts.progress_interval);
        let mut last_save = Instant::now();
        let mut speed = SpeedMeter::new();
        let mut control_open = true;
        let mut first_error: Option<Error> = None;

        loop {
            tokio::select! {
                joined = workers.join_next() => match joined {
                    None => break,
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(e))) => {
                        if first_error.is_none() {
                            warn!("download failed: {e}");
                            first_error = Some(e);
                            control_open = false;
                            let _ = stop_tx.send(Control::Pause);
                        }
                    }
                    Some(Err(join_err)) => {
                        if first_error.is_none() {
                            first_error = Some(Error::Io(std::io::Error::other(join_err.to_string())));
                            control_open = false;
                            let _ = stop_tx.send(Control::Pause);
                        }
                    }
                },
                _ = tick.tick() => {
                    let snapshot = ctx.snapshot();
                    let downloaded: u64 = snapshot.iter().map(|s| s.pos - s.start).sum();
                    let speed = speed.update(ctx.received.load(Ordering::Relaxed));
                    let connections = ctx.active.load(Ordering::Relaxed);
                    progress.send_modify(|p| {
                        p.downloaded = downloaded;
                        p.speed = speed;
                        p.connections = connections;
                    });
                    if last_save.elapsed() >= opts.state_save_interval {
                        last_save = Instant::now();
                        persist(&sync_handle, state_file, total, &etag, snapshot).await?;
                    }
                },
                changed = control.changed(), if control_open => match changed {
                    Ok(()) => { let _ = stop_tx.send(*control.borrow_and_update()); }
                    Err(_) => control_open = false,
                },
            }
        }

        let final_control = *stop_tx.borrow();
        let snapshot = ctx.snapshot();
        let downloaded: u64 = snapshot.iter().map(|s| s.pos - s.start).sum();
        progress.send_modify(|p| {
            p.downloaded = downloaded;
            p.speed = 0;
            p.connections = 0;
        });

        if final_control == Control::Cancel && first_error.is_none() {
            drop(sync_handle);
            remove_partial(part, state_file);
            return Ok(Outcome::Cancelled);
        }

        let complete = snapshot.iter().all(Segment::is_done);
        persist(&sync_handle, state_file, total, &etag, snapshot).await?;

        if let Some(e) = first_error {
            return Err(e);
        }
        if final_control == Control::Pause {
            return Ok(Outcome::Paused);
        }
        if !complete {
            return Err(Error::Incomplete);
        }

        drop(sync_handle);
        finalize(part, dest, state_file, Some(total)).await?;
        Ok(Outcome::Completed {
            path: dest.to_owned(),
            size: total,
        })
    }

    /// Fallback for servers without range support (or unknown size): one
    /// connection, and an interrupted download starts over.
    #[allow(clippy::too_many_arguments)]
    async fn download_single(
        &self,
        url: &str,
        dest: &Path,
        part: &Path,
        state_file: &Path,
        expected: Option<u64>,
        opts: &DownloadOptions,
        mut control: watch::Receiver<Control>,
        progress: &watch::Sender<Progress>,
    ) -> Result<Outcome> {
        let _ = std::fs::remove_file(state_file);
        let mut failures = 0u32;

        'attempt: loop {
            let file = Arc::new(File::create(part)?);
            let mut written = 0u64;
            let mut speed = SpeedMeter::new();
            let mut received = 0u64;
            let mut last_report = Instant::now();
            progress.send_modify(|p| {
                p.downloaded = 0;
                p.connections = 1;
            });

            let result: Result<()> = async {
                let resp = tokio::select! {
                    r = self.http.get(url).send() => r?,
                    _ = wait_stop(&mut control) => return Ok(()),
                };
                check_status(resp.status())?;
                let mut stream = resp.bytes_stream();
                let mut buf = Vec::with_capacity(opts.buffer_size);
                loop {
                    let chunk = tokio::select! {
                        c = stream.next() => c,
                        _ = wait_stop(&mut control) => return Ok(()),
                    };
                    let last = match chunk {
                        Some(Ok(bytes)) => {
                            self.limiter.acquire(bytes.len()).await;
                            received += bytes.len() as u64;
                            buf.extend_from_slice(&bytes);
                            false
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => true,
                    };
                    if buf.len() >= opts.buffer_size || (last && !buf.is_empty()) {
                        let n = buf.len();
                        buf = write_blocking(file.clone(), buf, n, written).await?;
                        written += n as u64;
                        buf.clear();
                    }
                    if last_report.elapsed() >= opts.progress_interval || last {
                        last_report = Instant::now();
                        let s = speed.update(received);
                        progress.send_modify(|p| {
                            p.downloaded = written;
                            p.speed = s;
                        });
                    }
                    if last {
                        return match expected {
                            Some(size) if written < size => Err(Error::Incomplete),
                            _ => Ok(()),
                        };
                    }
                }
            }
            .await;

            let ctl = *control.borrow();
            if ctl != Control::Run {
                drop(file);
                remove_partial(part, state_file);
                progress.send_modify(|p| {
                    p.downloaded = 0;
                    p.speed = 0;
                    p.connections = 0;
                });
                return Ok(if ctl == Control::Pause {
                    Outcome::Paused
                } else {
                    Outcome::Cancelled
                });
            }

            match result {
                Ok(()) => {
                    drop(file);
                    let size = finalize(part, dest, state_file, expected).await?;
                    progress.send_modify(|p| {
                        p.speed = 0;
                        p.connections = 0;
                    });
                    return Ok(Outcome::Completed {
                        path: dest.to_owned(),
                        size,
                    });
                }
                Err(e) if e.is_retryable() && failures < opts.max_retries => {
                    failures += 1;
                    warn!("attempt {failures} failed, restarting: {e}");
                    tokio::select! {
                        _ = tokio::time::sleep(backoff(opts.retry_backoff, failures)) => {}
                        _ = wait_stop(&mut control) => {}
                    }
                    continue 'attempt;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

struct Ctx {
    http: HttpClient,
    limiter: Arc<RateLimiter>,
    url: String,
    total: u64,
    part: PathBuf,
    opts: DownloadOptions,
    slots: Mutex<Vec<Slot>>,
    /// Bytes received from the network (for speed; includes discarded bytes).
    received: AtomicU64,
    active: AtomicUsize,
}

struct Slot {
    seg: Segment,
    busy: bool,
}

impl Ctx {
    fn snapshot(&self) -> Vec<Segment> {
        self.slots.lock().unwrap().iter().map(|s| s.seg).collect()
    }

    /// Take an unfinished idle segment, or split the biggest busy one and
    /// take its second half (dynamic segmentation, like IDM). Slots are only
    /// ever appended, so indices stay valid.
    fn claim(&self) -> Option<usize> {
        let mut slots = self.slots.lock().unwrap();
        if let Some(i) = slots.iter().position(|s| !s.busy && !s.seg.is_done()) {
            slots[i].busy = true;
            return Some(i);
        }
        let (i, remaining) = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.busy)
            .map(|(i, s)| (i, s.seg.remaining()))
            .max_by_key(|&(_, r)| r)?;
        if remaining < 2 * self.opts.min_segment_size.max(1) {
            return None;
        }
        let victim = &mut slots[i].seg;
        let mid = victim.pos + remaining / 2;
        let stolen = Segment {
            start: mid,
            end: victim.end,
            pos: mid,
        };
        victim.end = mid;
        slots.push(Slot {
            seg: stolen,
            busy: true,
        });
        Some(slots.len() - 1)
    }

    fn release(&self, i: usize) {
        self.slots.lock().unwrap()[i].busy = false;
    }

    fn bounds(&self, i: usize) -> (u64, u64) {
        let s = self.slots.lock().unwrap()[i].seg;
        (s.pos, s.end)
    }

    fn end(&self, i: usize) -> u64 {
        self.slots.lock().unwrap()[i].seg.end
    }

    fn advance(&self, i: usize, to: u64) {
        let mut slots = self.slots.lock().unwrap();
        let seg = &mut slots[i].seg;
        seg.pos = seg.pos.max(to.min(seg.end));
    }
}

enum Fetch {
    Done,
    Stopped,
}

async fn worker(ctx: Arc<Ctx>, mut stop: watch::Receiver<Control>) -> Result<()> {
    let file = Arc::new(OpenOptions::new().write(true).open(&ctx.part)?);
    let mut buf = Vec::with_capacity(ctx.opts.buffer_size);
    loop {
        if *stop.borrow_and_update() != Control::Run {
            return Ok(());
        }
        let Some(i) = ctx.claim() else {
            return Ok(());
        };
        ctx.active.fetch_add(1, Ordering::Relaxed);
        let result = run_slot(&ctx, i, &file, &mut buf, &mut stop).await;
        ctx.active.fetch_sub(1, Ordering::Relaxed);
        ctx.release(i);
        result?;
    }
}

/// Download one segment, retrying transient errors from where it left off.
async fn run_slot(
    ctx: &Ctx,
    i: usize,
    file: &Arc<File>,
    buf: &mut Vec<u8>,
    stop: &mut watch::Receiver<Control>,
) -> Result<()> {
    let mut failures = 0u32;
    loop {
        let before = ctx.bounds(i).0;
        match fetch(ctx, i, file, buf, stop).await {
            Ok(Fetch::Done | Fetch::Stopped) => return Ok(()),
            Err(e) if e.is_retryable() => {
                if ctx.bounds(i).0 > before {
                    failures = 0;
                }
                failures += 1;
                if failures > ctx.opts.max_retries {
                    return Err(Error::RetriesExhausted {
                        attempts: failures,
                        last: e.to_string(),
                    });
                }
                debug!("segment {i}: attempt {failures} failed: {e}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff(ctx.opts.retry_backoff, failures)) => {}
                    _ = wait_stop(stop) => return Ok(()),
                }
            }
            Err(e) => return Err(e),
        }
    }
}

async fn fetch(
    ctx: &Ctx,
    i: usize,
    file: &Arc<File>,
    buf: &mut Vec<u8>,
    stop: &mut watch::Receiver<Control>,
) -> Result<Fetch> {
    if *stop.borrow() != Control::Run {
        return Ok(Fetch::Stopped);
    }
    let (pos, end) = ctx.bounds(i);
    if pos >= end {
        return Ok(Fetch::Done);
    }

    let request = ctx
        .http
        .get(&ctx.url)
        .header(RANGE, format!("bytes={}-{}", pos, end - 1));
    let resp = tokio::select! {
        r = request.send() => r?,
        _ = wait_stop(stop) => return Ok(Fetch::Stopped),
    };
    check_status(resp.status())?;
    if resp.status() != StatusCode::PARTIAL_CONTENT {
        // A 200 here would be the whole file from byte 0: writing it at
        // `pos` would corrupt the output.
        return Err(Error::BadRange(format!(
            "expected 206 Partial Content, got {}",
            resp.status()
        )));
    }
    let range = content_range(resp.headers())
        .ok_or_else(|| Error::BadRange("missing Content-Range".into()))?;
    if range.start != pos {
        return Err(Error::BadRange(format!(
            "requested offset {pos}, server sent {}",
            range.start
        )));
    }
    if range.total.is_some_and(|t| t != ctx.total) {
        return Err(Error::RemoteChanged);
    }

    let mut stream = resp.bytes_stream();
    let mut offset = pos;
    buf.clear();
    loop {
        let chunk = tokio::select! {
            biased;
            _ = wait_stop(stop) => {
                flush(ctx, i, file, buf, &mut offset).await?;
                return Ok(Fetch::Stopped);
            }
            c = stream.next() => c,
        };
        match chunk {
            Some(Ok(bytes)) => {
                ctx.limiter.acquire(bytes.len()).await;
                ctx.received
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                buf.extend_from_slice(&bytes);
                if buf.len() >= ctx.opts.buffer_size
                    && flush(ctx, i, file, buf, &mut offset).await?
                {
                    return Ok(Fetch::Done);
                }
            }
            Some(Err(e)) => {
                flush(ctx, i, file, buf, &mut offset).await?;
                return Err(e.into());
            }
            None => {
                return if flush(ctx, i, file, buf, &mut offset).await? {
                    Ok(Fetch::Done)
                } else {
                    Err(Error::Incomplete)
                };
            }
        }
    }
}

/// Write the buffer at `offset`, clipped to the segment's current end (which
/// may have shrunk because another connection took over its tail).
/// Returns true once the segment is complete.
async fn flush(
    ctx: &Ctx,
    i: usize,
    file: &Arc<File>,
    buf: &mut Vec<u8>,
    offset: &mut u64,
) -> Result<bool> {
    let end = ctx.end(i);
    let n = (buf.len() as u64).min(end.saturating_sub(*offset)) as usize;
    if n > 0 {
        *buf = write_blocking(file.clone(), std::mem::take(buf), n, *offset).await?;
        *offset += n as u64;
        ctx.advance(i, *offset);
    }
    buf.clear();
    Ok(*offset >= ctx.end(i))
}

async fn write_blocking(file: Arc<File>, buf: Vec<u8>, n: usize, offset: u64) -> Result<Vec<u8>> {
    let (buf, res) = tokio::task::spawn_blocking(move || {
        let res = write_all_at(&file, &buf[..n], offset);
        (buf, res)
    })
    .await
    .map_err(|e| std::io::Error::other(e.to_string()))?;
    res?;
    Ok(buf)
}

/// Flush file data to disk, *then* record progress, so the state file never
/// claims bytes that could be lost in a crash or power cut.
async fn persist(
    file: &Arc<File>,
    state_file: &Path,
    total: u64,
    etag: &Option<String>,
    segments: Vec<Segment>,
) -> Result<()> {
    let file = file.clone();
    let path = state_file.to_owned();
    let state = SavedState {
        version: SavedState::VERSION,
        total,
        etag: etag.clone(),
        segments,
    };
    tokio::task::spawn_blocking(move || {
        file.sync_data()?;
        state.save(&path)
    })
    .await
    .map_err(|e| std::io::Error::other(e.to_string()))??;
    Ok(())
}

async fn finalize(
    part: &Path,
    dest: &Path,
    state_file: &Path,
    expected: Option<u64>,
) -> Result<u64> {
    let part = part.to_owned();
    let dest = dest.to_owned();
    let state_file = state_file.to_owned();
    tokio::task::spawn_blocking(move || -> Result<u64> {
        OpenOptions::new().write(true).open(&part)?.sync_all()?;
        let actual = std::fs::metadata(&part)?.len();
        if let Some(expected) = expected {
            if actual != expected {
                return Err(Error::SizeMismatch { expected, actual });
            }
        }
        std::fs::rename(&part, &dest)?;
        let _ = std::fs::remove_file(&state_file);
        Ok(actual)
    })
    .await
    .map_err(|e| std::io::Error::other(e.to_string()))?
}

fn remove_partial(part: &Path, state_file: &Path) {
    let _ = std::fs::remove_file(part);
    let _ = std::fs::remove_file(state_file);
}

/// Resolves when the control value is (or becomes) anything but `Run`.
async fn wait_stop(rx: &mut watch::Receiver<Control>) {
    if rx.wait_for(|c| *c != Control::Run).await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn backoff(base: Duration, attempt: u32) -> Duration {
    let factor = 1u32 << attempt.saturating_sub(1).min(5);
    (base * factor).min(Duration::from_secs(60))
}

struct SpeedMeter {
    last_bytes: u64,
    last_time: Instant,
    ema: f64,
}

impl SpeedMeter {
    fn new() -> Self {
        Self {
            last_bytes: 0,
            last_time: Instant::now(),
            ema: 0.0,
        }
    }

    fn update(&mut self, bytes: u64) -> u64 {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64();
        if dt > 0.0 {
            let instant = bytes.saturating_sub(self.last_bytes) as f64 / dt;
            self.ema = if self.ema == 0.0 {
                instant
            } else {
                0.3 * instant + 0.7 * self.ema
            };
            self.last_bytes = bytes;
            self.last_time = now;
        }
        self.ema as u64
    }
}
