//! Capture route choice (D-15): process tap, then the 2-channel virtual device,
//! else refuse. A pure function of what the spike found, so every row of the
//! decision table is a unit test.

/// State of the audio-capture permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Refused,
    /// The system has not decided yet (the dialog was not answered), or the check is unavailable.
    Undecided,
}

/// Why the process tap cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    PermissionRefused,
    AppMissing,
}

/// The route the spike captures from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Tap,
    TwoCh,
    /// No route: the only case in which the spike refuses to capture.
    Refused,
}

/// What the spike found at start.
#[derive(Debug, Clone, Copy)]
pub struct RouteInputs {
    pub permission: Permission,
    pub target_running: bool,
    pub two_ch_found: bool,
    /// The `--ignore-2ch` switch: act as if the 2-channel device is not installed.
    pub ignore_2ch: bool,
}

/// Causes (in print order) and the chosen route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    pub causes: Vec<Cause>,
    pub route: Route,
}

pub fn choose_route(inputs: &RouteInputs) -> RoutePlan {
    let mut causes = Vec::new();
    if inputs.permission != Permission::Granted {
        causes.push(Cause::PermissionRefused);
    }
    if !inputs.target_running {
        causes.push(Cause::AppMissing);
    }
    let route = if causes.is_empty() {
        Route::Tap
    } else if inputs.two_ch_found && !inputs.ignore_2ch {
        Route::TwoCh
    } else {
        Route::Refused
    };
    RoutePlan { causes, route }
}

/// First look at the permission. Returns the answer when no dialog is needed, `None` when the
/// system must be asked. A tap needs a target process, so with no target the dialog cannot be
/// triggered and an undecided permission counts as granted (the tap is not going to be used).
pub fn first_look(seen: Permission, has_target_process: bool) -> Option<Permission> {
    match seen {
        Permission::Undecided if has_target_process => None,
        Permission::Undecided => Some(Permission::Granted),
        decided => Some(decided),
    }
}

/// One step of waiting for the dialog: `Some` when the wait is over. A timeout, or a stop
/// signal, counts as refused: nothing allowed the capture.
pub fn wait_step(seen: Permission, timed_out: bool) -> Option<Permission> {
    match seen {
        Permission::Undecided if timed_out => Some(Permission::Refused),
        Permission::Undecided => None,
        decided => Some(decided),
    }
}

/// The tap failed to start although the plan chose it: use the 2-channel device when allowed,
/// else refuse (D-15).
pub fn after_tap_error(two_ch_found: bool, ignore_2ch: bool) -> Route {
    if two_ch_found && !ignore_2ch {
        Route::TwoCh
    } else {
        Route::Refused
    }
}

/// The status line of a route.
pub fn route_line(route: Route) -> &'static str {
    match route {
        Route::Tap => "route=tap",
        Route::TwoCh => "route=2ch",
        Route::Refused => "NO_CAPTURE_ROUTE 2ch=missing",
    }
}

