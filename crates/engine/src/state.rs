use std::path::Path;

use serde::{Deserialize, Serialize};

/// A byte range `[start, end)` of the output file. Every byte in
/// `[start, pos)` has been written to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub pos: u64,
}

impl Segment {
    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.end
    }
}

/// Sidecar file (`<name>.part.state`) that makes downloads resumable across
/// pauses, crashes and restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub version: u32,
    pub total: u64,
    pub etag: Option<String>,
    pub segments: Vec<Segment>,
}

impl SavedState {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        let state: Self = serde_json::from_slice(&data).ok()?;
        (state.version == Self::VERSION && state.is_consistent()).then_some(state)
    }

    /// Atomically replace the state file (write temp file, then rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("state.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn downloaded(&self) -> u64 {
        self.segments.iter().map(|s| s.pos - s.start).sum()
    }

    /// Segments must be in bounds, non-overlapping and cover the whole file.
    fn is_consistent(&self) -> bool {
        let mut segs = self.segments.clone();
        segs.sort_by_key(|s| s.start);
        let mut expected = 0;
        for s in &segs {
            if s.start != expected || s.pos < s.start || s.pos > s.end {
                return false;
            }
            expected = s.end;
        }
        expected == self.total
    }
}

/// Split `[0, total)` into at most `parts` segments of at least `min_size` bytes.
pub fn split(total: u64, parts: usize, min_size: u64) -> Vec<Segment> {
    let max_parts = (total / min_size.max(1)).max(1);
    let parts = (parts.max(1) as u64).min(max_parts);
    let chunk = total / parts;
    (0..parts)
        .map(|i| {
            let start = i * chunk;
            let end = if i == parts - 1 { total } else { start + chunk };
            Segment {
                start,
                end,
                pos: start,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_covers_everything() {
        let segs = split(1000, 3, 1);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[2].end, 1000);
        for w in segs.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
    }

    #[test]
    fn split_respects_min_size() {
        assert_eq!(split(10, 8, 4).len(), 2);
        assert_eq!(split(3, 8, 4).len(), 1);
        assert_eq!(split(0, 8, 4).len(), 1);
    }

    #[test]
    fn consistency_check() {
        let mut st = SavedState {
            version: SavedState::VERSION,
            total: 100,
            etag: None,
            segments: split(100, 4, 1),
        };
        assert!(st.is_consistent());
        st.segments.pop();
        assert!(!st.is_consistent());
    }
}
