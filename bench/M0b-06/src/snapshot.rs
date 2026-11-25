//! What the system says about focus at one moment: the frontmost application and its topmost
//! normal window. Two snapshots are compared before and after the panel shows (AC-23, AC-29).

/// One reading, taken by the platform layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub tag: String,
    pub front_pid: i32,
    pub front_bundle: String,
    /// Window number and owner pid of the topmost layer-0 window of the frontmost application.
    pub window: Option<(i64, i32)>,
    /// Whether this spike's own application is the active one.
    pub own_active: bool,
}

impl Snapshot {
    /// `SNAP <tag> front_pid=<n> front_bundle=<id> window=<number>:<pid>|none own_active=0|1`
    pub fn to_line(&self) -> String {
        let window = match self.window {
            Some((number, pid)) => format!("{number}:{pid}"),
            None => "none".to_string(),
        };
        let bundle = if self.front_bundle.is_empty() {
            "none"
        } else {
            &self.front_bundle
        };
        format!(
            "SNAP {} front_pid={} front_bundle={} window={} own_active={}",
            self.tag,
            self.front_pid,
            bundle,
            window,
            u8::from(self.own_active)
        )
    }

    /// Reads the text after the timestamp of a `SNAP` line.
    pub fn parse(rest: &str) -> Option<Snapshot> {
        let mut words = rest.split_whitespace();
        if words.next()? != "SNAP" {
            return None;
        }
        let tag = words.next()?.to_string();
        let front_pid = words.next()?.strip_prefix("front_pid=")?.parse().ok()?;
        let front_bundle = words.next()?.strip_prefix("front_bundle=")?.to_string();
        let window = match words.next()?.strip_prefix("window=")? {
            "none" => None,
            pair => {
                let (number, pid) = pair.split_once(':')?;
                Some((number.parse().ok()?, pid.parse().ok()?))
            }
        };
        let own_active = match words.next()?.strip_prefix("own_active=")? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        if words.next().is_some() {
            return None;
        }
        Some(Snapshot {
            tag,
            front_pid,
            front_bundle,
            window,
            own_active,
        })
    }
}

/// The same frontmost application and the same key window (the topmost window of that
/// application). The tag and `own_active` are not part of the comparison.
pub fn same_focus(a: &Snapshot, b: &Snapshot) -> bool {
    a.front_pid == b.front_pid && a.front_bundle == b.front_bundle && a.window == b.window
}

/// True when every snapshot equals the first one by `same_focus` and none has this spike active.
pub fn all_same_as_first(list: &[Snapshot]) -> bool {
    match list.first() {
        Some(first) => list.iter().all(|s| same_focus(first, s) && !s.own_active),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(tag: &str, pid: i32, window: Option<(i64, i32)>, own: bool) -> Snapshot {
        Snapshot {
            tag: tag.to_string(),
            front_pid: pid,
            front_bundle: "app.example.browser".to_string(),
            window,
            own_active: own,
        }
    }

    #[test]
    fn to_line_and_parse_round_trip() {
        let s = snap("before", 4242, Some((901, 4242)), false);
        let line = s.to_line();
        assert_eq!(
            line,
            "SNAP before front_pid=4242 front_bundle=app.example.browser window=901:4242 own_active=0"
        );
        assert_eq!(Snapshot::parse(&line), Some(s));
    }

    #[test]
    fn a_missing_window_is_written_as_none_and_read_back() {
        let s = snap("t1", 7, None, true);
        let line = s.to_line();
        assert!(line.ends_with("window=none own_active=1"));
        assert_eq!(Snapshot::parse(&line), Some(s));
    }

    #[test]
    fn parse_refuses_a_line_that_is_not_a_snapshot_or_is_incomplete() {
        assert_eq!(Snapshot::parse("SHOWN"), None);
        assert_eq!(Snapshot::parse("SNAP x front_pid=1"), None);
        assert_eq!(
            Snapshot::parse("SNAP x front_pid=a front_bundle=b window=none own_active=0"),
            None
        );
    }

    #[test]
    fn same_focus_ignores_the_tag_and_own_active() {
        let a = snap("before", 10, Some((5, 10)), false);
        let b = snap("after", 10, Some((5, 10)), true);
        assert!(same_focus(&a, &b));
    }

    #[test]
    fn same_focus_fails_when_the_frontmost_app_or_the_window_differs() {
        let a = snap("before", 10, Some((5, 10)), false);
        assert!(!same_focus(&a, &snap("after", 11, Some((5, 10)), false)));
        assert!(!same_focus(&a, &snap("after", 10, Some((6, 10)), false)));
        assert!(!same_focus(&a, &snap("after", 10, None, false)));
    }

    #[test]
    fn all_same_as_first_accepts_twenty_equal_snapshots() {
        let first = snap("before", 10, Some((5, 10)), false);
        let mut list = vec![first.clone()];
        for i in 0..20 {
            list.push(snap(&format!("c{i}"), 10, Some((5, 10)), false));
        }
        assert!(all_same_as_first(&list));
    }

    #[test]
    fn all_same_as_first_fails_when_one_differs_or_our_app_is_active() {
        let mut list = vec![snap("before", 10, Some((5, 10)), false)];
        for i in 0..5 {
            list.push(snap(&format!("c{i}"), 10, Some((5, 10)), false));
        }
        let mut other = list.clone();
        other[3] = snap("c2", 99, Some((5, 99)), false);
        assert!(!all_same_as_first(&other));
        list[4].own_active = true;
        assert!(!all_same_as_first(&list));
        assert!(!all_same_as_first(&[]));
    }
}
