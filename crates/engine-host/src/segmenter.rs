//! Sentence segmentation over recognized pieces. Pure; no clock, no state.
//!
//! Rules, in order, for the gap `pause` between the current segment and the
//! next piece (a pause is a gap greater than zero):
//! 1. Force a cut when the current segment is at least `MAX_SEGMENT_MS` long
//!    and there is a pause.
//! 2. If the current text ends with a sentence-final mark, cut once the pause
//!    reaches `END_OF_SENTENCE_WAIT_MS`; before that the sentence is still open.
//! 3. Otherwise merge when the pause is under `MERGE_PAUSE_MS`, else cut.
//!
//! The constants are starting values and are expected to be tuned.

/// Wait after a sentence-final mark before the sentence closes.
pub const END_OF_SENTENCE_WAIT_MS: u64 = 250;
/// Pauses shorter than this join a piece with no final mark to the next one.
pub const MERGE_PAUSE_MS: u64 = 600;
/// Longest segment before a cut is forced at the next pause.
pub const MAX_SEGMENT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl Piece {
    pub fn new(text: impl Into<String>, start_ms: u64, end_ms: u64) -> Self {
        Self {
            text: text.into(),
            start_ms,
            end_ms,
        }
    }

    fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// True when the text ends with `.`, `!`, `?` or an ellipsis, ignoring
/// trailing closing quotes and brackets.
pub fn has_sentence_final_mark(text: &str) -> bool {
    let trimmed = text.trim_end_matches(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '\'' | ')' | ']' | '\u{201D}' | '\u{2019}')
    });
    matches!(trimmed.chars().last(), Some('.' | '!' | '?' | '\u{2026}'))
}

fn should_cut(current: &Piece, next: &Piece) -> bool {
    let pause = next.start_ms.saturating_sub(current.end_ms);
    if current.duration_ms() >= MAX_SEGMENT_MS && pause > 0 {
        return true;
    }
    if has_sentence_final_mark(&current.text) {
        return pause >= END_OF_SENTENCE_WAIT_MS;
    }
    pause >= MERGE_PAUSE_MS
}

fn merge(current: &mut Piece, next: Piece) {
    current.text.push(' ');
    current.text.push_str(&next.text);
    current.end_ms = current.end_ms.max(next.end_ms);
}

/// Groups time-ordered pieces into sentence segments. Pieces with empty text
/// are dropped.
pub fn segment(pieces: &[Piece]) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut current: Option<Piece> = None;
    for p in pieces {
        let text = p.text.trim();
        if text.is_empty() {
            continue;
        }
        let next = Piece::new(text, p.start_ms, p.end_ms);
        current = Some(match current.take() {
            None => next,
            Some(mut cur) => {
                if should_cut(&cur, &next) {
                    out.push(cur);
                    next
                } else {
                    merge(&mut cur, next);
                    cur
                }
            }
        });
    }
    out.extend(current);
    out
}

/// Streaming form of [`segment`]: keeps the one open piece between calls.
///
/// `push` adds a piece and returns the sentence it closed, if any. `flush`
/// closes the open sentence when the silence after it has lasted long enough,
/// so the last sentence before a long silence is emitted without waiting for
/// the next piece. Both use the same cut rules as [`segment`].
#[derive(Debug, Default)]
pub struct Segmenter {
    open: Option<Piece>,
}

