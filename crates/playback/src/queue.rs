//! The single ordered playback queue.
//!
//! Segments play strictly in enqueue order and each id plays once. Speed-up
//! is the only tool for catching up; nothing is cut, skipped or reordered
//! while playback runs. When the output device is unplugged the queue is
//! held; if the hold lasts `HOLD_TIMEOUT_MS`, the remaining segments are
//! marked `NotPlayed`. The clock is always a parameter (milliseconds).
//!
//! Which side of the audio callback: this module is CONTROL SIDE ONLY. The
//! queue allocates (`enqueue` boxes an id, the maps grow) and is not
//! thread-safe, so a real-time audio callback must never touch it. The control
//! thread drains it into pre-allocated sample buffers that the callback reads
//! (guardrail 8).
//!
//! A segment is marked `Played` only after the player accepted it
//! (`play_next`, or `peek_next` then `mark_played`). A failing player leaves
//! the segment at the front, so a play error never drops speech.

use crate::error::PlaybackError;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Highest pitch-preserving speed-up.
pub const MAX_SPEED_FACTOR: f32 = 1.15;
/// Backlog below this plays at normal speed. Starting value.
pub const SPEEDUP_START_BACKLOG_MS: u32 = 2_000;
/// Backlog at which the speed-up reaches its cap. Starting value.
pub const SPEEDUP_FULL_BACKLOG_MS: u32 = 6_000;
/// Most segments the queue holds at once. `enqueue` refuses the next one with
/// `PlaybackError::QueueFull`. Starting value: far above a normal backlog
/// (the speed-up already reaches its cap at 6 s), so a refusal means the
/// player has stalled and the caller must react.
pub const MAX_QUEUED_SEGMENTS: usize = 4_096;
/// Hold duration after which remaining segments are marked `NotPlayed`.
pub const HOLD_TIMEOUT_MS: u64 = 30_000;
/// How many finished (played or not played) ids are remembered so a late
/// duplicate is still ignored. Starting value.
pub const PLAYED_ID_MEMORY: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Partner's translated voice, played to the owner.
    Incoming,
    /// Owner's translated voice, sent into the meeting.
    Outgoing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub segment_id: String,
    pub direction: Direction,
    pub samples_len_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentState {
    Queued,
    Played,
    NotPlayed,
}

/// Plays one segment at a speed factor. A real backend comes later.
///
/// Contract: `play` returns `Ok` only after the player took the whole sentence,
/// because the queue then marks it played and never offers it again. After an
/// error, or a hold shorter than `HOLD_TIMEOUT_MS`, the queue offers the same
/// sentence again, to be played from its start.
pub trait SegmentPlayer {
    fn play(&mut self, segment: &Segment, speed: f32) -> Result<(), PlaybackError>;
}

/// Placeholder player until a real backend exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullPlayer;

impl SegmentPlayer for NullPlayer {
    fn play(&mut self, _segment: &Segment, _speed: f32) -> Result<(), PlaybackError> {
        Err(PlaybackError::NotImplemented)
    }
}

/// Speed-up for a given backlog: 1.0 up to the start threshold, then linear
/// up to `MAX_SPEED_FACTOR` at the full threshold, never above the cap.
pub fn speed_factor(backlog_ms: u32) -> f32 {
    if backlog_ms <= SPEEDUP_START_BACKLOG_MS {
        return 1.0;
    }
    let span = (SPEEDUP_FULL_BACKLOG_MS - SPEEDUP_START_BACKLOG_MS) as f32;
    let over = (backlog_ms - SPEEDUP_START_BACKLOG_MS) as f32;
    let t = (over / span).min(1.0);
    (1.0 + (MAX_SPEED_FACTOR - 1.0) * t).min(MAX_SPEED_FACTOR)
}

/// A queued segment with its shared id key, allocated once at enqueue.
#[derive(Debug)]
struct Queued {
    segment: Segment,
    key: Arc<str>,
}

