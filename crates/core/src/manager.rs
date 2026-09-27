use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use fitdl_engine::{Control, Engine, Error as EngineError, Outcome, Progress, RateLimiter};
use fitdl_scraper::{fuckingfast, is_fuckingfast, sanitize_filename, Page};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify};
use tracing::{info, warn};

use crate::config::{AppPaths, Config};

pub type ItemId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Resolving,
    Downloading,
    Paused,
    Completed,
    Failed,
}

impl Status {
    pub fn is_active(self) -> bool {
        matches!(self, Status::Resolving | Status::Downloading)
    }
}

/// A queued file, as saved in `queue.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    /// What was added: a fuckingfast.co landing page or any direct URL.
    pub url: String,
    /// Link text from the page.
    pub label: String,
    pub filename: String,
    pub dir: PathBuf,
    pub game: Option<String>,
    pub status: Status,
    pub size: Option<u64>,
    pub downloaded: u64,
    pub error: Option<String>,
    pub added_at: u64,
    pub completed_at: Option<u64>,
    /// Direct link resolved by the browser extension, used before resolving
    /// again ourselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_url: Option<String>,
}

impl Item {
    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.filename)
    }
}

/// An item plus live transfer stats, for the UI and API.
#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    #[serde(flatten)]
    pub item: Item,
    pub speed: u64,
    pub connections: usize,
    pub eta_secs: Option<u64>,
}

/// A link to add, as sent by the extension or UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewLink {
    pub url: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub direct_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddResult {
    pub game: Option<String>,
    pub added: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub active: usize,
    pub queued: usize,
    pub completed: usize,
    pub failed: usize,
    pub speed: u64,
}

struct Entry {
    item: Item,
    speed: u64,
    connections: usize,
    control: Option<watch::Sender<Control>>,
}

struct Inner {
    paths: AppPaths,
    persist: bool,
    config: RwLock<Config>,
    engine: Engine,
    limiter: Arc<RateLimiter>,
    entries: Mutex<Vec<Entry>>,
    wake: Notify,
    next_id: AtomicU64,
    dirty: AtomicBool,
    shutting_down: AtomicBool,
}

/// The download queue. Cheap to clone; all clones share state.
#[derive(Clone)]
pub struct Manager {
    inner: Arc<Inner>,
}

impl Manager {
    /// Load config and queue from `paths` and start the scheduler.
    /// Must be called inside a Tokio runtime.
    pub fn start(paths: AppPaths) -> Result<Self> {
        let config = Config::load_or_create(&paths.config)?;
        let items = load_queue(&paths.queue);
        Self::start_with(paths, config, items, true)
    }

    /// A manager that keeps its queue in memory only (used by the CLI).
    pub fn start_ephemeral(paths: AppPaths, config: Config) -> Result<Self> {
        Self::start_with(paths, config, Vec::new(), false)
    }

    fn start_with(
        paths: AppPaths,
        config: Config,
        mut items: Vec<Item>,
        persist: bool,
    ) -> Result<Self> {
        for item in &mut items {
            // Interrupted by a crash or exit: pick them up again.
            if item.status.is_active() {
                item.status = Status::Queued;
            }
        }
        let limiter = Arc::new(RateLimiter::new(config.speed_limit_bytes()));
        let engine = Engine::new(&config.user_agent, limiter.clone())?;
        let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let manager = Self {
            inner: Arc::new(Inner {
                paths,
                persist,
                config: RwLock::new(config),
                engine,
                limiter,
                entries: Mutex::new(
                    items
                        .into_iter()
                        .map(|item| Entry {
                            item,
                            speed: 0,
                            connections: 0,
                            control: None,
                        })
                        .collect(),
                ),
                wake: Notify::new(),
                next_id: AtomicU64::new(next_id),
                dirty: AtomicBool::new(false),
                shutting_down: AtomicBool::new(false),
            }),
        };
        tokio::spawn(manager.clone().scheduler());
        if persist {
            tokio::spawn(manager.clone().autosave());
        }
        Ok(manager)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.inner.paths
    }

    pub fn engine(&self) -> &Engine {
        &self.inner.engine
    }

    pub fn config(&self) -> Config {
        self.inner.config.read().unwrap().clone()
    }

