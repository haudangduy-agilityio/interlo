//! Opening a model that is not there, or is not a model, is a typed error
//! that names the file and never the folder. No model file is needed.

use engine_host::{EngineError, OnnxVad};
use std::path::PathBuf;

/// A fresh folder under the system temp folder, removed by the caller.
fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("interlo-m1-36-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn open_error(path: &std::path::Path) -> Option<EngineError> {
    OnnxVad::open(path).err()
}

#[test]
fn open_of_missing_file_is_a_backend_error() -> std::io::Result<()> {
    let dir = temp_dir("missing")?;
    let error = open_error(&dir.join("missing-model.onnx"));
    std::fs::remove_dir_all(&dir)?;
    assert!(matches!(error, Some(EngineError::Backend(_))), "{error:?}");
    Ok(())
}

#[test]
fn open_error_names_the_file_and_not_the_folder() -> std::io::Result<()> {
    let dir = temp_dir("names")?;
    let error = open_error(&dir.join("missing-model.onnx"));
    std::fs::remove_dir_all(&dir)?;
    let text = error.map(|e| e.to_string()).unwrap_or_default();
    assert!(text.contains("missing-model.onnx"), "{text}");
    assert!(
        !text.contains(&dir.to_string_lossy().into_owned()),
        "{text}"
    );
    Ok(())
}

#[test]
fn open_of_a_text_file_is_a_backend_error() -> std::io::Result<()> {
    let dir = temp_dir("text")?;
    let path = dir.join("not-a-model.onnx");
    std::fs::write(&path, "this is not a model")?;
    let error = open_error(&path);
    std::fs::remove_dir_all(&dir)?;
    let text = error.as_ref().map(ToString::to_string).unwrap_or_default();
    assert!(matches!(error, Some(EngineError::Backend(_))), "{text}");
    assert!(text.contains("not-a-model.onnx"), "{text}");
    assert!(
        !text.contains(&dir.to_string_lossy().into_owned()),
        "{text}"
    );
    Ok(())
}
