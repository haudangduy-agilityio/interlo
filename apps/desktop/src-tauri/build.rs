/// Every command the webview may call. Each one gets an `allow-<name>` permission
/// that `capabilities/default.json` grants to the main window; a command that is
/// not listed here and granted there is refused. Keep in step with
/// `generate_handler!` in `src/lib.rs` (the test `security_config` checks it).
const COMMANDS: &[&str] = &[
    "check_setup",
    "list_models",
    "download_model",
    "cancel_model_download",
    "start_meeting",
    "stop_meeting",
    "set_mute",
    "list_meetings",
    "get_settings",
    "set_setting",
    "list_notes",
    "add_note",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("tauri build failed: {error:#}");
        std::process::exit(1);
    }
}
