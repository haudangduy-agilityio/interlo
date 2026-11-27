//! Contract and rule tests for meeting-core.
//! Fixture: fixtures/protocol/session_example.jsonl (shared with the UI tests).

// Test-only file: its helper fns are not `#[test]` fns, so the clippy.toml test exemption skips them.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use meeting_core::coordinator::{Applied, SessionView};
use meeting_core::protocol::{
    Envelope, EventPayload, Language, SegmentStatus, SessionStatus, TranscriptSegment,
    TranslationSegment, TranslationStatus,
};

const FIXTURE: &str = include_str!("../../../fixtures/protocol/session_example.jsonl");

fn fixture_envelopes() -> Vec<Envelope> {
    FIXTURE
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line parses"))
        .collect()
}

fn seg(revision: u32, status: SegmentStatus, text: &str) -> EventPayload {
    EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
        segment_id: "seg_x".into(),
        source_id: "incoming".into(),
        revision,
        start_ms: 0,
        end_ms: 1000,
        status,
        language: Language::En,
        text: text.into(),
    })
}

fn tr(source_revision: u32, revision: u32, text: &str) -> EventPayload {
    EventPayload::TranslationSegmentUpdated(TranslationSegment {
        translation_id: "tr_x".into(),
        source_segment_id: "seg_x".into(),
        source_revision,
        target_language: Language::Vi,
        text: text.into(),
        status: TranslationStatus::Final,
        provider_id: "remote-provider-a".into(),
        revision,
    })
}

fn env(id: &str, seq: u64, payload: &EventPayload) -> Envelope {
    Envelope::new(id, "session_t", seq, "2026-11-13T00:00:00.000Z", payload).unwrap()
}

#[test]
fn fixture_parses_and_replays_to_completed_session() {
    let mut view = SessionView::new("session_01");
    for e in fixture_envelopes() {
        assert_eq!(view.apply(&e), Applied::Applied, "event {}", e.event_id);
    }
    assert_eq!(view.status(), SessionStatus::Completed);
    let caps = view.captions();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].source_text, "We can ship this on Friday.");
    assert_eq!(caps[0].source_status, SegmentStatus::Final);
    assert_eq!(
        caps[0].target_text.as_deref(),
        Some("Chúng ta có thể phát hành vào thứ Sáu.")
    );
    assert_eq!(view.speaker_label("spk_1"), Some("Partner 1"));
}

#[test]
fn envelope_round_trips_through_json() {
    for e in fixture_envelopes() {
        let text = serde_json::to_string(&e).unwrap();
        let back: Envelope = serde_json::from_str(&text).unwrap();
        assert_eq!(back, e);
        back.event().expect("typed payload decodes");
    }
}

#[test]
fn duplicate_event_id_is_a_no_op() {
    let mut view = SessionView::new("session_t");
    let e = env("evt_a", 1, &seg(1, SegmentStatus::Partial, "hello"));
    assert_eq!(view.apply(&e), Applied::Applied);
    assert_eq!(view.apply(&e), Applied::Duplicate);
}

#[test]
fn out_of_order_sequence_is_rejected() {
    let mut view = SessionView::new("session_t");
    assert_eq!(
        view.apply(&env("evt_a", 5, &seg(1, SegmentStatus::Partial, "a"))),
        Applied::Applied
    );
    let late = env("evt_b", 4, &seg(2, SegmentStatus::Partial, "b"));
    assert!(matches!(view.apply(&late), Applied::Rejected(_)));
}

#[test]
fn stale_segment_revision_cannot_overwrite_newer() {
    let mut view = SessionView::new("session_t");
    view.apply(&env("evt_1", 1, &seg(2, SegmentStatus::Partial, "newer")));
    let older = env("evt_2", 2, &seg(1, SegmentStatus::Partial, "older"));
    assert!(matches!(view.apply(&older), Applied::Rejected(_)));
    assert_eq!(view.captions()[0].source_text, "newer");
}

#[test]
fn final_segment_is_immutable() {
    let mut view = SessionView::new("session_t");
    view.apply(&env("evt_1", 1, &seg(1, SegmentStatus::Final, "done")));
    let change = env("evt_2", 2, &seg(2, SegmentStatus::Partial, "changed"));
    assert!(matches!(view.apply(&change), Applied::Rejected(_)));
    assert_eq!(view.captions()[0].source_text, "done");
}

