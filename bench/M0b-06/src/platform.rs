//! The macOS part: the displays, the frontmost application and its topmost window. This is the
//! only module that calls system APIs directly, and it reads identities and geometry only.

use crate::geometry::{Display, Rect};
use crate::snapshot::Snapshot;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSScreen, NSWorkspace};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFString, CFType};
use objc2_core_graphics::{
    kCGNullWindowID, kCGWindowLayer, kCGWindowNumber, kCGWindowOwnerPID,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};

/// The displays in the order of `NSScreen.screens`: the main display is first.
pub fn displays(mtm: MainThreadMarker) -> Vec<Display> {
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| {
            let f = screen.frame();
            Display {
                frame: Rect {
                    x: f.origin.x,
                    y: f.origin.y,
                    w: f.size.width,
                    h: f.size.height,
                },
                scale: screen.backingScaleFactor(),
            }
        })
        .collect()
}

fn int(dict: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<i64> {
    dict.get(key)?.downcast_ref::<CFNumber>()?.as_i64()
}

/// Window number and owner pid of the topmost on-screen window of layer 0 that belongs to `pid`.
fn topmost_window(pid: i32) -> Option<(i64, i32)> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID)?;
    // SAFETY: the list holds one dictionary per window; the keys are strings and the values
    // are numbers, strings or dictionaries, read back through `downcast_ref`.
    let list: &CFArray<CFDictionary<CFString, CFType>> = unsafe { list.cast_unchecked() };
    // SAFETY: the three keys are constant strings of the system, valid for the whole run.
    let (owner_key, layer_key, number_key) =
        unsafe { (kCGWindowOwnerPID, kCGWindowLayer, kCGWindowNumber) };
    list.iter().find_map(|dict| {
        let owner = i32::try_from(int(&dict, owner_key)?).ok()?;
        (owner == pid && int(&dict, layer_key)? == 0)
            .then(|| (int(&dict, number_key), owner))
            .and_then(|(number, owner)| number.map(|n| (n, owner)))
    })
}

/// One reading of the frontmost application, its topmost window and whether this app is active.
pub fn snapshot(tag: &str, mtm: MainThreadMarker) -> Snapshot {
    let front = NSWorkspace::sharedWorkspace().frontmostApplication();
    let (front_pid, front_bundle) = match front {
        Some(app) => (
            app.processIdentifier(),
            app.bundleIdentifier()
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
        None => (-1, String::new()),
    };
    Snapshot {
        tag: tag.to_string(),
        front_pid,
        front_bundle,
        window: topmost_window(front_pid),
        own_active: NSApplication::sharedApplication(mtm).isActive(),
    }
}

/// The windows of `pid` as `number:layer`, from the whole list or from the ones on the screen
/// now (the current Space). A window that exists but is not on the current Space is only in the
/// first.
fn windows_of(pid: i32, on_screen_only: bool) -> Vec<String> {
    let options = if on_screen_only {
        CGWindowListOption::OptionOnScreenOnly
    } else {
        CGWindowListOption::OptionAll
    };
    let Some(list) = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else {
        return Vec::new();
    };
    // SAFETY: same list shape as in `topmost_window`.
    let list: &CFArray<CFDictionary<CFString, CFType>> = unsafe { list.cast_unchecked() };
    // SAFETY: the three keys are constant strings of the system, valid for the whole run.
    let (owner_key, layer_key, number_key) =
        unsafe { (kCGWindowOwnerPID, kCGWindowLayer, kCGWindowNumber) };
    list.iter()
        .filter_map(|dict| {
            let owner = i32::try_from(int(&dict, owner_key)?).ok()?;
            if owner != pid {
                return None;
            }
            Some(format!(
                "{}:{}",
                int(&dict, number_key)?,
                int(&dict, layer_key)?
            ))
        })
        .collect()
}

/// One line for the log: the windows of this process, all and on the current Space.
pub fn own_windows_line() -> String {
    let pid = i32::try_from(std::process::id()).unwrap_or(-1);
    format!(
        "WINDOWS pid={pid} all=[{}] onscreen=[{}]",
        windows_of(pid, false).join(" "),
        windows_of(pid, true).join(" ")
    )
}
