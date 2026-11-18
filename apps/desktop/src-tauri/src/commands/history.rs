#[cfg(not(any(test, feature = "demo-data")))]
use crate::ipc::IpcError;
#[cfg(any(test, feature = "demo-data"))]
use crate::ipc::MeetingStatus;
use crate::ipc::{IpcResult, MeetingSummary};

/// Sample rows, newest first. Tests and a `demo-data` build only.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_meetings() -> Vec<MeetingSummary> {
    vec![
        MeetingSummary {
            id: "meeting-2".to_string(),
            title: "Partner call".to_string(),
            started_at_ms: 1_763_090_000_000,
            duration_sec: 920,
            status: MeetingStatus::Generating,
        },
        MeetingSummary {
            id: "meeting-1".to_string(),
            title: "Weekly sync".to_string(),
            started_at_ms: 1_763_000_000_000,
            duration_sec: 1_845,
            status: MeetingStatus::Ready,
        },
    ]
}

/// Order rows newest first; the order the UI relies on.
pub fn newest_first(mut rows: Vec<MeetingSummary>) -> Vec<MeetingSummary> {
    rows.sort_by_key(|m| std::cmp::Reverse(m.started_at_ms));
    rows
}

/// The meeting history the owner sees. There is no meeting store yet, so a
/// default build answers `NOT_IMPLEMENTED` instead of inventing rows.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_meetings() -> IpcResult<Vec<MeetingSummary>> {
    Ok(newest_first(sample_meetings()))
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn current_meetings() -> IpcResult<Vec<MeetingSummary>> {
    Err(IpcError::not_implemented("Meeting history"))
}

#[tauri::command]
pub async fn list_meetings() -> IpcResult<Vec<MeetingSummary>> {
    current_meetings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_orders_unsorted_rows() {
        let mut rows = sample_meetings();
        rows.reverse();
        let sorted = newest_first(rows);
        assert!(sorted
            .windows(2)
            .all(|w| w[0].started_at_ms > w[1].started_at_ms));
    }

    #[test]
    fn sample_meetings_are_newest_first() {
        let rows = sample_meetings();
        assert!(rows
            .windows(2)
            .all(|w| w[0].started_at_ms > w[1].started_at_ms));
    }

    #[test]
    fn meeting_summary_uses_camel_case_names() {
        let rows = sample_meetings();
        assert_eq!(rows.len(), 2);
        let json = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(json["startedAtMs"], 1_763_090_000_000u64);
        assert_eq!(json["durationSec"], 920);
        assert_eq!(json["status"], "generating");
    }
}