#[test]
fn translation_for_stale_source_revision_is_not_shown() {
    let mut view = SessionView::new("session_t");
    view.apply(&env("evt_1", 1, &seg(1, SegmentStatus::Partial, "first")));
    view.apply(&env("evt_2", 2, &tr(1, 1, "bản dịch cũ")));
    view.apply(&env("evt_3", 3, &seg(2, SegmentStatus::Partial, "second")));
    // Source moved to revision 2, so the revision-1 translation no longer matches.
    assert_eq!(view.captions()[0].target_text, None);
    // A translation for revision 2 is accepted and shown.
    assert!(matches!(
        view.apply(&env("evt_4", 4, &tr(2, 2, "bản dịch mới"))),
        Applied::Applied
    ));
    assert_eq!(
        view.captions()[0].target_text.as_deref(),
        Some("bản dịch mới")
    );
}

#[test]
fn translation_for_unknown_source_is_rejected() {
    let mut view = SessionView::new("session_t");
    let res = view.apply(&env("evt_1", 1, &tr(1, 1, "x")));
    assert!(matches!(res, Applied::Rejected(_)));
}

#[test]
fn session_mismatch_is_rejected() {
    let mut view = SessionView::new("other_session");
    let res = view.apply(&env("evt_1", 1, &seg(1, SegmentStatus::Partial, "x")));
    assert!(matches!(res, Applied::Rejected(_)));
}

#[test]
fn unknown_event_type_is_rejected_not_panicking() {
    let mut view = SessionView::new("session_t");
    let mut e = env("evt_1", 1, &seg(1, SegmentStatus::Partial, "x"));
    e.event_type = "transcript.segment.exploded".into();
    assert!(matches!(view.apply(&e), Applied::Rejected(_)));
}

// ---- Review findings: final translations, failure language, captions cost,
// ---- session transitions, dedupe window, secure journal, privacy default.

fn seg_at(id: &str, revision: u32, start_ms: u64, language: Language) -> EventPayload {
    EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
        segment_id: id.into(),
        source_id: "incoming".into(),
        revision,
        start_ms,
        end_ms: start_ms + 500,
        status: SegmentStatus::Final,
        language,
        text: format!("line {id}"),
    })
}

fn tr_of(
    translation_id: &str,
    source: &str,
    revision: u32,
    status: TranslationStatus,
    text: &str,
) -> EventPayload {
    EventPayload::TranslationSegmentUpdated(TranslationSegment {
        translation_id: translation_id.into(),
        source_segment_id: source.into(),
        source_revision: 1,
        target_language: Language::Vi,
        text: text.into(),
        status,
        provider_id: "remote-provider-a".into(),
        revision,
    })
}

fn failed(translation_id: &str, source: &str) -> EventPayload {
    EventPayload::TranslationSegmentFailed(meeting_core::protocol::TranslationFailure {
        translation_id: translation_id.into(),
        source_segment_id: source.into(),
        source_revision: 1,
        provider_id: "remote-provider-a".into(),
        reason: "timeout".into(),
    })
}

fn applied(view: &mut SessionView, seq: u64, payload: &EventPayload) -> Applied {
    view.apply(&env(&format!("evt_{seq}"), seq, payload))
}

#[test]
fn failed_event_cannot_replace_a_final_translation() {
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    assert_eq!(
        applied(&mut view, 2, &tr(1, 1, "xin chào")),
        Applied::Applied
    );
    assert!(matches!(
        applied(&mut view, 3, &failed("tr_x", "seg_x")),
        Applied::Rejected(_)
    ));
    let cap = &view.captions()[0];
    assert_eq!(cap.target_text.as_deref(), Some("xin chào"));
    assert_eq!(cap.translation_status, Some(TranslationStatus::Final));
}

#[test]
fn higher_revision_cannot_replace_a_final_translation() {
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    applied(&mut view, 2, &tr(1, 1, "xin chào"));
    assert!(matches!(
        applied(&mut view, 3, &tr(1, 2, "đã bị sửa")),
        Applied::Rejected(_)
    ));
    assert_eq!(view.captions()[0].target_text.as_deref(), Some("xin chào"));
}

#[test]
fn another_translation_id_cannot_replace_a_final_translation() {
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    applied(&mut view, 2, &tr(1, 1, "xin chào"));
    let other = tr_of("tr_other", "seg_x", 9, TranslationStatus::Final, "bản khác");
    assert!(matches!(
        applied(&mut view, 3, &other),
        Applied::Rejected(_)
    ));
    assert!(matches!(
        applied(&mut view, 4, &failed("tr_other", "seg_x")),
        Applied::Rejected(_)
    ));
    assert_eq!(view.captions()[0].target_text.as_deref(), Some("xin chào"));
}

