//! A source scan for the callback-side files: none of them may name a
//! lock, a blocking primitive or a log call. The probe block handler and the
//! helpers it calls sit in marked regions of `src/probe.rs`; each region is
//! scanned for lock, log, blocking and allocating names. The guards in `realtime_guard.rs`
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

/// Names that allocate. The probe block handler runs in a real-time section and
/// must hold none of them.
const ALLOCATING: &[&str] = &[
    "Box::new",
    "Vec::",
    "vec!",
    "String::",
    "format!",
    ".to_string(",
    ".to_vec(",
    ".to_owned(",
    ".collect(",
    ".push(",
    "Arc::new",
    "Rc::new",
    ".clone()",
];

/// Names that wait or hand work to another thread. The probe regions run on
/// the audio thread and must hold none of them.
const BLOCKING: &[&str] = &[
    "sleep(", "thread::", ".join(", ".recv(", ".send(", ".wait(", ".park(",
];

/// The marked regions of `src/probe.rs` that the audio thread runs.
const PROBE_REGIONS: &[&str] = &[
    "probe-block-handler",
    "probe-block-support",
    "probe-clock-now",
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
    names_in(text, BANNED)
}

/// The names of `list` that `text` contains.
fn names_in(text: &str, list: &'static [&'static str]) -> Vec<&'static str> {
    list.iter()
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

/// The text between the marker lines `// realtime-scan:begin <name>` and
/// `// realtime-scan:end <name>`. A marker is a whole line. An error names the
/// marker that is wrong: a marker that is missing or twice in the text, an end
/// before its begin, or a region with fewer than 3 code lines.
fn marked_region<'a>(text: &'a str, name: &str) -> Result<&'a str, String> {
    let begin = format!("// realtime-scan:begin {name}");
    let end = format!("// realtime-scan:end {name}");
    // Byte range of the one line that holds exactly `marker`.
    let marker_line = |marker: &str| -> Result<(usize, usize), String> {
        let mut hits = Vec::new();
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            if line.trim() == marker {
                hits.push((offset, offset + line.len()));
            }
            offset += line.len();
        }
        match hits.as_slice() {
            [] => Err(format!("the marker `{marker}` is missing")),
            [one] => Ok(*one),
            _ => Err(format!("the marker `{marker}` appears more than once")),
        }
    };
    let (_, region_start) = marker_line(&begin)?;
    let (region_end, _) = marker_line(&end)?;
    if region_end < region_start {
        return Err(format!("`{end}` comes before `{begin}`"));
    }
    let region = &text[region_start..region_end];
    let code_lines = region
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .count();
    if code_lines < 3 {
        return Err(format!(
            "the region of `{name}` holds {code_lines} code lines, fewer than 3"
        ));
    }
    Ok(region)
}

#[test]
fn the_probe_regions_hold_no_lock_log_blocking_or_allocating_name() {
    let text = read_source("src/probe.rs");
    for name in PROBE_REGIONS {
        let region = marked_region(&text, name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            banned_names_in(region),
            Vec::<&str>::new(),
            "`{name}` names a lock, a blocking primitive or a log call"
        );
        assert_eq!(
            names_in(region, BLOCKING),
            Vec::<&str>::new(),
            "`{name}` names a call that waits or hands work to another thread"
        );
        assert_eq!(
            names_in(region, ALLOCATING),
            Vec::<&str>::new(),
            "`{name}` names an allocating call"
        );
    }
}

#[test]
fn a_marker_that_is_missing_doubled_or_misplaced_fails() {
    let body = "a();\nb();\nc();\n";
    let ok = format!("// realtime-scan:begin x\n{body}// realtime-scan:end x\n");
    assert_eq!(marked_region(&ok, "x"), Ok(body));
    let no_begin = format!("{body}// realtime-scan:end x\n");
    assert!(marked_region(&no_begin, "x")
        .unwrap_err()
        .contains("begin x` is missing"));
    let no_end = format!("// realtime-scan:begin x\n{body}");
    assert!(marked_region(&no_end, "x")
        .unwrap_err()
        .contains("end x` is missing"));
    let twice = format!("{ok}// realtime-scan:begin x\n");
    assert!(marked_region(&twice, "x")
        .unwrap_err()
        .contains("more than once"));
    let swapped = format!("// realtime-scan:end x\n{body}// realtime-scan:begin x\n");
    assert!(marked_region(&swapped, "x")
        .unwrap_err()
        .contains("comes before"));
    // A marker named inside other text is not a marker line.
    let quoted = format!("let m = \"// realtime-scan:begin x\";\n{body}// realtime-scan:end x\n");
    assert!(marked_region(&quoted, "x")
        .unwrap_err()
        .contains("begin x` is missing"));
    let empty = "// realtime-scan:begin x\n// realtime-scan:end x\n";
    assert!(marked_region(empty, "x")
        .unwrap_err()
        .contains("fewer than 3"));
    let thin = "// realtime-scan:begin x\na();\n// note\n// realtime-scan:end x\n";
    assert!(marked_region(thin, "x")
        .unwrap_err()
        .contains("fewer than 3"));
}

#[test]
fn an_allocating_name_inside_the_marked_region_is_caught() {
    let text = "// realtime-scan:begin x\nlet a = 1;\nlet b = Box::new(2);\nlet c = v.push(3);\n// realtime-scan:end x\n";
    let region = marked_region(text, "x").unwrap();
    assert_eq!(names_in(region, ALLOCATING), vec!["Box::new", ".push("]);
}

#[test]
fn a_blocking_name_inside_the_marked_region_is_caught() {
    let text = "// realtime-scan:begin x\nlet a = 1;\nstd::thread::sleep(d);\ntx.send(1);\n// realtime-scan:end x\n";
    let region = marked_region(text, "x").unwrap();
    assert_eq!(
        names_in(region, BLOCKING),
        vec!["sleep(", "thread::", ".send("]
    );
}

#[test]
fn a_banned_name_inside_the_marked_region_is_caught() {
    let text = "// realtime-scan:begin x\nlet a = 1;\nlet g = m.lock();\nlet b = 2;\n// realtime-scan:end x\n";
    let region = marked_region(text, "x").unwrap();
    assert_eq!(banned_names_in(region), vec![".lock("]);
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