#[derive(Debug, Default)]
pub struct PlaybackQueue {
    queue: VecDeque<Queued>,
    /// Every queued id, plus the last `PLAYED_ID_MEMORY` finished ids.
    states: HashMap<Arc<str>, SegmentState>,
    /// Finished ids, oldest first; the front is forgotten past the limit.
    finished: VecDeque<Arc<str>>,
    /// Sum of `samples_len_ms` of the queued segments. Wide so it cannot wrap.
    backlog_ms: u64,
    held_since_ms: Option<u64>,
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a segment at the back. `Ok(true)` when queued, `Ok(false)` when the
    /// id was enqueued before and is still remembered (every queued id, and the
    /// last `PLAYED_ID_MEMORY` finished ids): the duplicate is ignored. When
    /// `MAX_QUEUED_SEGMENTS` are already queued, the call fails with
    /// `PlaybackError::QueueFull` and the segment is not stored.
    pub fn enqueue(&mut self, segment: Segment) -> Result<bool, PlaybackError> {
        if self.states.contains_key(segment.segment_id.as_str()) {
            return Ok(false);
        }
        if self.queue.len() >= MAX_QUEUED_SEGMENTS {
            return Err(PlaybackError::QueueFull {
                max: MAX_QUEUED_SEGMENTS,
            });
        }
        let key: Arc<str> = Arc::from(segment.segment_id.as_str());
        self.states.insert(Arc::clone(&key), SegmentState::Queued);
        self.backlog_ms = self
            .backlog_ms
            .saturating_add(u64::from(segment.samples_len_ms));
        self.queue.push_back(Queued { segment, key });
        Ok(true)
    }

    /// The next segment in order, still queued. `None` while the queue is held
    /// or empty. Call `mark_played` once the player has taken it.
    pub fn peek_next(&self) -> Option<&Segment> {
        if self.is_held() {
            return None;
        }
        self.queue.front().map(|q| &q.segment)
    }

    /// Marks the front segment `Played` and removes it. Fails with
    /// `NotNextSegment` when `segment_id` is not at the front, so nothing is
    /// skipped or reordered. Works while the queue is held: the segment was
    /// already playing when the hold began.
    pub fn mark_played(&mut self, segment_id: &str) -> Result<(), PlaybackError> {
        match self.queue.front() {
            Some(front) if front.segment.segment_id == segment_id => {}
            _ => return Err(PlaybackError::NotNextSegment(segment_id.to_string())),
        }
        let Some(Queued { segment, key }) = self.queue.pop_front() else {
            return Err(PlaybackError::NotNextSegment(segment_id.to_string()));
        };
        self.backlog_ms = self
            .backlog_ms
            .saturating_sub(u64::from(segment.samples_len_ms));
        self.finish(key, SegmentState::Played);
        Ok(())
    }

    /// Plays the next segment at the current speed and marks it played only
    /// when the player succeeded. `Ok(None)` while held or empty. On a player
    /// error the segment stays at the front, still `Queued`, and the error is
    /// returned so the caller can retry or hold.
    pub fn play_next(
        &mut self,
        player: &mut dyn SegmentPlayer,
    ) -> Result<Option<Segment>, PlaybackError> {
        let speed = self.current_speed();
        let Some(segment) = self.peek_next().cloned() else {
            return Ok(None);
        };
        player.play(&segment, speed)?;
        self.mark_played(&segment.segment_id)?;
        Ok(Some(segment))
    }

    /// Records the final state of a segment and forgets the oldest finished
    /// ids past the memory limit.
    fn finish(&mut self, key: Arc<str>, state: SegmentState) {
        self.states.insert(Arc::clone(&key), state);
        self.finished.push_back(key);
        while self.finished.len() > PLAYED_ID_MEMORY {
            if let Some(old) = self.finished.pop_front() {
                self.states.remove(&old);
            }
        }
    }