    /// Apply and save new settings. Speed limit and parallelism apply
    /// immediately; per-file settings apply to downloads started afterwards.
    pub fn update_config(&self, mut config: Config) -> Result<Config> {
        config.normalize();
        {
            let current = self.inner.config.read().unwrap();
            // The token is not editable from the UI/API.
            config.api_token = current.api_token.clone();
        }
        if self.inner.persist {
            config.save(&self.inner.paths.config)?;
        }
        self.inner.limiter.set_rate(config.speed_limit_bytes());
        *self.inner.config.write().unwrap() = config.clone();
        self.inner.wake.notify_one();
        Ok(config)
    }

    pub fn items(&self) -> Vec<ItemView> {
        let entries = self.inner.entries.lock().unwrap();
        entries
            .iter()
            .map(|e| ItemView {
                eta_secs: match (e.item.size, e.speed) {
                    (Some(size), speed) if speed > 0 && e.item.status == Status::Downloading => {
                        Some(size.saturating_sub(e.item.downloaded) / speed)
                    }
                    _ => None,
                },
                item: e.item.clone(),
                speed: e.speed,
                connections: e.connections,
            })
            .collect()
    }

    pub fn totals(&self) -> Totals {
        let entries = self.inner.entries.lock().unwrap();
        let mut t = Totals::default();
        for e in entries.iter() {
            match e.item.status {
                Status::Resolving | Status::Downloading => t.active += 1,
                Status::Queued => t.queued += 1,
                Status::Completed => t.completed += 1,
                Status::Failed => t.failed += 1,
                Status::Paused => {}
            }
            t.speed += e.speed;
        }
        t
    }

    /// Queue links. Links already in the queue are skipped.
    pub fn add(&self, links: Vec<NewLink>, game: Option<String>) -> AddResult {
        let cfg = self.config();
        let game = game
            .map(|g| fitdl_scraper::folder_name(&g))
            .filter(|g| !g.is_empty());
        let dir = match (&game, cfg.subfolder_per_game) {
            (Some(g), true) => cfg.download_dir.join(g),
            _ => cfg.download_dir.clone(),
        };
        let mut added = 0;
        let mut skipped = 0;
        {
            let mut entries = self.inner.entries.lock().unwrap();
            for link in links {
                let url = link.url.trim().to_owned();
                if url.is_empty() || entries.iter().any(|e| e.item.url == url) {
                    skipped += 1;
                    continue;
                }
                let label = link.label.unwrap_or_default().trim().to_owned();
                let filename = match link.filename.filter(|f| !f.trim().is_empty()) {
                    Some(f) => sanitize_filename(&f),
                    None => fitdl_scraper::filename_for(&url, &label),
                };
                let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
                entries.push(Entry {
                    item: Item {
                        id,
                        label: if label.is_empty() {
                            filename.clone()
                        } else {
                            label
                        },
                        url,
                        filename,
                        dir: dir.clone(),
                        game: game.clone(),
                        status: Status::Queued,
                        size: None,
                        downloaded: 0,
                        error: None,
                        added_at: now(),
                        completed_at: None,
                        direct_url: link.direct_url.filter(|u| u.starts_with("http")),
                    },
                    speed: 0,
                    connections: 0,
                    control: None,
                });
                added += 1;
            }
        }
        if added > 0 {
            info!("queued {added} file(s) into {}", dir.display());
            self.changed();
        }
        AddResult {
            game,
            added,
            skipped,
        }
    }

    /// Fetch a game page and list its download links.
    pub async fn scrape(&self, page_url: &str) -> Result<Page> {
        fitdl_scraper::fetch_page(self.inner.engine.http(), page_url)
            .await
            .with_context(|| format!("could not read {page_url}"))
    }

    /// Scrape a game page and queue its files.
    pub async fn add_page(&self, page_url: &str, include_optional: bool) -> Result<AddResult> {
        let page = self.scrape(page_url).await?;
        if page.links.is_empty() {
            anyhow::bail!("no fuckingfast.co links found on {page_url}");
        }
        let links = page
            .links
            .into_iter()
            .filter(|l| include_optional || !l.optional)
            .map(|l| NewLink {
                url: l.url,
                label: Some(l.label),
                filename: Some(l.filename),
                direct_url: None,
            })
            .collect();
        Ok(self.add(links, page.game.or(page.title)))
    }