impl Segmenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The sentence that is still open, if any.
    pub fn open(&self) -> Option<&Piece> {
        self.open.as_ref()
    }

    /// Adds a piece. Returns the previous sentence when this piece closes it.
    /// A piece with empty text is dropped.
    pub fn push(&mut self, piece: Piece) -> Option<Piece> {
        let text = piece.text.trim();
        if text.is_empty() {
            return None;
        }
        let next = Piece::new(text, piece.start_ms, piece.end_ms);
        match self.open.take() {
            None => {
                self.open = Some(next);
                None
            }
            Some(mut cur) => {
                if should_cut(&cur, &next) {
                    self.open = Some(next);
                    Some(cur)
                } else {
                    merge(&mut cur, next);
                    self.open = Some(cur);
                    None
                }
            }
        }
    }

    /// Closes the open sentence when `now_ms` is far enough past its end: the
    /// same pause that would cut it if a next piece started at `now_ms`.
    pub fn flush(&mut self, now_ms: u64) -> Option<Piece> {
        let cur = self.open.as_ref()?;
        let silence_start = Piece::new("", now_ms, now_ms);
        if should_cut(cur, &silence_start) {
            self.open.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str, start: u64, end: u64) -> Piece {
        Piece::new(text, start, end)
    }

    #[test]
    fn constants_have_the_decided_starting_values() {
        assert_eq!(END_OF_SENTENCE_WAIT_MS, 250);
        assert_eq!(MERGE_PAUSE_MS, 600);
        assert_eq!(MAX_SEGMENT_MS, 10_000);
    }

    #[test]
    fn empty_input_gives_no_segments() {
        assert!(segment(&[]).is_empty());
        assert!(segment(&[p("   ", 0, 100)]).is_empty());
    }

    #[test]
    fn final_mark_cuts_after_the_wait() {
        let out = segment(&[p("Hello there.", 0, 1000), p("Next one", 1250, 2000)]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "Hello there.");
        assert_eq!(out[1].text, "Next one");
    }

    #[test]
    fn final_mark_keeps_sentence_open_before_the_wait() {
        let out = segment(&[p("Hello there.", 0, 1000), p("Next one", 1249, 2000)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "Hello there. Next one");
    }

    #[test]
    fn short_pause_merges_pieces_without_final_mark() {
        let out = segment(&[p("we should", 0, 800), p("ship it", 1399, 2000)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "we should ship it");
        assert_eq!((out[0].start_ms, out[0].end_ms), (0, 2000));
    }

    #[test]
    fn long_pause_cuts_pieces_without_final_mark() {
        let out = segment(&[p("we should", 0, 800), p("ship it", 1400, 2000)]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn forced_cut_at_ten_seconds_on_the_next_pause() {
        let out = segment(&[
            p("one", 0, 5000),
            p("two", 5100, 10_000),
            p("three", 10_100, 11_000),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "one two");
        assert_eq!(out[1].text, "three");
    }

    #[test]
    fn no_forced_cut_without_a_pause() {
        let out = segment(&[p("one", 0, 10_000), p("two", 10_000, 10_500)]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn under_ten_seconds_is_not_forced() {
        let out = segment(&[p("one", 0, 9_999), p("two", 10_100, 11_000)]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn final_mark_detection() {
        assert!(has_sentence_final_mark("Done."));
        assert!(has_sentence_final_mark("Really?!  "));
        assert!(has_sentence_final_mark("He said \"stop.\""));
        assert!(has_sentence_final_mark("Wait\u{2026}"));
        assert!(!has_sentence_final_mark("and then"));
        assert!(!has_sentence_final_mark(""));
    }

    #[test]
    fn flush_after_final_mark_waits_250_ms() {
        let mut seg = Segmenter::new();
        assert!(seg.push(p("Hello there.", 0, 1000)).is_none());
        assert!(seg.flush(1249).is_none());
        assert!(seg.open().is_some());
        let closed = seg.flush(1250).expect("sentence closes at 250 ms");
        assert_eq!(closed.text, "Hello there.");
        assert!(seg.open().is_none());
    }

    #[test]
    fn flush_without_final_mark_waits_600_ms() {
        let mut seg = Segmenter::new();
        assert!(seg.push(p("we should", 0, 800)).is_none());
        assert!(seg.flush(1399).is_none());
        let closed = seg.flush(1400).expect("sentence closes at 600 ms");
        assert_eq!(closed.text, "we should");
        assert_eq!((closed.start_ms, closed.end_ms), (0, 800));
    }

    #[test]
    fn flush_of_a_ten_second_segment_cuts_at_the_first_pause() {
        let mut seg = Segmenter::new();
        seg.push(p("a long run", 0, 10_000));
        assert!(seg.flush(10_000).is_none());
        assert!(seg.flush(10_001).is_some());
    }

    #[test]
    fn flush_with_nothing_open_gives_nothing() {
        let mut seg = Segmenter::new();
        assert!(seg.flush(1_000_000).is_none());
        seg.push(p("   ", 0, 100));
        assert!(seg.flush(1_000_000).is_none());
    }

    #[test]
    fn flush_twice_emits_the_sentence_once() {
        let mut seg = Segmenter::new();
        seg.push(p("Done.", 0, 500));
        assert!(seg.flush(5_000).is_some());
        assert!(seg.flush(6_000).is_none());
    }

    #[test]
    fn flush_before_the_end_of_the_piece_keeps_it_open() {
        let mut seg = Segmenter::new();
        seg.push(p("Done.", 0, 5_000));
        assert!(seg.flush(100).is_none());
    }

    #[test]
    fn streaming_push_matches_segment_and_flush_emits_the_tail() {
        let pieces = [
            p("Hello there.", 0, 1000),
            p("Next one", 1250, 2000),
            p("and more", 2100, 3000),
            p("Fresh start", 4000, 4500),
        ];
        let mut seg = Segmenter::new();
        let mut out: Vec<Piece> = pieces.iter().cloned().filter_map(|x| seg.push(x)).collect();
        out.extend(seg.flush(4500 + MERGE_PAUSE_MS));
        assert_eq!(out, segment(&pieces));
    }
}
