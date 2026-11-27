//! Event names and payloads pushed from the shell to the webview.
//!
//! Streams use events (or channels); request/response work uses commands.

use serde::{Deserialize, Serialize};

use crate::ipc::Direction;

pub const MEETING_CAPTION: &str = "meeting://caption";
pub const MEETING_LATENCY: &str = "meeting://latency";
pub const MEETING_STATUS: &str = "meeting://status";
pub const MEETING_BANNER: &str = "meeting://banner";
pub const MODELS_PROGRESS: &str = "models://progress";

/// Caption events are batched per this window; see `CaptionCoalescer`.
pub const CAPTION_WINDOW_MS: u64 = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionEvent {
    pub line_id: String,
    pub direction: Direction,
    pub is_final: bool,
    pub source: String,
    pub target: String,
    /// From the moment the end of the speech was detected, about 256 ms after
    /// it, to the moment the text was ready, in milliseconds.
    pub delay_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyEvent {
    pub p50_ms: u32,
    pub p95_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Idle,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub state: RunState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BannerKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BannerEvent {
    pub kind: BannerKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelProgressState {
    Downloading,
    Verifying,
    Ok,
    Error,
}

/// Progress of one model download. No emitter exists yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsProgressEvent {
    pub id: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub state: ModelProgressState,
}

/// Receives `meeting://status` changes. The Tauri app handle is the real sink;
/// tests use a recorder.
pub trait StatusSink {
    fn emit_status(&self, state: RunState);
}

impl<R: tauri::Runtime> StatusSink for tauri::AppHandle<R> {
    fn emit_status(&self, state: RunState) {
        emit(self, MEETING_STATUS, StatusEvent { state });
    }
}

/// Receives everything a session sends to the webview. The Tauri app handle is
/// the real sink.
pub trait SessionSink: StatusSink + Send + Sync {
    fn emit_caption(&self, event: CaptionEvent);
    fn emit_latency(&self, event: LatencyEvent);
    fn emit_banner(&self, event: BannerEvent);
}

impl<R: tauri::Runtime> SessionSink for tauri::AppHandle<R> {
    fn emit_caption(&self, event: CaptionEvent) {
        emit(self, MEETING_CAPTION, event);
    }

    fn emit_latency(&self, event: LatencyEvent) {
        emit(self, MEETING_LATENCY, event);
    }

    fn emit_banner(&self, event: BannerEvent) {
        emit(self, MEETING_BANNER, event);
    }
}

/// Send one event to the webview.
fn emit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &'static str,
    payload: impl Serialize + Clone,
) {
    // Logs carry event names and error codes only (docs/guardrails.md rule 6).
    if tauri::Emitter::emit(app, name, payload).is_err() {
        eprintln!("emit failed: {name}");
    }
}

/// Batches caption events: within one window it keeps one event per `lineId`,
/// the latest, in the order the lines first appeared. A final line is never
/// replaced, so a late draft or a second final cannot rewrite it.
///
/// Pure: the caller passes the time of each call. The first event of a batch
/// opens the window; the batch is due `window_ms` later.
#[derive(Debug)]
pub struct CaptionCoalescer {
    window_ms: u64,
    opened_at_ms: Option<u64>,
    pending: Vec<CaptionEvent>,
}

impl CaptionCoalescer {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            opened_at_ms: None,
            pending: Vec::new(),
        }
    }

    /// Take the batch when its window has ended.
    fn take_if_due(&mut self, now_ms: u64) -> Option<Vec<CaptionEvent>> {
        let opened = self.opened_at_ms?;
        if now_ms.saturating_sub(opened) < self.window_ms {
            return None;
        }
        self.opened_at_ms = None;
        Some(std::mem::take(&mut self.pending))
    }

    /// Add an event. When the open window had already ended, returns that
    /// batch and the event starts the next window.
    pub fn push(&mut self, event: CaptionEvent, now_ms: u64) -> Option<Vec<CaptionEvent>> {
        let released = self.take_if_due(now_ms);
        if self.opened_at_ms.is_none() {
            self.opened_at_ms = Some(now_ms);
        }
        match self.pending.iter_mut().find(|e| e.line_id == event.line_id) {
            Some(slot) if slot.is_final => {}
            Some(slot) => *slot = event,
            None => self.pending.push(event),
        }
        released
    }

    /// Take the batch when its window has ended.
    pub fn poll(&mut self, now_ms: u64) -> Option<Vec<CaptionEvent>> {
        self.take_if_due(now_ms)
    }

    /// When the open window ends, if one is open.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.opened_at_ms.map(|t| t.saturating_add(self.window_ms))
    }

    /// Take everything now, for example when the meeting stops.
    pub fn flush(&mut self) -> Vec<CaptionEvent> {
        self.opened_at_ms = None;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caption(line: &str, is_final: bool, text: &str) -> CaptionEvent {
        CaptionEvent {
            line_id: line.to_string(),
            direction: Direction::In,
            is_final,
            source: text.to_string(),
            target: String::new(),
            delay_ms: 0,
        }
    }

    fn texts(batch: &[CaptionEvent]) -> Vec<(&str, &str)> {
        batch
            .iter()
            .map(|e| (e.line_id.as_str(), e.source.as_str()))
            .collect()
    }

    #[test]
    fn window_is_50_ms() {
        assert_eq!(CAPTION_WINDOW_MS, 50);
    }

    #[test]
    fn nothing_is_released_before_the_window_ends() {
        let mut c = CaptionCoalescer::new(CAPTION_WINDOW_MS);
        assert!(c.push(caption("a", false, "x"), 1_000).is_none());
        assert!(c.poll(1_049).is_none());
        assert_eq!(c.next_deadline_ms(), Some(1_050));
        let batch = c.poll(1_050).unwrap();
        assert_eq!(texts(&batch), [("a", "x")]);
        assert!(c.poll(2_000).is_none());
        assert_eq!(c.next_deadline_ms(), None);
    }

    #[test]
    fn latest_revision_per_line_wins_in_first_seen_order() {
        let mut c = CaptionCoalescer::new(50);
        assert!(c.push(caption("b", false, "b1"), 0).is_none());
        assert!(c.push(caption("a", false, "a1"), 10).is_none());
        assert!(c.push(caption("b", false, "b2"), 20).is_none());
        assert!(c.push(caption("b", false, "b3"), 49).is_none());
        let batch = c.poll(50).unwrap();
        assert_eq!(texts(&batch), [("b", "b3"), ("a", "a1")]);
    }

    #[test]
    fn a_final_line_is_never_rewritten() {
        let mut c = CaptionCoalescer::new(50);
        assert!(c.push(caption("a", false, "draft"), 0).is_none());
        assert!(c.push(caption("a", true, "final"), 5).is_none());
        assert!(c.push(caption("a", false, "late draft"), 6).is_none());
        assert!(c.push(caption("a", true, "other final"), 7).is_none());
        let batch = c.poll(50).unwrap();
        assert_eq!(texts(&batch), [("a", "final")]);
        assert!(batch[0].is_final);
    }

    #[test]
    fn a_push_after_the_window_releases_the_old_batch_and_opens_a_new_one() {
        let mut c = CaptionCoalescer::new(50);
        assert!(c.push(caption("a", false, "a1"), 0).is_none());
        let old = c.push(caption("b", false, "b1"), 60).unwrap();
        assert_eq!(texts(&old), [("a", "a1")]);
        assert!(c.poll(109).is_none());
        let next = c.poll(110).unwrap();
        assert_eq!(texts(&next), [("b", "b1")]);
    }

    #[test]
    fn flush_releases_everything_at_once() {
        let mut c = CaptionCoalescer::new(50);
        assert!(c.flush().is_empty());
        assert!(c.push(caption("a", false, "a1"), 0).is_none());
        assert!(c.push(caption("b", true, "b1"), 1).is_none());
        assert_eq!(texts(&c.flush()), [("a", "a1"), ("b", "b1")]);
        assert!(c.poll(1_000).is_none());
    }

    #[test]
    fn clock_going_backwards_does_not_panic_or_release_early() {
        let mut c = CaptionCoalescer::new(50);
        assert!(c.push(caption("a", false, "a1"), 1_000).is_none());
        assert!(c.poll(10).is_none());
        assert!(c.push(caption("b", false, "b1"), 5).is_none());
        assert_eq!(texts(&c.poll(1_050).unwrap()), [("a", "a1"), ("b", "b1")]);
    }
}
