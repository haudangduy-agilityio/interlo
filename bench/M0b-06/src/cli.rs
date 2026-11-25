//! `m0b06 analyze <check> ...`: runs one offline check of `analysis` over files and prints one
//! line `<CHECK> PASS|NO <reason>`. The exit code is 0 for PASS, 1 for NO and 64 for bad arguments.

use crate::analysis::{self, Verdict};

/// Runs the check named by `args` (everything after `analyze`). `read` returns the text of a file.
pub fn analyze(args: &[String], read: &dyn Fn(&str) -> Option<String>) -> (i32, String) {
    match run(args, read) {
        Ok((name, v)) => {
            let word = if v.pass { "PASS" } else { "NO" };
            (i32::from(!v.pass), format!("{name} {word} {}", v.reason))
        }
        Err(message) => (64, format!("usage error: {message}")),
    }
}

fn run(
    args: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Result<(&'static str, Verdict), String> {
    let file = |i: usize| -> Result<String, String> {
        let path = args.get(i).ok_or("a file argument is missing")?;
        read(path).ok_or_else(|| format!("cannot read {path}"))
    };
    let int = |i: usize| -> Result<u128, String> {
        let text = args.get(i).ok_or("a number argument is missing")?;
        text.parse().map_err(|_| format!("not a number: {text}"))
    };
    let expected = |n: usize| {
        if args.len() == n {
            Ok(())
        } else {
            Err(format!("`{}` takes {} argument(s)", args[0], n - 1))
        }
    };
    match args.first().map(String::as_str) {
        Some("ac23") => {
            expected(3)?;
            Ok(("AC-23", analysis::ac23(&file(1)?, &file(2)?)))
        }
        Some("ac25") => {
            expected(5)?;
            let index = args[3]
                .parse::<usize>()
                .map_err(|_| format!("not a number: {}", args[3]))?;
            match args[4].as_str() {
                "normal" => Ok(("AC-25", analysis::ac25(&file(1)?, &file(2)?, index, false))),
                "control" => Ok((
                    "AC-25-control",
                    analysis::ac25(&file(1)?, &file(2)?, index, true),
                )),
                other => Err(format!("mode must be normal or control, got {other}")),
            }
        }
        Some("ac29") => {
            expected(2)?;
            Ok(("AC-29", analysis::ac29(&file(1)?)))
        }
        Some("shot-timing") => {
            expected(6)?;
            Ok((
                "SHOT-TIMING",
                analysis::shot_timing(int(1)?, int(2)?, int(3)?, int(4)?, int(5)?),
            ))
        }
        Some("fullscreen") => {
            expected(2)?;
            Ok(("FULLSCREEN", analysis::fullscreen(&file(1)?)))
        }
        Some("lsof") => {
            expected(2)?;
            Ok(("AC-31", analysis::lsof(&file(1)?)))
        }
        Some(other) => Err(format!("unknown check {other}")),
        None => Err("no check named".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn files(spike: &'static str, page: &'static str) -> impl Fn(&str) -> Option<String> {
        move |path: &str| match path {
            "spike.log" => Some(spike.to_string()),
            "page.log" | "lsof.log" => Some(page.to_string()),
            _ => None,
        }
    }

    const SPIKE: &str = "1000 SNAP before front_pid=10 front_bundle=b window=5:10 own_active=0\n1100 SHOWN\n1700 SNAP after front_pid=10 front_bundle=b window=5:10 own_active=0\n";

    #[test]
    fn ac23_prints_pass_and_exits_zero() {
        let (code, line) = analyze(
            &args(&["ac23", "spike.log", "page.log"]),
            &files(SPIKE, "2000 KEY key=a\n"),
        );
        assert_eq!(code, 0, "{line}");
        assert!(line.starts_with("AC-23 PASS "), "{line}");
    }

    #[test]
    fn ac23_prints_no_and_exits_one() {
        let (code, line) = analyze(&args(&["ac23", "spike.log", "page.log"]), &files(SPIKE, ""));
        assert_eq!(code, 1);
        assert!(line.starts_with("AC-23 NO "), "{line}");
    }

    #[test]
    fn ac25_takes_the_target_number_and_the_mode() {
        let page = "100 TARGET x=960 y=900 size=80\n200 CLICK x=960 y=900\n";
        let f = files("150 SHOWN\n", page);
        let (code, line) = analyze(&args(&["ac25", "spike.log", "page.log", "1", "normal"]), &f);
        assert_eq!(code, 0, "{line}");
        assert!(line.starts_with("AC-25 PASS "), "{line}");
        let (code, line) = analyze(
            &args(&["ac25", "spike.log", "page.log", "1", "control"]),
            &f,
        );
        assert_eq!(code, 1);
        assert!(line.starts_with("AC-25-control NO "), "{line}");
    }

    #[test]
    fn shot_timing_and_the_file_checks_work() {
        let none = files("", "");
        let (code, line) = analyze(
            &args(&["shot-timing", "10000", "11000", "11300", "1000", "500"]),
            &none,
        );
        assert_eq!(
            (code, line.starts_with("SHOT-TIMING PASS ")),
            (0, true),
            "{line}"
        );
        let (code, _) = analyze(
            &args(&["shot-timing", "10000", "13000", "13300", "1000", "500"]),
            &none,
        );
        assert_eq!(code, 1);
        let (code, line) = analyze(
            &args(&["fullscreen", "page.log"]),
            &files("", "5 FULLSCREEN v=yes iw=1 ih=1 sw=1 sh=1\n"),
        );
        assert_eq!(
            (code, line.starts_with("FULLSCREEN PASS ")),
            (0, true),
            "{line}"
        );
        let (code, line) = analyze(
            &args(&["lsof", "lsof.log"]),
            &files("", "# sample 1\n# lsof exit=1\n"),
        );
        assert_eq!((code, line.starts_with("AC-31 PASS ")), (0, true), "{line}");
    }

    #[test]
    fn bad_arguments_exit_64_and_never_panic() {
        let none = files("", "");
        for bad in [
            vec![],
            vec!["nothing"],
            vec!["ac23"],
            vec!["ac23", "spike.log"],
            vec!["ac23", "missing.log", "page.log"],
            vec!["ac25", "spike.log", "page.log", "x", "normal"],
            vec!["ac25", "spike.log", "page.log", "1", "sideways"],
            vec!["shot-timing", "a", "b", "c", "d", "e"],
            vec!["shot-timing", "1", "2", "3", "4"],
            vec!["lsof"],
        ] {
            let (code, line) = analyze(&args(&bad), &none);
            assert_eq!(code, 64, "{bad:?} gave {line}");
            assert!(!line.is_empty(), "{bad:?}");
        }
    }
}
