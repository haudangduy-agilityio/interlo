//! Offline checks over the logs of a run: the spike log, the test page log and the socket samples.
//! They read text only; the screenshot pixels are read by `harness/shot_check.py`.

use crate::log::parse_line;
use crate::snapshot::{all_same_as_first, same_focus, Snapshot};

/// The result of one check: a verdict and the deciding reason, printed on one line.
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub pass: bool,
    pub reason: String,
}

fn yes(reason: impl Into<String>) -> Verdict {
    Verdict {
        pass: true,
        reason: reason.into(),
    }
}

fn no(reason: impl Into<String>) -> Verdict {
    Verdict {
        pass: false,
        reason: reason.into(),
    }
}

type Entry<'a> = (u128, &'a str, &'a str);

fn entries(text: &str) -> Vec<Entry<'_>> {
    text.lines().filter_map(parse_line).collect()
}

fn snapshots(entries: &[Entry<'_>]) -> Vec<(u128, Snapshot)> {
    entries
        .iter()
        .filter(|(_, event, _)| *event == "SNAP")
        .filter_map(|(ts, event, rest)| {
            Snapshot::parse(&format!("{event} {rest}")).map(|s| (*ts, s))
        })
        .collect()
}

/// The value of `name=<number>` in the rest of a line.
fn number(rest: &str, name: &str) -> Option<f64> {
    rest.split_whitespace()
        .find_map(|w| w.strip_prefix(name)?.strip_prefix('='))
        .and_then(|v| v.parse().ok())
}

/// AC-23: the same frontmost application and window before and after `SHOWN`, this spike not
/// active, and a key typed after `SHOWN` reached the test page and not the panel.
pub fn ac23(spike: &str, page: &str) -> Verdict {
    let spike_entries = entries(spike);
    let snaps = snapshots(&spike_entries);
    let Some((before_ts, before)) = snaps.iter().find(|(_, s)| s.tag == "before") else {
        return no("no `before` snapshot in the spike log");
    };
    let Some(shown_ts) = spike_entries
        .iter()
        .find(|(ts, e, _)| *e == "SHOWN" && ts >= before_ts)
        .map(|(ts, _, _)| *ts)
    else {
        return no("no SHOWN event after the `before` snapshot");
    };
    let Some((_, after)) = snaps
        .iter()
        .find(|(ts, s)| s.tag == "after" && *ts >= shown_ts)
    else {
        return no("no `after` snapshot after SHOWN");
    };
    if !same_focus(before, after) {
        return no(format!(
            "focus changed: before front_pid={} window={:?}, after front_pid={} window={:?}",
            before.front_pid, before.window, after.front_pid, after.window
        ));
    }
    if after.own_active {
        return no("the spike application is active after SHOWN");
    }
    if spike_entries.iter().any(|(_, e, _)| *e == "PANEL_KEY") {
        return no("the panel received a key");
    }
    let key = entries(page)
        .into_iter()
        .find(|(ts, e, _)| *e == "KEY" && *ts > shown_ts);
    match key {
        Some((ts, _, _)) => yes(format!(
            "same front_pid={} window={:?} before and after; key reached the test page {} ms after SHOWN",
            after.front_pid,
            after.window,
            ts - shown_ts
        )),
        None => no("no key reached the test page after SHOWN"),
    }
}

/// AC-25 and its control, for the `target_index`-th `TARGET` line of the page log (1 is the
/// first). The window of the check runs from that line to the next `TARGET` line.
/// `expect_panel_click` false: the page logged a click inside the target square and the panel
/// logged none. True (control, click-through off): the panel logged a click and the page none.
pub fn ac25(spike: &str, page: &str, target_index: usize, expect_panel_click: bool) -> Verdict {
    let page_entries = entries(page);
    let targets: Vec<&Entry<'_>> = page_entries
        .iter()
        .filter(|(_, e, _)| *e == "TARGET")
        .collect();
    let Some((start, _, rest)) = target_index
        .checked_sub(1)
        .and_then(|i| targets.get(i))
        .map(|t| **t)
    else {
        return no(format!(
            "the page log has no TARGET line number {target_index}"
        ));
    };
    let spike_entries = entries(spike);
    // The window of the control ends when the harness turns click-through back on. The window of
    // the real check ends when the harness turns it off for the control.
    let switch = spike_entries
        .iter()
        .find(|(ts, e, rest)| {
            *e == "CLICKTHROUGH"
                && *ts >= start
                && rest.trim() == if expect_panel_click { "on" } else { "off" }
        })
        .map_or(u128::MAX, |t| t.0);
    let end = targets
        .get(target_index)
        .map_or(u128::MAX, |t| t.0)
        .min(switch);
    let (Some(tx), Some(ty), Some(size)) =
        (number(rest, "x"), number(rest, "y"), number(rest, "size"))
    else {
        return no("the TARGET line has no x, y and size");
    };
    let in_window = |ts: u128| ts >= start && ts < end;
    let page_clicks: Vec<(f64, f64)> = page_entries
        .iter()
        .filter(|(ts, e, _)| *e == "CLICK" && in_window(*ts))
        .filter_map(|(_, _, r)| Some((number(r, "x")?, number(r, "y")?)))
        .collect();
    let panel_clicks = spike_entries
        .iter()
        .filter(|(ts, e, _)| *e == "PANEL_CLICK" && in_window(*ts))
        .count();
    let inside = page_clicks
        .iter()
        .filter(|(x, y)| (x - tx).abs() <= size / 2.0 && (y - ty).abs() <= size / 2.0)
        .count();
    if expect_panel_click {
        if panel_clicks == 0 {
            no("control: the panel logged no click with click-through off")
        } else if !page_clicks.is_empty() {
            no(format!(
                "control: the page also logged {} click(s)",
                page_clicks.len()
            ))
        } else {
            yes(format!(
                "control: the panel logged {panel_clicks} click(s) and the page none"
            ))
        }
    } else if panel_clicks > 0 {
        no(format!("the panel logged {panel_clicks} click(s)"))
    } else if inside == 0 {
        no(format!(
            "no page click inside the {size} point square ({} click(s) elsewhere)",
            page_clicks.len()
        ))
    } else {
        yes(format!(
            "{inside} click(s) reached the test page inside the square; the panel logged none"
        ))
    }
}

/// AC-29: the baseline snapshot `before`, then 20 snapshots tagged `cyc-NN-shown` or
/// `cyc-NN-hidden`, all equal to the baseline, with at least 10 `SHOWN` and 10 `HIDDEN` events.
pub fn ac29(spike: &str) -> Verdict {
    let all = entries(spike);
    let snaps = snapshots(&all);
    let Some((_, base)) = snaps.iter().find(|(_, s)| s.tag == "before") else {
        return no("no `before` snapshot in the spike log");
    };
    let cycle: Vec<Snapshot> = snaps
        .iter()
        .filter(|(_, s)| s.tag.starts_with("cyc-"))
        .map(|(_, s)| s.clone())
        .collect();
    if cycle.len() != 20 {
        return no(format!(
            "expected 20 cycle snapshots, found {}",
            cycle.len()
        ));
    }
    let shown = all.iter().filter(|(_, e, _)| *e == "SHOWN").count();
    let hidden = all.iter().filter(|(_, e, _)| *e == "HIDDEN").count();
    if shown < 10 || hidden < 10 {
        return no(format!(
            "expected 10 SHOWN and 10 HIDDEN events, found {shown} and {hidden}"
        ));
    }
    let mut list = vec![base.clone()];
    list.extend(cycle);
    if all_same_as_first(&list) {
        yes(format!(
            "all 20 snapshots equal the baseline (front_pid={})",
            base.front_pid
        ))
    } else {
        no("one of the 20 snapshots differs from the baseline, or the spike was active")
    }
}

/// The screenshot was taken `target_ms` after `shown_ts`, within `tol_ms`: both the start and
/// the end of the capture command lie in the window.
pub fn shot_timing(
    shown_ts: u128,
    start: u128,
    end: u128,
    target_ms: u128,
    tol_ms: u128,
) -> Verdict {
    let low = shown_ts + target_ms.saturating_sub(tol_ms);
    let high = shown_ts + target_ms + tol_ms;
    if start <= end && start >= low && end <= high {
        yes(format!(
            "capture {}..{} ms after the event, window {}..{}",
            start - shown_ts,
            end - shown_ts,
            low - shown_ts,
            high - shown_ts
        ))
    } else {
        no(format!(
            "capture {}..{} ms is outside {}..{} ms after the event",
            start.saturating_sub(shown_ts),
            end.saturating_sub(shown_ts),
            low - shown_ts,
            high - shown_ts
        ))
    }
}

/// The test page said it filled the display (native full-screen).
pub fn fullscreen(page: &str) -> Verdict {
    let last = entries(page)
        .into_iter()
        .rfind(|(_, e, _)| *e == "FULLSCREEN");
    let answer = |rest: &str| {
        rest.split_whitespace()
            .find_map(|w| w.strip_prefix("v="))
            .map(str::to_string)
    };
    match last.map(|(_, _, rest)| answer(rest)) {
        Some(Some(v)) if v == "yes" => yes("the page fills the display"),
        Some(Some(v)) if v == "no" => no("the last report says the page does not fill the display"),
        Some(_) => no("the last size report is not readable"),
        None => no("the page never reported its size"),
    }
}

/// AC-31: no socket line with a remote address that is not loopback and no wildcard listener.
/// The text is the output of `lsof -nP -a -p <pid> -i` with `# sample` and `# lsof exit` lines.
pub fn lsof(log: &str) -> Verdict {
    let samples = log.lines().filter(|l| l.starts_with("# sample")).count();
    if samples == 0 {
        return no("no sample in the log");
    }
    let mut sockets = 0;
    for line in log
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("COMMAND"))
    {
        if line.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(pos) = words.iter().position(|w| *w == "TCP" || *w == "UDP") else {
            return no(format!("unreadable socket line: {line}"));
        };
        let Some(name) = words.get(pos + 1) else {
            return no(format!("unreadable socket line: {line}"));
        };
        sockets += 1;
        for endpoint in name.split("->") {
            let host = endpoint.rsplit_once(':').map_or(endpoint, |(h, _)| h);
            if matches!(host, "*" | "0.0.0.0" | "[::]") {
                return no(format!("wildcard listener: {line}"));
            }
            if !(host == "[::1]" || host == "localhost" || host.starts_with("127.")) {
                return no(format!("address that is not loopback: {line}"));
            }
        }
    }
    yes(format!(
        "{samples} sample(s), {sockets} socket line(s), all loopback, no wildcard listener"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP_B: &str =
        "SNAP before front_pid=10 front_bundle=app.example.browser window=5:10 own_active=0";
    const SNAP_A: &str =
        "SNAP after front_pid=10 front_bundle=app.example.browser window=5:10 own_active=0";

    fn spike_ok() -> String {
        format!("1000 {SNAP_B}\n1100 SHOWN\n1700 {SNAP_A}\n")
    }

    #[test]
    fn ac23_passes_when_focus_is_unchanged_and_the_key_reached_the_page() {
        let page = "1000 FULLSCREEN yes\n1500 STEP key\n2000 KEY key=a\n";
        let v = ac23(&spike_ok(), page);
        assert!(v.pass, "{}", v.reason);
    }

    #[test]
    fn ac23_says_no_when_the_frontmost_app_changed() {
        let spike = format!(
            "1000 {SNAP_B}\n1100 SHOWN\n1700 SNAP after front_pid=99 front_bundle=app.example.spike window=7:99 own_active=1\n"
        );
        let v = ac23(&spike, "2000 KEY key=a\n");
        assert!(!v.pass);
        assert!(v.reason.contains("focus"), "{}", v.reason);
    }

    #[test]
    fn ac23_says_no_when_our_app_is_active_even_if_the_front_app_is_unchanged() {
        let spike = format!(
            "1000 {SNAP_B}\n1100 SHOWN\n1700 SNAP after front_pid=10 front_bundle=app.example.browser window=5:10 own_active=1\n"
        );
        assert!(!ac23(&spike, "2000 KEY key=a\n").pass);
    }

    #[test]
    fn ac23_says_no_when_no_key_followed_the_show() {
        let v = ac23(&spike_ok(), "900 KEY key=a\n");
        assert!(!v.pass);
        assert!(v.reason.contains("key"), "{}", v.reason);
        assert!(!ac23(&spike_ok(), "").pass);
    }

    #[test]
    fn ac23_says_no_when_the_panel_got_the_key() {
        let spike = format!("{}1800 PANEL_KEY\n", spike_ok());
        assert!(!ac23(&spike, "2000 KEY key=a\n").pass);
    }

    #[test]
    fn ac23_says_no_when_a_snapshot_or_the_show_is_missing() {
        assert!(!ac23("1100 SHOWN\n", "2000 KEY key=a\n").pass);
        assert!(
            !ac23(
                &format!("1000 {SNAP_B}\n1700 {SNAP_A}\n"),
                "2000 KEY key=a\n"
            )
            .pass
        );
        assert!(!ac23(&format!("1000 {SNAP_B}\n1100 SHOWN\n"), "2000 KEY key=a\n").pass);
    }

    const PAGE_TARGET: &str = "100 TARGET x=960 y=900 size=80\n";

    #[test]
    fn ac25_passes_when_the_page_got_the_click_inside_the_square_and_the_panel_none() {
        let page = format!("{PAGE_TARGET}200 CLICK x=970 y=890\n");
        let v = ac25("150 SHOWN\n", &page, 1, false);
        assert!(v.pass, "{}", v.reason);
    }

    #[test]
    fn ac25_says_no_when_the_click_is_outside_the_square() {
        let page = format!("{PAGE_TARGET}200 CLICK x=1010 y=900\n");
        assert!(!ac25("150 SHOWN\n", &page, 1, false).pass);
        let page = format!("{PAGE_TARGET}200 CLICK x=960 y=939\n");
        assert!(ac25("150 SHOWN\n", &page, 1, false).pass);
        // exactly size / 2 from the centre is still inside
        let page = format!("{PAGE_TARGET}200 CLICK x=1000 y=900\n");
        assert!(ac25("150 SHOWN\n", &page, 1, false).pass);
    }

    #[test]
    fn ac25_says_no_when_the_panel_logged_a_click_or_the_page_got_none() {
        let page = format!("{PAGE_TARGET}200 CLICK x=960 y=900\n");
        assert!(!ac25("250 PANEL_CLICK 960,900\n", &page, 1, false).pass);
        assert!(!ac25("", PAGE_TARGET, 1, false).pass);
    }

    #[test]
    fn ac25_control_wants_a_panel_click_and_no_page_click_in_its_own_window() {
        let page = format!("{PAGE_TARGET}200 CLICK x=960 y=900\n300 TARGET x=960 y=900 size=80\n");
        let spike = "350 PANEL_CLICK 960,900\n";
        let v = ac25(spike, &page, 2, true);
        assert!(v.pass, "{}", v.reason);
        // the first window is the real check: its click belongs to the page, the panel click is in window 2
        assert!(ac25(spike, &page, 1, false).pass);
        assert!(!ac25("", &page, 2, true).pass);
        let both = format!("{page}350 CLICK x=960 y=900\n");
        assert!(!ac25(spike, &both, 2, true).pass);
    }

    #[test]
    fn ac25_counts_page_clicks_only_while_click_through_is_on() {
        // The owner clicked again after the harness turned click-through off for the control.
        let page = format!("{PAGE_TARGET}200 CLICK x=960 y=900\n");
        let spike = "120 CLICKTHROUGH off\n";
        assert!(!ac25(spike, &page, 1, false).pass);
        let early = format!("{PAGE_TARGET}110 CLICK x=960 y=900\n200 CLICK x=960 y=900\n");
        assert!(ac25(spike, &early, 1, false).pass);
    }

    #[test]
    fn ac25_control_ignores_page_clicks_after_click_through_is_back_on() {
        let page = format!("{PAGE_TARGET}300 CLICK x=960 y=900\n");
        let spike = "110 CLICKTHROUGH off\n130 PANEL_CLICK 5,5\n140 CLICKTHROUGH on\n";
        let v = ac25(spike, &page, 1, true);
        assert!(v.pass, "{}", v.reason);
        // a page click while click-through is off still fails the control
        let bad = format!("{PAGE_TARGET}120 CLICK x=960 y=900\n");
        assert!(!ac25(spike, &bad, 1, true).pass);
        // no panel click while click-through is off
        assert!(
            !ac25(
                "110 CLICKTHROUGH off\n140 CLICKTHROUGH on\n",
                &page,
                1,
                true
            )
            .pass
        );
    }

    #[test]
    fn ac25_says_no_for_a_target_index_that_does_not_exist() {
        assert!(!ac25("", PAGE_TARGET, 2, false).pass);
        assert!(!ac25("", PAGE_TARGET, 0, false).pass);
    }

    fn cycle_log(break_at: Option<usize>) -> String {
        let mut s = format!("1000 {SNAP_B}\n");
        for i in 1..=10 {
            for (n, what) in [(2 * i - 1, "shown"), (2 * i, "hidden")] {
                let pid = if break_at == Some(n) { 99 } else { 10 };
                s.push_str(&format!(
                    "{} {}\n",
                    2000 + n * 10,
                    if what == "shown" { "SHOWN" } else { "HIDDEN" }
                ));
                s.push_str(&format!(
                    "{} SNAP cyc-{i:02}-{what} front_pid={pid} front_bundle=app.example.browser window=5:{pid} own_active=0\n",
                    2005 + n * 10
                ));
            }
        }
        s
    }

    #[test]
    fn ac29_passes_for_twenty_snapshots_equal_to_the_baseline() {
        let v = ac29(&cycle_log(None));
        assert!(v.pass, "{}", v.reason);
    }

    #[test]
    fn ac29_says_no_when_one_of_the_twenty_differs() {
        assert!(!ac29(&cycle_log(Some(7))).pass);
    }

    #[test]
    fn ac29_says_no_for_a_missing_baseline_or_fewer_than_twenty_snapshots() {
        let log = cycle_log(None);
        let without_base: String = log.lines().skip(1).map(|l| format!("{l}\n")).collect();
        assert!(!ac29(&without_base).pass);
        let short: String = log.lines().take(30).map(|l| format!("{l}\n")).collect();
        assert!(!ac29(&short).pass);
    }

    #[test]
    fn shot_timing_accepts_both_ends_inside_the_window_and_refuses_outside() {
        assert!(shot_timing(10_000, 11_000, 11_300, 1000, 500).pass);
        assert!(shot_timing(10_000, 10_500, 11_500, 1000, 500).pass);
        assert!(!shot_timing(10_000, 10_499, 11_000, 1000, 500).pass);
        assert!(!shot_timing(10_000, 11_000, 11_501, 1000, 500).pass);
        assert!(shot_timing(10_000, 69_000, 70_000, 60_000, 2_000).pass);
        assert!(!shot_timing(10_000, 80_000, 80_300, 60_000, 2_000).pass);
        assert!(!shot_timing(10_000, 11_000, 10_900, 1000, 500).pass);
    }

    #[test]
    fn fullscreen_needs_the_last_report_to_say_yes() {
        let yes = "FULLSCREEN v=yes iw=3440 ih=1440 sw=3440 sh=1440";
        let no = "FULLSCREEN v=no iw=1200 ih=800 sw=3440 sh=1440";
        assert!(fullscreen(&format!("10 {yes}\n")).pass);
        assert!(!fullscreen(&format!("10 {yes}\n20 {no}\n")).pass);
        assert!(fullscreen(&format!("10 {no}\n20 {yes}\n")).pass);
        assert!(!fullscreen("").pass);
        assert!(!fullscreen("10 FULLSCREEN v=maybe\n").pass);
    }

    const LSOF_HEAD: &str = "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n";

    #[test]
    fn lsof_passes_for_no_sockets_and_for_loopback_only() {
        assert!(lsof("# sample 1\n# lsof exit=1\n").pass);
        let log = format!(
            "# sample 1\n{LSOF_HEAD}m0b06 41 u 12u IPv4 0x1 0t0 TCP 127.0.0.1:5000->127.0.0.1:5001 (ESTABLISHED)\nm0b06 41 u 13u IPv6 0x2 0t0 TCP [::1]:5002->[::1]:5003 (ESTABLISHED)\n# lsof exit=0\n"
        );
        let v = lsof(&log);
        assert!(v.pass, "{}", v.reason);
    }

    #[test]
    fn lsof_says_no_for_a_remote_address_or_a_wildcard_listener() {
        let remote = format!(
            "{LSOF_HEAD}m0b06 41 u 12u IPv4 0x1 0t0 TCP 192.168.1.5:5000->93.184.216.34:443 (ESTABLISHED)\n"
        );
        assert!(!lsof(&remote).pass);
        for name in [
            "*:8080 (LISTEN)",
            "0.0.0.0:8080 (LISTEN)",
            "[::]:8080 (LISTEN)",
        ] {
            let log = format!("{LSOF_HEAD}m0b06 41 u 12u IPv4 0x1 0t0 TCP {name}\n");
            assert!(!lsof(&log).pass, "{name}");
        }
    }

    #[test]
    fn lsof_says_no_for_an_empty_log_because_nothing_was_sampled() {
        assert!(!lsof("").pass);
    }
}
