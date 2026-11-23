//! The capture run: pick the route (D-15), capture, print status lines and level
//! readings. Writes no audio anywhere; the only output is text on stdout.

use crate::capture::{Capture, RateWatch, TonePlayer};
use crate::log::emit;
use crate::monitor::{Monitor, MonitorEvent};
use crate::opts::Opts;
use crate::ring::StatsRing;
use crate::route::{choose_route, Permission, Route, RouteInputs};
use crate::{ca, sig, targets};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Seconds the self tone plays (AC-12).
const SELF_TONE_SECONDS: u64 = 5;
/// Peak amplitude of the self tone (starting value; the harness lowers the output volume).
const SELF_TONE_AMPLITUDE: f32 = 0.2;
/// Longest wait for an answer to the permission dialog (seconds, starting value).
const PERMISSION_WAIT_SECONDS: u64 = 90;

/// Process objects of the target app (empty when it is not running or not using audio).
fn target_objects(marker: &str) -> (Vec<u32>, Vec<u32>) {
    let text = std::process::Command::new("/bin/ps")
        .args(["-axww", "-o", "pid=,ppid=,command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let entries = targets::parse_ps(&text);
    let pids = targets::select_target_pids(&entries, marker, std::process::id());
    let objects = pids
        .iter()
        .filter_map(|p| ca::process_object_for_pid(*p))
        .collect();
    (pids, objects)
}

fn join(values: &[u32]) -> String {
    values
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Decide the permission. When it is not decided yet, trigger the system dialog by creating
/// a tap once, then wait for the answer.
fn resolve_permission(objects: &[u32], ring: &Arc<StatsRing>) -> Permission {
    match ca::fresh_permission() {
        ca::Tcc::Granted => return Permission::Granted,
        ca::Tcc::Refused => return Permission::Refused,
        ca::Tcc::Unknown => {}
    }
    if objects.is_empty() {
        return Permission::Granted;
    }
    emit("PERMISSION_PROMPT waiting for the system dialog");
    let probe = Capture::start_tap(objects, Arc::clone(ring));
    if let Err(e) = &probe {
        emit(&format!("PERMISSION_PROBE_ERROR {e}"));
    }
    let deadline = Instant::now() + Duration::from_secs(PERMISSION_WAIT_SECONDS);
    let mut last_diag = Instant::now();
    let verdict = loop {
        match ca::fresh_permission() {
            ca::Tcc::Granted => break Permission::Granted,
            ca::Tcc::Refused => break Permission::Refused,
            ca::Tcc::Unknown if Instant::now() >= deadline || sig::stopped() => {
                break Permission::Refused
            }
            ca::Tcc::Unknown => {
                std::thread::sleep(Duration::from_millis(250));
                if last_diag.elapsed() >= Duration::from_secs(2) {
                    last_diag = Instant::now();
                    emit(&format!("PERMISSION_WAIT callbacks={}", ring.callbacks()));
                }
            }
        }
    };
    drop(probe);
    verdict
}

fn start_route(
    route: Route,
    objects: &[u32],
    two_ch: Option<u32>,
    ring: &Arc<StatsRing>,
) -> Result<Capture, String> {
    match (route, two_ch) {
        (Route::Tap, _) => Capture::start_tap(objects, Arc::clone(ring)),
        (Route::TwoCh, Some(device)) => Capture::start_device(device, Arc::clone(ring)),
        _ => Err("no capture route".to_string()),
    }
}

/// Run the spike. Returns the process exit code.
pub fn run(opts: &Opts) -> i32 {
    sig::install();
    emit(&format!(
        "START pid={} ignore_2ch={}",
        std::process::id(),
        opts.ignore_2ch
    ));
    if let Some(path) = &opts.pidfile {
        let _ = std::fs::write(path, std::process::id().to_string());
    }

    let ring = Arc::new(StatsRing::new());
    let marker = opts.target_arg.clone().unwrap_or_default();
    let mut player: Option<TonePlayer> = None;
    if opts.control_include_self {
        match TonePlayer::start(0.0) {
            Ok(p) => player = Some(p),
            Err(e) => emit(&format!("SELF_TONE_ERROR {e}")),
        }
    }
    let (pids, mut objects) = target_objects(&marker);
    if opts.control_include_self {
        let own = (0..40).find_map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            ca::process_object_for_pid(std::process::id())
        });
        objects.extend(own);
    }
    let two_ch = ca::find_two_ch_device();
    let permission = resolve_permission(&objects, &ring);
    let plan = choose_route(&RouteInputs {
        permission,
        target_running: !objects.is_empty(),
        two_ch_found: two_ch.is_some(),
        ignore_2ch: opts.ignore_2ch,
    });
    for line in plan.status_lines() {
        emit(&line);
    }
    if plan.refuses() {
        return 2;
    }

    let mut route = plan.route;
    let mut capture = match start_route(route, &objects, two_ch, &ring) {
        Ok(c) => c,
        Err(e) => {
            emit(&format!("TAP_ERROR {e}"));
            match (route, two_ch.filter(|_| !opts.ignore_2ch)) {
                (Route::Tap, Some(device)) => {
                    match Capture::start_device(device, Arc::clone(&ring)) {
                        Ok(c) => {
                            route = Route::TwoCh;
                            emit("route=2ch");
                            c
                        }
                        Err(e2) => {
                            emit(&format!("CAPTURE_ERROR {e2}"));
                            return 2;
                        }
                    }
                }
                _ => {
                    emit("NO_CAPTURE_ROUTE 2ch=missing");
                    return 2;
                }
            }
        }
    };
    let event = if route == Route::Tap {
        "TAP_START"
    } else {
        "DEVICE_START"
    };
    emit(&format!(
        "{event} rate={} pids={} objects={}",
        capture.rate_hz,
        join(&pids),
        join(&objects)
    ));

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let changes = Arc::new(AtomicU64::new(0));
    let watch_device = ca::default_output_device();
    let _watch = watch_device.and_then(|d| RateWatch::new(d, &changes).ok());
    let mut seen_changes = 0;
    let mut last_rate = capture.rate_hz;

    let mut monitor = Monitor::new();
    let mut index = 0u64;
    let mut tone_end: Option<Instant> = None;
    let mut last_stat = Instant::now();

    while !sig::stopped() {
        while let Some(w) = ring.pop() {
            index += 1;
            emit(&format!(
                "WIN i={index} level={:.1} tone={:.1}",
                w.rms_dbfs, w.tone_dbfs
            ));
            for ev in monitor.push(w) {
                match ev {
                    MonitorEvent::IdleFloor {
                        floor_dbfs,
                        tone_dbfs,
                    } => {
                        emit(&format!("IDLE_FLOOR dbfs={floor_dbfs:.1}"));
                        emit(&format!("IDLE_TONE dbfs={tone_dbfs:.1}"));
                        if opts.self_tone {
                            if let Some(p) = &player {
                                p.set_amplitude(SELF_TONE_AMPLITUDE);
                            } else {
                                match TonePlayer::start(SELF_TONE_AMPLITUDE) {
                                    Ok(p) => player = Some(p),
                                    Err(e) => emit(&format!("SELF_TONE_ERROR {e}")),
                                }
                            }
                            if player.is_some() {
                                emit("SELF_TONE_START");
                                tone_end =
                                    Some(Instant::now() + Duration::from_secs(SELF_TONE_SECONDS));
                            }
                        }
                    }
                    MonitorEvent::Silent => emit("SILENT"),
                }
            }
        }
        if tone_end.is_some_and(|end| Instant::now() >= end) {
            tone_end = None;
            player = None;
            emit("SELF_TONE_END");
        }
        while let Ok(line) = rx.try_recv() {
            let line = line.trim();
            if line.is_empty() {
                emit("SOURCE_END");
            } else if let Some(name) = line.strip_prefix("mark ") {
                emit(&format!("MARK {name}"));
            }
        }
        let now_changes = changes.load(Ordering::Relaxed);
        if now_changes != seen_changes {
            seen_changes = now_changes;
            let new_rate = watch_device.and_then(ca::device_rate).unwrap_or(0.0) as u32;
            emit(&format!("RATE_CHANGE from={last_rate} to={new_rate}"));
            let started = Instant::now();
            drop(capture);
            let (_, objects) = target_objects(&marker);
            match start_route(route, &objects, two_ch, &ring) {
                Ok(c) => {
                    last_rate = c.rate_hz;
                    emit(&format!(
                        "RESTART rate={} took_ms={}",
                        c.rate_hz,
                        started.elapsed().as_millis()
                    ));
                    capture = c;
                }
                Err(e) => {
                    emit(&format!("RESTART_FAILED {e}"));
                    return 3;
                }
            }
        }
        if last_stat.elapsed() >= Duration::from_secs(5) {
            last_stat = Instant::now();
            emit(&format!(
                "STAT callbacks={} dropped={}",
                ring.callbacks(),
                ring.dropped()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    emit("STOP");
    drop(capture);
    0
}
