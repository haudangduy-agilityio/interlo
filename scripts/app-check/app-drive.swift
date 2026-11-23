// Drives one running app by its pid with the Accessibility API and synthetic key
// events. Used by scripts/app-check.sh for the typing check. The pid (not the app
// name) is used because more than one build of the app can be running.
//
// Usage:
//   app-drive raise <pid> [bounce]    bring the app and its first window to the front; "bounce" first activates
//                                     the Finder so the app takes the current input source when it comes back
//   app-drive press <pid> <text>      press the first element named <text> (title, description or value)
//   app-drive focus-text <pid>        give keyboard focus to the first text area
//   app-drive read-text <pid>         print the value of the first text area
//   app-drive keys <pid> <delay-ms> <text>  type <text> with key events (US layout key codes), one key every <delay-ms>
//   app-drive chord <pid> <name>      send select-all, copy or delete
// Key events are posted to the pid, so they can only reach that app. The input method composes text only
// while the app is in front, so keys also stops (exit 4) as soon as the app is not in front: the text that
// was typed up to then is partial and the caller must start again.
//   app-drive trusted                 print a line when the Accessibility permission is granted (exit 3 otherwise)
//   app-drive dump <pid>              print the element tree (role, title, description), for debugging
//
// Needs the Accessibility permission of the calling app (AXIsProcessTrusted).
import AppKit
import ApplicationServices
import Foundation

func fail(_ message: String, _ code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
    return value as AnyObject?
}

func text(_ element: AXUIElement, _ name: String) -> String? { attribute(element, name) as? String }

func children(_ element: AXUIElement) -> [AXUIElement] {
    (attribute(element, kAXChildrenAttribute as String) as? [AXUIElement]) ?? []
}

func actions(_ element: AXUIElement) -> [String] {
    var names: CFArray?
    guard AXUIElementCopyActionNames(element, &names) == .success else { return [] }
    return (names as? [String]) ?? []
}

/// Depth-first walk; stops at the first element for which visit returns true.
@discardableResult
func walk(_ element: AXUIElement, depth: Int = 0, _ visit: (AXUIElement, Int) -> Bool) -> Bool {
    if visit(element, depth) { return true }
    if depth > 40 { return false }
    for child in children(element) where walk(child, depth: depth + 1, visit) { return true }
    return false
}

func appElement(_ argument: String) -> (AXUIElement, pid_t) {
    guard let pid = pid_t(argument), pid > 1 else { fail("bad pid \(argument)", 64) }
    return (AXUIElementCreateApplication(pid), pid)
}

func names(_ element: AXUIElement) -> [String] {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap { text(element, $0 as String) }
}

// US layout key codes; the Telex input source reads the same physical keys.
let keyCodes: [Character: CGKeyCode] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11,
    "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "o": 31, "u": 32, "i": 34, "p": 35,
    "l": 37, "j": 38, "k": 40, "n": 45, "m": 46, ",": 43, ".": 47, " ": 49,
]

/// The pid of the app that owns the top window on screen, read from the window server (never cached).
func topWindowPID() -> pid_t? {
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    for window in windows where (window[kCGWindowLayer as String] as? NSNumber)?.intValue == 0 {
        return (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
    }
    return nil
}

/// Exit code 4: the app is not in front.
func requireFront(_ argument: String) {
    guard let pid = pid_t(argument) else { fail("bad pid \(argument)", 64) }
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.02))  // let the workspace refresh its state
    let active = NSWorkspace.shared.frontmostApplication?.processIdentifier
    let top = topWindowPID()
    if active != pid || top != pid {
        fail("stopping: the app \(pid) is not in front (active app \(active.map(String.init) ?? "none"), top window of \(top.map(String.init) ?? "none"))", 4)
    }
}

func post(_ code: CGKeyCode, flags: CGEventFlags = [], to pid: pid_t) {
    let source = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down) else {
            fail("could not create a key event")
        }
        event.flags = flags
        event.postToPid(pid)  // always one pid: a key event must never reach whatever app is in front
        usleep(15_000)
    }
}

