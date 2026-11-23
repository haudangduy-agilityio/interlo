//! The capture run: pick the route (D-15), capture, print status lines and level
//! readings. Writes no audio anywhere; the only output is text on stdout.

use crate::analysis::SELF_TONE_MS;
use crate::capture::{Capture, RateWatch, TonePlayer};
use crate::cli::{EXIT_REFUSED, EXIT_RESTART_FAILED};
use crate::levels::{WindowLevels, FLOOR_DBFS};
use crate::log::emit;
use crate::monitor::{Monitor, MonitorEvent, SilenceFill};
use crate::opts::Opts;
use crate::ring::StatsRing;
use crate::route::{
    after_tap_error, choose_route, first_look, route_line, wait_step, Permission, Route,
    RouteInputs,
};
use crate::targets::{join, pids_of};
use crate::{ca, sig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Peak amplitude of the self tone (starting value; the harness lowers the output volume).
const SELF_TONE_AMPLITUDE: f32 = 0.2;
/// Longest wait for an answer to the permission dialog (seconds, starting value).
const PERMISSION_WAIT_SECONDS: u64 = 90;
/// How often the target app's process objects are looked up again (ms, starting value). An app
/// may create its audio helper process after the tap started, or restart it.
const RETARGET_POLL_MS: u128 = 1_000;
/// Wait for this process to get a Core Audio process object in the AC-12 control run (ms).
const OWN_OBJECT_WAIT_MS: u64 = 2_000;

/// The target app's pids and the process objects among them. With `include_self` (AC-12 control
/// only) this process is added to the objects, once it has one.
fn target_objects(marker: &str, include_self: bool) -> (Vec<u32>, Vec<u32>) {
    let pids = pids_of(marker);
    let mut objects: Vec<u32> = pids
        .iter()
        .filter_map(|p| ca::process_object_for_pid(*p))
        .collect();
    if include_self {
        let own = (0..OWN_OBJECT_WAIT_MS / 50).find_map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            ca::process_object_for_pid(std::process::id())
        });
        objects.extend(own);
    }
    objects.sort_unstable();
    objects.dedup();
    (pids, objects)
}

/// Decide the permission. When it is not decided yet, trigger the system dialog by creating
/// a tap once, then wait for the answer.
fn resolve_permission(objects: &[u32], ring: &Arc<StatsRing>) -> Permission {
    if let Some(p) = first_look(ca::fresh_permission(), !objects.is_empty()) {
        return p;
    }
    emit("PERMISSION_PROMPT waiting for the system dialog");
    let probe = Capture::start_tap(objects, Arc::clone(ring));
    if let Err(e) = &probe {
        emit(&format!("PERMISSION_PROBE_ERROR {e}"));
    }
    let deadline = Instant::now() + Duration::from_secs(PERMISSION_WAIT_SECONDS);
    let mut last_diag = Instant::now();
    let verdict = loop {
        let timed_out = Instant::now() >= deadline || sig::stopped();
        if let Some(p) = wait_step(ca::fresh_permission(), timed_out) {
            if timed_out && p == Permission::Refused {
                emit("PERMISSION_TIMEOUT no answer: counted as refused");
            }
            break p;
        }
        std::thread::sleep(Duration::from_millis(250));
        if last_diag.elapsed() >= Duration::from_secs(2) {
            last_diag = Instant::now();
            emit(&format!("PERMISSION_WAIT callbacks={}", ring.callbacks()));
        }
    };
    drop(probe);
    verdict
}

/// Everything the capture needs to start again after a sample rate change or a new target.
struct Session {
    ring: Arc<StatsRing>,
    marker: String,
    include_self: bool,
    route: Route,
    two_ch: Option<u32>,
    objects: Vec<u32>,
    capture: Capture,
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

/// Start the planned route. When the tap fails to start, fall back as D-15 says. `Err` carries
/// the exit code of a run that cannot capture.
fn start_capture(
    planned: Route,
    objects: &[u32],
    two_ch: Option<u32>,
    ignore_2ch: bool,
    ring: &Arc<StatsRing>,
) -> Result<(Capture, Route), i32> {
    match start_route(planned, objects, two_ch, ring) {
        Ok(c) => Ok((c, planned)),
        Err(e) if planned == Route::Tap => {
            emit(&format!("TAP_ERROR {e}"));
            let route = after_tap_error(two_ch.is_some(), ignore_2ch);
            emit(route_line(route));
            match start_route(route, objects, two_ch, ring) {
                Ok(c) => Ok((c, route)),
                Err(e2) => {
                    emit(&format!("CAPTURE_ERROR {e2}"));
                    Err(EXIT_REFUSED)
                }
            }
        }
        Err(e) => {
            emit(&format!("CAPTURE_ERROR {e}"));
            Err(EXIT_REFUSED)
        }
    }
}

impl Session {
    /// Stop the capture and start it again, with the target looked up anew.
    fn restart(&mut self, why: &str) -> Result<(), i32> {
        let started = Instant::now();
        if self.route == Route::Tap {
            let (_, objects) = target_objects(&self.marker, self.include_self);
            if !objects.is_empty() {
                self.objects = objects;
            }
        }
        // The old capture is stopped by `Drop` when it is replaced below.
        match start_route(self.route, &self.objects, self.two_ch, &self.ring) {
            Ok(c) => {
                emit(&format!(
                    "{why} rate={} took_ms={}",
                    c.rate_hz,
                    started.elapsed().as_millis()
                ));
                self.capture = c;
                Ok(())
            }
            Err(e) => {
                emit(&format!("RESTART_FAILED {e}"));
                Err(EXIT_RESTART_FAILED)
            }
        }
    }
}

/// The spike's own 1 kHz tone (AC-12), played after `IDLE_TONE`.
#[derive(Default)]
struct SelfTone {
    player: Option<TonePlayer>,
    end: Option<Instant>,
}

impl SelfTone {
    fn on_idle(&mut self) {
        if let Some(p) = &self.player {
            p.set_amplitude(SELF_TONE_AMPLITUDE);
        } else {
            match TonePlayer::start(SELF_TONE_AMPLITUDE) {
                Ok(p) => self.player = Some(p),
                Err(e) => emit(&format!("SELF_TONE_ERROR {e}")),
            }
        }
        if self.player.is_some() {
            emit("SELF_TONE_START");
            self.end = Some(Instant::now() + Duration::from_millis(SELF_TONE_MS));
        }
    }

