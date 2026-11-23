// Prints one line per on-screen window of a process: id, title, width, height
// (tab separated, sizes in points). Used by scripts/app-check.sh, which polls it
// every 100 ms. The window title needs the Screen Recording permission of the
// calling app; without it the title is empty and the script reports BLOCKED.
//
// Usage: window-info <pid>
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
    FileHandle.standardError.write(Data("usage: window-info <pid>\n".utf8))
    exit(64)
}

let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows {
    guard (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
        (window[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
        let id = (window[kCGWindowNumber as String] as? NSNumber)?.intValue,
        let bounds = window[kCGWindowBounds as String] as? NSDictionary,
        let rect = CGRect(dictionaryRepresentation: bounds)
    else { continue }
    let title = window[kCGWindowName as String] as? String ?? ""
    print("\(id)\t\(title)\t\(Int(rect.width.rounded()))\t\(Int(rect.height.rounded()))")
}
