//! Speech start and end events from the frame answers of a `Vad`.
//!
//! The detector counts frames. It holds no audio and no clock: a position is a
//! sample count from the start of the stream, so a test gives the same events
//! every time. The caller cuts the stream into frames of `FRAME_SAMPLES`.

use crate::error::EngineError;
use crate::traits::Vad;

/// Samples in one frame: 32 ms of mono audio at 16 kHz.
pub const FRAME_SAMPLES: usize = 512;

/// How many frames in a row confirm a start and an end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechConfig {
    start_frames: u32,
    end_frames: u32,
}

impl SpeechConfig {
    /// Start after `start_frames` speech frames in a row, end after
    /// `end_frames` non-speech frames in a row. Zero is refused.
    pub fn new(start_frames: u32, end_frames: u32) -> Result<Self, EngineError> {
        if start_frames == 0 || end_frames == 0 {
            return Err(EngineError::Backend(
                "speech config needs at least one frame to start and to end".into(),
            ));
        }
        Ok(Self {
            start_frames,
            end_frames,
        })
    }
}

impl Default for SpeechConfig {
    /// 3 frames (96 ms) to start, 8 frames (256 ms) to end.
    fn default() -> Self {
        Self {
            start_frames: 3,
            end_frames: 8,
        }
    }
}

/// A boundary of speech. `at_sample` counts samples from the start of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEvent {
    /// The first frame of the run of speech frames that confirmed the start.
    Start { at_sample: u64 },
    /// The position just after the last speech frame.
    End { at_sample: u64 },
}

/// Turns one answer per frame into start and end events.
#[derive(Debug, Clone)]
pub struct SpeechDetector {
    config: SpeechConfig,
    position: u64,
    speech_run: u32,
    silence_run: u32,
    in_speech: bool,
}

impl SpeechDetector {
    pub fn new(config: SpeechConfig) -> Self {
        Self {
            config,
            position: 0,
            speech_run: 0,
            silence_run: 0,
            in_speech: false,
        }
    }

    /// Ask `vad` about one frame and return the event that this frame confirms.
    /// The state changes only when `vad` answered.
    pub fn push(
        &mut self,
        vad: &mut dyn Vad,
        frame: &[f32],
    ) -> Result<Option<SpeechEvent>, EngineError> {
        if frame.len() != FRAME_SAMPLES {
            return Err(EngineError::Backend(format!(
                "a frame holds {FRAME_SAMPLES} samples, got {}",
                frame.len()
            )));
        }
        let speech = vad.is_speech(frame)?;
        self.position = self.position.saturating_add(FRAME_SAMPLES as u64);
        let frame_len = FRAME_SAMPLES as u64;
        if self.in_speech {
            if speech {
                self.silence_run = 0;
                return Ok(None);
            }
            self.silence_run += 1;
            if self.silence_run < self.config.end_frames {
                return Ok(None);
            }
            let back = u64::from(self.silence_run) * frame_len;
            self.in_speech = false;
            self.silence_run = 0;
            return Ok(Some(SpeechEvent::End {
                at_sample: self.position.saturating_sub(back),
            }));
        }
        if !speech {
            self.speech_run = 0;
            return Ok(None);
        }
        self.speech_run += 1;
        if self.speech_run < self.config.start_frames {
            return Ok(None);
        }
        let back = u64::from(self.speech_run) * frame_len;
        self.in_speech = true;
        self.speech_run = 0;
        Ok(Some(SpeechEvent::Start {
            at_sample: self.position.saturating_sub(back),
        }))
    }

