use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use fitdl_engine::DownloadOptions;
use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 7878;
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

/// Settings stored in `config.toml`, editable by hand or from the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub download_dir: PathBuf,
    pub max_parallel_files: usize,
    pub connections_per_file: usize,
    pub min_segment_size_mb: u64,
    pub buffer_size_kb: usize,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    pub speed_limit_kbps: u64,
    pub state_save_interval_s: u64,
    pub subfolder_per_game: bool,
    pub user_agent: String,
    pub server_port: u16,
    pub api_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_dir: dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("FitDL"),
            max_parallel_files: 3,
            connections_per_file: 4,
            min_segment_size_mb: 4,
            buffer_size_kb: 1024,
            max_retries: 10,
            retry_backoff_ms: 2000,
            speed_limit_kbps: 0,
            state_save_interval_s: 2,
            subfolder_per_game: true,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            server_port: DEFAULT_PORT,
            api_token: String::new(),
        }
    }
}

impl Config {
    /// Load from `path`, creating it with defaults if missing. Out-of-range
    /// values are clamped so a typo can't break the app.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str::<Config>(&text)
                .with_context(|| format!("invalid config file {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let before = cfg.clone();
        cfg.normalize();
        if cfg != before || !path.exists() {
            cfg.save(path)?;
        }
        Ok(cfg)
    }

    pub fn normalize(&mut self) {
        self.max_parallel_files = self.max_parallel_files.clamp(1, 32);
        self.connections_per_file = self.connections_per_file.clamp(1, 32);
        self.min_segment_size_mb = self.min_segment_size_mb.clamp(1, 1024);
        self.buffer_size_kb = self.buffer_size_kb.clamp(16, 16 * 1024);
        self.max_retries = self.max_retries.min(1000);
        self.state_save_interval_s = self.state_save_interval_s.clamp(1, 60);
        if self.user_agent.trim().is_empty() {
            self.user_agent = DEFAULT_USER_AGENT.to_owned();
        }
        if self.server_port == 0 {
            self.server_port = DEFAULT_PORT;
        }
        if self.api_token.len() < 16 {
            self.api_token = random_token();
        }
    }

    pub fn download_options(&self) -> DownloadOptions {
        DownloadOptions {
            connections: self.connections_per_file,
            min_segment_size: self.min_segment_size_mb * 1024 * 1024,
            buffer_size: self.buffer_size_kb * 1024,
            max_retries: self.max_retries,
            retry_backoff: Duration::from_millis(self.retry_backoff_ms),
            state_save_interval: Duration::from_secs(self.state_save_interval_s),
            progress_interval: Duration::from_millis(250),
        }
    }

    pub fn speed_limit_bytes(&self) -> u64 {
        self.speed_limit_kbps * 1024
    }

    /// Write a commented config file (atomically).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
        let text = format!(
            r#"# FitDL configuration. Changes made here are picked up on the next start;
# changes made in the app's Settings apply immediately.

# Where downloads are saved.
download_dir = {download_dir}

# Put each game's files in its own sub-folder (named after the game).
subfolder_per_game = {subfolder_per_game}

# How many files download at the same time.
max_parallel_files = {max_parallel_files}

# Parallel connections per file (IDM uses 8). More is not always faster:
# many hosts cap the total speed per IP. 4 is a good start.
connections_per_file = {connections_per_file}

# A segment is never split below this size (MB).
min_segment_size_mb = {min_segment_size_mb}

# Per-connection write buffer (KB). Bigger = fewer disk writes.
buffer_size_kb = {buffer_size_kb}

# Retries per segment before a file is marked failed, and the base delay
# between retries (doubles each time, capped at 60 s).
max_retries = {max_retries}
retry_backoff_ms = {retry_backoff_ms}

# Global speed limit in KB/s for all downloads together. 0 = unlimited.
speed_limit_kbps = {speed_limit_kbps}

# How often progress is flushed to disk so downloads survive crashes (seconds).
state_save_interval_s = {state_save_interval_s}

# User-Agent sent to servers.
user_agent = {user_agent}

# Local API used by the browser extension (listens on 127.0.0.1 only).
server_port = {server_port}

# Secret for scripts calling the local API (header X-FitDL-Token).
# The browser extension does not need it.
api_token = {api_token}
"#,
            download_dir = q(&self.download_dir.to_string_lossy()),
            subfolder_per_game = self.subfolder_per_game,
            max_parallel_files = self.max_parallel_files,
            connections_per_file = self.connections_per_file,
            min_segment_size_mb = self.min_segment_size_mb,
            buffer_size_kb = self.buffer_size_kb,
            max_retries = self.max_retries,
            retry_backoff_ms = self.retry_backoff_ms,
            speed_limit_kbps = self.speed_limit_kbps,
            state_save_interval_s = self.state_save_interval_s,
            user_agent = q(&self.user_agent),
            server_port = self.server_port,
            api_token = q(&self.api_token),
        );
        crate::write_atomic(path, text.as_bytes())
            .with_context(|| format!("writing {}", path.display()))
    }
}

fn random_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where FitDL keeps its files.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub dir: PathBuf,
    pub config: PathBuf,
    pub queue: PathBuf,
}

impl AppPaths {
    /// Portable mode if `config.toml` sits next to the executable, otherwise
    /// the per-user config dir (`%APPDATA%\FitDL` on Windows).
    pub fn detect() -> Self {
        let portable = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .filter(|dir| dir.join("config.toml").is_file());
        let dir = portable.unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("FitDL")
        });
        Self::in_dir(dir)
    }

    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            config: dir.join("config.toml"),
            queue: dir.join("queue.json"),
            dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_clamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config::load_or_create(&path).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.api_token.len(), 32);

        let text = std::fs::read_to_string(&path).unwrap();
        let edited = text.replace("connections_per_file = 4", "connections_per_file = 500");
        std::fs::write(&path, edited).unwrap();
        let cfg2 = Config::load_or_create(&path).unwrap();
        assert_eq!(cfg2.connections_per_file, 32);
        assert_eq!(cfg2.api_token, cfg.api_token);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "max_parallel_files = 5\n").unwrap();
        let cfg = Config::load_or_create(&path).unwrap();
        assert_eq!(cfg.max_parallel_files, 5);
        assert_eq!(cfg.connections_per_file, 4);
    }
}
