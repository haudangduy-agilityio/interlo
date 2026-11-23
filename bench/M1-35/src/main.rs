//! M1-35 spike: load each of the four product models in process and print the first output.
//! Roles: vad, translate, voice, asr. One process per role. The result line goes to stderr;
//! the `voice` role writes 16 kHz mono f32 little-endian samples to stdout, and `vad` and `asr`
//! read the same format from stdin.

mod asr;
mod common;
mod translate;
mod vad;
mod voice;

use std::path::PathBuf;
use std::process::ExitCode;

use common::{failure_line, result_line, Config, Outcome, Res};

type RoleFn = fn(&Config) -> Res<Outcome>;

/// The entry point of a role, or `None` for a name that is not one of the four roles.
fn find_role(name: &str) -> Option<RoleFn> {
    match name {
        "vad" => Some(vad::run),
        "translate" => Some(translate::run),
        "voice" => Some(voice::run),
        "asr" => Some(asr::run),
        _ => None,
    }
}

fn parse_args() -> Result<(String, RoleFn, Config), String> {
    let mut args = std::env::args().skip(1);
    let role = args.next().unwrap_or_default();
    let mut cfg = Config {
        models_dir: PathBuf::from("bench/M1-35/models"),
        reading_list: PathBuf::from("docs/m0/reading-list.md"),
        show_text: false,
    };
    while let Some(flag) = args.next() {
        if flag == "--show-text" {
            cfg.show_text = true;
            continue;
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--models-dir" => cfg.models_dir = PathBuf::from(value),
            "--reading-list" => cfg.reading_list = PathBuf::from(value),
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    let run = find_role(&role).ok_or_else(|| format!("unknown role '{role}'"))?;
    Ok((role, run, cfg))
}

fn main() -> ExitCode {
    let (role, run, cfg) = match parse_args() {
        Ok(parsed) => parsed,
        Err(reason) => {
            eprintln!("{}", failure_line("none", &reason));
            return ExitCode::from(2);
        }
    };
    match run(&cfg) {
        Ok(outcome) => {
            eprintln!("{}", result_line(&role, &outcome));
            if outcome.pass {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(reason) => {
            eprintln!("{}", failure_line(&role, &reason));
            ExitCode::from(1)
        }
    }
}
