//! Desktop shell for Interlo.
//!
//! The bridge between the webview and the Rust core owns commands for control
//! (request and response) and channels for streams (captions, latency, status).
//! It must never send settings or keys to the webview, and it must never allow
//! broad capabilities: the main window gets `core:default` and `core:window:allow-show`
//! (so the page can show the window that starts hidden), nothing else about windows.

pub mod clock;
pub mod commands;
pub mod events;
pub mod first_paint;
pub mod ipc;
pub mod native_confirm;
pub mod navigation;

use commands::meeting::MeetingState;

/// Build the app, register every command and run the event loop.
// A shell that cannot start has nothing to fall back to, so it aborts with a message.
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .plugin(navigation::guard())
        .setup(|app| {
            first_paint::show_after_delay(app.handle());
            Ok(())
        })
        .manage(MeetingState::default())
        .invoke_handler(tauri::generate_handler![
            commands::setup::check_setup,
            commands::models::list_models,
            commands::models::download_model,
            commands::models::cancel_model_download,
            commands::meeting::start_meeting,
            commands::meeting::stop_meeting,
            commands::meeting::set_mute,
            commands::history::list_meetings,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::notes::list_notes,
            commands::notes::add_note,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Interlo desktop shell");
}
