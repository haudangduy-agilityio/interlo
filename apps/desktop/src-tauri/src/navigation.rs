//! Navigation guard for the webview.
//!
//! The webview may show the app's own pages and nothing else: a link, a
//! redirect or a script cannot move the window to another origin, where the
//! command bridge would be exposed to foreign content.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::webview::PageLoadEvent;
use tauri::{Manager, Runtime, Url};

#[cfg(feature = "demo-data")]
use crate::commands::meeting::MeetingState;
#[cfg(not(feature = "demo-data"))]
use crate::session::Session;

/// Allow the app's own origin (`tauri://localhost`) and, in development, the
/// dev server origin. Everything else is denied.
pub fn is_allowed_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let app_origin =
        url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none();
    let dev_origin = dev_url.is_some_and(|dev| {
        url.scheme() == dev.scheme()
            && url.host_str() == dev.host_str()
            && url.port_or_known_default() == dev.port_or_known_default()
    });
    app_origin || dev_origin
}

/// Plugin that applies `is_allowed_navigation` to every webview.
pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navigation-guard")
        .on_navigation(|webview, url| {
            let dev_url = if tauri::is_dev() {
                webview.app_handle().config().build.dev_url.clone()
            } else {
                None
            };
            is_allowed_navigation(url, dev_url.as_ref())
        })
        .on_page_load(|webview, payload| {
            let app = webview.app_handle();
            #[cfg(not(feature = "demo-data"))]
            {
                let session = app.state::<Session>();
                stop_session_for_page_load(
                    webview.label(),
                    payload.event(),
                    session.is_active(),
                    || {
                        let _ = session.stop(app);
                    },
                );
            }
            #[cfg(feature = "demo-data")]
            {
                let state = app.state::<MeetingState>();
                stop_session_for_page_load(
                    webview.label(),
                    payload.event(),
                    state.is_running(),
                    || {
                        let _ = crate::commands::meeting::stop(&state, app);
                    },
                );
            }
        })
        .build()
}

/// Stop once when the page begins loading, but leave the first idle load alone.
fn stop_session_for_page_load(
    label: &str,
    event: PageLoadEvent,
    active: bool,
    stop: impl FnOnce(),
) {
    if label == "main" && event == PageLoadEvent::Started && active {
        stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Url;

    #[test]
    fn a_page_load_stops_only_an_active_session() {
        use crate::session::testing::Rig;
        use tauri::webview::PageLoadEvent;

        let rig = Rig::new();
        stop_session_for_page_load(
            "main",
            PageLoadEvent::Started,
            rig.session.is_active(),
            || {
                rig.stop().unwrap();
            },
        );
        assert!(!rig.session.is_active());
        assert!(
            rig.events.all().is_empty(),
            "idle page load has no error/event"
        );
        assert_eq!(rig.fake.open_captures(), 0);

        rig.start().unwrap();
        stop_session_for_page_load(
            "main",
            PageLoadEvent::Finished,
            rig.session.is_active(),
            || {
                panic!("the finished event must not stop twice");
            },
        );
        assert!(rig.session.is_active(), "finish is not a second stop");

        stop_session_for_page_load(
            "captions",
            PageLoadEvent::Started,
            rig.session.is_active(),
            || {
                rig.stop().unwrap();
            },
        );
        assert!(
            rig.session.is_active(),
            "another webview load does not stop the session"
        );
        assert_eq!(rig.fake.open_captures(), 1);

        stop_session_for_page_load(
            "main",
            PageLoadEvent::Started,
            rig.session.is_active(),
            || {
                rig.stop().unwrap();
            },
        );
        assert!(!rig.session.is_active());
        assert_eq!(
            rig.fake.open_captures(),
            0,
            "page load closes the microphone"
        );
        assert_eq!(
            rig.events.statuses().last(),
            Some(&crate::events::RunState::Idle)
        );
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn app_origin_is_allowed() {
        assert!(is_allowed_navigation(&url("tauri://localhost/"), None));
        assert!(is_allowed_navigation(
            &url("tauri://localhost/index.html#/notes"),
            None
        ));
    }

    #[test]
    fn other_origins_are_denied() {
        for text in [
            "https://example.com/",
            "http://localhost:5173/",
            "http://localhost/",
            "tauri://evil/",
            "tauri://localhost.evil.com/",
            "tauri://localhost:8080/",
            "tauri://user@evil.com/",
            "file:///etc/hosts",
            "about:blank",
            "data:text/html,hello",
            "javascript:alert(1)",
            "ipc://localhost/",
            "asset://localhost/x",
        ] {
            assert!(!is_allowed_navigation(&url(text), None), "{text}");
        }
    }

    #[test]
    fn dev_server_origin_is_allowed_only_when_given() {
        let dev = url("http://localhost:5173");
        assert!(is_allowed_navigation(
            &url("http://localhost:5173/src/main.tsx"),
            Some(&dev)
        ));
        for text in [
            "http://localhost:5174/",
            "https://localhost:5173/",
            "http://127.0.0.1:5173/",
            "http://localhost:5173@evil.com/",
            "http://evil.com:5173/",
        ] {
            assert!(!is_allowed_navigation(&url(text), Some(&dev)), "{text}");
        }
    }
}
