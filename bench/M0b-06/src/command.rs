//! The commands the spike reads from its input, one per line.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Show the panel, on display `n` (1 is the main display) or on the last one used.
    Show(Option<usize>),
    Hide,
    /// Move the panel to display `n` while it is shown.
    Move(usize),
    ClickThrough(bool),
    /// Log a snapshot with this tag.
    Snapshot(String),
    /// Show and hide `n` times, a snapshot after each of the 2n transitions.
    Cycle(u32),
    /// Log the windows of this application: all of them, and the ones on the current Space.
    Windows,
    Quit,
}

pub fn parse(line: &str) -> Result<Command, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let display = |text: &str| -> Result<usize, String> {
        match text.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(n),
            _ => Err(format!("display number must be 1 or more, got {text:?}")),
        }
    };
    match words.as_slice() {
        ["show"] => Ok(Command::Show(None)),
        ["show", n] => Ok(Command::Show(Some(display(n)?))),
        ["hide"] => Ok(Command::Hide),
        ["move", n] => Ok(Command::Move(display(n)?)),
        ["clickthrough", "on"] => Ok(Command::ClickThrough(true)),
        ["clickthrough", "off"] => Ok(Command::ClickThrough(false)),
        ["snapshot", tag] => {
            let ok = !tag.is_empty()
                && tag.len() <= 40
                && tag
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
            if ok {
                Ok(Command::Snapshot((*tag).to_string()))
            } else {
                Err(format!("bad snapshot tag {tag:?}"))
            }
        }
        ["cycle", n] => match n.parse::<u32>() {
            Ok(n) if (1..=100).contains(&n) => Ok(Command::Cycle(n)),
            _ => Err(format!("cycle count must be 1 to 100, got {n:?}")),
        },
        ["windows"] => Ok(Command::Windows),
        ["quit"] => Ok(Command::Quit),
        [] => Err("empty command".to_string()),
        other => Err(format!("unknown command {:?}", other.join(" "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_command() {
        assert_eq!(parse("show"), Ok(Command::Show(None)));
        assert_eq!(parse("show 2"), Ok(Command::Show(Some(2))));
        assert_eq!(parse("hide"), Ok(Command::Hide));
        assert_eq!(parse("move 1"), Ok(Command::Move(1)));
        assert_eq!(parse("clickthrough on"), Ok(Command::ClickThrough(true)));
        assert_eq!(parse("clickthrough off"), Ok(Command::ClickThrough(false)));
        assert_eq!(
            parse("snapshot before"),
            Ok(Command::Snapshot("before".to_string()))
        );
        assert_eq!(parse("cycle 10"), Ok(Command::Cycle(10)));
        assert_eq!(parse("windows"), Ok(Command::Windows));
        assert_eq!(parse("quit"), Ok(Command::Quit));
    }

    #[test]
    fn trims_the_line_and_the_spaces() {
        assert_eq!(parse("  show   2  \n"), Ok(Command::Show(Some(2))));
    }

    #[test]
    fn refuses_a_display_number_of_zero_or_not_a_number() {
        assert!(parse("show 0").is_err());
        assert!(parse("show x").is_err());
        assert!(parse("move 0").is_err());
        assert!(parse("move").is_err());
    }

    #[test]
    fn refuses_a_cycle_count_outside_one_to_one_hundred() {
        assert!(parse("cycle 0").is_err());
        assert!(parse("cycle 101").is_err());
        assert!(parse("cycle -1").is_err());
        assert_eq!(parse("cycle 100"), Ok(Command::Cycle(100)));
        assert_eq!(parse("cycle 1"), Ok(Command::Cycle(1)));
    }

    #[test]
    fn refuses_a_snapshot_tag_with_odd_characters_or_no_tag() {
        assert!(parse("snapshot").is_err());
        assert!(parse("snapshot a b").is_err());
        assert!(parse("snapshot a/b").is_err());
        assert!(parse(&format!("snapshot {}", "x".repeat(41))).is_err());
    }

    #[test]
    fn refuses_unknown_empty_and_extra_arguments_without_panicking() {
        assert!(parse("").is_err());
        assert!(parse("dance").is_err());
        assert!(parse("hide now").is_err());
        assert!(parse("clickthrough maybe").is_err());
        assert!(parse("quit 1").is_err());
    }
}