let args = CommandLine.arguments
guard args.count >= 2 else { fail("usage: app-drive raise|press|focus-text|text-length|keys|chord|dump ...", 64) }
guard AXIsProcessTrusted() else { fail("BLOCKED: AXIsProcessTrusted() is false for the app that runs this tool", 3) }

switch args[1] {
case "raise":
    guard args.count == 3 || args.count == 4 else { fail("usage: app-drive raise <pid> [bounce]", 64) }
    let (app, pid) = appElement(args[2])
    if args.count == 4 && args[3] == "bounce" {
        NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.finder").first?.activate()
        usleep(800_000)
    }
    NSRunningApplication(processIdentifier: pid)?.activate()
    AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
    if let window = (attribute(app, kAXWindowsAttribute as String) as? [AXUIElement])?.first {
        AXUIElementPerformAction(window, kAXRaiseAction as CFString)
        AXUIElementSetAttributeValue(window, kAXMainAttribute as CFString, kCFBooleanTrue)
    }
    usleep(500_000)
    requireFront(args[2])
    print("front: \(pid)")

case "press":
    guard args.count == 4 else { fail("usage: app-drive press <pid> <text>", 64) }
    let (app, _) = appElement(args[2])
    var pressed = false
    walk(app) { element, _ in
        guard actions(element).contains(kAXPressAction as String), names(element).contains(args[3]) else { return false }
        pressed = AXUIElementPerformAction(element, kAXPressAction as CFString) == .success
        return true
    }
    if !pressed { fail("no pressable element named \"\(args[3])\"") }
    print("pressed: \(args[3])")

case "focus-text", "read-text":
    guard args.count == 3 else { fail("usage: app-drive \(args[1]) <pid>", 64) }
    let (app, _) = appElement(args[2])
    var found: AXUIElement?
    walk(app) { element, _ in
        if text(element, kAXRoleAttribute as String) == (kAXTextAreaRole as String) {
            found = element
            return true
        }
        return false
    }
    guard let area = found else { fail("no text area in the app") }
    if args[1] == "focus-text" {
        guard AXUIElementSetAttributeValue(area, kAXFocusedAttribute as CFString, kCFBooleanTrue) == .success else {
            fail("could not focus the text area")
        }
        print("focused: text area")
    } else {
        print(text(area, kAXValueAttribute as String) ?? "")
    }

case "keys":
    guard args.count == 5, let delay = useconds_t(args[3]), let target = pid_t(args[2]), target > 1 else { fail("usage: app-drive keys <pid> <delay-ms> <text>", 64) }
    for character in args[4] {
        requireFront(args[2])
        let lower = Character(character.lowercased())
        guard let code = keyCodes[lower] else { fail("no key code for \"\(character)\"", 64) }
        post(code, flags: character.isUppercase ? .maskShift : [], to: target)
        usleep(delay * 1000)
    }
    print("typed \(args[4].count) keys")

case "chord":
    guard args.count == 4 else { fail("usage: app-drive chord <pid> select-all|copy|delete", 64) }
    guard let target = pid_t(args[2]), target > 1 else { fail("bad pid \(args[2])", 64) }
    switch args[3] {
    case "select-all": post(0, flags: .maskCommand, to: target)
    case "copy": post(8, flags: .maskCommand, to: target)
    case "delete": post(51, to: target)
    default: fail("unknown chord \(args[3])", 64)
    }
    usleep(300_000)
    print("sent: \(args[3])")

case "dump":
    guard args.count == 3 else { fail("usage: app-drive dump <pid>", 64) }
    let (app, _) = appElement(args[2])
    walk(app) { element, depth in
        let role = text(element, kAXRoleAttribute as String) ?? "?"
        let label = names(element).map { String($0.prefix(40)) }.joined(separator: " | ")
        print(String(repeating: " ", count: depth) + role + (label.isEmpty ? "" : "  " + label))
        return false
    }

case "trusted":
    print("AXIsProcessTrusted: true")

default:
    fail("unknown command \(args[1])", 64)
}
