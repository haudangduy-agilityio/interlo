//! Segmenter output feeds the playback queue.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use engine_host::{segment, Piece, Segmenter};
use playback::{
    Direction, PlaybackQueue, Segment, SegmentState, HOLD_TIMEOUT_MS, MAX_SPEED_FACTOR,
};

/// Turn each segmenter output into a queue segment. The id is its position, so
/// the order in the queue is checkable.
fn to_segments(pieces: &[Piece]) -> Vec<Segment> {
    segment(pieces)
        .iter()
        .enumerate()
        .map(|(i, p)| Segment {
            segment_id: format!("seg_{i}"),
            direction: Direction::Incoming,
            samples_len_ms: u32::try_from(p.end_ms - p.start_ms).unwrap_or(u32::MAX),
        })
        .collect()
}

/// Sentences separated by a pause that always cuts: each piece ends with a mark
/// and the next one starts 300 ms later.
fn sentence_pieces(count: u64, len_ms: u64) -> Vec<Piece> {
    (0..count)
        .map(|i| {
            let start = i * (len_ms + 300);
            Piece::new(format!("Sentence {i}."), start, start + len_ms)
        })
        .collect()
}

/// Peek, then mark played: what a player that always succeeds does.
fn take_next(queue: &mut PlaybackQueue) -> Option<Segment> {
    let segment = queue.peek_next()?.clone();
    queue.mark_played(&segment.segment_id).unwrap();
    Some(segment)
}

fn fill(queue: &mut PlaybackQueue, segments: Vec<Segment>) {
    for s in segments {
        assert!(queue.enqueue(s).unwrap());
    }
}

#[test]
fn segment_order_is_kept_from_segmenter_to_playback() {
    let segments = to_segments(&sentence_pieces(5, 1_000));
    assert_eq!(segments.len(), 5);
    let mut queue = PlaybackQueue::new();
    fill(&mut queue, segments);

    let played: Vec<String> = std::iter::from_fn(|| take_next(&mut queue))
        .map(|s| s.segment_id)
        .collect();
    assert_eq!(played, ["seg_0", "seg_1", "seg_2", "seg_3", "seg_4"]);
}

#[test]
fn a_segment_plays_once_even_when_the_segmenter_output_is_fed_twice() {
    let mut queue = PlaybackQueue::new();
    fill(&mut queue, to_segments(&sentence_pieces(3, 1_000)));
    for s in to_segments(&sentence_pieces(3, 1_000)) {
        assert!(!queue.enqueue(s).unwrap(), "same id must be ignored");
    }
    assert_eq!(queue.len(), 3);

    let first_pass = std::iter::from_fn(|| take_next(&mut queue)).count();
    assert_eq!(first_pass, 3);

    // After playing, the same ids are still refused and nothing plays again.
    for s in to_segments(&sentence_pieces(3, 1_000)) {
        assert!(!queue.enqueue(s).unwrap());
    }
    assert!(take_next(&mut queue).is_none());
    assert_eq!(queue.state("seg_1"), Some(SegmentState::Played));
}

#[test]
fn backlog_speed_never_exceeds_the_cap_while_draining() {
    // 30 segments of 4 s each: a backlog far above the full speed-up threshold.
    let mut queue = PlaybackQueue::new();
    fill(&mut queue, to_segments(&sentence_pieces(30, 4_000)));
    assert!(queue.backlog_ms() > 100_000);

    let mut steps = 0;
    let mut saw_speed_up = false;
    while !queue.is_empty() {
        let speed = queue.current_speed();
        assert!(speed >= 1.0, "speed {speed} is below normal");
        assert!(
            speed <= MAX_SPEED_FACTOR,
            "speed {speed} is above {MAX_SPEED_FACTOR}"
        );
        saw_speed_up |= speed > 1.0;
        assert!(take_next(&mut queue).is_some());
        steps += 1;
    }
    assert_eq!(steps, 30);
    assert!(saw_speed_up, "a large backlog must speed playback up");
}

#[test]
fn the_speed_cap_is_1_15() {
    // Pinned once, as the decided value (FR-60 to FR-64).
    assert_eq!(MAX_SPEED_FACTOR, 1.15);
}

