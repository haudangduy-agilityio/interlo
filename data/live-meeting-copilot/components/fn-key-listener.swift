import Cocoa

private var fnWasDown = false
private var translationChordStartedAt: TimeInterval = 0
private var translationChordCandidate = false
private var translationChordSawBoth = false
private var eventTap: CFMachPort?
private var standardInputBuffer = ""

private let translationChordMaxDuration: TimeInterval = 1.2

private final class PasteAccessibilityObservation {
    let id: String
    let processIdentifier: pid_t
    let applicationElement: AXUIElement
    var element: AXUIElement?
    var observer: AXObserver?
    var enabledManualAccessibility = false
    var changed = false
    var observedElementIsEditable = false
    var registeredNotifications: [CFString] = []
    var textValuesBeforePaste: [String] = []

    init(id: String, processIdentifier: pid_t, applicationElement: AXUIElement) {
        self.id = id
        self.processIdentifier = processIdentifier
        self.applicationElement = applicationElement
    }
}

private var pasteAccessibilityObservation: PasteAccessibilityObservation?

private func emit(_ value: String) {
    print(value)
    fflush(stdout)
}

private func resetTranslationChord() {
    translationChordStartedAt = 0
    translationChordCandidate = false
    translationChordSawBoth = false
}

private let pasteAccessibilityCallback: AXObserverCallback = { _, _, _, refcon in
    guard let refcon else { return }
    let observation = Unmanaged<PasteAccessibilityObservation>
        .fromOpaque(refcon)
        .takeUnretainedValue()
    observation.changed = true
}

private let manualAccessibilityAttribute = "AXManualAccessibility" as CFString
private let editableAccessibilityRoles: Set<String> = [
    kAXTextFieldRole as String,
    kAXTextAreaRole as String,
    kAXComboBoxRole as String,
]

private func accessibilityElementAttribute(
    _ element: AXUIElement,
    _ attribute: CFString
) -> AXUIElement? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success,
          let value,
          CFGetTypeID(value) == AXUIElementGetTypeID() else {
        return nil
    }
    return (value as! AXUIElement)
}

private func accessibilityStringAttribute(
    _ element: AXUIElement,
    _ attribute: CFString
) -> String? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success else {
        return nil
    }
    return value as? String
}

private func accessibilityChildren(of element: AXUIElement) -> [AXUIElement] {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(
        element,
        kAXChildrenAttribute as CFString,
        &value
    ) == .success,
    let children = value as? [AXUIElement] else {
        return []
    }
    return children
}

private func isEditableAccessibilityElement(_ element: AXUIElement) -> Bool {
    if let role = accessibilityStringAttribute(element, kAXRoleAttribute as CFString),
       editableAccessibilityRoles.contains(role) {
        return true
    }
    var settable = DarwinBoolean(false)
    return AXUIElementIsAttributeSettable(
        element,
        kAXValueAttribute as CFString,
        &settable
    ) == .success && settable.boolValue
}

private func accessibleTextValues(in root: AXUIElement) -> [String] {
    let maximumElementCount = 4_000
    let maximumCharacterCount = 8_000_000
    var queue = [root]
    var nextIndex = 0
    var seen = Set<CFHashCode>()
    var values: [String] = []
    var characterCount = 0

    while nextIndex < queue.count && seen.count < maximumElementCount {
        let element = queue[nextIndex]
        nextIndex += 1
        let hash = CFHash(element)
        if seen.contains(hash) { continue }
        seen.insert(hash)

        if isEditableAccessibilityElement(element),
           let value = accessibilityStringAttribute(element, kAXValueAttribute as CFString),
           characterCount + value.count <= maximumCharacterCount {
            values.append(value)
            characterCount += value.count
        }
        queue.append(contentsOf: accessibilityChildren(of: element))
    }
    return values
}

private func occurrenceCount(of needle: String, in values: [String]) -> Int {
    guard !needle.isEmpty else { return 0 }
    return values.reduce(into: 0) { total, value in
        var searchRange = value.startIndex..<value.endIndex
        while let range = value.range(of: needle, range: searchRange) {
            total += 1
            searchRange = range.upperBound..<value.endIndex
        }
    }
}

