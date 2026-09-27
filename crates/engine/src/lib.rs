//! Fast, resumable, multi-connection HTTP download engine.
//!
//! * Each file is split into segments fetched in parallel with HTTP `Range`
//!   requests, each connection writing straight to its own offset.
//! * When a connection finishes early it takes over half of the largest
//!   remaining segment, so all connections stay busy until the very end.
//! * Progress is persisted next to the file (`.part.state`) after the data is
//!   flushed to disk, so a crash, pause or restart resumes where it stopped.
//! * The output is only renamed to its final name after the size is verified.

mod download;
mod error;
mod fileio;
mod http;
mod limiter;
mod probe;
mod state;

pub use download::{part_path, state_path, Control, DownloadOptions, Engine, Outcome, Progress};
pub use error::{Error, Result};
pub use http::HttpClient;
pub use limiter::RateLimiter;
pub use probe::{parse_content_disposition, RemoteInfo};
pub use state::{SavedState, Segment};
