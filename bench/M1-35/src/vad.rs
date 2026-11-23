//! Role `vad`: voice activity detection on 16 kHz samples read from stdin.
//! Pass rule (AC-08): the highest speech probability over the 512-sample frames of the input is
//! above 0.5, and over 1 second of digital silence it is below 0.5.

use std::time::Instant;

use ort::session::Session;
use ort::value::Tensor;

use crate::common::*;

const FRAME: usize = 512;
const CONTEXT: usize = 64;
const THRESHOLD: f32 = 0.5;
const SILENCE_SAMPLES: usize = 16_000;

pub fn run(cfg: &Config) -> Res<Outcome> {
    let t_load = Instant::now();
    let env = new_environment()?;
    let mut session = load_session(&env, &cfg.models_dir.join("vad/vad.onnx"))?;
    let load_ms = ms(t_load);

    let samples = read_samples_from_stdin()?;
    if samples.len() < FRAME {
        return Err(format!("input has {} samples, need {FRAME}", samples.len()));
    }

    let t_first = Instant::now();
    let speech_max = max_probability(&mut session, &samples)?;
    let silence_max = max_probability(&mut session, &vec![0.0f32; SILENCE_SAMPLES])?;
    let first_ms = ms(t_first);

    let mut out = Outcome::new(load_ms, first_ms);
    out.detail = format!("speech_max={speech_max:.3} silence_max={silence_max:.3}");
    out.pass = speech_max > THRESHOLD && silence_max < THRESHOLD;
    if !out.pass {
        out.reason = format!(
            "speech_max must be above {THRESHOLD} and silence_max below {THRESHOLD}: {}",
            out.detail
        );
    }
    Ok(out)
}

/// Runs the detector over every full 512-sample frame, carrying its state, and returns the
/// highest speech probability.
fn max_probability(session: &mut Session, samples: &[f32]) -> Res<f32> {
    let mut state = vec![0.0f32; 2 * 128];
    let mut context = vec![0.0f32; CONTEXT];
    let mut max = 0.0f32;
    for frame in samples.as_chunks::<FRAME>().0 {
        let mut x = Vec::with_capacity(CONTEXT + FRAME);
        x.extend_from_slice(&context);
        x.extend_from_slice(frame);
        let inputs = vec![
            (
                "input",
                Tensor::from_array(([1usize, CONTEXT + FRAME], x))
                    .ctx("input tensor")?
                    .into_dyn(),
            ),
            (
                "state",
                Tensor::from_array(([2usize, 1, 128], state.clone()))
                    .ctx("state tensor")?
                    .into_dyn(),
            ),
            (
                "sr",
                Tensor::from_array(([0usize; 0], vec![16_000i64]))
                    .ctx("rate tensor")?
                    .into_dyn(),
            ),
        ];
        let outputs = session.run(inputs).ctx("run")?;
        let prob = outputs
            .get("output")
            .ok_or("output missing")?
            .try_extract_tensor::<f32>()
            .ctx("output")?
            .1
            .first()
            .copied()
            .ok_or("output is empty")?;
        let next = outputs
            .get("stateN")
            .ok_or("state output missing")?
            .try_extract_tensor::<f32>()
            .ctx("state output")?
            .1;
        if next.len() != state.len() {
            return Err(format!("state output has {} values", next.len()));
        }
        state.copy_from_slice(next);
        max = max.max(prob);
        context.copy_from_slice(&frame[FRAME - CONTEXT..]);
    }
    Ok(max)
}
