//! Navigation guard for the webview.
//!
//! The webview may show the app's own pages and nothing else: a link, a
//! redirect or a script cannot move the window to another origin, where the
//! command bridge would be exposed to foreign content.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime, Url};

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
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Url;

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