#[test]
fn failure_reports_the_target_language_of_the_source_segment() {
    let mut view = SessionView::new("session_t");
    // Outgoing Vietnamese is translated into English, not Vietnamese.
    applied(&mut view, 1, &seg_at("seg_x", 1, 0, Language::Vi));
    applied(&mut view, 2, &seg_at("seg_y", 1, 10, Language::En));
    applied(&mut view, 3, &failed("tr_x", "seg_x"));
    applied(&mut view, 4, &failed("tr_y", "seg_y"));
    let caps = view.captions();
    assert_eq!(caps[0].target_language, Some(Language::En));
    assert_eq!(caps[0].translation_status, Some(TranslationStatus::Failed));
    assert_eq!(caps[0].target_text, None);
    assert_eq!(caps[1].target_language, Some(Language::Vi));
}

#[test]
fn retry_under_a_new_id_wins_over_an_earlier_failure_in_either_order() {
    // Failure first, then a retry with a later (greater) translation id.
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    applied(&mut view, 2, &failed("tr_a", "seg_x"));
    let retry = tr_of("tr_b", "seg_x", 1, TranslationStatus::Final, "xin chào");
    assert_eq!(applied(&mut view, 3, &retry), Applied::Applied);
    assert_eq!(view.captions()[0].target_text.as_deref(), Some("xin chào"));

    // Retry gets a smaller id than the failure: ordering by id must not matter.
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    applied(&mut view, 2, &failed("tr_b", "seg_x"));
    let retry = tr_of("tr_a", "seg_x", 1, TranslationStatus::Final, "xin chào");
    assert_eq!(applied(&mut view, 3, &retry), Applied::Applied);
    assert_eq!(view.captions()[0].target_text.as_deref(), Some("xin chào"));

    // A failure that arrives after a good translation (other id) does not hide it.
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg(1, SegmentStatus::Final, "hello"));
    let good = tr_of("tr_a", "seg_x", 1, TranslationStatus::Partial, "xin");
    applied(&mut view, 2, &good);
    applied(&mut view, 3, &failed("tr_b", "seg_x"));
    assert_eq!(view.captions()[0].target_text.as_deref(), Some("xin"));
}

#[test]
fn captions_stay_fast_and_correct_with_five_thousand_lines() {
    const LINES: u64 = 5_000;
    let mut view = SessionView::new("session_t");
    let mut seq = 0;
    for i in 0..LINES {
        // Out-of-order start times, so the sort has real work to do.
        let start = (i * 7_919) % 100_003;
        seq += 1;
        applied(
            &mut view,
            seq,
            &seg_at(&format!("seg_{i:05}"), 1, start, Language::En),
        );
    }
    for i in 0..LINES {
        let id = format!("seg_{i:05}");
        // One failed attempt and one good retry per line.
        seq += 1;
        applied(&mut view, seq, &failed(&format!("tr_a_{i:05}"), &id));
        seq += 1;
        let good = tr_of(
            &format!("tr_b_{i:05}"),
            &id,
            1,
            TranslationStatus::Final,
            "ok",
        );
        applied(&mut view, seq, &good);
    }
    let started = std::time::Instant::now();
    let caps = view.captions();
    let elapsed = started.elapsed();
    assert_eq!(caps.len(), LINES as usize);
    assert!(caps.windows(2).all(|w| w[0].start_ms <= w[1].start_ms));
    assert!(caps.iter().all(|c| c.target_text.as_deref() == Some("ok")));
    // Generous bound for an unoptimised test build: the quadratic version
    // needs several times this, the indexed version a small fraction of it.
    assert!(
        elapsed < std::time::Duration::from_millis(150),
        "captions() took {elapsed:?}"
    );
}

fn status_change(from: SessionStatus, to: SessionStatus) -> EventPayload {
    EventPayload::SessionStatusChanged { from, to }
}

