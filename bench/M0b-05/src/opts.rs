//! Command line of the spike.

use std::path::PathBuf;

/// Options of the capture run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Opts {
    /// A text that the command line of the target app (or its parent) holds.
    pub target_arg: Option<String>,
    /// Act as if the 2-channel virtual device is not installed (AC-35, AC-36).
    pub ignore_2ch: bool,
    /// Play a 1 kHz tone from this process for 5 s after `IDLE_TONE` (AC-12).
    pub self_tone: bool,
    /// Negative control for AC-12 only: put this process into the tap so its own tone is captured.
    pub control_include_self: bool,
    /// Write the pid of the spike to this file at start.
    pub pidfile: Option<PathBuf>,
    /// Write the exit code to this file just before exit (the bundle is started through
    /// `open`, which does not hand the exit code back).
    pub exitfile: Option<PathBuf>,
}

/// Parse the arguments after the program name (the capture run, not a subcommand).
pub fn parse(args: &[String]) -> Result<Opts, String> {
    let mut opts = Opts::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--target-arg" => {
                let v = it.next().ok_or("--target-arg needs a value")?;
                if v.is_empty() {
                    return Err("--target-arg must not be empty".to_string());
                }
                opts.target_arg = Some(v.clone());
            }
            "--pidfile" => {
                opts.pidfile = Some(PathBuf::from(it.next().ok_or("--pidfile needs a value")?));
            }
            "--exitfile" => {
                opts.exitfile = Some(PathBuf::from(it.next().ok_or("--exitfile needs a value")?));
            }
            "--ignore-2ch" => opts.ignore_2ch = true,
            "--self-tone" => opts.self_tone = true,
            "--control-include-self" => {
                opts.control_include_self = true;
                opts.self_tone = true;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_args_give_defaults() {
        assert_eq!(parse(&a(&[])), Ok(Opts::default()));
    }

    #[test]
    fn reads_every_option() {
        let Ok(o) = parse(&a(&[
            "--target-arg",
            "--user-data-dir=/x",
            "--ignore-2ch",
            "--self-tone",
            "--pidfile",
            "/p",
        ])) else {
            panic!("a valid command line must parse");
        };
        assert_eq!(o.target_arg.as_deref(), Some("--user-data-dir=/x"));
        assert!(o.ignore_2ch);
        assert!(o.self_tone);
        assert!(!o.control_include_self);
        assert_eq!(o.pidfile, Some(PathBuf::from("/p")));
    }

    #[test]
    fn reads_the_exit_file() {
        let Ok(o) = parse(&a(&["--exitfile", "/e"])) else {
            panic!("a valid command line must parse");
        };
        assert_eq!(o.exitfile, Some(PathBuf::from("/e")));
        assert!(parse(&a(&["--exitfile"])).is_err());
    }

    #[test]
    fn control_switch_implies_the_self_tone() {
        let Ok(o) = parse(&a(&["--control-include-self"])) else {
            panic!("a valid command line must parse");
        };
        assert!(o.control_include_self && o.self_tone);
    }

    #[test]
    fn missing_value_is_an_error() {
        assert!(parse(&a(&["--target-arg"])).is_err());
        assert!(parse(&a(&["--pidfile"])).is_err());
    }

    #[test]
    fn unknown_option_is_an_error() {
        assert_eq!(
            parse(&a(&["--bogus"])),
            Err("unknown option: --bogus".to_string())
        );
    }

    #[test]
    fn empty_target_arg_is_an_error() {
        assert!(parse(&a(&["--target-arg", ""])).is_err());
    }
}
