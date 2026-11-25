//! Command line parsing. Every error here is a usage error: the program exits with code 2.

use std::path::PathBuf;

use crate::run::Pace;

pub struct RunArgs {
    pub stage: String,
    pub file: PathBuf,
    pub log: Option<PathBuf>,
    pub pace: Pace,
    pub id: Option<String>,
}

pub enum Command {
    Run(RunArgs),
    Synth { out: PathBuf },
}

pub const USAGE: &str = "usage: m0-03 run --stage <name> --file <wav> [--log <path>] [--pace fast|realtime] [--id <id>]\n       m0-03 synth --out <wav>";

/// An ID is 1 to 32 characters: lower-case letters, digits and `_`, starting with a letter.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    first_ok
        && id.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Parse the arguments after the program name.
pub fn parse(argv: &[String]) -> Result<Command, String> {
    let (command, rest) = argv.split_first().ok_or_else(|| USAGE.to_string())?;
    match command.as_str() {
        "run" => parse_run(rest),
        "synth" => parse_synth(rest),
        other => Err(format!("unknown command '{other}'\n{USAGE}")),
    }
}

/// Split `--flag value` pairs. Every flag needs a value and may appear once.
fn pairs(rest: &[String]) -> Result<Vec<(&str, &str)>, String> {
    let mut out: Vec<(&str, &str)> = Vec::new();
    let mut items = rest.iter();
    while let Some(flag) = items.next() {
        let value = items
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        if out.iter().any(|(f, _)| *f == flag.as_str()) {
            return Err(format!("{flag} is given twice"));
        }
        out.push((flag.as_str(), value.as_str()));
    }
    Ok(out)
}

fn parse_run(rest: &[String]) -> Result<Command, String> {
    let mut stage = None;
    let mut file = None;
    let mut log = None;
    let mut pace = Pace::Fast;
    let mut id = None;
    for (flag, value) in pairs(rest)? {
        match flag {
            "--stage" => stage = Some(value.to_string()),
            "--file" => file = Some(PathBuf::from(value)),
            "--log" => log = Some(PathBuf::from(value)),
            "--pace" => {
                pace = match value {
                    "fast" => Pace::Fast,
                    "realtime" => Pace::Realtime,
                    other => return Err(format!("--pace must be fast or realtime, not '{other}'")),
                }
            }
            "--id" => {
                if !valid_id(value) {
                    return Err(format!("--id '{value}' is not an ID (lower-case letters, digits, _; 1 to 32 characters; starts with a letter)"));
                }
                id = Some(value.to_string());
            }
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
    }
    Ok(Command::Run(RunArgs {
        stage: stage.ok_or_else(|| format!("--stage is required\n{USAGE}"))?,
        file: file.ok_or_else(|| format!("--file is required\n{USAGE}"))?,
        log,
        pace,
        id,
    }))
}

fn parse_synth(rest: &[String]) -> Result<Command, String> {
    let mut out = None;
    for (flag, value) in pairs(rest)? {
        match flag {
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
    }
    Ok(Command::Synth {
        out: out.ok_or_else(|| format!("--out is required\n{USAGE}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn run_args(extra: &[&str]) -> Result<Command, String> {
        let mut v = vec!["run", "--stage", "energy_ref", "--file", "a.wav"];
        v.extend_from_slice(extra);
        parse(&strs(&v))
    }

    #[test]
    fn args_parse_a_full_run_command() {
        match run_args(&["--log", "x.log", "--pace", "realtime", "--id", "vi_001"]) {
            Ok(Command::Run(a)) => {
                assert_eq!(a.stage, "energy_ref");
                assert_eq!(a.file, PathBuf::from("a.wav"));
                assert_eq!(a.log, Some(PathBuf::from("x.log")));
                assert_eq!(a.pace, Pace::Realtime);
                assert_eq!(a.id.as_deref(), Some("vi_001"));
            }
            _ => panic!("expected a run command"),
        }
        match run_args(&[]) {
            Ok(Command::Run(a)) => {
                assert_eq!(a.pace, Pace::Fast);
                assert!(a.log.is_none() && a.id.is_none());
            }
            _ => panic!("expected a run command"),
        }
    }

    #[test]
    fn args_parse_a_synth_command() {
        match parse(&strs(&["synth", "--out", "s.wav"])) {
            Ok(Command::Synth { out }) => assert_eq!(out, PathBuf::from("s.wav")),
            _ => panic!("expected a synth command"),
        }
        assert!(parse(&strs(&["synth"])).is_err());
    }

    #[test]
    fn args_reject_id_with_space() {
        assert!(run_args(&["--id", "bad id"]).is_err());
    }

    #[test]
    fn args_accept_id_of_1_char() {
        assert!(run_args(&["--id", "a"]).is_ok());
    }

    #[test]
    fn args_accept_id_of_32_chars() {
        let id = format!("a{}", "1".repeat(31));
        assert_eq!(id.len(), 32);
        assert!(run_args(&["--id", &id]).is_ok());
    }

    #[test]
    fn args_accept_id_with_letters_digits_and_underscores() {
        assert!(valid_id("en_us_001"));
    }

    #[test]
    fn args_reject_id_of_33_chars() {
        let id = format!("a{}", "1".repeat(32));
        assert_eq!(id.len(), 33);
        assert!(run_args(&["--id", &id]).is_err());
    }

    #[test]
    fn args_reject_empty_id() {
        assert!(run_args(&["--id", ""]).is_err());
    }

    #[test]
    fn args_reject_id_not_starting_with_lower_case_letter() {
        assert!(run_args(&["--id", "1abc"]).is_err());
        assert!(run_args(&["--id", "_abc"]).is_err());
        assert!(run_args(&["--id", "Abc"]).is_err());
    }

    #[test]
    fn args_reject_missing_stage() {
        assert!(parse(&strs(&["run", "--file", "a.wav"])).is_err());
    }

    #[test]
    fn args_reject_missing_file() {
        assert!(parse(&strs(&["run", "--stage", "energy_ref"])).is_err());
    }

    #[test]
    fn args_reject_bad_pace() {
        assert!(run_args(&["--pace", "slow"]).is_err());
    }

    #[test]
    fn args_reject_unknown_flag_missing_value_and_command() {
        assert!(run_args(&["--speed", "2"]).is_err());
        assert!(run_args(&["--log"]).is_err());
        assert!(parse(&[]).is_err());
        assert!(parse(&strs(&["play"])).is_err());
    }
}