    /// Forget the position and every counted frame. No event comes out.
    pub fn reset(&mut self) {
        self.position = 0;
        self.speech_run = 0;
        self.silence_run = 0;
        self.in_speech = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Engine;
    use meeting_core::policy::Provider;
    use std::collections::VecDeque;

    /// A Vad that returns a prepared answer per call and counts its calls.
    struct Scripted {
        answers: VecDeque<Result<bool, EngineError>>,
        calls: usize,
        provider: Provider,
    }

    impl Scripted {
        fn new(answers: impl IntoIterator<Item = Result<bool, EngineError>>) -> Self {
            Self {
                answers: answers.into_iter().collect(),
                calls: 0,
                provider: Provider::local("scripted"),
            }
        }

        fn of(answers: &[bool]) -> Self {
            Self::new(answers.iter().map(|a| Ok(*a)))
        }
    }

    impl Engine for Scripted {
        fn provider(&self) -> &Provider {
            &self.provider
        }
    }

    impl Vad for Scripted {
        fn is_speech(&mut self, _frame: &[f32]) -> Result<bool, EngineError> {
            self.calls += 1;
            self.answers
                .pop_front()
                .unwrap_or_else(|| Err(EngineError::Backend("script ended".into())))
        }
    }

    const FRAME: [f32; FRAME_SAMPLES] = [0.0; FRAME_SAMPLES];

    /// `n` answers of `speech` each, appended in order.
    fn runs(parts: &[(bool, usize)]) -> Vec<bool> {
        parts
            .iter()
            .flat_map(|(speech, n)| std::iter::repeat_n(*speech, *n))
            .collect()
    }

    /// Push one frame per answer; the result holds one entry per frame.
    fn feed(
        det: &mut SpeechDetector,
        answers: &[bool],
    ) -> Result<Vec<Option<SpeechEvent>>, EngineError> {
        let mut vad = Scripted::of(answers);
        answers.iter().map(|_| det.push(&mut vad, &FRAME)).collect()
    }

    fn events(out: &[Option<SpeechEvent>]) -> Vec<SpeechEvent> {
        out.iter().flatten().copied().collect()
    }

    #[test]
    fn three_speech_frames_after_silence_start_speech_at_first_frame() -> Result<(), EngineError> {
        for k in [0_usize, 5] {
            let mut det = SpeechDetector::new(SpeechConfig::default());
            let out = feed(&mut det, &runs(&[(false, k), (true, 3)]))?;
            let last = out.len() - 1;
            assert!(out[..last].iter().all(Option::is_none), "k={k}");
            assert_eq!(
                out[last],
                Some(SpeechEvent::Start {
                    at_sample: (k * FRAME_SAMPLES) as u64
                }),
                "k={k}"
            );
        }
        Ok(())
    }

    #[test]
    fn eight_silent_frames_after_speech_end_speech_after_last_speech_frame(
    ) -> Result<(), EngineError> {
        for m in [0_usize, 4] {
            let mut det = SpeechDetector::new(SpeechConfig::default());
            let out = feed(&mut det, &runs(&[(true, 3 + m), (false, 8)]))?;
            let last = out.len() - 1;
            assert!(out[3 + m..last].iter().all(Option::is_none), "m={m}");
            assert_eq!(
                out[last],
                Some(SpeechEvent::End {
                    at_sample: ((3 + m) * FRAME_SAMPLES) as u64
                }),
                "m={m}"
            );
        }
        Ok(())
    }

    #[test]
    fn two_speech_frames_make_no_event() -> Result<(), EngineError> {
        let mut det = SpeechDetector::new(SpeechConfig::default());
        let out = feed(&mut det, &runs(&[(true, 2), (false, 20)]))?;
        assert!(out.iter().all(Option::is_none));
        Ok(())
    }

    #[test]
    fn seven_silent_frames_inside_speech_make_no_end() -> Result<(), EngineError> {
        let mut det = SpeechDetector::new(SpeechConfig::default());
        let out = feed(
            &mut det,
            &runs(&[(true, 3), (false, 7), (true, 1), (false, 8)]),
        )?;
        assert_eq!(
            events(&out),
            vec![
                SpeechEvent::Start { at_sample: 0 },
                SpeechEvent::End {
                    at_sample: (11 * FRAME_SAMPLES) as u64
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn two_utterances_give_start_end_start_end_in_order() -> Result<(), EngineError> {
        let mut det = SpeechDetector::new(SpeechConfig::default());
        let out = feed(
            &mut det,
            &runs(&[(true, 4), (false, 8), (true, 4), (false, 8)]),
        )?;
        let f = FRAME_SAMPLES as u64;
        assert_eq!(
            events(&out),
            vec![
                SpeechEvent::Start { at_sample: 0 },
                SpeechEvent::End { at_sample: 4 * f },
                SpeechEvent::Start { at_sample: 12 * f },
                SpeechEvent::End { at_sample: 16 * f },
            ]
        );
        Ok(())
    }

    #[test]
    fn config_of_one_frame_ends_on_next_frame() -> Result<(), EngineError> {
        let mut det = SpeechDetector::new(SpeechConfig::new(1, 1)?);
        let out = feed(&mut det, &[true, false])?;
        assert_eq!(
            out,
            vec![
                Some(SpeechEvent::Start { at_sample: 0 }),
                Some(SpeechEvent::End {
                    at_sample: FRAME_SAMPLES as u64
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn config_with_zero_frames_is_refused() {
        assert!(matches!(
            SpeechConfig::new(0, 8),
            Err(EngineError::Backend(_))
        ));
        assert!(matches!(
            SpeechConfig::new(3, 0),
            Err(EngineError::Backend(_))
        ));
    }

    #[test]
    fn reset_clears_counters_and_position_and_emits_nothing() -> Result<(), EngineError> {
        // Two speech frames counted, then reset.
        let mut det = SpeechDetector::new(SpeechConfig::default());
        feed(&mut det, &[true, true])?;
        det.reset();
        let out = feed(&mut det, &[true, true, true])?;
        assert_eq!(events(&out), vec![SpeechEvent::Start { at_sample: 0 }]);

        // Inside speech, then reset: no End, and a new Start at position 0.
        let mut det = SpeechDetector::new(SpeechConfig::default());
        let before = feed(&mut det, &runs(&[(true, 5), (false, 3)]))?;
        assert_eq!(events(&before).len(), 1);
        det.reset();
        let after = feed(&mut det, &[true, true, true])?;
        assert_eq!(events(&after), vec![SpeechEvent::Start { at_sample: 0 }]);
        Ok(())
    }

    #[test]
    fn wrong_frame_length_is_refused_and_vad_not_called() -> Result<(), EngineError> {
        for len in [0_usize, 511, 513] {
            let mut det = SpeechDetector::new(SpeechConfig::default());
            let mut vad = Scripted::of(&[true]);
            let frame = vec![0.0_f32; len];
            assert!(
                matches!(det.push(&mut vad, &frame), Err(EngineError::Backend(_))),
                "len={len}"
            );
            assert_eq!(vad.calls, 0, "len={len}");
            // The detector behaves as a fresh one afterwards.
            let out = feed(&mut det, &[true, true, true])?;
            assert_eq!(events(&out), vec![SpeechEvent::Start { at_sample: 0 }]);
        }
        Ok(())
    }

    #[test]
    fn vad_error_is_returned_and_leaves_state() -> Result<(), EngineError> {
        let mut det = SpeechDetector::new(SpeechConfig::default());
        let boom = EngineError::Backend("boom".into());
        let mut vad = Scripted::new([Ok(true), Err(boom.clone()), Ok(true), Ok(true)]);
        assert_eq!(det.push(&mut vad, &FRAME)?, None);
        assert_eq!(det.push(&mut vad, &FRAME), Err(boom));
        assert_eq!(det.push(&mut vad, &FRAME)?, None);
        // Two speech frames counted so far (the failed frame did not count):
        // the next one confirms the start at position 0.
        assert_eq!(
            det.push(&mut vad, &FRAME)?,
            Some(SpeechEvent::Start { at_sample: 0 })
        );
        Ok(())
    }
}
