//! The run: open the file, feed the chunks to one stage, write the log. The runner reads the
//! session clock only through the log writer, so the stage and the runner share one clock.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use crate::args::{self, Command, RunArgs};
use crate::clock::{Clock, Sleeper};
use crate::log::{Header, LogWriter, Scoped};
use crate::stage::{self, Stage, CHUNK_SAMPLES, CHUNK_US};
use crate::synth;
use crate::wav::{self, WavError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Feed every chunk at once.
    Fast,
    /// Deliver chunk `i` not before `20 ms * i` after `input_open`.
    Realtime,
}

impl Pace {
    pub fn as_str(self) -> &'static str {
        match self {
            Pace::Fast => "fast",
            Pace::Realtime => "realtime",
        }
    }
}

/// What the process gives the run: the clock, the sleeper and the wall time for the header.
pub struct Env<'a> {
    pub clock: &'a dyn Clock,
    pub sleeper: &'a dyn Sleeper,
    pub wall_start_epoch_ms: u64,
}

/// Run one stage on the bytes of one file (or on the error that reading it gave). Returns true
/// when the run finished. On a failure it writes one `error_*` event and `session_end`.
pub fn run_session(
    log: &mut LogWriter,
    stage: &mut dyn Stage,
    input: std::io::Result<Vec<u8>>,
    pace: Pace,
    sleeper: &dyn Sleeper,
) -> bool {
    let samples = match input {
        Err(_) => return fail(log, "error_io"),
        Ok(bytes) => match wav::parse(&bytes) {
            Ok(samples) => samples,
            Err(WavError::BadWav) => return fail(log, "error_bad_wav"),
            Err(WavError::BadFormat) => return fail(log, "error_bad_format"),
        },
    };
    let name = stage.name();
    let opened = log.emit_for(
        "harness",
        "input_open",
        &[
            ("samples", samples.len() as u64),
            ("rate", u64::from(wav::RATE_HZ)),
        ],
    );
    log.emit_for(name, "stage_start", &[]);
    let mut chunks = 0u64;
    for (i, chunk) in samples.chunks(CHUNK_SAMPLES).enumerate() {
        if pace == Pace::Realtime {
            wait_until(log, sleeper, opened + CHUNK_US * i as u64);
        }
        let mut sink = Scoped {
            log: &mut *log,
            stage: name,
        };
        stage.on_chunk((i * CHUNK_SAMPLES) as u64, chunk, &mut sink);
        chunks += 1;
    }
    log.emit_for("harness", "input_end", &[("chunks", chunks)]);
    stage.finish(&mut Scoped {
        log: &mut *log,
        stage: name,
    });
    log.emit_for(name, "stage_end", &[]);
    log.emit_for("harness", "session_end", &[]);
    log.flush();
    !log.failed()
}

fn fail(log: &mut LogWriter, error_event: &str) -> bool {
    log.emit_for("harness", error_event, &[]);
    log.emit_for("harness", "session_end", &[]);
    log.flush();
    false
}

/// Sleep until the session clock reads `target_us`. The sleeper can overshoot, never undershoot.
fn wait_until(log: &LogWriter, sleeper: &dyn Sleeper, target_us: u64) {
    loop {
        let now = log.now_us();
        if now >= target_us {
            return;
        }
        sleeper.sleep_us(target_us - now);
    }
}

/// Run the command line `argv` (without the program name). Returns the exit code: 0 finished,
/// 1 the run failed, 2 a usage error.
pub fn execute(argv: &[String], env: &Env, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    match args::parse(argv) {
        Err(message) => usage_error(stderr, &message),
        Ok(Command::Synth { out }) => synth_command(&out, stderr),
        Ok(Command::Run(run)) => run_command(run, env, stdout, stderr),
    }
}

fn usage_error(stderr: &mut dyn Write, message: &str) -> u8 {
    let _ = writeln!(stderr, "{message}");
    2
}

/// Create a file that must not exist yet. An existing file or a missing folder is a usage error.
fn create_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

/// The largest input file the harness reads (about 2 hours of 16 kHz audio). A bigger file, or
/// a device that never ends, is an `error_io` run and not an out-of-memory crash.
const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;