    fn tick(&mut self) {
        if self.end.is_some_and(|end| Instant::now() >= end) {
            self.end = None;
            self.player = None;
            emit("SELF_TONE_END");
        }
    }
}

fn prepare_dirs() {
    // The folders that the AC-21 scan looks in exist from the start; nothing is written into them.
    let _ = std::fs::create_dir_all(std::env::temp_dir().join("m0b-05"));
    if let Some(home) = std::env::var_os("HOME") {
        let cache = std::path::Path::new(&home).join("Library/Caches/app.interlo.bench.m0b05");
        let _ = std::fs::create_dir_all(cache);
    }
}

fn spawn_stdin_reader() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Lines from the harness: an empty line is the end of the source, `mark <name>` a time mark.
fn handle_commands(rx: &mpsc::Receiver<String>) {
    while let Ok(line) = rx.try_recv() {
        let line = line.trim();
        if line.is_empty() {
            emit("SOURCE_END");
        } else if let Some(name) = line.strip_prefix("mark ") {
            emit(&format!("MARK {name}"));
        }
    }
}

fn handle_monitor_event(ev: MonitorEvent, self_tone_wanted: bool, tone: &mut SelfTone) {
    match ev {
        MonitorEvent::IdleFloor {
            floor_dbfs,
            tone_dbfs,
        } => {
            emit(&format!("IDLE_FLOOR dbfs={floor_dbfs:.1}"));
            emit(&format!("IDLE_TONE dbfs={tone_dbfs:.1}"));
            if self_tone_wanted {
                tone.on_idle();
            }
        }
        MonitorEvent::Silent => emit("SILENT"),
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
    prepare_dirs();
    if let Some(path) = &opts.pidfile {
        if let Err(e) = std::fs::write(path, std::process::id().to_string()) {
            emit(&format!("PIDFILE_ERROR {e}"));
        }
    }

    let ring = Arc::new(StatsRing::new());
    let marker = opts.target_arg.clone().unwrap_or_default();
    let mut tone = SelfTone::default();
    if opts.control_include_self {
        match TonePlayer::start(0.0) {
            Ok(p) => tone.player = Some(p),
            Err(e) => emit(&format!("SELF_TONE_ERROR {e}")),
        }
    }
    let (pids, objects) = target_objects(&marker, opts.control_include_self);
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
        return EXIT_REFUSED;
    }

    let (capture, route) = match start_capture(plan.route, &objects, two_ch, opts.ignore_2ch, &ring)
    {
        Ok(started) => started,
        Err(code) => return code,
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
    let mut last_rate = capture.rate_hz;
    let mut session = Session {
        ring: Arc::clone(&ring),
        marker,
        include_self: opts.control_include_self,
        route,
        two_ch,
        objects,
        capture,
    };

    let commands = spawn_stdin_reader();
    let changes = Arc::new(AtomicU64::new(0));
    let watch_device = ca::default_output_device();
    let _watch = watch_device.and_then(|d| RateWatch::new(d, &changes).ok());
    let mut seen_changes = 0;

    let mut monitor = Monitor::new();
    let mut index = 0u64;
    let mut last_stat = Instant::now();
    let mut last_retarget = Instant::now();
    let clock = Instant::now();
    let mut fill = SilenceFill::new(0);

    while !sig::stopped() {
        let now_ms = clock.elapsed().as_millis() as u64;
        let mut batch: Vec<(WindowLevels, bool)> = Vec::new();
        while let Some(w) = ring.pop() {
            fill.on_window(now_ms);
            batch.push((w, false));
        }
        if fill.poll(now_ms, ring.callbacks()) {
            let silent = WindowLevels {
                rms_dbfs: FLOOR_DBFS,
                tone_dbfs: FLOOR_DBFS,
            };
            batch.push((silent, true));
        }
        for (w, synth) in batch {
            index += 1;
            emit(&format!(
                "WIN i={index} level={:.1} tone={:.1}{}",
                w.rms_dbfs,
                w.tone_dbfs,
                if synth { " synth=1" } else { "" }
            ));
            for ev in monitor.push(w) {
                handle_monitor_event(ev, opts.self_tone, &mut tone);
            }
        }
        tone.tick();
        handle_commands(&commands);

        let now_changes = changes.load(Ordering::Relaxed);
        if now_changes != seen_changes {
            seen_changes = now_changes;
            let new_rate = watch_device.and_then(ca::device_rate).unwrap_or(0.0) as u32;
            emit(&format!("RATE_CHANGE from={last_rate} to={new_rate}"));
            if let Err(code) = session.restart("RESTART") {
                return code;
            }
            last_rate = session.capture.rate_hz;
        } else if session.route == Route::Tap
            && last_retarget.elapsed().as_millis() >= RETARGET_POLL_MS
        {
            last_retarget = Instant::now();
            let (_, fresh) = target_objects(&session.marker, session.include_self);
            if !fresh.is_empty() && fresh != session.objects {
                emit(&format!("RETARGET objects={}", join(&fresh)));
                if let Err(code) = session.restart("RETARGET_DONE") {
                    return code;
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
    0
}