#[test]
fn illegal_session_transition_is_rejected_and_status_is_kept() {
    use SessionStatus::*;
    let mut view = SessionView::new("session_t");
    let path = [Preparing, Running, Stopping, Completed];
    let mut from = Idle;
    for (i, to) in path.into_iter().enumerate() {
        let n = i as u64 + 1;
        assert_eq!(
            applied(&mut view, n, &status_change(from, to)),
            Applied::Applied
        );
        from = to;
    }
    let res = applied(&mut view, 5, &status_change(Completed, Running));
    match res {
        Applied::Rejected(reason) => assert!(
            reason.contains("illegal session transition"),
            "reason was: {reason}"
        ),
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert_eq!(view.status(), Completed);
    // Idle -> Running skips Preparing; also illegal.
    let mut view = SessionView::new("session_t");
    assert!(matches!(
        applied(&mut view, 1, &status_change(Idle, Running)),
        Applied::Rejected(_)
    ));
    assert_eq!(view.status(), Idle);
}

#[test]
fn live_dedupe_memory_is_bounded_and_duplicates_are_still_ignored() {
    let mut view = SessionView::new("session_t");
    let first = env("evt_first", 1, &seg(1, SegmentStatus::Partial, "a"));
    assert_eq!(view.apply(&first), Applied::Applied);
    for n in 2..=6_000u64 {
        let label = EventPayload::SpeakerLabelUpdated {
            speaker_id: "spk".into(),
            label: format!("label {n}"),
        };
        assert_eq!(applied(&mut view, n, &label), Applied::Applied);
    }
    assert!(
        view.seen_event_id_count() < 2_000,
        "dedupe set holds {} ids",
        view.seen_event_id_count()
    );
    // Recent and very old duplicates are both ignored; neither changes state.
    let recent = env(
        "evt_5999",
        5_999,
        &EventPayload::SpeakerLabelUpdated {
            speaker_id: "spk".into(),
            label: "label 5999".into(),
        },
    );
    assert_ne!(view.apply(&recent), Applied::Applied);
    assert_ne!(view.apply(&first), Applied::Applied);
    assert_eq!(view.speaker_label("spk"), Some("label 6000"));
}

#[test]
fn replay_view_keeps_every_event_id_for_exact_duplicate_detection() {
    let mut view = SessionView::replay("session_t");
    let first = env("evt_first", 1, &seg(1, SegmentStatus::Partial, "a"));
    assert_eq!(view.apply(&first), Applied::Applied);
    for n in 2..=3_000u64 {
        let label = EventPayload::SpeakerLabelUpdated {
            speaker_id: "spk".into(),
            label: format!("label {n}"),
        };
        applied(&mut view, n, &label);
    }
    assert_eq!(view.apply(&first), Applied::Duplicate);
}

#[test]
fn default_policy_is_local_only() {
    use meeting_core::policy::{EgressDenied, PrivacyPolicy, Provider};
    let remote = Provider::remote("remote-provider-a");
    let mut p = PrivacyPolicy::default();
    p.grant("remote-provider-a", meeting_core::protocol::DataKind::Text);
    assert_eq!(p, {
        let mut q = PrivacyPolicy::local_only();
        q.grant("remote-provider-a", meeting_core::protocol::DataKind::Text);
        q
    });
    assert_eq!(
        p.check(&remote, meeting_core::protocol::DataKind::Text),
        Err(EgressDenied::LocalOnly("remote-provider-a".into()))
    );
}

#[test]
fn product_names_are_absent_from_sources_and_fixtures() {
    let needle = ["open", "ai"].concat();
    let files: [(&str, &str); 4] = [
        ("fixtures/protocol/session_example.jsonl", FIXTURE),
        ("src/policy.rs", include_str!("../src/policy.rs")),
        ("src/coordinator.rs", include_str!("../src/coordinator.rs")),
        (
            "tests/replay_and_rules.rs",
            include_str!("replay_and_rules.rs"),
        ),
    ];
    for (name, text) in files {
        assert!(
            !text.to_ascii_lowercase().contains(&needle),
            "{name} names a real product"
        );
    }
}

#[test]
fn crate_doc_states_the_must_never_rule() {
    let lib = include_str!("../src/lib.rs");
    let head: String = lib
        .lines()
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(head.contains("Must never"), "lib.rs doc: {head}");
    for part in [
        "store audio",
        "rewrite a final line",
        "leave the Mac by default",
    ] {
        assert!(head.contains(part), "missing {part:?}");
    }
}

#[test]
fn translation_id_cannot_move_to_another_source_segment() {
    let mut view = SessionView::new("session_t");
    applied(&mut view, 1, &seg_at("seg_a", 1, 0, Language::En));
    applied(&mut view, 2, &seg_at("seg_b", 1, 10, Language::En));
    let first = tr_of("tr_1", "seg_a", 1, TranslationStatus::Partial, "một");
    assert_eq!(applied(&mut view, 3, &first), Applied::Applied);
    let moved = tr_of("tr_1", "seg_b", 2, TranslationStatus::Partial, "hai");
    assert!(matches!(
        applied(&mut view, 4, &moved),
        Applied::Rejected(_)
    ));
    let caps = view.captions();
    assert_eq!(caps[0].target_text.as_deref(), Some("một"));
    assert_eq!(caps[1].target_text, None);
}
