// Reads and selects the keyboard input source with the Text Input Source API.
// Used by scripts/app-check.sh to switch to the built-in Vietnamese (Telex)
// input source for the typing check and to put the original one back.
//
// Usage:
//   input-source current        print the id of the current input source
//   input-source list           print the ids of the enabled input sources
//   input-source select <id>    select an enabled input source; exit 1 when it did not change
import Carbon.HIToolbox
import Foundation

func property(_ source: TISInputSource, _ key: CFString) -> String? {
    guard let raw = TISGetInputSourceProperty(source, key) else { return nil }
    return Unmanaged<CFString>.fromOpaque(raw).takeUnretainedValue() as String
}

func currentID() -> String {
    let source = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
    return property(source, kTISPropertyInputSourceID) ?? ""
}

func enabledSources() -> [TISInputSource] {
    let list = TISCreateInputSourceList(nil, false)?.takeRetainedValue() as? [TISInputSource]
    return list ?? []
}

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write(Data("usage: input-source current|list|select <id>\n".utf8))
    exit(64)
}

switch args[1] {
case "current":
    print(currentID())
case "list":
    for source in enabledSources() {
        let selectable = (property(source, kTISPropertyInputSourceIsSelectCapable) != nil)
        print("\(property(source, kTISPropertyInputSourceID) ?? "?")\t\(selectable ? "selectable" : "not selectable")")
    }
case "select":
    guard args.count == 3 else { exit(64) }
    let wanted = args[2]
    guard let source = enabledSources().first(where: { property($0, kTISPropertyInputSourceID) == wanted }) else {
        FileHandle.standardError.write(Data("input source \(wanted) is not enabled\n".utf8))
        exit(1)
    }
    let status = TISSelectInputSource(source)
    // The switch is applied by the system a moment later.
    for _ in 0..<20 {
        if currentID() == wanted { print(wanted); exit(0) }
        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
    }
    FileHandle.standardError.write(Data("selecting \(wanted) returned \(status) and the current source is \(currentID())\n".utf8))
    exit(1)
default:
    exit(64)
}
