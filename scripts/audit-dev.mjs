#!/usr/bin/env node
// Audits the UI's whole dependency tree (dev tooling included) and fails on any
// high or critical advisory that is not in ACCEPTED below.
//
// `npm audit` cannot ignore one advisory, so this script reads its JSON report
// and compares the root advisories (the ones npm links by URL) with ACCEPTED.
// Every accepted advisory has a reason and an expiry date. After the date the
// script fails again, so an exception cannot be forgotten.
//
// Usage: node scripts/audit-dev.mjs [ui-dir]      (default: apps/ui)
import { spawnSync } from "node:child_process";

const ACCEPTED = {
  // picomatch <= 4.0.7: ReDoS in extglob matching. picomatch 4.0.7 is the latest
  // release, so no patched version exists. It reaches the UI only through vite,
  // vitest and typescript-eslint, which match glob patterns from our own config
  // and file tree. Nothing from outside the repo reaches it, and it is not in the
  // shipped bundle (`npm audit --omit=dev` is clean).
  "GHSA-fc5q-39xc-vjpf": { until: "2027-02-18", package: "picomatch" },
};

const uiDir = process.argv[2] ?? "apps/ui";
const run = spawnSync("npm", ["--prefix", uiDir, "audit", "--json"], {
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
});
let report;
try {
  report = JSON.parse(run.stdout);
} catch {
  console.error("audit-dev: npm audit gave no JSON report:\n" + (run.stderr || run.stdout));
  process.exit(2);
}
if (report.error) {
  console.error(`audit-dev: npm audit failed: ${report.error.summary ?? JSON.stringify(report.error)}`);
  process.exit(2);
}

const today = new Date().toISOString().slice(0, 10);
const blocking = new Map(); // advisory id -> description
const accepted = new Set();
for (const [name, vuln] of Object.entries(report.vulnerabilities ?? {})) {
  for (const via of vuln.via) {
    if (typeof via === "string") continue; // a dependent of another vulnerable package
    if (via.severity !== "high" && via.severity !== "critical") continue;
    const id = via.url?.split("/").pop() ?? `${name}:${via.title}`;
    const rule = ACCEPTED[id];
    if (rule && rule.until >= today) accepted.add(id);
    else {
      const why = rule ? `accepted until ${rule.until}, now expired` : "not accepted";
      blocking.set(id, `${via.name} ${via.range}: ${via.title} (${why}) ${via.url ?? ""}`);
    }
  }
}

for (const id of accepted) {
  console.log(`audit-dev: accepted ${id} (${ACCEPTED[id].package}) until ${ACCEPTED[id].until}`);
}
if (blocking.size > 0) {
  for (const line of blocking.values()) console.error(`audit-dev: BLOCKING ${line}`);
  process.exit(1);
}
console.log("audit-dev: no unaccepted high or critical advisories");
