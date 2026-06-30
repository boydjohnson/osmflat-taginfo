//! Resolve the taginfo `data_until` timestamp (design §4.1): the parent's
//! replication timestamp if set, else the archive directory mtime, else now.
//! Formatted as taginfo does — `YYYY-MM-DDThh:mm:ssZ`, UTC, second precision.

use osmflat::Osm;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

pub fn data_until(parent: &Osm, archive_dir: &Path) -> String {
    let secs = parent_replication_secs(parent)
        .or_else(|| dir_mtime_secs(archive_dir))
        .unwrap_or_else(now_secs);
    format_utc(secs)
}

fn parent_replication_secs(parent: &Osm) -> Option<i64> {
    let ts = parent.header().replication_timestamp();
    (ts > 0).then_some(ts)
}

fn dir_mtime_secs(dir: &Path) -> Option<i64> {
    let modified = std::fs::metadata(dir).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `2026-06-29T00:00:00Z`. Built from components so the trailing `Z` is exact
/// (time's RFC3339 emits `+00:00`, which taginfo does not).
fn format_utc(unix_secs: i64) -> String {
    match OffsetDateTime::from_unix_timestamp(unix_secs) {
        Ok(dt) => format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            dt.year(),
            u8::from(dt.month()),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
        ),
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}
