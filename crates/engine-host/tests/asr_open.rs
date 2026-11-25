//! Opening a recognition model folder that is incomplete, or whose files are
//! not valid, is a typed error that names the file or the key and never the
//! folder. No model file is needed: the tests write small text files into a
//! temporary folder and remove it.

use engine_host::{EngineError, OnnxRecognizer};
use std::path::{Path, PathBuf};

/// A byte-level tokenizer with six text tokens and the three prompt tokens.
const TOKENIZER: &str = r#"{"version":"1.0","truncation":null,"padding":null,
"added_tokens":[
{"id":6,"content":"<|vi|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
{"id":7,"content":"<|transcribe|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
{"id":8,"content":"<|notimestamps|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true}],
"normalizer":null,
"pre_tokenizer":{"type":"ByteLevel","add_prefix_space":false,"trim_offsets":true,"use_regex":true},
"post_processor":null,
"decoder":{"type":"ByteLevel","add_prefix_space":true,"trim_offsets":true,"use_regex":true},
"model":{"type":"BPE","dropout":null,"unk_token":null,"continuing_subword_prefix":null,
"end_of_word_suffix":null,"fuse_unk":false,"byte_fallback":false,"ignore_merges":false,
"vocab":{"a":0,"b":1,"c":2,"Ġ":3,"d":4,"e":5},"merges":[]}}"#;

const CONFIG: &str = r#"{"d_model": 512, "decoder_attention_heads": 8, "decoder_layers": 6,
"decoder_start_token_id": 50258, "eos_token_id": 50257, "max_target_positions": 448,
"num_mel_bins": 80}"#;

/// A fresh folder under the system temp folder, removed by the caller.
fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("interlo-m1-37-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write(dir: &Path, name: &str, text: &str) -> std::io::Result<()> {
    std::fs::write(dir.join(name), text)
}

/// The text of the error of `open`, or a marker when it did not fail.
fn open_error_text(dir: &Path) -> (bool, String) {
    match OnnxRecognizer::open(dir) {
        Err(EngineError::Backend(text)) => (true, text),
        Err(other) => (false, other.to_string()),
        Ok(_) => (false, "open succeeded".to_string()),
    }
}

fn folder_text(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

#[test]
fn open_of_a_missing_folder_or_file_is_a_backend_error_naming_the_file() -> std::io::Result<()> {
    let parent = temp_dir("missing")?;
    let absent = parent.join("no-such-folder");
    let (is_backend, text) = open_error_text(&absent);
    assert!(is_backend, "{text}");
    assert!(text.contains("config.json"), "{text}");
    assert!(!text.contains(&folder_text(&parent)), "{text}");

    let incomplete = parent.join("incomplete");
    std::fs::create_dir_all(&incomplete)?;
    write(&incomplete, "config.json", CONFIG)?;
    write(&incomplete, "tokenizer.json", TOKENIZER)?;
    let (is_backend, text) = open_error_text(&incomplete);
    std::fs::remove_dir_all(&parent)?;
    assert!(is_backend, "{text}");
    assert!(text.contains("encoder.onnx"), "{text}");
    assert!(!text.contains(&folder_text(&parent)), "{text}");
    Ok(())
}

#[test]
fn open_of_a_text_file_as_model_is_a_backend_error() -> std::io::Result<()> {
    let dir = temp_dir("text")?;
    write(&dir, "config.json", CONFIG)?;
    write(&dir, "tokenizer.json", TOKENIZER)?;
    write(&dir, "encoder.onnx", "this is not a model")?;
    write(&dir, "decoder.onnx", "this is not a model")?;
    let (is_backend, text) = open_error_text(&dir);
    std::fs::remove_dir_all(&dir)?;
    assert!(is_backend, "{text}");
    assert!(text.contains("encoder.onnx"), "{text}");
    assert!(!text.contains(&folder_text(&dir)), "{text}");
    Ok(())
}

#[test]
fn open_of_a_bad_tokenizer_or_config_is_a_backend_error_naming_the_file_or_key(
) -> std::io::Result<()> {
    let bad_tokenizer = temp_dir("bad-tokenizer")?;
    write(&bad_tokenizer, "config.json", CONFIG)?;
    write(&bad_tokenizer, "tokenizer.json", "not json")?;
    let (is_backend, text) = open_error_text(&bad_tokenizer);
    std::fs::remove_dir_all(&bad_tokenizer)?;
    assert!(is_backend, "{text}");
    assert!(text.contains("tokenizer.json"), "{text}");
    assert!(!text.contains("encoder.onnx"), "{text}");
    assert!(!text.contains(&folder_text(&bad_tokenizer)), "{text}");

    let bad_config = temp_dir("bad-config")?;
    write(
        &bad_config,
        "config.json",
        &CONFIG.replace("decoder_start_token_id", "other_key"),
    )?;
    write(&bad_config, "tokenizer.json", TOKENIZER)?;
    let (is_backend, text) = open_error_text(&bad_config);
    std::fs::remove_dir_all(&bad_config)?;
    assert!(is_backend, "{text}");
    assert!(text.contains("decoder_start_token_id"), "{text}");
    assert!(!text.contains("encoder.onnx"), "{text}");
    assert!(!text.contains(&folder_text(&bad_config)), "{text}");
    Ok(())
}
