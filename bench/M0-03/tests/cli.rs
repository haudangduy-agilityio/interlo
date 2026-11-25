//! Process tests: run the built binary the way the README does and check the files it makes.
//! No sound card, no model, no network. Files go to a temporary folder outside the repository.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers; a failed unwrap fails the test

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("m0-03-cli-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_m0-03"))
        .args(args)
        .output()
        .unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn synth(dir: &Path) -> PathBuf {
    let wav = dir.join("a.wav");
    assert_eq!(bin(&["synth", "--out", s(&wav)]).status.code(), Some(0));
    wav
}

fn run(wav: &Path, log: &Path, extra: &[&str]) -> Output {
    let mut args = vec![
        "run",
        "--stage",
        "energy_ref",
        "--file",
        s(wav),
        "--log",
        s(log),
    ];
    args.extend_from_slice(extra);
    bin(&args)
}

/// Event lines of a log file: the lines that do not start with `#`.
fn events(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(String::from)
        .collect()
}

/// The same lines without the time column (what `cut -f2-` gives).
fn order(log: &Path) -> Vec<String> {
    events(log)
        .iter()
        .map(|l| l.split_once('\t').unwrap().1.to_string())
        .collect()
}

fn t_us(line: &str) -> u64 {
    line.split('\t').next().unwrap().parse().unwrap()
}

#[test]
fn cli_run_prints_the_nine_events() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let log = dir.join("r1.log");
    assert_eq!(run(&wav, &log, &[]).status.code(), Some(0));
    assert_eq!(
        order(&log),
        vec![
            "harness\tinput_open\tsamples=56000\trate=16000",
            "energy_ref\tstage_start",
            "energy_ref\tspeech_start\tsample=8000",
            "energy_ref\tspeech_end\tsample=24000",
            "energy_ref\tspeech_start\tsample=32000",
            "energy_ref\tspeech_end\tsample=48000",
            "harness\tinput_end\tchunks=175",
            "energy_ref\tstage_end",
            "harness\tsession_end",
        ]
    );
    let times: Vec<u64> = events(&log).iter().map(|l| t_us(l)).collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "times went down: {times:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_replay_twice_gives_empty_diff_of_event_streams() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let (p1, p2) = (dir.join("p1.log"), dir.join("p2.log"));
    assert_eq!(run(&wav, &p1, &[]).status.code(), Some(0));
    assert_eq!(run(&wav, &p2, &[]).status.code(), Some(0));
    assert_eq!(order(&p1), order(&p2));
    let _ = std::fs::remove_dir_all(dir);
}

/// One real 3.5 s paced run: same event order as the fast run, and the input ends late enough.
#[test]
fn cli_realtime_gives_same_order_and_input_end_at_least_3480000_us_after_open() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let (p1, log) = (dir.join("p1.log"), dir.join("p3.log"));
    assert_eq!(run(&wav, &p1, &[]).status.code(), Some(0));
    assert_eq!(
        run(&wav, &log, &["--pace", "realtime"]).status.code(),
        Some(0)
    );
    assert_eq!(order(&p1), order(&log));
    let lines = events(&log);
    let open = lines.iter().find(|l| l.contains("\tinput_open")).unwrap();
    let end = lines.iter().find(|l| l.contains("\tinput_end")).unwrap();
    assert!(t_us(end) - t_us(open) >= 3_480_000, "{open} / {end}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_missing_file_exits_1_with_error_io() {
    let dir = temp_dir();
    let log = dir.join("e1.log");
    let out = run(&dir.join("none.wav"), &log, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        order(&log),
        vec!["harness\terror_io", "harness\tsession_end"]
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_text_file_exits_1_with_error_bad_wav() {
    let dir = temp_dir();
    let log = dir.join("e2.log");
    let text = dir.join("x.wav");
    std::fs::write(&text, "not audio").unwrap();
    let out = run(&text, &log, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        order(&log),
        vec!["harness\terror_bad_wav", "harness\tsession_end"]
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_unknown_stage_exits_2_and_writes_no_log() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let log = dir.join("e3.log");
    let out = bin(&[
        "run",
        "--stage",
        "nope",
        "--file",
        s(&wav),
        "--log",
        s(&log),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!log.exists());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_run_writes_only_the_log() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let out_dir = dir.join("out");
    std::fs::create_dir(&out_dir).unwrap();
    assert_eq!(
        run(&wav, &out_dir.join("w.log"), &[]).status.code(),
        Some(0)
    );
    let names: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec!["w.log".to_string()]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_existing_log_is_not_overwritten() {
    let dir = temp_dir();
    let wav = synth(&dir);
    let log = dir.join("w.log");
    assert_eq!(run(&wav, &log, &[]).status.code(), Some(0));
    let before = std::fs::read(&log).unwrap();
    assert_eq!(run(&wav, &log, &[]).status.code(), Some(2));
    assert_eq!(std::fs::read(&log).unwrap(), before);
    assert_eq!(bin(&["synth", "--out", s(&wav)]).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cli_synth_twice_gives_same_bytes() {
    let dir = temp_dir();
    let (a, b) = (dir.join("s1.wav"), dir.join("s2.wav"));
    assert_eq!(bin(&["synth", "--out", s(&a)]).status.code(), Some(0));
    assert_eq!(bin(&["synth", "--out", s(&b)]).status.code(), Some(0));
    let bytes = std::fs::read(&a).unwrap();
    assert_eq!(bytes.len(), 112_044);
    assert_eq!(bytes, std::fs::read(&b).unwrap());
    let _ = std::fs::remove_dir_all(dir);
}
