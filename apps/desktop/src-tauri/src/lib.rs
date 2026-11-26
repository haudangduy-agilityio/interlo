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
pub mod models_dir;
pub mod native_confirm;
pub mod navigation;
pub mod session;

#[cfg(feature = "demo-data")]
use commands::meeting::MeetingState;
#[cfg(not(feature = "demo-data"))]
use session::{machine::ThisMac, model_check::ModelCheck, Session};
use tauri::Manager;

/// Build the app, register every command and run the event loop.
// A shell that cannot start has nothing to fall back to, so it aborts with a message.
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .plugin(navigation::guard())
        // Registered only for the native confirmation (`native_confirm.rs`): the
        // main window is granted no `dialog:` permission, so the webview cannot
        // call it.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            first_paint::show_after_delay(app.handle());
            // The sample flow of a `demo-data` build.
            #[cfg(feature = "demo-data")]
            app.manage(MeetingState::default());
            // The launch check of the model files starts here, on its own thread.
            #[cfg(not(feature = "demo-data"))]
            app.manage(Session::new(
                Box::new(ThisMac::default()),
                ModelCheck::launch(
                    models_dir::from_environment(),
                    model_store::packs(),
                    rustix::process::getuid().as_raw(),
                    Box::new(model_store::Sha256Hasher),
                ),
                session::policy::shared_policy(),
            ));
            Ok(())
        })
        .manage(model_store::DownloadRegistry::default())
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