impl RoutePlan {
    /// The status lines in print order: each cause, then the route line.
    pub fn status_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .causes
            .iter()
            .map(|c| match c {
                Cause::PermissionRefused => "PERMISSION_REFUSED".to_string(),
                Cause::AppMissing => "app=missing".to_string(),
            })
            .collect();
        lines.push(route_line(self.route).to_string());
        lines
    }

    /// True when the spike must exit non-zero without capturing.
    pub fn refuses(&self) -> bool {
        self.route == Route::Refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        permission: Permission,
        target_running: bool,
        two_ch_found: bool,
        ignore_2ch: bool,
    ) -> RoutePlan {
        choose_route(&RouteInputs {
            permission,
            target_running,
            two_ch_found,
            ignore_2ch,
        })
    }

    #[test]
    fn tap_is_used_when_permission_and_target_are_fine() {
        let p = plan(Permission::Granted, true, true, false);
        assert_eq!(p.route, Route::Tap);
        assert_eq!(p.status_lines(), vec!["route=tap"]);
        assert!(!p.refuses());
    }

    #[test]
    fn refused_permission_falls_back_to_2ch_ac17() {
        let p = plan(Permission::Refused, true, true, false);
        assert_eq!(p.status_lines(), vec!["PERMISSION_REFUSED", "route=2ch"]);
        assert!(!p.refuses());
    }

    #[test]
    fn missing_target_falls_back_to_2ch_ac20() {
        let p = plan(Permission::Granted, false, true, false);
        assert_eq!(p.status_lines(), vec!["app=missing", "route=2ch"]);
    }

    #[test]
    fn both_causes_print_in_order_then_2ch_ac40() {
        let p = plan(Permission::Refused, false, true, false);
        assert_eq!(
            p.status_lines(),
            vec!["PERMISSION_REFUSED", "app=missing", "route=2ch"]
        );
        assert!(!p.refuses());
    }

    #[test]
    fn ignoring_2ch_keeps_the_tap_ac35() {
        let p = plan(Permission::Granted, true, true, true);
        assert_eq!(p.status_lines(), vec!["route=tap"]);
    }

    #[test]
    fn no_route_when_tap_fails_and_2ch_is_ignored_ac36() {
        let p = plan(Permission::Refused, false, true, true);
        assert_eq!(
            p.status_lines(),
            vec![
                "PERMISSION_REFUSED",
                "app=missing",
                "NO_CAPTURE_ROUTE 2ch=missing"
            ]
        );
        assert!(p.refuses());
    }

    #[test]
    fn single_cause_without_2ch_refuses() {
        let p = plan(Permission::Refused, true, false, false);
        assert_eq!(
            p.status_lines(),
            vec!["PERMISSION_REFUSED", "NO_CAPTURE_ROUTE 2ch=missing"]
        );
        assert!(p.refuses());
    }

    #[test]
    fn missing_target_without_2ch_refuses() {
        let p = plan(Permission::Granted, false, false, false);
        assert_eq!(
            p.status_lines(),
            vec!["app=missing", "NO_CAPTURE_ROUTE 2ch=missing"]
        );
        assert!(p.refuses());
    }

    #[test]
    fn tap_without_2ch_device_still_captures() {
        let p = plan(Permission::Granted, true, false, false);
        assert_eq!(p.route, Route::Tap);
        assert!(!p.refuses());
    }

    #[test]
    fn first_look_asks_the_system_only_when_undecided_and_a_target_exists() {
        assert_eq!(
            first_look(Permission::Granted, true),
            Some(Permission::Granted)
        );
        assert_eq!(
            first_look(Permission::Refused, false),
            Some(Permission::Refused)
        );
        assert_eq!(first_look(Permission::Undecided, true), None);
        assert_eq!(
            first_look(Permission::Undecided, false),
            Some(Permission::Granted)
        );
    }

    #[test]
    fn waiting_ends_on_an_answer_a_timeout_or_a_stop() {
        assert_eq!(
            wait_step(Permission::Granted, false),
            Some(Permission::Granted)
        );
        assert_eq!(
            wait_step(Permission::Refused, false),
            Some(Permission::Refused)
        );
        assert_eq!(wait_step(Permission::Undecided, false), None);
        assert_eq!(
            wait_step(Permission::Undecided, true),
            Some(Permission::Refused)
        );
    }

    #[test]
    fn a_tap_that_fails_to_start_falls_back_to_2ch_or_refuses() {
        assert_eq!(after_tap_error(true, false), Route::TwoCh);
        assert_eq!(after_tap_error(true, true), Route::Refused);
        assert_eq!(after_tap_error(false, false), Route::Refused);
    }

    #[test]
    fn route_lines_are_the_ones_the_checks_read() {
        assert_eq!(route_line(Route::Tap), "route=tap");
        assert_eq!(route_line(Route::TwoCh), "route=2ch");
        assert_eq!(route_line(Route::Refused), "NO_CAPTURE_ROUTE 2ch=missing");
    }

    #[test]
    fn undecided_permission_is_not_used_for_the_tap() {
        let p = plan(Permission::Undecided, true, true, false);
        assert_eq!(p.status_lines(), vec!["PERMISSION_REFUSED", "route=2ch"]);
    }
}
