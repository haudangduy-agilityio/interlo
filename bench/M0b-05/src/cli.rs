//! Subcommands next to the capture run: `analyze` (read a run log and answer one check),
//! `rate` (read or set the default output device sample rate) and `probe-output` (list which
//! processes have an output stream running, by pid only).

use crate::analysis::{self, Log, Verdict};
use crate::log::{emit, epoch_ms};
use crate::{ca, targets};
use std::time::{Duration, Instant};

/// Exit code for a bad command line.
pub const EXIT_USAGE: i32 = 64;

fn verdict_line(v: &Verdict) -> (i32, String) {
    if v.pass {
        (0, format!("PASS {}", v.detail))
    } else {
        (1, format!("NO {}", v.detail))
    }
}

fn split_list(s: &str) -> Vec<&str> {
    s.split(',').filter(|p| !p.is_empty()).collect()
}

/// `analyze <check> <file> [args...]`. `read` returns the text of a file. Gives the exit code
/// and the line to print.
pub fn analyze(args: &[String], read: &dyn Fn(&str) -> Option<String>) -> (i32, String) {
    let usage = || {
        (
            EXIT_USAGE,
            "usage: m0b05 analyze <check> <file> [args]".to_string(),
        )
    };
    let (Some(check), Some(file)) = (args.first(), args.get(1)) else {
        return usage();
    };
    let Some(text) = read(file) else {
        return (EXIT_USAGE, format!("cannot read {file}"));
    };
    let arg = |i: usize| args.get(i + 2).map(String::as_str);
    let log = Log::parse(&text);
    let verdict = match (check.as_str(), arg(0), arg(1)) {
        ("ac11", Some(label), _) => analysis::check_ac11(&log, label),
        ("ac12", _, _) => analysis::check_ac12(&log),
        ("ac13", Some(label), Some(secs)) => match secs.parse() {
            Ok(secs) => analysis::check_ac13(&log, label, secs),
            Err(_) => return usage(),
        },
        ("ac14", Some(label), _) => analysis::check_ac14(&log, label),
        ("ac16", _, _) => analysis::check_ac16(&log),
        ("ac18", _, _) => analysis::check_ac18(&log),
        ("route", Some(lines), _) => analysis::check_route_lines(&log, &split_list(lines)),
        ("noroute", Some(causes), _) => analysis::check_no_route(&log, &split_list(causes)),
        ("routepresent", Some(route), Some(label)) => {
            analysis::check_route_present(&log, route, label)
        }
        ("lsof", _, _) => analysis::check_lsof(&text),
        _ => return usage(),
    };
    verdict_line(&verdict)
}

/// `rate get` or `rate set <hz>` on the default output device.
pub fn rate(args: &[String]) -> i32 {
    let Some(device) = ca::default_output_device() else {
        println!("RATE_ERROR no default output device");
        return 2;
    };
    let before = ca::device_rate(device).unwrap_or(0.0);
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("get"), _) => {
            println!("RATE hz={before}");
            0
        }
        (Some("set"), Some(hz)) => {
            let Ok(hz) = hz.parse::<f64>() else {
                return EXIT_USAGE;
            };
            if let Err(status) = ca::set_device_rate(device, hz) {
                println!("RATE_ERROR set failed: {status}");
                return 2;
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut after = ca::device_rate(device).unwrap_or(0.0);
            while (after - hz).abs() > 0.5 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
                after = ca::device_rate(device).unwrap_or(0.0);
            }
            println!("RATE from={before} to={after}");
            i32::from((after - hz).abs() > 0.5) * 2
        }
        _ => EXIT_USAGE,
    }
}

/// `probe-output <seconds> [--target-arg <text>]`: print the pids that have an output stream
/// running whenever the set changes. Reads Core Audio metadata only, never audio.
pub fn probe_output(args: &[String]) -> i32 {
    let Some(secs) = args.first().and_then(|s| s.parse::<u64>().ok()) else {
        return EXIT_USAGE;
    };
    let marker = args
        .iter()
        .position(|a| a == "--target-arg")
        .and_then(|i| args.get(i + 1));
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut last = String::new();
    while Instant::now() < deadline {
        let running: Vec<u32> = ca::process_objects()
            .iter()
            .filter(|p| p.running_output && p.pid > 0)
            .map(|p| p.pid as u32)
            .collect();
        let target: Vec<u32> = match marker {
            Some(m) => {
                let text = std::process::Command::new("/bin/ps")
                    .args(["-axww", "-o", "pid=,ppid=,command="])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_default();
                let pids =
                    targets::select_target_pids(&targets::parse_ps(&text), m, std::process::id());
                running
                    .iter()
                    .copied()
                    .filter(|p| pids.contains(p))
                    .collect()
            }
            None => Vec::new(),
        };
        let other: Vec<u32> = running
            .iter()
            .copied()
            .filter(|p| !target.contains(p))
            .collect();
        let now = format!(
            "OUTPUT_PROCS target={} other={}",
            join(&target),
            join(&other)
        );
        if now != last {
            emit(&now);
            last = now;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    emit(&format!("PROBE_END t_ms={}", epoch_ms()));
    0
}

fn join(v: &[u32]) -> String {
    v.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn reader(text: &'static str) -> impl Fn(&str) -> Option<String> {
        move |f| (f == "run.log").then(|| text.to_string())
    }

    #[test]
    fn analyze_prints_pass_and_exits_zero() {
        let read = reader("1000 TAP_START rate=48000\n5000 SILENT\n");
        let (code, line) = analyze(&args(&["ac16", "run.log"]), &read);
        assert_eq!(code, 0);
        assert!(line.starts_with("PASS "), "{line}");
    }

    #[test]
    fn analyze_prints_no_and_exits_one() {
        let read = reader("1000 TAP_START rate=48000\n");
        let (code, line) = analyze(&args(&["ac16", "run.log"]), &read);
        assert_eq!(code, 1);
        assert!(line.starts_with("NO "), "{line}");
    }

    #[test]
    fn analyze_route_splits_the_expected_lines_on_commas() {
        let read = reader(
            "100 START pid=1\n110 app=missing\n120 route=2ch\n300 DEVICE_START rate=48000\n",
        );
        let (code, _) = analyze(&args(&["route", "run.log", "app=missing,route=2ch"]), &read);
        assert_eq!(code, 0);
    }

    #[test]
    fn analyze_with_a_bad_command_line_exits_usage() {
        let read = reader("");
        assert_eq!(analyze(&args(&[]), &read).0, EXIT_USAGE);
        assert_eq!(analyze(&args(&["ac99", "run.log"]), &read).0, EXIT_USAGE);
        assert_eq!(
            analyze(&args(&["ac16", "missing.log"]), &read).0,
            EXIT_USAGE
        );
        assert_eq!(
            analyze(&args(&["ac13", "run.log", "mark", "x"]), &read).0,
            EXIT_USAGE
        );
    }
}
