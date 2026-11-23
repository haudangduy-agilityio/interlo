//! Role `translate`: translates the test sentence to English with a greedy decode.
//! Pass rule (AC-09): the English output is not empty and differs from the input text.
//!
//! The decoder file is a merged decoder. Every step runs the whole prefix with the cache branch
//! off and empty past tensors. That costs more per step than a cached loop and is enough for a
//! spike that checks one sentence.

use std::time::Instant;

use ort::session::Session;
use ort::value::{DynValue, Tensor};
use tokenizers::Tokenizer;

use crate::common::*;

const MAX_NEW_TOKENS: usize = 96;
const LAYERS: usize = 6;
const HEADS: usize = 8;
const HEAD_DIM: usize = 64;

pub fn run(cfg: &Config) -> Res<Outcome> {
    let t_load = Instant::now();
    let dir = cfg.models_dir.join("translation");
    let env = new_environment()?;
    let mut encoder = load_session(&env, &dir.join("encoder.onnx"))?;
    let mut decoder = load_session(&env, &dir.join("decoder.onnx"))?;
    let tokenizer = load_tokenizer(&dir.join("tokenizer.json"))?;
    let config = read_text(&dir.join("config.json"))?;
    let start = json_int(&config, "decoder_start_token_id")?;
    let eos = json_int(&config, "eos_token_id")?;
    let load_ms = ms(t_load);

    let input = sentence(&cfg.reading_list)?;

    let t_first = Instant::now();
    let output = translate(&mut encoder, &mut decoder, &tokenizer, &input, start, eos)?;
    let first_ms = ms(t_first);

    let words = output.split_whitespace().count();
    let mut out = Outcome::new(load_ms, first_ms);
    out.detail = format!("words={words}");
    out.pass = !output.trim().is_empty() && output.trim() != input.trim();
    if !out.pass {
        out.reason = if output.trim().is_empty() {
            "output is empty".to_string()
        } else {
            "output equals the input".to_string()
        };
    }
    if cfg.show_text {
        eprintln!("TEXT={}", output.trim());
    }
    Ok(out)
}

/// The tokenizer file declares a precompiled normalizer with no character map, which the
/// tokenizer crate rejects with a panic. The spike drops that normalizer; the test sentence is
/// already in composed form, so the ids do not change.
fn load_tokenizer(path: &std::path::Path) -> Res<Tokenizer> {
    let text = read_text(path)?;
    let text = match text.find("\"normalizer\"") {
        Some(at) => {
            let open = text[at..].find('{').map(|i| at + i);
            let close = open.and_then(|o| text[o..].find('}').map(|i| o + i));
            match (open, close) {
                (Some(o), Some(c)) if text[o..c].contains("Precompiled") => {
                    format!("{}null{}", &text[..o], &text[c + 1..])
                }
                _ => text,
            }
        }
        None => text,
    };
    Tokenizer::from_bytes(text.as_bytes()).map_err(|e| format!("tokenizer: {e}"))
}

fn translate(
    encoder: &mut Session,
    decoder: &mut Session,
    tokenizer: &Tokenizer,
    input: &str,
    start: i64,
    eos: i64,
) -> Res<String> {
    let encoding = tokenizer
        .encode(input, true)
        .map_err(|e| format!("tokenize: {e}"))?;
    let ids: Vec<i64> = encoding.get_ids().iter().map(|&i| i as i64).collect();
    let n = ids.len();
    if n == 0 {
        return Err("tokenizer gave no ids".to_string());
    }

    let enc_out = encoder
        .run(vec![
            (
                "input_ids",
                Tensor::from_array(([1usize, n], ids))
                    .ctx("ids")?
                    .into_dyn(),
            ),
            (
                "attention_mask",
                Tensor::from_array(([1usize, n], vec![1i64; n]))
                    .ctx("mask")?
                    .into_dyn(),
            ),
        ])
        .ctx("encoder run")?;
    let hidden: Vec<f32> = enc_out
        .get("last_hidden_state")
        .ok_or("encoder output missing")?
        .try_extract_tensor::<f32>()
        .ctx("encoder output")?
        .1
        .to_vec();
    drop(enc_out);

    let mut decoded: Vec<i64> = vec![start];
    let mut generated: Vec<u32> = Vec::new();
    for _ in 0..MAX_NEW_TOKENS {
        let t = decoded.len();
        let mut inputs: Vec<(String, DynValue)> = vec![
            (
                "encoder_attention_mask".to_string(),
                Tensor::from_array(([1usize, n], vec![1i64; n]))
                    .ctx("mask")?
                    .into_dyn(),
            ),
            (
                "input_ids".to_string(),
                Tensor::from_array(([1usize, t], decoded.clone()))
                    .ctx("decoder ids")?
                    .into_dyn(),
            ),
            (
                "encoder_hidden_states".to_string(),
                Tensor::from_array(([1usize, n, 512], hidden.clone()))
                    .ctx("hidden")?
                    .into_dyn(),
            ),
            (
                "use_cache_branch".to_string(),
                Tensor::from_array(([1usize], vec![false]))
                    .ctx("branch flag")?
                    .into_dyn(),
            ),
        ];
        for layer in 0..LAYERS {
            for part in [
                "decoder.key",
                "decoder.value",
                "encoder.key",
                "encoder.value",
            ] {
                inputs.push((
                    format!("past_key_values.{layer}.{part}"),
                    Tensor::from_array(([1usize, HEADS, 0, HEAD_DIM], Vec::<f32>::new()))
                        .ctx("empty past")?
                        .into_dyn(),
                ));
            }
        }
        let outputs = decoder.run(inputs).ctx("decoder run")?;
        let (shape, logits) = outputs
            .get("logits")
            .ok_or("logits missing")?
            .try_extract_tensor::<f32>()
            .ctx("logits")?;
        let d = dims(shape);
        let vocab = *d.last().ok_or("logits shape is empty")?;
        if logits.len() < vocab || vocab == 0 {
            return Err("logits are empty".to_string());
        }
        let mut last = logits[logits.len() - vocab..].to_vec();
        // The start token doubles as the padding token and never appears in a translation.
        if let Some(v) = last.get_mut(start as usize) {
            *v = f32::NEG_INFINITY;
        }
        let next = argmax(&last) as i64;
        if next == eos {
            break;
        }
        decoded.push(next);
        generated.push(next as u32);
    }

    tokenizer
        .decode(&generated, true)
        .map_err(|e| format!("detokenize: {e}"))
}
