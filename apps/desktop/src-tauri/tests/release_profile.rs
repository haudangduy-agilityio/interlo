//! The release profile of the root `Cargo.toml` keeps its agreed shape (M0b-02):
//! thin LTO, one codegen unit, optimization level 3, a strip setting, and
//! unwinding kept so a panic in one engine thread does not abort the app
//! (D-135, FR-11). The file is read as text: the test adds no TOML crate.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

/// The `key = value` pairs of the `[profile.release]` section, comments removed.
fn release_profile(manifest: &str) -> Vec<(String, String)> {
    let mut in_section = false;
    let mut pairs = Vec::new();
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_section = line == "[profile.release]";
            continue;
        }
        if !in_section || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            pairs.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    pairs
}

/// Every way the profile differs from the agreed shape; empty means it is right.
fn profile_problems(manifest: &str) -> Vec<String> {
    let pairs = release_profile(manifest);
    let value = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    // Cargo accepts basic ("x") and literal ('x') TOML strings alike.
    let is_string = |key: &str, want: &str| {
        value(key).is_some_and(|v| v == format!("\"{want}\"") || v == format!("'{want}'"))
    };
    let mut problems = Vec::new();
    if !is_string("lto", "thin") {
        problems.push("lto must be \"thin\"".to_string());
    }
    if value("codegen-units") != Some("1") {
        problems.push("codegen-units must be 1".to_string());
    }
    if value("opt-level") != Some("3") {
        problems.push("opt-level must be 3".to_string());
    }
    if value("strip").is_none() {
        problems.push("a strip setting is required".to_string());
    }
    if is_string("panic", "abort") {
        problems.push("panic = \"abort\" is not allowed: unwinding stays".to_string());
    }
    problems
}

fn root_manifest() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.toml");
    fs::read_to_string(path).expect("read the root Cargo.toml")
}

#[test]
fn root_release_profile_has_the_agreed_shape() {
    let problems = profile_problems(&root_manifest());
    assert!(
        problems.is_empty(),
        "release profile problems: {problems:?}"
    );
}

#[test]
fn a_profile_with_panic_abort_is_refused() {
    let manifest = "[profile.release]\nlto = \"thin\"\ncodegen-units = 1\nopt-level = 3\nstrip = true\npanic = \"abort\"\n";
    let problems = profile_problems(manifest);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("panic"));
}

#[test]
fn a_profile_with_a_literal_string_panic_abort_is_refused() {
    let manifest = "[profile.release]\nlto = 'thin'\ncodegen-units = 1\nopt-level = 3\nstrip = true\npanic = 'abort'\n";
    let problems = profile_problems(manifest);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("panic"));
}

#[test]
fn a_profile_without_opt_level_and_strip_is_refused() {
    let manifest = "[profile.release]\nlto = \"thin\"\ncodegen-units = 1\n";
    let problems = profile_problems(manifest);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("opt-level")));
    assert!(problems.iter().any(|p| p.contains("strip")));
}

#[test]
fn only_the_release_section_is_read() {
    let manifest = "[profile.dev]\nopt-level = 0\nstrip = true\n[profile.release]\nlto = \"thin\"\ncodegen-units = 1\nopt-level = 3\nstrip = true # symbols\n";
    assert!(profile_problems(manifest).is_empty());
}
