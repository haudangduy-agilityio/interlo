//! The main window starts hidden. The page shows it once it has mounted, so the
//! owner never sees the empty native window colour. This module is the safety net:
//! if the page never shows it, the shell does, after a short delay.

use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

/// How long the shell waits for the page before it shows the window itself.
const FALLBACK_DELAY: Duration = Duration::from_secs(3);

/// True when the fallback must show the window. An unknown state counts as hidden.
fn fallback_must_show(visible: Option<bool>) -> bool {
    visible != Some(true)
}

/// Show the main window after `FALLBACK_DELAY` unless the page already did.
pub fn show_after_delay<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK_DELAY);
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if fallback_must_show(window.is_visible().ok()) {
            // Nothing to report: a window that cannot be shown has no other path.
            let _ = window.show();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_shows_a_hidden_window() {
        assert!(fallback_must_show(Some(false)));
    }

    #[test]
    fn the_fallback_shows_a_window_whose_state_is_unknown() {
        assert!(fallback_must_show(None));
    }

    #[test]
    fn the_fallback_leaves_a_visible_window_alone() {
        assert!(!fallback_must_show(Some(true)));
    }

    #[test]
    fn the_fallback_delay_is_three_seconds_as_docs_ipc_contract_says() {
        assert_eq!(FALLBACK_DELAY, Duration::from_secs(3));
    }
}
