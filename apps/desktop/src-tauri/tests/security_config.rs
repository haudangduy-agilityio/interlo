//! The webview security settings: CSP, and one explicit permission per command.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn json(relative: &str) -> Value {
    serde_json::from_str(&read(relative)).unwrap()
}

/// Command names listed in `generate_handler!` in `src/lib.rs`.
fn handler_commands() -> BTreeSet<String> {
    let lib = read("src/lib.rs");
    let body = lib
        .split("generate_handler![")
        .nth(1)
        .expect("generate_handler! in lib.rs")
        .split(']')
        .next()
        .unwrap();
    body.split(',')
        .filter_map(|entry| entry.trim().rsplit("::").next())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Command names in the `COMMANDS` list of `build.rs`.
fn manifest_commands() -> BTreeSet<String> {
    let build = read("build.rs");
    let body = build
        .split("COMMANDS: &[&str] = &[")
        .nth(1)
        .expect("COMMANDS list in build.rs")
        .split("];")
        .next()
        .unwrap();
    body.split(',')
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

#[test]
fn csp_locks_down_base_uri_forms_objects_and_framing() {
    let config = json("tauri.conf.json");
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    let directives: BTreeSet<&str> = csp.split(';').map(str::trim).collect();
    for required in [
        "default-src 'self'",
        "base-uri 'none'",
        "form-action 'none'",
        "object-src 'none'",
        "frame-ancestors 'none'",
    ] {
        assert!(
            directives.contains(required),
            "CSP lacks `{required}`: {csp}"
        );
    }
}

#[test]
fn every_command_in_the_handler_is_in_the_app_manifest() {
    let handler = handler_commands();
    assert_eq!(handler.len(), 12, "{handler:?}");
    assert_eq!(handler, manifest_commands());
}

#[test]
fn main_window_is_granted_exactly_the_app_commands_and_core_defaults() {
    let capability = json("capabilities/default.json");
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    let granted: BTreeSet<String> = capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    let mut expected: BTreeSet<String> = manifest_commands()
        .iter()
        .map(|c| format!("allow-{}", c.replace('_', "-")))
        .collect();
    expected.insert("core:default".to_string());
    // The page shows the window that starts hidden (src/first_paint.rs); nothing else about windows.
    expected.insert("core:window:allow-show".to_string());
    assert_eq!(granted, expected);
}

/// The CSP as a map: directive name to its source list, in lowercase.
fn csp_directives() -> std::collections::BTreeMap<String, Vec<String>> {
    let config = json("tauri.conf.json");
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    csp.split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut parts = d.split_whitespace();
            let name = parts.next().unwrap().to_ascii_lowercase();
            (name, parts.map(str::to_string).collect())
        })
        .collect()
}

#[test]
fn csp_never_allows_eval_and_never_allows_inline_script() {
    let directives = csp_directives();
    for (name, sources) in &directives {
        assert!(
            !sources.iter().any(|s| s == "'unsafe-eval'"),
            "`{name}` allows 'unsafe-eval'"
        );
    }
    // With no script-src, the browser falls back to default-src for scripts.
    let script_sources = directives
        .get("script-src")
        .or_else(|| directives.get("default-src"))
        .expect("script-src or default-src");
    assert!(
        !script_sources.iter().any(|s| s == "'unsafe-inline'"),
        "script sources allow 'unsafe-inline': {script_sources:?}"
    );
    assert!(
        !script_sources
            .iter()
            .any(|s| s == "*" || s == "data:" || s == "blob:"),
        "script sources are wider than the app origin: {script_sources:?}"
    );
}

/// True when a UI source file or `index.html` writes an inline style in markup.
fn ui_has_inline_style() -> bool {
    fn visit(dir: &std::path::Path, found: &mut bool) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, found);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let is_source = name.ends_with(".ts") || name.ends_with(".tsx");
            if !is_source || name.contains(".test.") {
                continue;
            }
            if fs::read_to_string(&path).unwrap().contains("style=") {
                *found = true;
            }
        }
    }
    let ui = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui");
    let mut found = false;
    visit(&ui.join("src"), &mut found);
    let html = fs::read_to_string(ui.join("index.html")).unwrap();
    found || html.contains("<style") || html.contains("style=")
}

#[test]
fn csp_keeps_unsafe_inline_for_styles_only_while_the_ui_writes_inline_styles() {
    // A ratchet: `'unsafe-inline'` in style-src is allowed only while a UI file
    // writes `style=` in markup. Today none does (`DownloadProgress.tsx` sets the
    // bar width through the element's style object), so the CSP must not allow it.
    // A new `style=` fails this test; set the value from script or use a class.
    let directives = csp_directives();
    let styles = directives.get("style-src").expect("style-src");
    let allows_inline = styles.iter().any(|s| s == "'unsafe-inline'");
    assert_eq!(
        allows_inline,
        ui_has_inline_style(),
        "style-src {styles:?} and the UI's inline styles disagree"
    );
}

#[test]
fn the_global_tauri_bridge_is_off_and_the_ui_uses_the_package() {
    // The UI calls commands and listens to events through the @tauri-apps/api
    // package (apps/ui/src/ipc/tauriTransport.ts), which needs only the internals
    // the shell always injects. With the global API off, a script in the page has
    // no `window.__TAURI__` name to reach for. The per-command grants in
    // `capabilities/` and the CSP script rules above stay the controls.
    let config = json("tauri.conf.json");
    assert_eq!(config["app"]["withGlobalTauri"], Value::Bool(false));
}