    pub fn pause(&self, id: ItemId) {
        self.with_entry(id, |e| match e.item.status {
            Status::Queued => e.item.status = Status::Paused,
            s if s.is_active() => send(&e.control, Control::Pause),
            _ => {}
        });
    }

    pub fn resume(&self, id: ItemId) {
        self.with_entry(id, |e| {
            if matches!(e.item.status, Status::Paused | Status::Failed) {
                e.item.status = Status::Queued;
                e.item.error = None;
            }
        });
    }

    pub fn pause_all(&self) {
        let ids: Vec<_> = self.items().iter().map(|v| v.item.id).collect();
        ids.into_iter().for_each(|id| self.pause(id));
    }

    pub fn resume_all(&self) {
        let ids: Vec<_> = self.items().iter().map(|v| v.item.id).collect();
        ids.into_iter().for_each(|id| self.resume(id));
    }

    /// Remove from the list. With `delete_files`, partial data (and for
    /// completed items, the downloaded file) is deleted too.
    pub fn remove(&self, id: ItemId, delete_files: bool) {
        let removed = {
            let mut entries = self.inner.entries.lock().unwrap();
            let Some(pos) = entries.iter().position(|e| e.item.id == id) else {
                return;
            };
            let entry = entries.remove(pos);
            if entry.item.status.is_active() {
                send(
                    &entry.control,
                    if delete_files {
                        Control::Cancel
                    } else {
                        Control::Pause
                    },
                );
            }
            entry.item
        };
        if delete_files && !removed.status.is_active() {
            let path = removed.path();
            let _ = std::fs::remove_file(fitdl_engine::part_path(&path));
            let _ = std::fs::remove_file(fitdl_engine::state_path(&path));
            if removed.status == Status::Completed {
                let _ = std::fs::remove_file(&path);
            }
        }
        self.changed();
    }

    pub fn clear_completed(&self) {
        self.inner
            .entries
            .lock()
            .unwrap()
            .retain(|e| e.item.status != Status::Completed);
        self.changed();
    }

    pub fn get(&self, id: ItemId) -> Option<Item> {
        let entries = self.inner.entries.lock().unwrap();
        entries
            .iter()
            .find(|e| e.item.id == id)
            .map(|e| e.item.clone())
    }

