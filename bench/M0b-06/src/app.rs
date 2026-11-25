//! The spike run: one Tauri window with the flags of mechanism 1 (SPEC O-7), driven by lines on
//! standard input and logging one line per event on standard output.

use crate::command::{self, Command};
use crate::geometry::{page_point, panel_rect, pick_display, px_rect, Display, Rect};
use crate::log::emit;
use crate::native_panel;
use crate::platform;
use objc2::MainThreadMarker;
use std::io::BufRead;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, RunEvent, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const PANEL_W: f64 = 1200.0;
const PANEL_H: f64 = 120.0;
const BOTTOM_MARGIN: f64 = 120.0;
/// Time the system gets to settle after a show or hide before a snapshot is taken.
const SETTLE: Duration = Duration::from_millis(600);
/// How long the transparent prime stays before it ends.
const PRIME_TIME: Duration = Duration::from_millis(2000);

#[tauri::command]
fn panel_click(x: f64, y: f64) {
    emit(&format!("PANEL_CLICK {x:.0},{y:.0}"));
}

#[tauri::command]
fn panel_key() {
    emit("PANEL_KEY");
}

/// Runs `f` on the main thread and waits for its result.
fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(MainThreadMarker) -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            let _ = tx.send(f(mtm));
        }
    })
    .ok()?;
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

/// The window under test: a Tauri window made with flags, or the native panel.
enum Panel {
    Flags(Box<WebviewWindow>),
    Native,
}

struct Spike {
    app: AppHandle,
    panel: Panel,
    display: usize,
    click_through: bool,
}

impl Spike {
    fn displays(&self) -> Vec<Display> {
        on_main(&self.app, platform::displays).unwrap_or_default()
    }

    fn snapshot(&self, tag: &str) {
        let tag = tag.to_string();
        match on_main(&self.app, move |mtm| platform::snapshot(&tag, mtm)) {
            Some(s) => emit(&s.to_line()),
            None => emit("ERROR snapshot timed out"),
        }
    }

    /// Puts the panel on display `n` and logs where it is. Returns false when there is no such display.
    fn place(&mut self, n: usize) -> bool {
        let all = self.displays();
        let (Some(target), Some(main)) = (pick_display(&all, n), pick_display(&all, 1)) else {
            emit(&format!("ERROR no display {n} ({} found)", all.len()));
            return false;
        };
        let rect = panel_rect(target.frame, PANEL_W, PANEL_H, BOTTOM_MARGIN);
        // The window API counts from the top-left corner of the main display.
        let top = main.frame.h - (rect.y + rect.h);
        match &self.panel {
            Panel::Flags(window) => {
                let _ = window.set_size(LogicalSize::new(rect.w, rect.h));
                let _ = window.set_position(LogicalPosition::new(rect.x, top));
            }
            Panel::Native => {
                on_main(&self.app, move |_| {
                    native_panel::place(rect.x, rect.y, rect.w, rect.h)
                });
            }
        }
        self.display = n;
        let f = target.frame;
        emit(&format!(
            "PANEL {},{},{},{} display={n} frame={},{},{},{} scale={}",
            rect.x, rect.y, rect.w, rect.h, f.x, f.y, f.w, f.h, target.scale
        ));
        let px = px_rect(rect, f, target.scale);
        emit(&format!(
            "RECT_PX {} {} {} {}",
            px.left, px.top, px.width, px.height
        ));
        let (cx, cy) = page_point(rect, f);
        emit(&format!("PAGE_POINT {cx:.0} {cy:.0}"));
        self.report_actual(main.frame, target.scale);
        true
    }

