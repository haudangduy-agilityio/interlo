//! Opening a translation model folder that is incomplete, or whose files are
//! not valid, is a typed error that names the file or the key and never the
//! folder. No model file is needed: the tests write small text files into a
//! temporary folder and remove it.

use engine_host::{EngineError, OnnxTranslator};
use std::path::{Path, PathBuf};

/// A word piece tokenizer with an end token template and the empty compiled
/// normalizer that the model's tokenizer file holds.
const TOKENIZER: &str = r###"{"version":"1.0","truncation":null,"padding":null,
"added_tokens":[
{"id":0,"content":"</s>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true}],
"normalizer":{"type":"Precompiled","precompiled_charsmap":null},
"pre_tokenizer":{"type":"Whitespace"},
"post_processor":{"type":"TemplateProcessing",
"single":[{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"</s>","type_id":0}}],
"pair":[{"Sequence":{"id":"A","type_id":0}},{"Sequence":{"id":"B","type_id":1}}],
"special_tokens":{"</s>":{"id":"</s>","ids":[0],"tokens":["</s>"]}}},
"decoder":null,
"model":{"type":"WordPiece","unk_token":"[UNK]","continuing_subword_prefix":"##",
"max_input_chars_per_word":100,"vocab":{"</s>":0,"[UNK]":1,"a":2}}}"###;

const CONFIG: &str = r#"{"d_model": 512, "decoder_attention_heads": 8, "decoder_layers": 6,
"decoder_start_token_id": 9, "eos_token_id": 0, "max_position_embeddings": 512,
"pad_token_id": 9}"#;

/// A fresh folder under the system temp folder, removed by the caller.
fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("interlo-m1-38-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A folder under `parent` with a config and a tokenizer file.
fn folder(parent: &Path, name: &str, config: &str, tokenizer: &str) -> std::io::Result<PathBuf> {
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.json"), config)?;
    std::fs::write(dir.join("tokenizer.json"), tokenizer)?;
    Ok(dir)
}

/// The text of the error of `open`, or a marker when it did not fail so.
fn open_error_text(dir: &Path) -> (bool, String) {
    match OnnxTranslator::open(dir) {
        Err(EngineError::Backend(text)) => (true, text),
        Err(other) => (false, other.to_string()),
        Ok(_) => (false, "open succeeded".to_string()),
    }
}

fn folder_text(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

#[test]
fn translator_open_of_a_missing_folder_or_file_names_the_file() -> std::io::Result<()> {
    let parent = temp_dir("missing")?;
    let absent = parent.join("no-such-folder");
    let incomplete = folder(&parent, "incomplete", CONFIG, TOKENIZER)?;
    let results = [
        (open_error_text(&absent), "config.json"),
        (open_error_text(&incomplete), "encoder.onnx"),
    ];
    std::fs::remove_dir_all(&parent)?;
    for ((is_backend, text), file) in results {
        assert!(is_backend, "{text}");
        assert!(text.contains(file), "{text}");
        assert!(!text.contains(&folder_text(&parent)), "{text}");
    }
    Ok(())
}

#[test]
fn translator_open_of_a_text_file_as_model_is_a_backend_error() -> std::io::Result<()> {
    let parent = temp_dir("text")?;
    let dir = folder(&parent, "model", CONFIG, TOKENIZER)?;
    std::fs::write(dir.join("encoder.onnx"), "this is not a model")?;
    std::fs::write(dir.join("decoder.onnx"), "this is not a model")?;
    let (is_backend, text) = open_error_text(&dir);
    std::fs::remove_dir_all(&parent)?;
    assert!(is_backend, "{text}");
    assert!(text.contains("encoder.onnx"), "{text}");
    assert!(!text.contains(&folder_text(&parent)), "{text}");
    Ok(())
}

#[test]
fn translator_open_of_a_bad_tokenizer_or_config_names_the_file_or_key() -> std::io::Result<()> {
    let parent = temp_dir("bad")?;
    let not_json = folder(&parent, "not-json", CONFIG, "not json")?;
    let compiled = TOKENIZER.replace(
        "\"precompiled_charsmap\":null",
        "\"precompiled_charsmap\":\"abc\"",
    );
    let unsupported = folder(&parent, "unsupported", CONFIG, &compiled)?;
    let config = CONFIG.replace("eos_token_id", "other_id");
    let no_key = folder(&parent, "no-key", &config, TOKENIZER)?;
    let results = [
        (open_error_text(&not_json), "tokenizer.json"),
        (open_error_text(&unsupported), "tokenizer.json"),
        (open_error_text(&no_key), "eos_token_id"),
    ];
    std::fs::remove_dir_all(&parent)?;
    for ((is_backend, text), expected) in results {
        assert!(is_backend, "{text}");
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains(&folder_text(&parent)), "{text}");
        assert!(!text.contains(".onnx"), "{text}");
    }
    Ok(())
}
