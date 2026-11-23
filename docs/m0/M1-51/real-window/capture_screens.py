#!/usr/bin/env python3
"""Capture every screen of the real Interlo window, light and dark (M1-51).

Usage: capture_screens.py <pid> <agent-desktop window id> <CG window id> <out dir>
Reads only the Interlo window (--window-id). Clicks go through the accessibility press action.
The macOS appearance is switched for the light capture only, and restored in a finally block.
"""
import datetime, json, subprocess, sys, time, os

PID, AD_WIN, CG_WIN, OUT = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
LOG = open(os.path.join(OUT, "actions.log"), "a")


def log(line):
    LOG.write(f"{datetime.datetime.now().isoformat(timespec='seconds')} {line}\n")
    LOG.flush()


def ad(*args):
    out = subprocess.run(["agent-desktop", *args], capture_output=True, text=True).stdout
    return json.loads(out)


def snapshot():
    d = ad("snapshot", "--window-id", AD_WIN, "-i", "--compact")
    refs = {}

    def walk(n):
        if isinstance(n, dict):
            if n.get("ref_id") and n.get("name"):
                refs.setdefault(n["name"], n["ref_id"])
            for c in n.get("children", []):
                walk(c)

    walk(d["data"]["tree"])
    return refs


def click(name, refs=None):
    refs = refs or snapshot()
    ref = refs[name]
    res = ad("click", ref)
    log(f"click name={name!r} ref={ref} ok={res.get('ok')} window={AD_WIN}")
    time.sleep(1.2)
    return res


def shot(theme, screen):
    path = os.path.join(OUT, f"{screen}-{theme}.png")
    subprocess.run(["screencapture", "-x", "-o", "-l", CG_WIN, path], check=True)
    log(f"capture screen={screen} theme={theme} window_id={CG_WIN} file={os.path.basename(path)}")


def appearance(dark):
    subprocess.run(["osascript", "-e", f'tell application "System Events" to tell appearance preferences to set dark mode to {"true" if dark else "false"}'], check=True)
    time.sleep(3)


def run(theme):
    log(f"theme={theme} start")
    click("Today"); shot(theme, "home")
    click("Live meeting"); shot(theme, "live")
    click("Meetings"); shot(theme, "meetings")
    click("Workspace"); shot(theme, "workspace-today")
    refs = snapshot()
    if "Notes" in refs:
        click("Notes", refs); shot(theme, "workspace-notes")
    click("Setup"); shot(theme, "setup")
    click("Today")
    click("Settings"); shot(theme, "settings")
    refs = snapshot()
    for name in ("Close settings",):
        if name in refs:
            click(name, refs)
    # Start with a setup check the shell cannot answer: a toast, no dialog.
    refs = snapshot()
    if "Start meeting" in refs:
        click("Start meeting", refs); shot(theme, "start-pressed")
    log(f"theme={theme} done")


original_dark = subprocess.run(["defaults", "read", "-g", "AppleInterfaceStyle"], capture_output=True, text=True).stdout.strip() == "Dark"
log(f"original appearance dark={original_dark}")
try:
    for theme in (["dark", "light"] if original_dark else ["light", "dark"]):
        want_dark = theme == "dark"
        if want_dark != original_dark:
            print(f"SWITCHING macOS appearance to {theme} for the capture; it is restored afterwards", flush=True)
            log(f"appearance switched to {theme}")
        appearance(want_dark)
        run(theme)
finally:
    appearance(original_dark)
    log(f"appearance restored dark={original_dark}")
    print("restored", flush=True)
