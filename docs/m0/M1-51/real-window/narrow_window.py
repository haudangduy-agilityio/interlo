#!/usr/bin/env python3
"""Resize the real Interlo window below 960 px and capture it (M1-51, part B).

Usage: narrow_window.py <pid> <agent-desktop window id> <CG window id> <out dir> [tag]
Reads only the Interlo window. The size is restored to 1200 by 780 at the end.
"""
import datetime, json, os, subprocess, sys, time

PID, AD_WIN, CG_WIN, OUT = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
TAG = sys.argv[5] if len(sys.argv) > 5 else "narrow"
LOG = open(os.path.join(OUT, "narrow.log"), "a")


def log(line):
    LOG.write(f"{datetime.datetime.now().isoformat(timespec='seconds')} {line}\n")
    LOG.flush()


def ad(*args):
    return json.loads(subprocess.run(["agent-desktop", *args], capture_output=True, text=True).stdout)


def bounds():
    for w in ad("list-windows", "--app", "Interlo")["data"]["windows"] if "windows" in ad("list-windows", "--app", "Interlo")["data"] else ad("list-windows", "--app", "Interlo")["data"]:
        if w.get("id") == AD_WIN:
            return w["bounds"]


def snapshot_refs():
    for attempt in range(4):
        d = ad("snapshot", "--window-id", AD_WIN, "-i", "--compact", "--timeout-ms", "8000")
        if "data" in d:
            break
        log(f"snapshot attempt {attempt + 1} failed: {json.dumps(d)[:200]}")
        time.sleep(1.5)
    refs = {}

    def walk(n):
        if isinstance(n, dict):
            if n.get("ref_id") and n.get("name"):
                refs.setdefault(n["name"], n["ref_id"])
            for c in n.get("children", []):
                walk(c)

    walk(d["data"]["tree"])
    return refs


def go(name):
    ref = snapshot_refs()[name]
    res = ad("click", ref)
    log(f"click name={name!r} ref={ref} ok={res.get('ok')}")
    time.sleep(1.0)


def outside_window(width):
    """Names of interactive elements whose bounds leave the window, from the accessibility tree."""
    d = ad("snapshot", "--window-id", AD_WIN, "-i", "--compact", "--include-bounds", "--timeout-ms", "8000")
    b = bounds()
    out = []

    def walk(n):
        if isinstance(n, dict):
            bb = n.get("bounds")
            if n.get("ref_id") and bb and bb["width"] > 0 and bb["x"] + bb["width"] > b["x"] + b["width"] - int(os.environ.get("PRETEND_NARROWER_BY", "0")) + 1:
                out.append(n.get("name") or n.get("role"))
            for c in n.get("children", []):
                walk(c)

    if "data" in d:
        walk(d["data"]["tree"])
    return out


def shot(width, screen):
    path = os.path.join(OUT, f"{TAG}-{width}-{screen}.png")
    subprocess.run(["screencapture", "-x", "-o", "-l", CG_WIN, path], check=True)
    log(f"capture width_asked={width} screen={screen} window_id={CG_WIN} file={os.path.basename(path)}")


widths = [int(w) for w in os.environ.get("WIDTHS", "900,800,760,640,480,300").split(",")]
try:
    for w in widths:
        r = ad("resize-window", "--window-id", AD_WIN, "--width", str(w), "--height", "780")
        time.sleep(1.5)
        b = bounds()
        log(f"resize asked={w}x780 ok={r.get('ok')} applied={b['width']}x{b['height']}")
        for nav, screen in (("Today", "home"), ("Live meeting", "live"), ("Workspace", "workspace")):
            go(nav)
            shot(w, screen)
            log(f"width={w} screen={screen} interactive_elements_right_of_window={outside_window(w)}")
finally:
    ad("resize-window", "--window-id", AD_WIN, "--width", "1200", "--height", "780")
    time.sleep(1)
    b = bounds()
    log(f"restored applied={b['width']}x{b['height']}")
    print("restored", flush=True)
