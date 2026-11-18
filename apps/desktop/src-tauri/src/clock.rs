//! Wall clock and id helpers shared by commands and events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Milliseconds since the Unix epoch; 0 when the clock is before the epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// `<prefix>-<now_ms>-<n>`. The counter suffix keeps ids unique when two ids
/// are made in the same millisecond.
pub fn unique_id(prefix: &str, now_ms: u64) -> String {
    let n = ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{now_ms}-{n}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_within_one_millisecond() {
        let ids: HashSet<String> = (0..10_000).map(|_| unique_id("note", 5)).collect();
        assert_eq!(ids.len(), 10_000);
    }

    #[test]
    fn id_starts_with_prefix_and_time() {
        let id = unique_id("meeting", 1_700_000_000_000);
        assert!(id.starts_with("meeting-1700000000000-"), "{id}");
    }

    #[test]
    fn now_ms_is_after_2023() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