    /// Reads the window position back from the window system, in the same space as `PANEL`.
    fn report_actual(&self, main: Rect, _scale: f64) {
        let Panel::Flags(window) = &self.panel else {
            return;
        };
        let factor = window.scale_factor().unwrap_or(1.0);
        if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
            let (x, w, h) = (
                f64::from(pos.x) / factor,
                f64::from(size.width) / factor,
                f64::from(size.height) / factor,
            );
            let y = main.h - (f64::from(pos.y) / factor + h);
            emit(&format!("PANEL_ACTUAL {x},{y},{w},{h}"));
        }
    }

    fn show(&mut self, n: Option<usize>) {
        let n = n.unwrap_or(self.display);
        if !self.place(n) {
            return;
        }
        self.apply_click_through();
        emit("SHOWN");
        match &self.panel {
            Panel::Flags(window) => {
                if let Err(e) = window.show() {
                    emit(&format!("ERROR show failed: {e}"));
                }
            }
            Panel::Native => {
                on_main(&self.app, |_| native_panel::show());
            }
        }
    }

    fn hide(&self) {
        emit("HIDDEN");
        match &self.panel {
            Panel::Flags(window) => {
                if let Err(e) = window.hide() {
                    emit(&format!("ERROR hide failed: {e}"));
                }
            }
            Panel::Native => {
                on_main(&self.app, |_| native_panel::hide());
            }
        }
    }

    fn apply_click_through(&self) {
        let on = self.click_through;
        match &self.panel {
            Panel::Flags(window) => {
                let _ = window.set_ignore_cursor_events(on);
            }
            Panel::Native => {
                on_main(&self.app, move |_| native_panel::set_click_through(on));
            }
        }
    }

    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Show(n) => self.show(n),
            Command::Hide => self.hide(),
            Command::Move(n) => {
                emit(&format!("MOVED display={n}"));
                self.place(n);
            }
            Command::ClickThrough(on) => {
                self.click_through = on;
                self.apply_click_through();
                emit(&format!("CLICKTHROUGH {}", if on { "on" } else { "off" }));
            }
            Command::Snapshot(tag) => self.snapshot(&tag),
            Command::Windows => emit(&platform::own_windows_line()),
            Command::Cycle(n) => {
                for i in 1..=n {
                    self.show(None);
                    std::thread::sleep(SETTLE);
                    self.snapshot(&format!("cyc-{i:02}-shown"));
                    self.hide();
                    std::thread::sleep(SETTLE);
                    self.snapshot(&format!("cyc-{i:02}-hidden"));
                }
            }
            Command::Quit => return false,
        }
        true
    }
}

fn write_file(path: Option<&str>, text: &str) {
    if let Some(path) = path {
        let _ = std::fs::write(path, text);
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Starts the spike and blocks until `quit`. `args` are the options after the program name.
pub fn run(args: &[String]) {
    let pidfile = option(args, "--pidfile");
    let exitfile = option(args, "--exitfile");
    let native = option(args, "--mechanism").as_deref() == Some("native");
    let prime = native && args.iter().any(|a| a == "--prime");

    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![panel_click, panel_key])
        .setup(move |app| {
            let handle = app.handle().clone();
            let panel = if native {
                on_main(&handle, native_panel::create);
                if prime {
                    on_main(&handle, |_| native_panel::prime_show());
                }
                Panel::Native
            } else {
                Panel::Flags(Box::new(
                    WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("panel.html".into()))
                        .inner_size(PANEL_W, PANEL_H)
                        .decorations(false)
                        .shadow(false)
                        .resizable(false)
                        .skip_taskbar(true)
                        .always_on_top(true)
                        .visible_on_all_workspaces(true)
                        .focused(false)
                        .focusable(false)
                        .visible(false)
                        .build()?,
                ))
            };
            let exitfile = exitfile.clone();
            write_file(pidfile.as_deref(), &std::process::id().to_string());
            emit(if native {
                "READY mechanism=native-panel"
            } else {
                "READY mechanism=tauri-flags"
            });
            std::thread::spawn(move || {
                if prime {
                    // Long enough for the launch activation to finish; the harness starts the
                    // test window after this.
                    std::thread::sleep(PRIME_TIME);
                    on_main(&handle, |_| native_panel::prime_end());
                    emit("PRIMED");
                }
                let mut spike = Spike {
                    app: handle.clone(),
                    panel,
                    display: 1,
                    click_through: true,
                };
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match command::parse(&line) {
                        Ok(c) => {
                            if !spike.handle(c) {
                                break;
                            }
                        }
                        Err(e) => emit(&format!("ERROR {e}")),
                    }
                }
                emit("QUIT");
                write_file(exitfile.as_deref(), "0");
                handle.exit(0);
            });
            Ok(())
        });
    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(e) => {
            emit(&format!("ERROR build failed: {e}"));
            write_file(option(args, "--exitfile").as_deref(), "1");
            std::process::exit(1);
        }
    };
    app.run(|_, event| {
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // Closing the last window must not end the run: only `quit` does.
            api.prevent_exit();
        }
    });
}
