//! Which processes belong to the target app. Pure text parsing of `ps` output so the
//! rule is testable without a running app.

/// One line of `ps -axo pid=,ppid=,command=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
}

/// Parse `ps -axo pid=,ppid=,command=` output. Lines that do not parse are skipped.
pub fn parse_ps(text: &str) -> Vec<ProcEntry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse().ok()?;
            let ppid = parts.next()?.parse().ok()?;
            let rest = line.trim_start();
            let after_pid = rest.split_once(char::is_whitespace)?.1.trim_start();
            let command = after_pid
                .split_once(char::is_whitespace)?
                .1
                .trim()
                .to_string();
            if command.is_empty() {
                return None;
            }
            Some(ProcEntry { pid, ppid, command })
        })
        .collect()
}

/// Markers of the spike's own bundle: a process whose command holds it is never a target.
pub const OWN_BUNDLE_MARKER: &str = "/m0b05.app/";

/// Pids of the target app: every process whose command line holds `marker`, plus all their
/// descendants. The spike's own process, and any process of the spike's own bundle, is left out
/// so the tap never includes this app (AC-12). Sorted, unique.
pub fn select_target_pids(entries: &[ProcEntry], marker: &str, own_pid: u32) -> Vec<u32> {
    if marker.is_empty() {
        return Vec::new();
    }
    let allowed = |e: &ProcEntry| e.pid != own_pid && !e.command.contains(OWN_BUNDLE_MARKER);
    let mut selected: Vec<u32> = entries
        .iter()
        .filter(|e| allowed(e) && e.command.contains(marker))
        .map(|e| e.pid)
        .collect();
    let mut grew = true;
    while grew {
        grew = false;
        for e in entries {
            if allowed(e) && !selected.contains(&e.pid) && selected.contains(&e.ppid) {
                selected.push(e.pid);
                grew = true;
            }
        }
    }
    selected.sort_unstable();
    selected.dedup();
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "\
    1     0 /sbin/launchd
 4001     1 /Applications/Browser.app/Contents/MacOS/Browser --user-data-dir=/tmp/prof-a --no-first-run
 4002  4001 /Applications/Browser.app/Contents/Frameworks/Helper --type=utility --service-sandbox-type=audio
 4003  4001 /Applications/Browser.app/Contents/Frameworks/Helper --type=renderer
 4100     1 /Applications/Browser.app/Contents/MacOS/Browser --user-data-dir=/tmp/prof-b
 4101  4100 /Applications/Browser.app/Contents/Frameworks/Helper --type=utility --service-sandbox-type=audio
 5000     1 /tmp/x/m0b05.app/Contents/MacOS/m0b05 --target-arg --user-data-dir=/tmp/prof-a
 5001  4002 /bin/sleep 1
garbage line
";

    #[test]
    fn parse_ps_reads_pid_ppid_and_command() {
        let e = parse_ps(PS);
        assert_eq!(e.len(), 8);
        assert_eq!(e[1].pid, 4001);
        assert_eq!(e[1].ppid, 1);
        assert!(e[1].command.starts_with("/Applications/Browser.app"));
        assert!(e[1].command.ends_with("--no-first-run"));
    }

    #[test]
    fn selects_the_marked_process_and_its_descendants_only() {
        let e = parse_ps(PS);
        let pids = select_target_pids(&e, "--user-data-dir=/tmp/prof-a", 9999);
        assert_eq!(pids, vec![4001, 4002, 4003, 5001]);
    }

    #[test]
    fn other_instance_is_not_selected_fr68() {
        let e = parse_ps(PS);
        let pids = select_target_pids(&e, "--user-data-dir=/tmp/prof-b", 9999);
        assert_eq!(pids, vec![4100, 4101]);
    }

    #[test]
    fn own_bundle_process_is_never_selected_ac12() {
        let e = parse_ps(PS);
        let pids = select_target_pids(&e, "--user-data-dir=/tmp/prof-a", 5000);
        assert!(!pids.contains(&5000));
        let pids = select_target_pids(&e, "--user-data-dir=/tmp/prof-a", 1234);
        assert!(!pids.contains(&5000));
    }

    #[test]
    fn no_match_gives_no_pids() {
        let e = parse_ps(PS);
        assert!(select_target_pids(&e, "--user-data-dir=/tmp/none", 9999).is_empty());
    }

    #[test]
    fn empty_marker_selects_nothing() {
        let e = parse_ps(PS);
        assert!(select_target_pids(&e, "", 9999).is_empty());
    }
}