private func stopPasteAccessibilityObservation() -> PasteAccessibilityObservation? {
    guard let observation = pasteAccessibilityObservation else { return nil }
    if let observer = observation.observer {
        if let element = observation.element {
            for notification in observation.registeredNotifications {
                AXObserverRemoveNotification(observer, element, notification)
            }
        }
        CFRunLoopRemoveSource(
            CFRunLoopGetCurrent(),
            AXObserverGetRunLoopSource(observer),
            .commonModes
        )
    }
    if observation.enabledManualAccessibility {
        AXUIElementSetAttributeValue(
            observation.applicationElement,
            manualAccessibilityAttribute,
            kCFBooleanFalse
        )
    }
    pasteAccessibilityObservation = nil
    return observation
}

private func focusedAccessibilityElement(of applicationElement: AXUIElement) -> AXUIElement? {
    guard var current = accessibilityElementAttribute(
        applicationElement,
        kAXFocusedUIElementAttribute as CFString
    ) else { return nil }

    // Chromium exposes its AXWebArea as the application's focused element.
    // The actual editor or terminal input can expose another focused element
    // inside that web area, so follow the chain to the deepest stable target.
    for _ in 0..<12 {
        guard let next = accessibilityElementAttribute(
            current,
            kAXFocusedUIElementAttribute as CFString
        ), !CFEqual(current, next) else { break }
        current = next
    }
    return current
}

private func focusedElementText(of applicationElement: AXUIElement) -> String? {
    guard let element = focusedAccessibilityElement(of: applicationElement) else { return nil }
    return accessibilityStringAttribute(element, kAXValueAttribute as CFString)
}

private func beginPasteAccessibilityObservation(id: String) {
    _ = stopPasteAccessibilityObservation()
    guard let application = NSWorkspace.shared.frontmostApplication else {
        emit("paste-observer-ready\t\(id)\tfalse")
        return
    }
    let processIdentifier = application.processIdentifier
    let applicationElement = AXUIElementCreateApplication(processIdentifier)
    let observation = PasteAccessibilityObservation(
        id: id,
        processIdentifier: processIdentifier,
        applicationElement: applicationElement
    )

    // Chromium/Electron apps (VS Code, most AI editors) keep their accessibility
    // tree disabled until an assistive client asks for it, which makes the paste
    // target invisible. Enable it for the observation window and restore on stop.
    var manualValue: CFTypeRef?
    let alreadyEnabled = AXUIElementCopyAttributeValue(
        applicationElement,
        manualAccessibilityAttribute,
        &manualValue
    ) == .success && (manualValue as? Bool) == true
    if !alreadyEnabled,
       AXUIElementSetAttributeValue(
           applicationElement,
           manualAccessibilityAttribute,
           kCFBooleanTrue
       ) == .success {
        observation.enabledManualAccessibility = true
    }

    pasteAccessibilityObservation = observation

    if let element = focusedAccessibilityElement(of: applicationElement) {
        observation.observedElementIsEditable = isEditableAccessibilityElement(element)
        observation.textValuesBeforePaste = accessibleTextValues(in: element)
        var observerValue: AXObserver?
        if AXObserverCreate(
            processIdentifier,
            pasteAccessibilityCallback,
            &observerValue
        ) == .success,
        let observer = observerValue {
            observation.element = element
            observation.observer = observer
            let refcon = Unmanaged.passUnretained(observation).toOpaque()
            for notification in [
                kAXValueChangedNotification as CFString,
                kAXSelectedTextChangedNotification as CFString,
            ] {
                if AXObserverAddNotification(observer, element, notification, refcon) == .success {
                    observation.registeredNotifications.append(notification)
                }
            }
            CFRunLoopAddSource(
                CFRunLoopGetCurrent(),
                AXObserverGetRunLoopSource(observer),
                .commonModes
            )
        }
    }
    // Even with no observable focused element yet (the tree may still be
    // building after the enable above), keep the observation alive: the
    // finish-time text lookup can still confirm the paste.
    emit("paste-observer-ready\t\(id)\ttrue")
}

private func finishPasteAccessibilityObservation(id: String, expectedText: String?, keepObserving: Bool = false) {
    guard let observation = pasteAccessibilityObservation,
          observation.id == id else {
        emit("paste-observer-result\t\(id)\tfalse")
        return
    }
    var changed = observation.changed && observation.observedElementIsEditable
    // A native editable-value notification already confirms the paste. Avoid
    // walking the editor's accessibility subtree again before reporting it.
    if !changed, let expectedText, !expectedText.isEmpty,
       let currentElement = focusedAccessibilityElement(of: observation.applicationElement) {
        let occurrencesBefore = occurrenceCount(
            of: expectedText,
            in: observation.textValuesBeforePaste
        )
        let occurrencesAfter = occurrenceCount(
            of: expectedText,
            in: accessibleTextValues(in: currentElement)
        )
        if occurrencesAfter > occurrencesBefore {
            changed = true
        }
    }
    if !keepObserving { _ = stopPasteAccessibilityObservation() }
    emit("paste-observer-result\t\(id)\t\(changed ? "true" : "false")")
}