#[test]
fn a_huge_backlog_runs_at_the_cap_and_a_small_one_at_normal_speed() {
    let mut big = PlaybackQueue::new();
    fill(&mut big, to_segments(&sentence_pieces(30, 4_000)));
    assert_eq!(big.current_speed(), MAX_SPEED_FACTOR);

    let mut small = PlaybackQueue::new();
    fill(&mut small, to_segments(&sentence_pieces(1, 1_000)));
    assert_eq!(small.current_speed(), 1.0);
}

#[test]
fn unplug_hold_marks_the_rest_not_played_after_30_seconds() {
    let mut queue = PlaybackQueue::new();
    fill(&mut queue, to_segments(&sentence_pieces(4, 1_000)));

    // One segment plays, then the output is unplugged at a fake time of 10 s.
    assert_eq!(
        take_next(&mut queue).map(|s| s.segment_id).as_deref(),
        Some("seg_0")
    );
    let unplugged_at = 10_000;
    queue.hold(unplugged_at);
    assert!(
        take_next(&mut queue).is_none(),
        "a held queue plays nothing"
    );

    // One millisecond short of the timeout: the rest is still waiting.
    assert_eq!(queue.expire_hold(unplugged_at + HOLD_TIMEOUT_MS - 1), 0);
    assert_eq!(queue.state("seg_1"), Some(SegmentState::Queued));

    // At the timeout the rest is marked not played, the played one is untouched.
    assert_eq!(queue.expire_hold(unplugged_at + HOLD_TIMEOUT_MS), 3);
    assert_eq!(queue.state("seg_0"), Some(SegmentState::Played));
    for id in ["seg_1", "seg_2", "seg_3"] {
        assert_eq!(queue.state(id), Some(SegmentState::NotPlayed), "{id}");
    }
    assert!(queue.is_empty());

    // Plugging back in does not bring the stale segments back.
    assert_eq!(queue.resume(unplugged_at + HOLD_TIMEOUT_MS + 1), 0);
    assert!(take_next(&mut queue).is_none());
}

#[test]
fn a_segment_enqueued_after_a_hold_expired_still_plays_after_resume() {
    let mut queue = PlaybackQueue::new();
    fill(&mut queue, to_segments(&sentence_pieces(1, 1_000)));
    queue.hold(0);
    assert_eq!(queue.expire_hold(HOLD_TIMEOUT_MS + 1_000), 1);

    // A fresh sentence arrives one second after the expiry; the output comes
    // back five seconds after it. The new segment is not stale.
    let late = Segment {
        segment_id: "late".into(),
        direction: Direction::Incoming,
        samples_len_ms: 1_000,
    };
    assert!(queue.enqueue(late).unwrap());
    assert_eq!(queue.resume(HOLD_TIMEOUT_MS + 6_000), 0);
    assert_eq!(
        take_next(&mut queue).map(|s| s.segment_id).as_deref(),
        Some("late")
    );
    assert_eq!(queue.state("seg_0"), Some(SegmentState::NotPlayed));
}

#[test]
fn the_last_sentence_before_silence_reaches_playback_without_a_next_piece() {
    let mut segmenter = Segmenter::new();
    let mut queue = PlaybackQueue::new();
    let mut next_id = 0;
    let mut enqueue = |queue: &mut PlaybackQueue, p: Piece| {
        let segment = Segment {
            segment_id: format!("seg_{next_id}"),
            direction: Direction::Incoming,
            samples_len_ms: u32::try_from(p.end_ms - p.start_ms).unwrap_or(u32::MAX),
        };
        next_id += 1;
        assert!(queue.enqueue(segment).unwrap());
    };

    for p in sentence_pieces(2, 1_000) {
        if let Some(closed) = segmenter.push(p) {
            enqueue(&mut queue, closed);
        }
    }
    // The second sentence (ends at 2_300 ms) is still open: nothing follows it.
    assert_eq!(queue.len(), 1);
    assert!(segmenter.flush(2_300 + 249).is_none());
    enqueue(&mut queue, segmenter.flush(2_300 + 250).unwrap());

    let played: Vec<String> = std::iter::from_fn(|| take_next(&mut queue))
        .map(|s| s.segment_id)
        .collect();
    assert_eq!(played, ["seg_0", "seg_1"]);
}
