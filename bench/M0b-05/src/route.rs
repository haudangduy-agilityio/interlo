//! Capture route choice (D-15): process tap, then the 2-channel virtual device,
//! else refuse. A pure function of what the spike found, so every row of the
//! decision table is a unit test.

/// State of the audio-capture permission when the spike starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Refused,
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
    None,
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
    if inputs.permission == Permission::Refused {
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
        Route::None
    };
    RoutePlan { causes, route }
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
        lines.push(match self.route {
            Route::Tap => "route=tap".to_string(),
            Route::TwoCh => "route=2ch".to_string(),
            Route::None => "NO_CAPTURE_ROUTE 2ch=missing".to_string(),
        });
        lines
    }

    /// True when the spike must exit non-zero without capturing.
    pub fn refuses(&self) -> bool {
        self.route == Route::None
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
}
