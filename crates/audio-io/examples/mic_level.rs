//! Manual run for M1-06: opens the microphone, prints a level meter and checks
//! that it moves when you speak.
//!
//! Run in a debug build, from the repository root:
//!
//! ```text
//! cargo run -p audio-io --example mic_level -- --seconds 10 --out "$EVIDENCE_DIR/mic-level.txt"
//! ```
//!
//! The program starts counting at the first callback of the stream, so the system
//! microphone dialog never eats into the windows. Cues on the terminal: stay
//! silent at 0 s, speak at 3 s, stay silent at 7 s. The output is text only: one
//! line per 100 ms (start time, peak, bar), then a summary and one line
//! `RESULT=PASS` or `RESULT=FAIL`. No audio is written anywhere.
//!
//! It installs the allocation guard in `Count` mode, so the run finishes and the
//! summary reports how many allocations and locks the callback made (0 expected).

use audio_io::mic::{select_input, MicBackend, SystemBackend};
use audio_io::realtime::{set_violation_mode, total_violations, GuardAllocator, ViolationMode};
use audio_io::{ring_buffer, CaptureSource, InputOpenError, MicCapture, MicConfig};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[global_allocator]
static GUARD: GuardAllocator = GuardAllocator;

/// One 100 ms bin, as 10 bins per second.
const BIN: Duration = Duration::from_millis(100);
/// How long to wait for the first callback (the owner may need to answer a dialog).
const FIRST_CALLBACK_WAIT: Duration = Duration::from_secs(60);

struct Args {
    seconds: u64,
    out: Option<PathBuf>,
    device: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seconds: 10,
        out: None,
        device: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--seconds" => {
                args.seconds = value("--seconds")?
                    .parse()
                    .map_err(|_| "--seconds needs a whole number".to_string())?;
            }
            "--out" => args.out = Some(PathBuf::from(value("--out")?)),
            "--device" => args.device = Some(value("--device")?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if args.seconds != 10 {
        return Err("the cues and result windows assume --seconds 10".to_string());
    }
    Ok(args)
}

/// Prints the text and, when a file is wanted, writes it there (creating the
/// folder if it is missing).
fn emit(text: &str, out: Option<&PathBuf>) {
    print!("{text}");
    if let Some(path) = out {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, text) {
            eprintln!("cannot write {}: {e}", path.display());
        }
    }
}

fn fail(reason: &str, out: Option<&PathBuf>) -> ! {
    emit(&format!("RESULT=FAIL reason={reason}\n"), out);
    std::process::exit(1);
}