    /// Number of segment ids the queue still remembers.
    pub fn remembered_ids(&self) -> usize {
        self.states.len()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Total length of the queued segments, capped at `u32::MAX`.
    pub fn backlog_ms(&self) -> u32 {
        u32::try_from(self.backlog_ms).unwrap_or(u32::MAX)
    }

    /// Speed factor for the current backlog.
    pub fn current_speed(&self) -> f32 {
        speed_factor(self.backlog_ms())
    }

    pub fn state(&self, segment_id: &str) -> Option<SegmentState> {
        self.states.get(segment_id).copied()
    }

    pub fn is_held(&self) -> bool {
        self.held_since_ms.is_some()
    }

    /// Keeps the queue as is, for example after the output is unplugged.
    /// A second call keeps the original hold time.
    pub fn hold(&mut self, now_ms: u64) {
        self.held_since_ms.get_or_insert(now_ms);
    }

    /// Applies the hold timeout. Returns how many segments were marked
    /// `NotPlayed`. After an expiry the hold stays on and its timer restarts
    /// at `now_ms`.
    pub fn expire_hold(&mut self, now_ms: u64) -> usize {
        let Some(since) = self.held_since_ms else {
            return 0;
        };
        if now_ms.saturating_sub(since) < HOLD_TIMEOUT_MS {
            return 0;
        }
        let dropped = self.queue.len();
        while let Some(Queued { key, .. }) = self.queue.pop_front() {
            self.finish(key, SegmentState::NotPlayed);
        }
        self.backlog_ms = 0;
        // The drained hold is over; a segment enqueued from now on starts a
        // new hold from this moment, so a late resume does not drop it.
        self.held_since_ms = Some(now_ms);
        dropped
    }

    /// Ends the hold. The timeout is applied first, so a hold that already
    /// lasted `HOLD_TIMEOUT_MS` cannot play stale segments.
    pub fn resume(&mut self, now_ms: u64) -> usize {
        let dropped = self.expire_hold(now_ms);
        self.held_since_ms = None;
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: &str, ms: u32) -> Segment {
        Segment {
            segment_id: id.into(),
            direction: Direction::Incoming,
            samples_len_ms: ms,
        }
    }

    /// Peek, then mark played: what a player that always succeeds does.
    fn take_next(q: &mut PlaybackQueue) -> Option<Segment> {
        let segment = q.peek_next()?.clone();
        q.mark_played(&segment.segment_id).unwrap();
        Some(segment)
    }

    fn drain_ids(q: &mut PlaybackQueue) -> Vec<String> {
        std::iter::from_fn(|| take_next(q))
            .map(|s| s.segment_id)
            .collect()
    }

    #[test]
    fn order_is_kept() {
        let mut q = PlaybackQueue::new();
        for id in ["c", "a", "b"] {
            assert!(q.enqueue(seg(id, 100)).unwrap());
        }
        assert_eq!(drain_ids(&mut q), vec!["c", "a", "b"]);
    }

    #[test]
    fn each_segment_plays_once() {
        let mut q = PlaybackQueue::new();
        assert!(q.enqueue(seg("a", 100)).unwrap());
        assert!(!q.enqueue(seg("a", 100)).unwrap());
        assert_eq!(q.len(), 1);
        assert_eq!(drain_ids(&mut q), vec!["a"]);
        // Even after it played, the same id is ignored.
        assert!(!q.enqueue(seg("a", 100)).unwrap());
        assert!(q.is_empty());
        assert_eq!(q.state("a"), Some(SegmentState::Played));
    }

    #[test]
    fn speed_is_normal_for_small_backlog() {
        assert_eq!(speed_factor(0), 1.0);
        assert_eq!(speed_factor(SPEEDUP_START_BACKLOG_MS), 1.0);
    }

    #[test]
    fn speed_grows_with_backlog() {
        let a = speed_factor(3_000);
        let b = speed_factor(5_000);
        assert!(a > 1.0);
        assert!(b > a);
    }

    #[test]
    fn speed_is_capped_at_1_15() {
        assert_eq!(speed_factor(SPEEDUP_FULL_BACKLOG_MS), 1.15);
        assert_eq!(speed_factor(60_000), 1.15);
        assert_eq!(speed_factor(u32::MAX), 1.15);
    }

    #[test]
    fn queue_speed_follows_backlog() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 4_000)).unwrap();
        q.enqueue(seg("b", 4_000)).unwrap();
        assert_eq!(q.backlog_ms(), 8_000);
        assert_eq!(q.current_speed(), 1.15);
    }

    #[test]
    fn hold_keeps_the_queue() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(1_000);
        assert!(q.is_held());
        assert!(take_next(&mut q).is_none());
        assert_eq!(q.len(), 1);
        assert_eq!(q.resume(2_000), 0);
        assert_eq!(drain_ids(&mut q), vec!["a"]);
    }

    #[test]
    fn hold_under_30s_does_not_expire() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(1_000);
        assert_eq!(q.expire_hold(1_000 + HOLD_TIMEOUT_MS - 1), 0);
        assert_eq!(q.state("a"), Some(SegmentState::Queued));
    }

    #[test]
    fn hold_of_30s_marks_remaining_not_played() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.enqueue(seg("b", 100)).unwrap();
        q.hold(1_000);
        assert_eq!(q.expire_hold(1_000 + HOLD_TIMEOUT_MS), 2);
        assert!(q.is_empty());
        assert_eq!(q.state("a"), Some(SegmentState::NotPlayed));
        assert_eq!(q.state("b"), Some(SegmentState::NotPlayed));
    }

    #[test]
    fn resume_after_timeout_does_not_play_stale_segments() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(0);
        assert_eq!(q.resume(HOLD_TIMEOUT_MS + 5), 1);
        assert!(!q.is_held());
        assert!(take_next(&mut q).is_none());
        assert_eq!(q.state("a"), Some(SegmentState::NotPlayed));
    }

    #[test]
    fn second_hold_keeps_the_first_hold_time() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(0);
        q.hold(20_000);
        assert_eq!(q.expire_hold(HOLD_TIMEOUT_MS), 1);
    }

    #[test]
    fn expire_without_hold_does_nothing() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        assert_eq!(q.expire_hold(1_000_000), 0);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn segment_enqueued_after_expiry_plays_after_resume() {
        let mut q = PlaybackQueue::new();
        q.hold(0);
        assert_eq!(q.expire_hold(31_000), 0);
        assert!(q.enqueue(seg("b", 100)).unwrap());
        // "b" has waited 5 s of a new hold, not 36 s of the old one.
        assert_eq!(q.resume(36_000), 0);
        assert_eq!(drain_ids(&mut q), vec!["b"]);
        assert_eq!(q.state("b"), Some(SegmentState::Played));
    }

    #[test]
    fn hold_that_lasts_on_after_expiry_expires_again_after_30s() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(0);
        assert_eq!(q.expire_hold(31_000), 1);
        q.enqueue(seg("b", 100)).unwrap();
        assert_eq!(q.expire_hold(31_000 + HOLD_TIMEOUT_MS - 1), 0);
        assert_eq!(q.expire_hold(31_000 + HOLD_TIMEOUT_MS), 1);
        assert_eq!(q.state("b"), Some(SegmentState::NotPlayed));
    }

    #[test]
    fn speed_just_over_start_threshold_is_barely_above_one() {
        let s = speed_factor(SPEEDUP_START_BACKLOG_MS + 1);
        assert!(s > 1.0);
        assert!(s < 1.001);
    }

    #[test]
    fn speed_just_under_full_threshold_stays_below_cap() {
        let s = speed_factor(SPEEDUP_FULL_BACKLOG_MS - 1);
        assert!(s > 1.149);
        assert!(s < MAX_SPEED_FACTOR);
    }

    #[test]
    fn backlog_follows_enqueue_and_dequeue() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 1_000)).unwrap();
        q.enqueue(seg("b", 2_500)).unwrap();
        assert_eq!(q.backlog_ms(), 3_500);
        take_next(&mut q);
        assert_eq!(q.backlog_ms(), 2_500);
        // A refused duplicate adds nothing.
        q.enqueue(seg("b", 9_000)).unwrap();
        assert_eq!(q.backlog_ms(), 2_500);
        take_next(&mut q);
        assert_eq!(q.backlog_ms(), 0);
    }

    #[test]
    fn backlog_drops_to_zero_when_hold_expires() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 1_000)).unwrap();
        q.hold(0);
        q.expire_hold(HOLD_TIMEOUT_MS);
        assert_eq!(q.backlog_ms(), 0);
        assert_eq!(q.current_speed(), 1.0);
    }

    #[test]
    fn played_id_memory_is_bounded() {
        let mut q = PlaybackQueue::new();
        let total = PLAYED_ID_MEMORY + 50;
        for i in 0..total {
            assert!(q.enqueue(seg(&format!("s{i}"), 10)).unwrap());
            assert!(take_next(&mut q).is_some());
        }
        assert_eq!(q.remembered_ids(), PLAYED_ID_MEMORY);
        // The oldest ids were forgotten; the newest are still known.
        assert_eq!(q.state("s0"), None);
        assert_eq!(
            q.state(&format!("s{}", total - 1)),
            Some(SegmentState::Played)
        );
    }

    #[test]
    fn recently_played_id_is_still_ignored_after_pruning() {
        let mut q = PlaybackQueue::new();
        for i in 0..(PLAYED_ID_MEMORY + 50) {
            q.enqueue(seg(&format!("s{i}"), 10)).unwrap();
            take_next(&mut q);
        }
        let recent = format!("s{}", PLAYED_ID_MEMORY + 49);
        assert!(!q.enqueue(seg(&recent, 10)).unwrap());
        assert!(q.is_empty());
    }

    #[test]
    fn queued_ids_are_never_pruned() {
        let mut q = PlaybackQueue::new();
        for i in 0..(PLAYED_ID_MEMORY + 50) {
            assert!(q.enqueue(seg(&format!("s{i}"), 1)).unwrap());
        }
        assert_eq!(q.len(), PLAYED_ID_MEMORY + 50);
        assert_eq!(q.state("s0"), Some(SegmentState::Queued));
        assert!(!q.enqueue(seg("s0", 1)).unwrap());
    }

    /// Fails the first `failures` calls, then succeeds; records every call.
    struct FlakyPlayer {
        failures: usize,
        calls: Vec<(String, f32)>,
    }

    impl SegmentPlayer for FlakyPlayer {
        fn play(&mut self, segment: &Segment, speed: f32) -> Result<(), PlaybackError> {
            self.calls.push((segment.segment_id.clone(), speed));
            if self.failures > 0 {
                self.failures -= 1;
                return Err(PlaybackError::NotImplemented);
            }
            Ok(())
        }
    }

    fn flaky(failures: usize) -> FlakyPlayer {
        FlakyPlayer {
            failures,
            calls: Vec::new(),
        }
    }

    #[test]
    fn a_play_error_keeps_the_segment_at_the_front_and_queued() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 1_000)).unwrap();
        q.enqueue(seg("b", 1_000)).unwrap();
        let mut player = flaky(1);

        assert_eq!(q.play_next(&mut player), Err(PlaybackError::NotImplemented));

        assert_eq!(q.len(), 2, "nothing was removed");
        assert_eq!(q.state("a"), Some(SegmentState::Queued));
        assert_eq!(q.backlog_ms(), 2_000);
        assert_eq!(q.peek_next().map(|s| s.segment_id.as_str()), Some("a"));

        // The retry plays the same segment first, then the next one in order.
        assert_eq!(q.play_next(&mut player).unwrap().unwrap().segment_id, "a");
        assert_eq!(q.play_next(&mut player).unwrap().unwrap().segment_id, "b");
        let called: Vec<&str> = player.calls.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(called, ["a", "a", "b"]);
        assert_eq!(q.state("a"), Some(SegmentState::Played));
        assert_eq!(q.state("b"), Some(SegmentState::Played));
    }

    /// Keeps every segment it was given.
    #[derive(Default)]
    struct KeepingPlayer(Vec<Segment>);

    impl SegmentPlayer for KeepingPlayer {
        fn play(&mut self, segment: &Segment, _speed: f32) -> Result<(), PlaybackError> {
            self.0.push(segment.clone());
            Ok(())
        }
    }

    #[test]
    fn a_later_version_of_a_queued_sentence_is_refused_and_never_spoken() {
        let mut q = PlaybackQueue::new();
        let mut player = KeepingPlayer::default();
        assert_eq!(q.enqueue(seg("s1", 1_200)), Ok(true));

        // Before the first version is offered.
        assert_eq!(q.enqueue(seg("s1", 1_500)), Ok(false));
        assert_eq!(q.backlog_ms(), 1_200);

        // While the first version is offered.
        assert_eq!(q.peek_next(), Some(&seg("s1", 1_200)));
        assert_eq!(q.enqueue(seg("s1", 1_500)), Ok(false));
        assert_eq!(q.backlog_ms(), 1_200);

        // Right after it was played.
        q.play_next(&mut player).unwrap();
        assert_eq!(q.enqueue(seg("s1", 1_500)), Ok(false));
        assert_eq!(q.backlog_ms(), 0);

        assert_eq!(q.play_next(&mut player), Ok(None));
        assert_eq!(player.0, vec![seg("s1", 1_200)]);
    }

    #[test]
    fn a_sentence_interrupted_by_a_short_hold_is_offered_again_first() {
        let mut q = PlaybackQueue::new();
        for (id, ms) in [("a", 1_000), ("b", 2_000), ("c", 3_000)] {
            q.enqueue(seg(id, ms)).unwrap();
        }
        // "a" is offered and playing when the output goes away.
        assert_eq!(q.peek_next(), Some(&seg("a", 1_000)));
        q.hold(5_000);

        assert_eq!(q.resume(5_000 + HOLD_TIMEOUT_MS - 1), 0);

        assert_eq!(q.peek_next(), Some(&seg("a", 1_000)));
        for id in ["a", "b", "c"] {
            assert_eq!(q.state(id), Some(SegmentState::Queued), "{id}");
        }
        assert_eq!(drain_ids(&mut q), vec!["a", "b", "c"]);
    }

    /// Two segments, the first one offered, held since 1 000 ms.
    fn offered_and_held() -> PlaybackQueue {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.enqueue(seg("b", 100)).unwrap();
        assert_eq!(q.peek_next(), Some(&seg("a", 100)));
        q.hold(1_000);
        q
    }

    #[test]
    fn the_hold_limit_drops_the_offered_sentence_through_expiry_and_resume() {
        let limit = 1_000 + HOLD_TIMEOUT_MS;

        let mut expired = offered_and_held();
        assert_eq!(expired.expire_hold(limit - 1), 0);
        assert_eq!(expired.state("a"), Some(SegmentState::Queued));
        assert_eq!(expired.state("b"), Some(SegmentState::Queued));
        assert_eq!(expired.expire_hold(limit), 2);
        assert_eq!(expired.resume(limit + 1), 0);

        let mut resumed = offered_and_held();
        assert_eq!(resumed.resume(limit), 2);

        for q in [expired, resumed] {
            assert_eq!(q.state("a"), Some(SegmentState::NotPlayed));
            assert_eq!(q.state("b"), Some(SegmentState::NotPlayed));
            assert!(!q.is_held());
            assert_eq!(q.peek_next(), None);
        }
    }

    #[test]
    fn a_sentence_that_keeps_failing_is_offered_again_and_never_skipped() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.enqueue(seg("b", 100)).unwrap();
        let mut player = flaky(3);

        for _ in 0..3 {
            assert_eq!(q.play_next(&mut player), Err(PlaybackError::NotImplemented));
        }

        let called: Vec<&str> = player.calls.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(called, ["a", "a", "a"]);
        assert_eq!(q.state("a"), Some(SegmentState::Queued));
        assert_eq!(q.state("b"), Some(SegmentState::Queued));
    }

    #[test]
    fn play_next_passes_the_current_speed_to_the_player() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 4_000)).unwrap();
        q.enqueue(seg("b", 4_000)).unwrap();
        let mut player = flaky(0);
        q.play_next(&mut player).unwrap();
        assert_eq!(player.calls, vec![("a".to_string(), MAX_SPEED_FACTOR)]);
    }

    #[test]
    fn play_next_does_nothing_while_held_or_empty() {
        let mut q = PlaybackQueue::new();
        let mut player = flaky(0);
        assert_eq!(q.play_next(&mut player), Ok(None));
        q.enqueue(seg("a", 100)).unwrap();
        q.hold(0);
        assert_eq!(q.play_next(&mut player), Ok(None));
        assert!(
            player.calls.is_empty(),
            "a held queue never calls the player"
        );
    }

    #[test]
    fn peek_does_not_mark_the_segment_played() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        assert_eq!(q.peek_next().map(|s| s.segment_id.as_str()), Some("a"));
        assert_eq!(q.peek_next().map(|s| s.segment_id.as_str()), Some("a"));
        assert_eq!(q.state("a"), Some(SegmentState::Queued));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn mark_played_refuses_a_segment_that_is_not_at_the_front() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        q.enqueue(seg("b", 100)).unwrap();
        assert_eq!(
            q.mark_played("b"),
            Err(PlaybackError::NotNextSegment("b".into()))
        );
        assert_eq!(
            q.mark_played("zzz"),
            Err(PlaybackError::NotNextSegment("zzz".into()))
        );
        assert_eq!(q.len(), 2, "nothing was skipped");
        q.mark_played("a").unwrap();
        assert_eq!(
            q.mark_played("a"),
            Err(PlaybackError::NotNextSegment("a".into())),
            "a segment is marked once"
        );
    }

    #[test]
    fn mark_played_after_a_hold_expired_the_segment_is_refused() {
        let mut q = PlaybackQueue::new();
        q.enqueue(seg("a", 100)).unwrap();
        let playing = q.peek_next().unwrap().segment_id.clone();
        q.hold(0);
        q.expire_hold(HOLD_TIMEOUT_MS);
        assert_eq!(
            q.mark_played(&playing),
            Err(PlaybackError::NotNextSegment("a".into()))
        );
        assert_eq!(q.state("a"), Some(SegmentState::NotPlayed));
    }

    #[test]
    fn enqueue_refuses_the_segment_after_the_cap_and_keeps_the_queue_intact() {
        let mut q = PlaybackQueue::new();
        for i in 0..MAX_QUEUED_SEGMENTS {
            assert_eq!(q.enqueue(seg(&format!("s{i}"), 1)), Ok(true));
        }
        assert_eq!(q.len(), MAX_QUEUED_SEGMENTS);

        assert_eq!(
            q.enqueue(seg("overflow", 1)),
            Err(PlaybackError::QueueFull {
                max: MAX_QUEUED_SEGMENTS
            })
        );
        assert_eq!(q.len(), MAX_QUEUED_SEGMENTS);
        assert_eq!(q.state("overflow"), None, "a refused id is not remembered");

        // A duplicate of a queued id is still a quiet duplicate, not a refusal.
        assert_eq!(q.enqueue(seg("s0", 1)), Ok(false));

        // Playing one segment frees one slot, and the refused segment fits.
        let mut player = flaky(0);
        q.play_next(&mut player).unwrap();
        assert_eq!(q.enqueue(seg("overflow", 1)), Ok(true));
        assert_eq!(q.len(), MAX_QUEUED_SEGMENTS);
    }

    #[test]
    fn backlog_does_not_wrap_at_the_extremes() {
        let mut q = PlaybackQueue::new();
        for i in 0..3 {
            q.enqueue(seg(&format!("s{i}"), u32::MAX)).unwrap();
        }
        assert_eq!(q.backlog_ms(), u32::MAX);
        assert_eq!(q.current_speed(), MAX_SPEED_FACTOR);
        while take_next(&mut q).is_some() {}
        assert_eq!(q.backlog_ms(), 0);
    }

    #[test]
    fn placeholder_player_is_not_implemented() {
        let mut p = NullPlayer;
        assert_eq!(
            p.play(&seg("a", 1), 1.0),
            Err(PlaybackError::NotImplemented)
        );
    }
}
