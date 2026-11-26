//! Span rules: which samples of the stream go to one recognition call.
//!
//! The builder holds no clock and calls no engine. Positions count samples from
//! the start of the stream, as the speech events do.

use crate::speech_events::{SpeechEvent, FRAME_SAMPLES};

/// Samples before the start of speech that a span also holds: 192 ms at 16 kHz.
/// Starting value: M1-26 may tune it.
pub const LEAD_IN_SAMPLES: usize = 3_072;
/// A span that reaches this length is cut: 15 s at 16 kHz. Starting value.
pub const MAX_SPAN_SAMPLES: usize = 240_000;
/// A shorter span is dropped: the recognizer takes 0.1 s at least.
pub const MIN_SPAN_SAMPLES: usize = 1_600;

/// The samples of one piece of speech.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// Position of the first sample.
    pub start_sample: u64,
    pub samples: Vec<f32>,
    /// True when the length limit cut the span, not an end of speech.
    pub forced: bool,
}

/// Samples kept while no span is open: the lead-in, and the frames that the
/// detector reads before it confirms a start (32 frames, about 1 s).
const HISTORY_SAMPLES: usize = LEAD_IN_SAMPLES + 32 * FRAME_SAMPLES;

/// Builds spans from the frames of one stream and the events of its detector.
#[derive(Debug, Default)]
pub struct SpanBuilder {
    /// The newest samples of the stream, from position `held_from` on.
    held: Vec<f32>,
    held_from: u64,
    /// Where the open span begins.
    open_from: Option<u64>,
    /// Where the last span ended; a new span never begins before it.
    last_end: u64,
}

