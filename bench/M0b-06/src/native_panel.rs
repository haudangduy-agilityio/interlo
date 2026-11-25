//! Mechanism 2 of O-7: a native non-activating panel. It is a borderless `NSPanel` with the
//! non-activating style, a collection behaviour that lets it join every Space and a full-screen
//! one, and a solid colour. Its window level reads layer 3 (floating) in the window list: see
//! `create`. A click or key that reaches it is
//! logged as `PANEL_CLICK` or `PANEL_KEY`.

use crate::log::emit;
use objc2::rc::Retained;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSPanel, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::RefCell;

/// `kCGStatusWindowLevel`. `setFloatingPanel(true)` below runs after `setLevel` and sets the level
/// back to floating (3); the sitting read layer 3 for this panel, and the panel showed over the
/// full-screen window with it, so the collection behaviour is what made it visible there.
const STATUS_LEVEL: isize = 25;

define_class!(
    // SAFETY: `NSPanel` has no subclassing requirements. The overrides only read their argument
    // and write a log line; `canBecomeKeyWindow` answers no so the panel never takes the key.
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "M0b06SpikePanel"]
    struct SpikePanel;

    impl SpikePanel {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = event.locationInWindow();
            emit(&format!("PANEL_CLICK {:.0},{:.0}", p.x, p.y));
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, _event: &NSEvent) {
            emit("PANEL_KEY");
        }

        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            false
        }
    }
);

thread_local! {
    static PANEL: RefCell<Option<Retained<SpikePanel>>> = const { RefCell::new(None) };
}

fn with_panel(f: impl FnOnce(&SpikePanel)) {
    PANEL.with(|cell| {
        if let Some(panel) = cell.borrow().as_ref() {
            f(panel);
        }
    });
}

/// Creates the panel (hidden) on the main thread. Does nothing if it exists.
pub fn create(mtm: MainThreadMarker) {
    PANEL.with(|cell| {
        if cell.borrow().is_some() {
            return;
        }
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1200.0, 120.0));
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let this = mtm.alloc::<SpikePanel>().set_ivars(());
        // SAFETY: the designated initialiser of `NSPanel`, called once on the main thread.
        let made: Retained<SpikePanel> = unsafe {
            msg_send![super(this), initWithContentRect: rect, styleMask: style, backing: NSBackingStoreType::Buffered, defer: false]
        };
        let panel = made;
        // The colour of `harness/page.html` for the panel: #E6007E.
        let colour = NSColor::colorWithSRGBRed_green_blue_alpha(230.0 / 255.0, 0.0, 126.0 / 255.0, 1.0);
        panel.setBackgroundColor(Some(&colour));
        panel.setOpaque(true);
        panel.setHasShadow(false);
        panel.setLevel(STATUS_LEVEL);
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        // SAFETY: the window stays owned by this thread-local; AppKit must not release it on close.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary,
        );
        panel.setIgnoresMouseEvents(true);
        *cell.borrow_mut() = Some(panel);
    });
}

/// Moves and sizes the panel: `x`, `y` is the bottom-left corner in global points.
pub fn place(x: f64, y: f64, w: f64, h: f64) {
    with_panel(|p| p.setFrame_display(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)), true));
}

/// Shows the panel without making it key and without activating this application.
pub fn show() {
    with_panel(|p| p.orderFrontRegardless());
}

pub fn hide() {
    with_panel(|p| p.orderOut(None));
}

pub fn set_click_through(on: bool) {
    with_panel(|p| p.setIgnoresMouseEvents(on));
}

/// Orders the panel front once, fully transparent. A freshly launched application takes the
/// focus when its first window is ordered front; doing it here, before the test window exists,
/// uses that up (the `--prime` option; the sitting records the run without it too).
pub fn prime_show() {
    with_panel(|p| {
        p.setAlphaValue(0.0);
        p.orderFrontRegardless();
    });
}

/// Ends the prime: hides the panel and restores its opacity.
pub fn prime_end() {
    with_panel(|p| {
        p.orderOut(None);
        p.setAlphaValue(1.0);
    });
}
