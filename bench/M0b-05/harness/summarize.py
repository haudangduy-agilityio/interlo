#!/usr/bin/env python3
"""Build SUMMARY.md and SHA256SUMS for an evidence folder made by evidence.sh.

Usage: summarize.py <evidence-dir>

Reads every `<step>/result.txt` (lines of `<AC> <STATUS> <detail>`), rolls the lines up to one
status per acceptance criterion, and quotes the line that decides it. Nothing is invented: an
AC with no result line is listed as NOT RUN.
"""

import hashlib
import os
import re
import sys

ROWS = [
    "AC-11",
    "AC-12",
    "AC-13",
    "AC-14",
    "AC-15",
    "AC-16",
    "AC-17",
    "AC-18",
    "AC-19",
    "AC-20",
    "AC-35",
    "AC-36",
    "AC-40",
    "AC-21",
    "AC-22",
    "AC-39",
]
NOTES = {
    "AC-19": "DEVIATION: the source is a tone written into the 2-channel device by a helper, not the browser output (see docs/m0/M0b-05.md)",
}
MANUAL = {
    "AC-15": (
        "BLOCKED",
        "needs desktop meeting app A with a second person (H-14): no account and no second person",
    ),
}


def base(ac):
    m = re.match(r"(AC-\d+)", ac)
    return m.group(1) if m else ac


def read_results(root):
    results = []
    for step in sorted(os.listdir(root)):
        path = os.path.join(root, step, "result.txt")
        if not os.path.isfile(path):
            continue
        for line in open(path, encoding="utf-8"):
            m = re.match(
                r"(\S+) (CONTROL-(?:OK|FAILED)\([^)]*\)|\S+) ?(.*)", line.rstrip("\n")
            )
            if m:
                results.append((m.group(1), m.group(2), m.group(3), step))
    return results


def rollup(lines):
    real = [l for l in lines if not l[1].startswith("CONTROL")]
    if not real:
        return "NOT RUN", None
    for want in ("BLOCKED", "NO", "NOT RUN"):
        hit = [l for l in real if l[1] == want]
        if hit:
            return want, hit[0]
    passes = [l for l in real if l[1] in ("PASS", "DONE")]
    return "PASS", passes[0] if passes else real[0]


def clip(text, n=230):
    text = " ".join(text.split())
    return text if len(text) <= n else text[: n - 3] + "..."


def meta(root):
    out = {}
    path = os.path.join(root, "00-meta", "meta.txt")
    if os.path.isfile(path):
        for line in open(path, encoding="utf-8"):
            k, _, v = line.strip().partition("=")
            out[k] = v
    return out


def main():
    root = os.path.abspath(sys.argv[1])
    results = read_results(root)
    m = meta(root)
    out = []
    out.append("# M0b-05 evidence summary\n")
    out.append(f"- Date (UTC): {m.get('date_utc', 'unknown')}")
    out.append(
        f"- Commit: {m.get('commit', 'unknown')} (working tree files changed: {m.get('dirty_files', 'unknown')})"
    )
    out.append(
        f"- macOS: {m.get('macos', 'unknown')} on {m.get('mac_model', 'unknown')}; {m.get('rustc', '')}"
    )
    out.append(f"- Spike binary SHA-256: {m.get('binary_sha256', 'unknown')}")
    out.append(
        "- Every status below is read from a raw file in this folder; `SHA256SUMS` holds the hash of every file."
    )
    out.append(
        "- PASS, NO, NOT RUN and BLOCKED are the SPEC status words. A check that could not run is never PASS.\n"
    )
    out.append("| AC | Status | File | Deciding line |")
    out.append("|---|---|---|---|")
    controls = []
    for ac in ROWS:
        lines = [r for r in results if base(r[0]) == ac]
        status, line = rollup(lines)
        if ac in MANUAL and not lines:
            status, why = MANUAL[ac]
            out.append(f"| {ac} | {status} | none | {why} |")
            continue
        if line is None:
            out.append(f"| {ac} | NOT RUN | none | no result line was written |")
        else:
            out.append(
                f"| {ac} | {status} | `{line[3]}/result.txt` | {clip(line[0] + ' ' + line[1] + ' ' + line[2])} |"
            )
        for c in lines:
            if c[1].startswith("CONTROL"):
                controls.append(c)
    out.append("\n## Repeated timings (median and spread)\n")
    for ac, status, detail, step in results:
        if ac.endswith("-repeat"):
            out.append(f"- {ac} {status}: {clip(detail, 400)} (`{step}/result.txt`)")
    out.append("\n## Negative controls\n")
    out.append(
        "A control runs the same check on input that must fail. CONTROL-OK means the check reported NO.\n"
    )
    out.append("| AC | Control | File | Output |")
    out.append("|---|---|---|---|")
    for ac, status, detail, step in controls:
        out.append(
            f"| {base(ac)} | {status} | `{step}/result.txt` | {clip(detail, 170)} |"
        )
    failed = [c for c in controls if c[1].startswith("CONTROL-FAILED")]
    out.append(f"\nControls that did not fail as they must: {len(failed)}.\n")
    out.append("## Other lines\n")
    for ac, status, detail, step in results:
        if base(ac) not in ROWS and not ac.startswith("PERMISSION"):
            out.append(f"- {ac} {status}: {clip(detail)} (`{step}/result.txt`)")
        if ac.startswith("PERMISSION"):
            out.append(f"- {ac} {status}: {clip(detail)} (`{step}/result.txt`)")
    with open(os.path.join(root, "SUMMARY.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")

    sums = []
    for dirpath, _, files in os.walk(root):
        for name in files:
            if name in ("SHA256SUMS", ".stop-clicker"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
            sums.append(f"{digest}  {rel}")
    with open(os.path.join(root, "SHA256SUMS"), "w", encoding="utf-8") as f:
        f.write("\n".join(sorted(sums, key=lambda s: s.split("  ", 1)[1])) + "\n")
    print(f"wrote {root}/SUMMARY.md and SHA256SUMS ({len(sums)} files)")


if __name__ == "__main__":
    main()