private func handleStandardInputLine(_ line: String) {
    let parts = line.split(separator: "\t", omittingEmptySubsequences: false)
    guard parts.count >= 2 else { return }
    let command = String(parts[0])
    let requestId = String(parts[1])
    if command == "paste-observer-begin" {
        beginPasteAccessibilityObservation(id: requestId)
    } else if command == "paste-observer-finish" || command == "paste-observer-check" {
        var expectedText: String?
        if parts.count >= 3,
           let decoded = Data(base64Encoded: String(parts[2])),
           let text = String(data: decoded, encoding: .utf8) {
            expectedText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        finishPasteAccessibilityObservation(
            id: requestId,
            expectedText: expectedText,
            keepObserving: command == "paste-observer-check"
        )
    }
}

private func startStandardInputListener() {
    FileHandle.standardInput.readabilityHandler = { handle in
        let data = handle.availableData
        if data.isEmpty {
            handle.readabilityHandler = nil
            return
        }
        guard let chunk = String(data: data, encoding: .utf8) else { return }
        DispatchQueue.main.async {
            standardInputBuffer += chunk
            while let newline = standardInputBuffer.firstIndex(of: "\n") {
                let line = String(standardInputBuffer[..<newline])
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                standardInputBuffer.removeSubrange(...newline)
                if !line.isEmpty {
                    handleStandardInputLine(line)
                }
            }
        }
    }
}

private let callback: CGEventTapCallBack = { _, type, event, _ in
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: true)
        }
        return Unmanaged.passUnretained(event)
    }

    if type == .keyDown {
        if translationChordSawBoth {
            translationChordCandidate = false
        }
        return Unmanaged.passUnretained(event)
    }

    guard type == .flagsChanged else {
        return Unmanaged.passUnretained(event)
    }

    let flags = event.flags
    let fnIsDown = flags.contains(.maskSecondaryFn)
    if fnIsDown != fnWasDown {
        fnWasDown = fnIsDown
        emit(fnIsDown ? "down" : "up")
    }

    let commandDown = flags.contains(.maskCommand)
    let optionDown = flags.contains(.maskAlternate)
    let disallowedModifierDown = flags.contains(.maskControl) || flags.contains(.maskShift)

    if commandDown && optionDown && !translationChordSawBoth {
        translationChordSawBoth = true
        translationChordCandidate = !disallowedModifierDown
        translationChordStartedAt = Date.timeIntervalSinceReferenceDate
    } else if translationChordSawBoth && disallowedModifierDown {
        translationChordCandidate = false
    }

    if translationChordSawBoth && !commandDown && !optionDown {
        let elapsed = Date.timeIntervalSinceReferenceDate - translationChordStartedAt
        let shouldTrigger = translationChordCandidate
            && translationChordStartedAt > 0
            && elapsed <= translationChordMaxDuration
        resetTranslationChord()
        if shouldTrigger {
            emit("translation")
        }
    }

    // This listener observes shortcuts only. Never consume a system event.
    return Unmanaged.passUnretained(event)
}

let mask = CGEventMask(
    (1 << CGEventType.flagsChanged.rawValue)
    | (1 << CGEventType.keyDown.rawValue)
)
guard let tap = CGEvent.tapCreate(
    tap: .cgSessionEventTap,
    place: .headInsertEventTap,
    options: .defaultTap,
    eventsOfInterest: mask,
    callback: callback,
    userInfo: nil
) else {
    fputs("Unable to create Fn event tap. Enable Accessibility permission for MeetingCopilot.\n", stderr)
    exit(2)
}
eventTap = tap

let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
CGEvent.tapEnable(tap: tap, enable: true)

let permissionTimer = Timer(timeInterval: 0.5, repeats: true) { _ in
    if !AXIsProcessTrusted() {
        fputs("Accessibility permission was revoked; stopping listener.\n", stderr)
        fflush(stderr)
        exit(3)
    }
}
RunLoop.current.add(permissionTimer, forMode: .common)
startStandardInputListener()
CFRunLoopRun()
