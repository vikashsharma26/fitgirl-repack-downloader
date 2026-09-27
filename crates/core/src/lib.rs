//! FitDL application core shared by the desktop app and the CLI: settings,
//! the persistent download queue, and the local API for the browser extension.

pub mod api;
pub mod catalog;
pub mod config;
pub mod manager;

pub use config::{AppPaths, Config};
pub use manager::{AddResult, Item, ItemId, ItemView, Manager, NewLink, Status, Totals};

use std::path::Path;

/// Write a file via a temporary file + rename, so it is never half-written.
pub fn write_atomic(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