fn open_error_name(error: &audio_io::CaptureError) -> String {
    match error {
        audio_io::CaptureError::InputOpen(InputOpenError::PermissionDenied) => {
            "PermissionDenied".to_string()
        }
        audio_io::CaptureError::InputOpen(InputOpenError::DeviceNotFound) => {
            "DeviceNotFound".to_string()
        }
        audio_io::CaptureError::InputOpen(InputOpenError::Other) => "Other".to_string(),
        other => format!("{other:?}"),
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("usage: mic_level [--seconds N] [--out FILE] [--device NAME]");
            std::process::exit(2);
        }
    };
    set_violation_mode(ViolationMode::Count);

    // What the program will open, for the summary.
    let mut probe = SystemBackend::new();
    let described = probe
        .input_devices()
        .ok()
        .and_then(|devices| {
            let index = select_input(&devices, args.device.as_deref()).ok()?;
            let format = probe.format_of(index).ok()?;
            Some((devices[index].name.clone(), format))
        })
        .map(|(name, format)| {
            (
                name,
                format.sample_rate,
                format.channels,
                format!("{:?}", format.sample),
            )
        });

    let mut capture = MicCapture::with_default_backend(MicConfig {
        device_name: args.device.clone(),
    });
    let level = capture.level();
    let (producer, mut consumer) = ring_buffer(1 << 16);
    if let Err(error) = capture.start(producer) {
        fail(
            &format!("open: {}", open_error_name(&error)),
            args.out.as_ref(),
        );
    }

    println!("Microphone opened. Waiting for the first block (answer the system dialog if it appears)...");
    let waited = Instant::now();
    while capture.stats().callbacks == 0 {
        if waited.elapsed() > FIRST_CALLBACK_WAIT {
            let _ = capture.stop();
            fail("no-callback", args.out.as_ref());
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let t0 = Instant::now();
    let _ = level.peak_since_last_read();
    let bins = args.seconds * 10;
    let mut peaks: Vec<f32> = Vec::with_capacity(usize::try_from(bins).unwrap_or(0));
    let mut drain = [0.0f32; 4096];
    let mut cues = [
        (0u64, "stay silent"),
        (30, "SPEAK NOW"),
        (70, "stay silent"),
    ]
    .into_iter();
    let mut next_cue = cues.next();
    for bin in 0..bins {
        if let Some((at, text)) = next_cue {
            if bin == at {
                println!("[{:>4.1} s] {text}", bin as f64 / 10.0);
                next_cue = cues.next();
            }
        }
        let deadline = t0 + BIN * u32::try_from(bin + 1).unwrap_or(u32::MAX);
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        peaks.push(level.peak_since_last_read());
        while consumer.pop_slice(&mut drain) > 0 {}
    }

    let stopped_at = capture.stop();
    let len_a = consumer.len();
    std::thread::sleep(Duration::from_millis(300));
    let len_b = consumer.len();
    let after_stop = if len_a == len_b {
        "unchanged"
    } else {
        "CHANGED"
    };

    // Windows by bin index (bin k starts at k * 0.1 s): speech 3.5 <= t < 6.5,
    // silence 0.0 <= t < 2.5 and 7.5 <= t < 10.0.
    let peak_in = |range: std::ops::Range<usize>| {
        peaks
            .iter()
            .enumerate()
            .filter(|(k, _)| range.contains(k))
            .fold(0.0f32, |m, (_, p)| m.max(*p))
    };
    let speech = peak_in(35..65);
    let silent = peak_in(0..25).max(peak_in(75..100));
    let violations = total_violations();
    let pass = speech >= 0.01
        && speech >= 4.0 * silent
        && violations.allocations == 0
        && violations.locks == 0;

    let mut text = String::new();
    for (k, peak) in peaks.iter().enumerate() {
        let bar = "#".repeat((peak * 50.0).round().clamp(0.0, 50.0) as usize);
        let _ = writeln!(text, "{:>5.1}  {:.4}  {}", k as f64 / 10.0, peak, bar);
    }
    let stats = capture.stats();
    let (name, rate, channels, sample) =
        described.unwrap_or_else(|| ("unknown".to_string(), 0, 0, "unknown".to_string()));
    let _ = writeln!(text, "--- summary ---");
    let _ = writeln!(text, "device: {name}");
    let _ = writeln!(text, "input rate: {rate} Hz");
    let _ = writeln!(text, "input channels: {channels}");
    let _ = writeln!(text, "sample format: {sample}");
    let _ = writeln!(text, "callbacks: {}", stats.callbacks);
    let _ = writeln!(text, "input frames: {}", stats.input_frames);
    let _ = writeln!(text, "samples dropped: {}", stats.samples_dropped);
    let _ = writeln!(text, "chunks refused: {}", stats.chunks_refused);
    let _ = writeln!(text, "stream failed: {}", capture.stream_failed());
    let _ = writeln!(text, "allocation violations: {}", violations.allocations);
    let _ = writeln!(text, "lock violations: {}", violations.locks);
    let _ = writeln!(text, "peak in speech window (3.5-6.5 s): {speech:.4}");
    let _ = writeln!(
        text,
        "peak in silent windows (0-2.5 s, 7.5-10 s): {silent:.4}"
    );
    let _ = writeln!(text, "stop: {stopped_at:?}");
    let _ = writeln!(text, "after stop: {after_stop}");
    let _ = writeln!(text, "RESULT={}", if pass { "PASS" } else { "FAIL" });
    emit(&text, args.out.as_ref());
    if !pass {
        std::process::exit(1);
    }
}