impl SpanBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one frame and the event that this frame confirmed. Returns the spans
    /// that are complete now, in order.
    pub fn push(&mut self, frame: &[f32], event: Option<SpeechEvent>) -> Vec<Span> {
        self.held.extend_from_slice(frame);
        let position = self.held_from + self.held.len() as u64;
        let mut spans = Vec::new();
        match event {
            Some(SpeechEvent::Start { at_sample }) if self.open_from.is_none() => {
                let from = at_sample
                    .saturating_sub(LEAD_IN_SAMPLES as u64)
                    .max(self.last_end)
                    .max(self.held_from);
                self.open_from = Some(from.min(position));
            }
            Some(SpeechEvent::End { at_sample }) => {
                if let Some(from) = self.open_from.take() {
                    let end = at_sample.min(position);
                    let from = self.cut_full_spans(from, end, &mut spans);
                    if end > from {
                        self.close(from, end, false, &mut spans);
                    }
                }
            }
            _ => {}
        }
        if let Some(from) = self.open_from {
            self.open_from = Some(self.cut_full_spans(from, position, &mut spans));
        }
        self.drop_old_samples(position);
        spans
    }

    /// Cut every full length between `from` and `to`. Returns where the rest
    /// begins.
    fn cut_full_spans(&mut self, mut from: u64, to: u64, spans: &mut Vec<Span>) -> u64 {
        while to.saturating_sub(from) >= MAX_SPAN_SAMPLES as u64 {
            let end = from + MAX_SPAN_SAMPLES as u64;
            self.close(from, end, true, spans);
            from = end;
        }
        from
    }

    /// End the span `from..end` and keep it when it is long enough.
    fn close(&mut self, from: u64, end: u64, forced: bool, spans: &mut Vec<Span>) {
        self.last_end = end;
        let first = (from - self.held_from) as usize;
        let length = (end - from) as usize;
        if length >= MIN_SPAN_SAMPLES {
            spans.push(Span {
                start_sample: from,
                samples: self.held[first..first + length].to_vec(),
                forced,
            });
        }
    }

    fn drop_old_samples(&mut self, position: u64) {
        let keep_from = self
            .open_from
            .unwrap_or_else(|| position.saturating_sub(HISTORY_SAMPLES as u64))
            .max(self.held_from);
        self.held.drain(..(keep_from - self.held_from) as usize);
        self.held_from = keep_from;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream where every sample holds its own position, so a test can tell
    /// which samples a span holds.
    #[derive(Default)]
    struct Stream {
        builder: SpanBuilder,
        position: u64,
    }

    impl Stream {
        /// Push whole frames until the position reaches `position`; the last
        /// frame carries `event`.
        fn until(&mut self, position: u64, event: Option<SpeechEvent>) -> Vec<Span> {
            let mut spans = Vec::new();
            while self.position < position {
                let frame: Vec<f32> = (self.position..self.position + FRAME_SAMPLES as u64)
                    .map(|at| at as f32)
                    .collect();
                self.position += FRAME_SAMPLES as u64;
                let event = event.filter(|_| self.position >= position);
                spans.extend(self.builder.push(&frame, event));
            }
            spans
        }

        fn start(&mut self, at_sample: u64, reported_at: u64) -> Vec<Span> {
            self.until(reported_at, Some(SpeechEvent::Start { at_sample }))
        }

        fn end(&mut self, at_sample: u64, reported_at: u64) -> Vec<Span> {
            self.until(reported_at, Some(SpeechEvent::End { at_sample }))
        }
    }

    /// First sample, last sample and length of a span.
    fn range(span: &Span) -> (f32, f32, usize) {
        (
            span.samples[0],
            span.samples[span.samples.len() - 1],
            span.samples.len(),
        )
    }

    fn lengths(spans: &[Span]) -> Vec<usize> {
        spans.iter().map(|s| s.samples.len()).collect()
    }

    #[test]
    fn span_has_a_lead_in_and_ends_at_the_end_event() {
        let mut stream = Stream::default();
        assert_eq!(stream.start(16_000, 17_536), []);
        let spans = stream.end(48_000, 52_096);
        assert_eq!(spans.len(), 1);
        assert_eq!(range(&spans[0]), (12_928.0, 47_999.0, 35_072));
        assert_eq!(spans[0].start_sample, 12_928);
        assert!(!spans[0].forced);

        // A start near the start of the stream: the span begins at sample 0.
        let mut stream = Stream::default();
        stream.start(1_024, 2_560);
        let spans = stream.end(16_000, 20_480);
        assert_eq!(range(&spans[0]), (0.0, 15_999.0, 16_000));

        // The lead-in never reaches into the span before.
        let mut stream = Stream::default();
        stream.start(0, 1_536);
        assert_eq!(
            range(&stream.end(12_000, 12_288)[0]),
            (0.0, 11_999.0, 12_000)
        );
        stream.start(13_000, 14_848);
        let spans = stream.end(20_000, 24_576);
        assert_eq!(range(&spans[0]), (12_000.0, 19_999.0, 8_000));
    }

    #[test]
    fn a_late_start_still_has_its_whole_lead_in() {
        // A long silence first: the builder must still hold the lead-in.
        let mut stream = Stream::default();
        stream.until(320_000, None);
        stream.start(320_000, 321_536);
        let spans = stream.end(340_000, 344_064);
        assert_eq!(range(&spans[0]), (316_928.0, 339_999.0, 23_072));
    }

    #[test]
    fn long_speech_is_cut_at_fifteen_seconds() {
        let mut stream = Stream::default();
        stream.start(0, 1_536);
        let spans = stream.end(500_000, 500_224);
        assert_eq!(lengths(&spans), [240_000, 240_000, 20_000]);
        assert_eq!(range(&spans[0]), (0.0, 239_999.0, 240_000));
        assert_eq!(range(&spans[1]), (240_000.0, 479_999.0, 240_000));
        assert_eq!(range(&spans[2]), (480_000.0, 499_999.0, 20_000));
        assert_eq!(
            spans.iter().map(|s| s.forced).collect::<Vec<_>>(),
            [true, true, false]
        );
        // Nothing is open after the end report.
        assert_eq!(stream.until(800_000, None), []);

        // The cut happens as soon as the span is full, without an end report.
        let mut stream = Stream::default();
        stream.start(0, 1_536);
        assert_eq!(stream.until(239_616, None), []);
        assert_eq!(lengths(&stream.until(240_128, None)), [240_000]);

        // Exactly 240000 samples of speech give one span.
        for reported_at in [240_000, 244_224] {
            let mut stream = Stream::default();
            let mut spans = stream.start(0, 1_536);
            spans.extend(stream.end(240_000, reported_at));
            spans.extend(stream.until(300_000, None));
            assert_eq!(lengths(&spans), [240_000], "reported at {reported_at}");
        }

        // An end report before the start of the open span sends nothing.
        let mut stream = Stream::default();
        stream.start(0, 1_536);
        assert_eq!(lengths(&stream.until(242_176, None)), [240_000]);
        assert_eq!(stream.end(238_080, 242_688), []);
        assert_eq!(stream.until(600_000, None), []);
    }

    #[test]
    fn span_under_the_minimum_is_dropped() {
        let mut stream = Stream::default();
        stream.start(0, 1_536);
        assert_eq!(stream.end(1_599, 2_048), []);

        let mut stream = Stream::default();
        stream.start(0, 1_536);
        assert_eq!(lengths(&stream.end(1_600, 2_048)), [1_600]);
    }
}
