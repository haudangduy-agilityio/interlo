//! A source scan for the callback-side files: none of them may name a
//! lock, a blocking primitive or a log call. The guards in `realtime_guard.rs`
//! catch an allocation or a guarded lock when the code runs; this scan catches a
//! name in the text, including code on a path the tests do not run.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

/// Names a callback-side file must not contain.
const BANNED: &[&str] = &[
    "Mutex",
    "RwLock",
    "Condvar",
    "Barrier",
    "Once",
    "OnceLock",
    "LazyLock",
    ".lock(",
    "parking_lot",
    "mpsc",
    "println!",
    "eprintln!",
    "dbg!",
    "tracing::",
    "log::",
    "diagnostics::",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True when `token` appears in `text` as a whole word: a token that starts or
/// ends with a word character must not touch another word character there.
fn contains_whole(text: &str, token: &str) -> bool {
    let starts_word = token.chars().next().is_some_and(is_word_char);
    let ends_word = token.chars().last().is_some_and(is_word_char);
    text.match_indices(token).any(|(at, _)| {
        let before_ok = !starts_word || !text[..at].chars().last().is_some_and(is_word_char);
        let after_ok = !ends_word
            || !text[at + token.len()..]
                .chars()
                .next()
                .is_some_and(is_word_char);
        before_ok && after_ok
    })
}

/// The banned names that `text` contains.
fn banned_names_in(text: &str) -> Vec<&'static str> {
    BANNED
        .iter()
        .copied()
        .filter(|name| contains_whole(text, name))
        .collect()
}

/// The part of a source file that is not test code: the text before the first
/// line `#[cfg(test)]`.
fn non_test_part(text: &str) -> &str {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end() == "#[cfg(test)]" {
            return &text[..offset];
        }
        offset += line.len();
    }
    text
}

fn read_source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"))
}

#[test]
fn callback_files_hold_no_lock_or_log_name() {
    let files = [
        ("src/mic_callback.rs", false),
        ("src/ring.rs", true),
        ("src/resample.rs", true),
    ];
    for (relative, cut_tests) in files {
        let text = read_source(relative);
        let scanned = if cut_tests {
            non_test_part(&text)
        } else {
            text.as_str()
        };
        assert!(scanned.len() > 200, "{relative}: nothing was scanned");
        assert_eq!(
            banned_names_in(scanned),
            Vec::<&str>::new(),
            "{relative} names a lock, a blocking primitive or a log call"
        );
    }
}

#[test]
fn the_scan_catches_a_lock_name_in_a_fixture() {
    assert_eq!(
        banned_names_in("let g = state.lock().unwrap();"),
        vec![".lock("]
    );
    assert_eq!(banned_names_in("println!(\"level\");"), vec!["println!"]);
    assert_eq!(banned_names_in("static M: Mutex<u8> = x;"), vec!["Mutex"]);
}

#[test]
fn the_scan_matches_whole_words_only() {
    // `FnOnce` is not `Once`, `eprintln!` is its own name, `block` is not `lock(`.
    assert_eq!(
        banned_names_in("fn run(f: impl FnOnce()) {} // block(1)"),
        Vec::<&str>::new()
    );
    assert_eq!(banned_names_in("eprintln!(\"x\");"), vec!["eprintln!"]);
    assert_eq!(banned_names_in("use std::sync::Once;"), vec!["Once"]);
}

#[test]
fn the_test_part_of_a_file_is_cut_at_the_cfg_test_line() {
    let text = "fn real() {}\n#[cfg(test)]\nmod tests { fn t() { println!(\"x\"); } }\n";
    assert_eq!(non_test_part(text), "fn real() {}\n");
    assert_eq!(non_test_part("fn only() {}\n"), "fn only() {}\n");
}