    /// Pause running downloads (so their progress is saved), wait for them
    /// to stop, and save the queue. Paused-by-shutdown items resume on the
    /// next start.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        {
            let entries = self.inner.entries.lock().unwrap();
            for e in entries.iter().filter(|e| e.item.status.is_active()) {
                send(&e.control, Control::Pause);
            }
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.totals().active > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.save();
    }

    /// Resolve when nothing is queued or running (used by the CLI).
    pub async fn wait_idle(&self) {
        loop {
            let t = self.totals();
            if t.active == 0 && t.queued == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn with_entry(&self, id: ItemId, f: impl FnOnce(&mut Entry)) {
        {
            let mut entries = self.inner.entries.lock().unwrap();
            if let Some(e) = entries.iter_mut().find(|e| e.item.id == id) {
                f(e);
            }
        }
        self.changed();
    }

    fn changed(&self) {
        self.inner.dirty.store(true, Ordering::Relaxed);
        self.inner.wake.notify_one();
    }

    async fn scheduler(self) {
        loop {
            if !self.inner.shutting_down.load(Ordering::SeqCst) {
                self.schedule();
            }
            tokio::select! {
                _ = self.inner.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    fn schedule(&self) {
        let max = self.config().max_parallel_files.max(1);
        let mut entries = self.inner.entries.lock().unwrap();
        let active = entries.iter().filter(|e| e.item.status.is_active()).count();
        let mut free = max.saturating_sub(active);
        for e in entries.iter_mut() {
            if free == 0 {
                break;
            }
            if e.item.status == Status::Queued {
                let (tx, rx) = watch::channel(Control::Run);
                e.control = Some(tx);
                e.item.status = Status::Resolving;
                e.item.error = None;
                free -= 1;
                tokio::spawn(self.clone().run(e.item.id, rx));
            }
        }
    }

    async fn run(self, id: ItemId, control: watch::Receiver<Control>) {
        let result = self.download(id, control).await;
        {
            let shutting_down = self.inner.shutting_down.load(Ordering::SeqCst);
            let mut entries = self.inner.entries.lock().unwrap();
            if let Some(e) = entries.iter_mut().find(|e| e.item.id == id) {
                e.control = None;
                e.speed = 0;
                e.connections = 0;
                match result {
                    Ok(Outcome::Completed { size, .. }) => {
                        e.item.status = Status::Completed;
                        e.item.size = Some(size);
                        e.item.downloaded = size;
                        e.item.completed_at = Some(now());
                        info!("completed {}", e.item.filename);
                    }
                    Ok(Outcome::Paused) if shutting_down => e.item.status = Status::Queued,
                    Ok(Outcome::Paused | Outcome::Cancelled) => e.item.status = Status::Paused,
                    Err(msg) => {
                        warn!("{} failed: {msg}", e.item.filename);
                        e.item.status = Status::Failed;
                        e.item.error = Some(msg);
                    }
                }
            }
        }
        self.changed();
    }

    async fn download(
        &self,
        id: ItemId,
        control: watch::Receiver<Control>,
    ) -> Result<Outcome, String> {
        let Some(item) = self.get(id) else {
            return Ok(Outcome::Cancelled);
        };
        let opts = self.config().download_options();
        let dest = item.path();

        let mut link = match item.direct_url.clone() {
            Some(direct) => direct,
            None => self.resolve(&item.url).await?,
        };

        let (progress_tx, progress_rx) = watch::channel(Progress::default());
        let forwarder = tokio::spawn(self.clone().forward_progress(id, progress_rx));
        self.set_status(id, Status::Downloading);

        let mut result = Err(String::new());
        for attempt in 0..3 {
            match self
                .inner
                .engine
                .download(&link, &dest, &opts, control.clone(), &progress_tx)
                .await
            {
                // Signed links expire: get a fresh one and continue where we were.
                Err(EngineError::LinkExpired(code)) if attempt < 2 && link != item.url => {
                    info!(
                        "link for {} expired (HTTP {code}), resolving again",
                        item.filename
                    );
                    self.set_status(id, Status::Resolving);
                    match self.resolve(&item.url).await {
                        Ok(fresh) => link = fresh,
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                    }
                    self.set_status(id, Status::Downloading);
                }
                other => {
                    result = other.map_err(|e| e.to_string());
                    break;
                }
            }
        }
        drop(progress_tx);
        let _ = forwarder.await;
        result
    }

    async fn resolve(&self, url: &str) -> Result<String, String> {
        if is_fuckingfast(url) {
            fuckingfast::resolve(self.inner.engine.http(), url)
                .await
                .map_err(|e| format!("could not get download link: {e}"))
        } else {
            Ok(url.to_owned())
        }
    }

    async fn forward_progress(self, id: ItemId, mut rx: watch::Receiver<Progress>) {
        while rx.changed().await.is_ok() {
            let p = rx.borrow_and_update().clone();
            let mut entries = self.inner.entries.lock().unwrap();
            if let Some(e) = entries.iter_mut().find(|e| e.item.id == id) {
                if p.total.is_some() {
                    e.item.size = p.total;
                }
                e.item.downloaded = p.downloaded;
                e.speed = p.speed;
                e.connections = p.connections;
            }
        }
    }

    fn set_status(&self, id: ItemId, status: Status) {
        let mut entries = self.inner.entries.lock().unwrap();
        if let Some(e) = entries.iter_mut().find(|e| e.item.id == id) {
            e.item.status = status;
        }
        self.inner.dirty.store(true, Ordering::Relaxed);
    }

    async fn autosave(self) {
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let active = self.totals().active > 0;
            if self.inner.dirty.swap(false, Ordering::Relaxed) || active {
                self.save();
            }
        }
    }

    fn save(&self) {
        if !self.inner.persist {
            return;
        }
        let items: Vec<Item> = self
            .inner
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.item.clone())
            .collect();
        let result = serde_json::to_vec_pretty(&items)
            .map_err(anyhow::Error::from)
            .and_then(|data| crate::write_atomic(&self.inner.paths.queue, &data));
        if let Err(e) = result {
            warn!("could not save queue: {e:#}");
        }
    }
}

fn send(control: &Option<watch::Sender<Control>>, value: Control) {
    if let Some(tx) = control {
        let _ = tx.send(value);
    }
}

fn load_queue(path: &Path) -> Vec<Item> {
    std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
