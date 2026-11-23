//! The main window starts hidden, so the owner never sees the empty native
//! window colour before the webview has painted. The shell shows it when the
//! page has loaded (`src/first_paint.rs`).

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn config() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn main_window_starts_hidden() {
    let config = config();
    let windows = config["app"]["windows"].as_array().unwrap();
    let main = windows
        .iter()
        .find(|window| window["label"] == "main")
        .expect("a window labelled main");
    assert_eq!(main["visible"], Value::Bool(false), "{main}");
}

#[test]
fn only_the_main_window_exists_so_none_is_left_hidden() {
    let config = config();
    let windows = config["app"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1, "{windows:?}");
}
