//! The bundle must say why it uses the microphone: without
//! `NSMicrophoneUsageDescription` macOS ends the app at the first microphone
//! access (M1-41, AC-12). Tauri merges `Info.plist` into the macOS bundle.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

fn plist() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Info.plist");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn info_plist_holds_the_microphone_usage_text() {
    let text = plist();
    let after_key = text
        .split("<key>NSMicrophoneUsageDescription</key>")
        .nth(1)
        .expect("the microphone usage key is missing");
    let value = after_key
        .split("<string>")
        .nth(1)
        .and_then(|rest| rest.split("</string>").next())
        .expect("the microphone usage key has no string value");
    assert!(
        value.trim().len() > 20,
        "the usage text is too short: {value:?}"
    );
    assert!(value.is_ascii(), "the usage text is English: {value:?}");
    assert!(value.contains("microphone"));
    assert!(value.contains("never saved"));
}

#[test]
fn info_plist_holds_no_other_key() {
    assert_eq!(plist().matches("<key>").count(), 1);
}
