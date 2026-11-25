//! Reference stage: speech or silence from the energy of each chunk. It proves the harness
//! works. It is not the product's voice activity detector.

use crate::log::EventSink;
use crate::stage::Stage;

/// A chunk is speech when its root mean square is at least this (starting value).
pub const SPEECH_RMS_MIN: f32 = 0.02;
/// Silent chunks in a row that end the speech: 10 chunks, 200 ms (starting value).
pub const HANGOVER_CHUNKS: u32 = 10;

pub struct EnergyRef {
    in_speech: bool,
    silent_run: u32,
    speech_end: u64,
}

impl EnergyRef {
    pub fn new() -> Self {
        Self {
            in_speech: false,
            silent_run: 0,
            speech_end: 0,
        }
    }
}

impl Default for EnergyRef {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for EnergyRef {
    fn name(&self) -> &'static str {
        "energy_ref"
    }

    fn on_chunk(&mut self, offset: u64, chunk: &[f32], sink: &mut dyn EventSink) {
        if chunk.is_empty() {
            return;
        }
        let energy: f32 = chunk.iter().map(|s| s * s).sum();
        let rms = (energy / chunk.len() as f32).sqrt();
        if rms >= SPEECH_RMS_MIN {
            if !self.in_speech {
                self.in_speech = true;
                sink.emit("speech_start", &[("sample", offset)]);
            }
            self.silent_run = 0;
            self.speech_end = offset + chunk.len() as u64;
        } else if self.in_speech {
            self.silent_run += 1;
            if self.silent_run >= HANGOVER_CHUNKS {
                self.in_speech = false;
                self.silent_run = 0;
                sink.emit("speech_end", &[("sample", self.speech_end)]);
            }
        }
    }

    fn finish(&mut self, sink: &mut dyn EventSink) {
        if self.in_speech {
            self.in_speech = false;
            sink.emit("speech_end", &[("sample", self.speech_end)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::CHUNK_SAMPLES;

    #[derive(Default)]
    struct Collect(Vec<(String, u64)>);

    impl EventSink for Collect {
        fn emit(&mut self, event: &'static str, fields: &[(&'static str, u64)]) {
            let sample = fields.iter().find(|f| f.0 == "sample").map_or(0, |f| f.1);
            self.0.push((event.to_string(), sample));
        }
    }

    fn loud() -> Vec<f32> {
        vec![0.3; CHUNK_SAMPLES]
    }

    fn quiet() -> Vec<f32> {
        vec![0.0; CHUNK_SAMPLES]
    }

    /// Feed chunks (true = loud) and return the events.
    fn feed(pattern: &[bool], finish: bool) -> Vec<(String, u64)> {
        let mut stage = EnergyRef::new();
        let mut sink = Collect::default();
        for (i, loud_chunk) in pattern.iter().enumerate() {
            let chunk = if *loud_chunk { loud() } else { quiet() };
            stage.on_chunk((i * CHUNK_SAMPLES) as u64, &chunk, &mut sink);
        }
        if finish {
            stage.finish(&mut sink);
        }
        sink.0
    }

    fn pattern(parts: &[(bool, usize)]) -> Vec<bool> {
        parts.iter().flat_map(|(v, n)| vec![*v; *n]).collect()
    }

    #[test]
    fn energy_ref_reports_bursts_at_chunk_offsets() {
        let p = pattern(&[
            (false, 25),
            (true, 50),
            (false, 25),
            (true, 50),
            (false, 25),
        ]);
        let events = feed(&p, true);
        let expected = vec![
            ("speech_start".to_string(), 8000),
            ("speech_end".to_string(), 24000),
            ("speech_start".to_string(), 32000),
            ("speech_end".to_string(), 48000),
        ];
        assert_eq!(events, expected);
    }

    #[test]
    fn energy_ref_ends_speech_after_ten_silent_chunks() {
        let p = pattern(&[(true, 3), (false, 10)]);
        let events = feed(&p, false);
        assert_eq!(
            events,
            vec![
                ("speech_start".to_string(), 0),
                ("speech_end".to_string(), 960)
            ]
        );
    }

    #[test]
    fn energy_ref_keeps_speech_after_nine_silent_chunks() {
        let p = pattern(&[(true, 3), (false, 9)]);
        let events = feed(&p, false);
        assert_eq!(events, vec![("speech_start".to_string(), 0)]);
    }

    #[test]
    fn energy_ref_flushes_speech_end_at_end_of_input() {
        let p = pattern(&[(false, 2), (true, 4)]);
        let events = feed(&p, true);
        assert_eq!(
            events,
            vec![
                ("speech_start".to_string(), 640),
                ("speech_end".to_string(), 1920)
            ]
        );
        assert_eq!(feed(&p, false), vec![("speech_start".to_string(), 640)]);
    }

    #[test]
    fn energy_ref_processes_short_final_chunk() {
        let mut stage = EnergyRef::new();
        let mut sink = Collect::default();
        stage.on_chunk(0, &quiet(), &mut sink);
        stage.on_chunk(320, &[0.5; 100], &mut sink);
        stage.finish(&mut sink);
        assert_eq!(
            sink.0,
            vec![
                ("speech_start".to_string(), 320),
                ("speech_end".to_string(), 420)
            ]
        );
    }
}
