//! Transcript coordinator: applies envelopes to a session view.
//!
//! Rules from the handoff protocol (section "Rules"):
//! 1. `sequence` must increase per session. Duplicate `eventId` is a no-op.
//! 2. `revision` increases per segment. Older revisions never overwrite newer ones.
//! 3. A translation references the exact source revision it used. A translation
//!    whose source revision is not the current one is rejected as stale.
//! 4. A final segment is immutable. Only an explicit correction may change it
//!    (not implemented in v0.1: final segments reject all updates). A final
//!    translation is immutable too: a later update, a failure or another
//!    translation id for the same source line is rejected.
//! 5. Partial text is never a confirmed fact; the view marks it as partial.
//! 6. Every envelope carries the session id; mismatches are rejected.
//! 7. A session status change must be a legal transition from the current status.

use crate::protocol::{
    Envelope, EventPayload, Language, SegmentStatus, SessionStatus, TranscriptSegment,
    TranslationSegment, TranslationStatus,
};
use crate::session;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use thiserror::Error;

/// Event ids a live view remembers for `Applied::Duplicate`. Starting value.
/// The monotonic `sequence` check already refuses older duplicates, so the
/// window only decides which message the caller sees.
const LIVE_EVENT_ID_WINDOW: usize = 1_024;

/// Why the coordinator refused an event. `apply` reports it as
/// `Applied::Rejected(text)`, where `text` is the `Display` form below.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum CoordinatorError {
    #[error("segment {segment_id} is final and immutable")]
    SegmentFinal { segment_id: String },
    #[error("stale revision {offered} for segment {segment_id} (current {current})")]
    StaleSegmentRevision {
        segment_id: String,
        offered: u32,
        current: u32,
    },
    #[error("unknown source segment {0}")]
    UnknownSourceSegment(String),
    #[error("stale translation: source revision {offered} != current {current}")]
    StaleTranslationSource { offered: u32, current: u32 },
    #[error("stale translation revision {0}")]
    StaleTranslationRevision(u32),
    #[error("stale failure for revision {0}")]
    StaleFailure(u32),
    #[error("translation {translation_id} belongs to segment {segment_id}")]
    TranslationOfOtherSegment {
        translation_id: String,
        segment_id: String,
    },
    #[error("translation {0} is final and immutable")]
    TranslationFinal(String),
    #[error("segment {segment_id} already has final translation {translation_id}")]
    SegmentHasFinalTranslation {
        segment_id: String,
        translation_id: String,
    },
    #[error("translation {0} has no revision left to record a failure")]
    RevisionOverflow(String),
    #[error("{0}")]
    IllegalTransition(#[from] session::IllegalTransition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Applied,
    Duplicate,
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub segment_id: String,
    pub source_id: String,
    pub start_ms: u64,
    pub source_text: String,
    pub source_status: SegmentStatus,
    /// Present only when a translation matches the current source revision.
    pub target_text: Option<String>,
    pub target_language: Option<Language>,
    pub translation_status: Option<TranslationStatus>,
}

/// Event ids already applied. Live views keep a bounded window; replay views
/// keep every id, because a replay is finite and must detect any duplicate.
#[derive(Debug)]
struct SeenEventIds {
    ids: HashSet<String>,
    order: VecDeque<String>,
    window: Option<usize>,
}

impl SeenEventIds {
    fn new(window: Option<usize>) -> Self {
        Self {
            ids: HashSet::new(),
            order: VecDeque::new(),
            window,
        }
    }

    fn contains(&self, event_id: &str) -> bool {
        self.ids.contains(event_id)
    }

    fn insert(&mut self, event_id: &str) {
        if !self.ids.insert(event_id.to_string()) {
            return;
        }
        if let Some(window) = self.window {
            self.order.push_back(event_id.to_string());
            while self.order.len() > window {
                if let Some(old) = self.order.pop_front() {
                    self.ids.remove(&old);
                }
            }
        }
    }
}

/// Translation ids that belong to one source segment, and the one to show.
#[derive(Debug, Default)]
struct SourceTranslations {
    ids: Vec<String>,
    best: Option<String>,
}

/// Which translation to show: a good one over a failed one, a final one over
/// a partial one, then the highest revision.
fn rank(t: &TranslationSegment) -> (bool, bool, u32) {
    (
        t.status != TranslationStatus::Failed,
        t.status == TranslationStatus::Final,
        t.revision,
    )
}

fn opposite(language: Language) -> Language {
    match language {
        Language::En => Language::Vi,
        Language::Vi => Language::En,
    }
}

