//! Command line entry point. All behaviour is in the library; this file only reads the process
//! environment, so that tests can call the same code with a fake clock.

use std::io::Write;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use m0_03::clock::{MonotonicClock, ThreadSleeper};
use m0_03::run::{execute, Env};

fn main() -> ExitCode {
    let mut argv = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(text) => argv.push(text),
            Err(_) => {
                eprintln!("an argument is not valid text");
                return ExitCode::from(2);
            }
        }
    }
    let clock = MonotonicClock::new();
    let env = Env {
        clock: &clock,
        sleeper: &ThreadSleeper,
        wall_start_epoch_ms: epoch_ms(),
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let code = execute(&argv, &env, &mut out, &mut std::io::stderr());
    let _ = out.flush();
    ExitCode::from(code)
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