/// Read a whole file, but refuse one that is longer than `max` bytes.
fn read_limited(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(std::io::Error::other("input file is too large"));
    }
    Ok(bytes)
}

fn synth_command(out: &Path, stderr: &mut dyn Write) -> u8 {
    let mut file = match create_new(out) {
        Ok(file) => file,
        Err(message) => return usage_error(stderr, &message),
    };
    match file.write_all(&synth::wav_bytes()) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(stderr, "cannot write {}: {e}", out.display());
            1
        }
    }
}

fn run_command(run: RunArgs, env: &Env, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let Some(mut stage) = stage::find(&run.stage) else {
        return usage_error(stderr, &format!("unknown stage '{}'", run.stage));
    };
    let mut file = match &run.log {
        Some(path) => match create_new(path) {
            Ok(file) => Some(BufWriter::new(file)),
            Err(message) => return usage_error(stderr, &message),
        },
        None => None,
    };
    let out: &mut dyn Write = match file.as_mut() {
        Some(file) => file,
        None => stdout,
    };
    let mut log = LogWriter::new(out, env.clock);
    log.write_header(&Header {
        wall_start_epoch_ms: env.wall_start_epoch_ms,
        stage: stage.name(),
        pace: run.pace.as_str(),
        id: run.id.as_deref(),
    });
    let input = read_limited(&run.file, MAX_INPUT_BYTES);
    if run_session(&mut log, &mut *stage, input, run.pace, env.sleeper) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::fake::{FakeClock, FakeSleeper};
    use crate::log::EventSink;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    const NINE_EVENTS: [&str; 9] = [
        "harness\tinput_open\tsamples=56000\trate=16000",
        "energy_ref\tstage_start",
        "energy_ref\tspeech_start\tsample=8000",
        "energy_ref\tspeech_end\tsample=24000",
        "energy_ref\tspeech_start\tsample=32000",
        "energy_ref\tspeech_end\tsample=48000",
        "harness\tinput_end\tchunks=175",
        "energy_ref\tstage_end",
        "harness\tsession_end",
    ];

    /// Run `stage` on `input` with a fresh log in memory. Returns the result and the log text.
    fn run_into_string(
        stage: &mut dyn Stage,
        input: std::io::Result<Vec<u8>>,
        pace: Pace,
        clock: &dyn Clock,
        sleeper: &dyn Sleeper,
    ) -> (bool, String) {
        let mut buf: Vec<u8> = Vec::new();
        let ok = {
            let mut log = LogWriter::new(&mut buf, clock);
            run_session(&mut log, stage, input, pace, sleeper)
        };
        (ok, String::from_utf8(buf).unwrap())
    }

    fn events(log: &str) -> Vec<&str> {
        log.lines().filter(|l| !l.starts_with('#')).collect()
    }

    fn without_time(log: &str) -> Vec<String> {
        events(log)
            .iter()
            .map(|l| {
                l.split_once('\t')
                    .map_or(String::new(), |p| p.1.to_string())
            })
            .collect()
    }

    fn time_of(line: &str) -> u64 {
        line.split('\t').next().unwrap().parse().unwrap()
    }

    fn energy() -> Box<dyn Stage> {
        stage::find("energy_ref").unwrap()
    }

    #[test]
    fn run_logs_events_in_fixed_order() {
        let clock = FakeClock::new(1000);
        let sleeper = FakeSleeper::new(&clock);
        let (ok, log) = run_into_string(
            &mut *energy(),
            Ok(synth::wav_bytes()),
            Pace::Fast,
            &clock,
            &sleeper,
        );
        assert!(ok);
        assert_eq!(without_time(&log), NINE_EVENTS);
    }

    #[test]
    fn run_stamps_every_line_from_one_clock() {
        let clock = FakeClock::new(1000);
        let sleeper = FakeSleeper::new(&clock);
        let (_, log) = run_into_string(
            &mut *energy(),
            Ok(synth::wav_bytes()),
            Pace::Fast,
            &clock,
            &sleeper,
        );
        let times: Vec<u64> = events(&log).iter().map(|l| time_of(l)).collect();
        assert_eq!(
            times,
            vec![1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000]
        );
    }

    /// A stage that notes the clock reading at every chunk.
    struct Probe<'a> {
        clock: &'a FakeClock,
        seen: RefCell<Vec<u64>>,
    }

    impl Stage for Probe<'_> {
        fn name(&self) -> &'static str {
            "probe"
        }
        fn on_chunk(&mut self, _offset: u64, _chunk: &[f32], _sink: &mut dyn EventSink) {
            self.seen.borrow_mut().push(self.clock.now_us());
        }
        fn finish(&mut self, _sink: &mut dyn EventSink) {}
    }

    #[test]
    fn run_realtime_waits_until_chunk_time() {
        let clock = FakeClock::new(1);
        let sleeper = FakeSleeper::new(&clock);
        let mut probe = Probe {
            clock: &clock,
            seen: RefCell::new(Vec::new()),
        };
        let (ok, log) = run_into_string(
            &mut probe,
            Ok(synth::wav_bytes()),
            Pace::Realtime,
            &clock,
            &sleeper,
        );
        assert!(ok);
        let open = events(&log)
            .iter()
            .find(|l| l.contains("input_open"))
            .map(|l| time_of(l));
        let t0 = open.unwrap();
        let seen = probe.seen.borrow();
        assert_eq!(seen.len(), 175);
        for (i, t) in seen.iter().enumerate() {
            assert!(
                *t >= t0 + CHUNK_US * i as u64,
                "chunk {i} came at {t}, open at {t0}"
            );
        }
        assert!(sleeper.calls() > 0);
    }

    #[test]
    fn run_fast_never_sleeps() {
        let clock = FakeClock::new(1);
        let sleeper = FakeSleeper::new(&clock);
        let (ok, _) = run_into_string(
            &mut *energy(),
            Ok(synth::wav_bytes()),
            Pace::Fast,
            &clock,
            &sleeper,
        );
        assert!(ok);
        assert_eq!(sleeper.calls(), 0);
    }

    #[test]
    fn run_fast_and_realtime_give_equal_event_order() {
        let first_clock = FakeClock::new(7);
        let second_clock = FakeClock::new(333);
        let s1 = FakeSleeper::new(&first_clock);
        let s2 = FakeSleeper::new(&second_clock);
        let (_, a) = run_into_string(
            &mut *energy(),
            Ok(synth::wav_bytes()),
            Pace::Fast,
            &first_clock,
            &s1,
        );
        let (_, b) = run_into_string(
            &mut *energy(),
            Ok(synth::wav_bytes()),
            Pace::Realtime,
            &second_clock,
            &s2,
        );
        assert_eq!(without_time(&a), without_time(&b));
        assert_eq!(without_time(&a).len(), 9);
    }

    #[test]
    fn run_bad_input_writes_error_event_then_session_end() {
        use crate::wav::make_wav;
        let two_ch = make_wav(1, 2, 16000, 16, &[0, 0, 0, 0], 4);
        let cases: Vec<(std::io::Result<Vec<u8>>, &str)> = vec![
            (
                Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                "error_io",
            ),
            (Ok(b"not audio".to_vec()), "error_bad_wav"),
            (Ok(two_ch), "error_bad_format"),
        ];
        for (input, name) in cases {
            let clock = FakeClock::new(1000);
            let sleeper = FakeSleeper::new(&clock);
            let (ok, log) = run_into_string(&mut *energy(), input, Pace::Fast, &clock, &sleeper);
            assert!(!ok, "{name}");
            assert_eq!(
                without_time(&log),
                vec![
                    format!("harness\t{name}"),
                    "harness\tsession_end".to_string()
                ]
            );
        }
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("m0-03-unit-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn exec(args: &[&str]) -> (u8, String, String) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let clock = FakeClock::new(1000);
        let sleeper = FakeSleeper::new(&clock);
        let env = Env {
            clock: &clock,
            sleeper: &sleeper,
            wall_start_epoch_ms: 5,
        };
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let code = execute(&argv, &env, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn path_str(p: &Path) -> &str {
        p.to_str().unwrap()
    }

    #[test]
    fn read_limited_accepts_max_bytes_and_refuses_one_more() {
        let dir = temp_dir();
        let file = dir.join("ten.bin");
        std::fs::write(&file, [7u8; 10]).unwrap();
        assert_eq!(read_limited(&file, 10).unwrap().len(), 10);
        assert!(read_limited(&file, 9).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_runs_a_file_and_writes_the_log() {
        let dir = temp_dir();
        let wav = dir.join("a.wav");
        let log = dir.join("r.log");
        assert_eq!(exec(&["synth", "--out", path_str(&wav)]).0, 0);
        let (code, _, _) = exec(&[
            "run",
            "--stage",
            "energy_ref",
            "--file",
            path_str(&wav),
            "--log",
            path_str(&log),
            "--id",
            "vi_001",
        ]);
        assert_eq!(code, 0);
        let text = std::fs::read_to_string(&log).unwrap();
        assert_eq!(text.lines().next(), Some("# interlo-bench-log v1"));
        assert!(text.contains("# wall_start_epoch_ms=5\n"));
        assert!(text.contains("# id=vi_001\n"));
        assert_eq!(without_time(&text), NINE_EVENTS);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_without_log_writes_to_stdout() {
        let dir = temp_dir();
        let wav = dir.join("a.wav");
        assert_eq!(exec(&["synth", "--out", path_str(&wav)]).0, 0);
        let (code, out, _) = exec(&["run", "--stage", "energy_ref", "--file", path_str(&wav)]);
        assert_eq!(code, 0);
        assert_eq!(without_time(&out), NINE_EVENTS);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_log_is_not_overwritten() {
        let dir = temp_dir();
        let wav = dir.join("a.wav");
        let log = dir.join("keep.log");
        assert_eq!(exec(&["synth", "--out", path_str(&wav)]).0, 0);
        std::fs::write(&log, "keep").unwrap();
        let (code, _, err) = exec(&[
            "run",
            "--stage",
            "energy_ref",
            "--file",
            path_str(&wav),
            "--log",
            path_str(&log),
        ]);
        assert_eq!(code, 2);
        assert!(!err.is_empty());
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_wav_is_not_overwritten() {
        let dir = temp_dir();
        let wav = dir.join("keep.wav");
        std::fs::write(&wav, "keep").unwrap();
        let (code, _, _) = exec(&["synth", "--out", path_str(&wav)]);
        assert_eq!(code, 2);
        assert_eq!(std::fs::read_to_string(&wav).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn log_in_missing_folder_is_usage_error() {
        let dir = temp_dir();
        let wav = dir.join("a.wav");
        assert_eq!(exec(&["synth", "--out", path_str(&wav)]).0, 0);
        let log = dir.join("nodir").join("r.log");
        let (code, _, _) = exec(&[
            "run",
            "--stage",
            "energy_ref",
            "--file",
            path_str(&wav),
            "--log",
            path_str(&log),
        ]);
        assert_eq!(code, 2);
        assert!(!log.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn synth_in_missing_folder_is_usage_error() {
        let dir = temp_dir();
        let wav = dir.join("nodir").join("s.wav");
        let (code, _, _) = exec(&["synth", "--out", path_str(&wav)]);
        assert_eq!(code, 2);
        assert!(!wav.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_unknown_stage_and_bad_arguments_exit_2_without_a_log() {
        let dir = temp_dir();
        let wav = dir.join("a.wav");
        let log = dir.join("r.log");
        assert_eq!(exec(&["synth", "--out", path_str(&wav)]).0, 0);
        let (code, _, _) = exec(&[
            "run",
            "--stage",
            "nope",
            "--file",
            path_str(&wav),
            "--log",
            path_str(&log),
        ]);
        assert_eq!(code, 2);
        let (code, _, _) = exec(&[
            "run",
            "--stage",
            "energy_ref",
            "--file",
            path_str(&wav),
            "--log",
            path_str(&log),
            "--id",
            "bad id",
        ]);
        assert_eq!(code, 2);
        assert!(!log.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_missing_input_file_exits_1_with_error_io() {
        let dir = temp_dir();
        let log = dir.join("e.log");
        let none = dir.join("none.wav");
        let (code, _, _) = exec(&[
            "run",
            "--stage",
            "energy_ref",
            "--file",
            path_str(&none),
            "--log",
            path_str(&log),
        ]);
        assert_eq!(code, 1);
        let text = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            without_time(&text),
            vec![
                "harness\terror_io".to_string(),
                "harness\tsession_end".to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