#[derive(Debug)]
pub struct SessionView {
    session_id: String,
    last_sequence: u64,
    seen_event_ids: SeenEventIds,
    status: SessionStatus,
    segments: BTreeMap<String, TranscriptSegment>,
    translations: BTreeMap<String, TranslationSegment>,
    by_source: HashMap<String, SourceTranslations>,
    speakers: BTreeMap<String, String>,
}

impl SessionView {
    /// A live view. It remembers a bounded window of recent event ids.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self::with_window(session_id, Some(LIVE_EVENT_ID_WINDOW))
    }

    /// A replay view over a recorded journal. It remembers every event id.
    pub fn replay(session_id: impl Into<String>) -> Self {
        Self::with_window(session_id, None)
    }

    fn with_window(session_id: impl Into<String>, window: Option<usize>) -> Self {
        Self {
            session_id: session_id.into(),
            last_sequence: 0,
            seen_event_ids: SeenEventIds::new(window),
            status: SessionStatus::Idle,
            segments: BTreeMap::new(),
            translations: BTreeMap::new(),
            by_source: HashMap::new(),
            speakers: BTreeMap::new(),
        }
    }

    /// How many event ids the duplicate check holds right now.
    pub fn seen_event_id_count(&self) -> usize {
        self.seen_event_ids.ids.len()
    }

    pub fn status(&self) -> SessionStatus {
        self.status
    }

    pub fn apply(&mut self, env: &Envelope) -> Applied {
        if env.session_id != self.session_id {
            return Applied::Rejected(format!("session mismatch: {}", env.session_id));
        }
        if self.seen_event_ids.contains(&env.event_id) {
            return Applied::Duplicate;
        }
        if env.sequence <= self.last_sequence {
            return Applied::Rejected(format!(
                "out of order: sequence {} <= {}",
                env.sequence, self.last_sequence
            ));
        }
        let event = match env.event() {
            Ok(e) => e,
            Err(err) => return Applied::Rejected(format!("unknown or invalid event: {err}")),
        };
        let result = match event {
            EventPayload::TranscriptSegmentUpdated(seg) => self.apply_segment(seg),
            EventPayload::TranslationSegmentUpdated(tr) => self.apply_translation(tr),
            EventPayload::TranslationSegmentFailed(f) => self.apply_translation_failure(
                f.translation_id,
                f.source_segment_id,
                f.source_revision,
                f.provider_id,
            ),
            EventPayload::SessionStatusChanged { to, .. } => session::transition(self.status, to)
                .map(|next| self.status = next)
                .map_err(CoordinatorError::from),
            EventPayload::SpeakerLabelUpdated { speaker_id, label } => {
                self.speakers.insert(speaker_id, label);
                Ok(())
            }
            EventPayload::EgressBlocked { .. } => Ok(()),
        };
        match result {
            Ok(()) => {
                self.last_sequence = env.sequence;
                self.seen_event_ids.insert(&env.event_id);
                Applied::Applied
            }
            Err(reason) => Applied::Rejected(reason.to_string()),
        }
    }

    fn apply_segment(&mut self, seg: TranscriptSegment) -> Result<(), CoordinatorError> {
        if let Some(current) = self.segments.get(&seg.segment_id) {
            if current.status == SegmentStatus::Final {
                return Err(CoordinatorError::SegmentFinal {
                    segment_id: seg.segment_id,
                });
            }
            if seg.revision <= current.revision {
                return Err(CoordinatorError::StaleSegmentRevision {
                    segment_id: seg.segment_id,
                    offered: seg.revision,
                    current: current.revision,
                });
            }
        }
        self.segments.insert(seg.segment_id.clone(), seg);
        Ok(())
    }

    /// A final translation never changes while its source revision is the
    /// current one: not by a newer revision, a failure or another translation
    /// id for the same source line.
    fn ensure_translation_open(
        &self,
        translation_id: &str,
        source_segment_id: &str,
        source_revision: u32,
    ) -> Result<(), CoordinatorError> {
        if let Some(current) = self.translations.get(translation_id) {
            if current.source_segment_id != source_segment_id {
                return Err(CoordinatorError::TranslationOfOtherSegment {
                    translation_id: translation_id.to_string(),
                    segment_id: current.source_segment_id.clone(),
                });
            }
            // A final translation of an older source revision is already
            // hidden, so a translation of the new revision may replace it.
            if current.status == TranslationStatus::Final
                && current.source_revision == source_revision
            {
                return Err(CoordinatorError::TranslationFinal(
                    translation_id.to_string(),
                ));
            }
        }
        let shown = self.shown_translation(source_segment_id);
        if let Some(best) = shown.filter(|t| {
            t.status == TranslationStatus::Final && t.source_revision == source_revision
        }) {
            return Err(CoordinatorError::SegmentHasFinalTranslation {
                segment_id: source_segment_id.to_string(),
                translation_id: best.translation_id.clone(),
            });
        }
        Ok(())
    }

    fn apply_translation(&mut self, tr: TranslationSegment) -> Result<(), CoordinatorError> {
        let source = self
            .segments
            .get(&tr.source_segment_id)
            .ok_or_else(|| CoordinatorError::UnknownSourceSegment(tr.source_segment_id.clone()))?;
        if tr.source_revision != source.revision {
            return Err(CoordinatorError::StaleTranslationSource {
                offered: tr.source_revision,
                current: source.revision,
            });
        }
        self.ensure_translation_open(
            &tr.translation_id,
            &tr.source_segment_id,
            tr.source_revision,
        )?;
        if let Some(current) = self.translations.get(&tr.translation_id) {
            if tr.revision <= current.revision {
                return Err(CoordinatorError::StaleTranslationRevision(tr.revision));
            }
        }
        self.store_translation(tr);
        Ok(())
    }

    fn apply_translation_failure(
        &mut self,
        translation_id: String,
        source_segment_id: String,
        source_revision: u32,
        provider_id: String,
    ) -> Result<(), CoordinatorError> {
        let source = self
            .segments
            .get(&source_segment_id)
            .ok_or_else(|| CoordinatorError::UnknownSourceSegment(source_segment_id.clone()))?;
        if source_revision != source.revision {
            return Err(CoordinatorError::StaleFailure(source_revision));
        }
        // Translation goes to the other language of the pair.
        let target_language = opposite(source.language);
        self.ensure_translation_open(&translation_id, &source_segment_id, source_revision)?;
        let revision = match self.translations.get(&translation_id) {
            Some(t) => t
                .revision
                .checked_add(1)
                .ok_or_else(|| CoordinatorError::RevisionOverflow(translation_id.clone()))?,
            None => 1,
        };
        self.store_translation(TranslationSegment {
            translation_id,
            source_segment_id,
            source_revision,
            target_language,
            text: String::new(),
            status: TranslationStatus::Failed,
            provider_id,
            revision,
        });
        Ok(())
    }

    /// Store a validated translation and refresh the one shown for its source.
    fn store_translation(&mut self, tr: TranslationSegment) {
        let source_id = tr.source_segment_id.clone();
        let source_revision = tr.source_revision;
        let entry = self.by_source.entry(source_id.clone()).or_default();
        if !entry.ids.contains(&tr.translation_id) {
            entry.ids.push(tr.translation_id.clone());
        }
        self.translations.insert(tr.translation_id.clone(), tr);

        let mut best: Option<&TranslationSegment> = None;
        if let Some(entry) = self.by_source.get(&source_id) {
            for t in entry.ids.iter().filter_map(|id| self.translations.get(id)) {
                if t.source_revision != source_revision {
                    continue;
                }
                if best.is_none_or(|b| rank(t) > rank(b)) {
                    best = Some(t);
                }
            }
        }
        let best = best.map(|t| t.translation_id.clone());
        if let Some(entry) = self.by_source.get_mut(&source_id) {
            entry.best = best;
        }
    }

    /// The translation chosen for a source segment, whatever its revision.
    fn shown_translation(&self, source_segment_id: &str) -> Option<&TranslationSegment> {
        let id = self.by_source.get(source_segment_id)?.best.as_deref()?;
        self.translations.get(id)
    }

    /// Captions ordered by start time. A translation is shown only when it
    /// matches the current source revision.
    pub fn captions(&self) -> Vec<Caption> {
        let mut segments: Vec<&TranscriptSegment> = self.segments.values().collect();
        segments.sort_by(|a, b| {
            a.start_ms
                .cmp(&b.start_ms)
                .then_with(|| a.segment_id.cmp(&b.segment_id))
        });
        segments
            .into_iter()
            .map(|seg| {
                let tr = self
                    .shown_translation(&seg.segment_id)
                    .filter(|t| t.source_revision == seg.revision);
                Caption {
                    segment_id: seg.segment_id.clone(),
                    source_id: seg.source_id.clone(),
                    start_ms: seg.start_ms,
                    source_text: seg.text.clone(),
                    source_status: seg.status,
                    target_text: tr
                        .filter(|t| t.status != TranslationStatus::Failed)
                        .map(|t| t.text.clone()),
                    target_language: tr.map(|t| t.target_language),
                    translation_status: tr.map(|t| t.status),
                }
            })
            .collect()
    }

    pub fn speaker_label(&self, speaker_id: &str) -> Option<&str> {
        self.speakers.get(speaker_id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TranslationFailure;

    const SESSION: &str = "session_t";

    fn envelope(seq: u64, payload: &EventPayload) -> Envelope {
        Envelope::new(
            format!("e{seq}"),
            SESSION,
            seq,
            "2026-11-13T00:00:00.000Z",
            payload,
        )
        .expect("envelope")
    }

    fn segment(status: SegmentStatus, revision: u32) -> EventPayload {
        EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
            segment_id: "seg_1".into(),
            source_id: "incoming".into(),
            revision,
            start_ms: 0,
            end_ms: 1_000,
            status,
            language: Language::En,
            text: "hello".into(),
        })
    }

    fn translation(status: TranslationStatus, revision: u32) -> EventPayload {
        EventPayload::TranslationSegmentUpdated(TranslationSegment {
            translation_id: "tr_1".into(),
            source_segment_id: "seg_1".into(),
            source_revision: 1,
            target_language: Language::Vi,
            text: "xin chao".into(),
            status,
            provider_id: "local".into(),
            revision,
        })
    }

    fn failure() -> EventPayload {
        EventPayload::TranslationSegmentFailed(TranslationFailure {
            translation_id: "tr_1".into(),
            source_segment_id: "seg_1".into(),
            source_revision: 1,
            provider_id: "local".into(),
            reason: "timeout".into(),
        })
    }

    #[test]
    fn a_failure_after_the_last_translation_revision_is_rejected_not_wrapped() {
        let mut view = SessionView::new(SESSION);
        let seg = segment(SegmentStatus::Stable, 1);
        assert_eq!(view.apply(&envelope(1, &seg)), Applied::Applied);
        let partial = translation(TranslationStatus::Partial, u32::MAX);
        assert_eq!(view.apply(&envelope(2, &partial)), Applied::Applied);

        let outcome = view.apply(&envelope(3, &failure()));

        assert_eq!(
            outcome,
            Applied::Rejected(CoordinatorError::RevisionOverflow("tr_1".into()).to_string())
        );
        // The stored translation is untouched.
        assert_eq!(
            view.captions()[0].translation_status,
            Some(TranslationStatus::Partial)
        );
    }

    #[test]
    fn a_failure_one_revision_below_the_maximum_is_recorded_at_the_maximum() {
        let mut view = SessionView::new(SESSION);
        let seg = segment(SegmentStatus::Stable, 1);
        view.apply(&envelope(1, &seg));
        let partial = translation(TranslationStatus::Partial, u32::MAX - 1);
        view.apply(&envelope(2, &partial));

        assert_eq!(view.apply(&envelope(3, &failure())), Applied::Applied);
        assert_eq!(
            view.captions()[0].translation_status,
            Some(TranslationStatus::Failed)
        );
    }

    #[test]
    fn each_refusal_reports_its_typed_reason_as_the_rejection_text() {
        let mut view = SessionView::new(SESSION);
        let final_seg = segment(SegmentStatus::Final, 1);
        view.apply(&envelope(1, &final_seg));
        let again = segment(SegmentStatus::Final, 2);
        assert_eq!(
            view.apply(&envelope(2, &again)),
            Applied::Rejected("segment seg_1 is final and immutable".into())
        );

        let mut view = SessionView::new(SESSION);
        let rev2 = segment(SegmentStatus::Stable, 2);
        view.apply(&envelope(1, &rev2));
        let rev1 = segment(SegmentStatus::Stable, 1);
        assert_eq!(
            view.apply(&envelope(2, &rev1)),
            Applied::Rejected("stale revision 1 for segment seg_1 (current 2)".into())
        );
        let tr = translation(TranslationStatus::Partial, 1);
        assert_eq!(
            view.apply(&envelope(3, &tr)),
            Applied::Rejected("stale translation: source revision 1 != current 2".into())
        );

        let mut empty = SessionView::new(SESSION);
        assert_eq!(
            empty.apply(&envelope(1, &translation(TranslationStatus::Partial, 1))),
            Applied::Rejected("unknown source segment seg_1".into())
        );
        let to_running = EventPayload::SessionStatusChanged {
            from: SessionStatus::Idle,
            to: SessionStatus::Running,
        };
        assert_eq!(
            empty.apply(&envelope(2, &to_running)),
            Applied::Rejected("illegal session transition Idle -> Running".into())
        );
    }
}
