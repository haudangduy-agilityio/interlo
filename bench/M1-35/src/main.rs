//! M1-35 spike: load each of the four product models in process and print the first output.
//! Roles: vad, translate, voice, asr. One process per role. The result line goes to stderr.

use std::process::ExitCode;

const ROLES: [&str; 4] = ["vad", "translate", "voice", "asr"];

fn result_line(role: &str, result: &str, reason: &str) -> String {
    format!("ROLE={role} RESULT={result} LOAD_MS=0 FIRST_MS=0 DETAIL= REASON={reason}")
}

fn main() -> ExitCode {
    let role = std::env::args().nth(1).unwrap_or_default();
    if !ROLES.contains(&role.as_str()) {
        eprintln!("{}", result_line(&role, "FAIL", "unknown role"));
        return ExitCode::from(2);
    }
    eprintln!("{}", result_line(&role, "FAIL", "not implemented"));
    ExitCode::from(1)
}
