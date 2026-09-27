use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("disk error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server returned HTTP {0}")]
    Status(u16),

    /// The (usually signed, short-lived) link is no longer valid. The caller
    /// should resolve a fresh link and call `download` again; progress is kept.
    #[error("download link expired or forbidden (HTTP {0})")]
    LinkExpired(u16),

    #[error("server sent an unexpected byte range: {0}")]
    BadRange(String),

    #[error("connection closed before the segment was complete")]
    Incomplete,

    #[error("remote file changed since the download started")]
    RemoteChanged,

    #[error("final size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("gave up after {attempts} attempts: {last}")]
    RetriesExhausted { attempts: u32, last: String },
}

impl Error {
    /// Transient failures worth retrying on the same link.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Http(_) | Error::Incomplete | Error::BadRange(_) => true,
            Error::Status(code) => *code == 429 || *code == 408 || *code >= 500,
            _ => false,
        }
    }
}
