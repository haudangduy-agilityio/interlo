#!/usr/bin/env node
// Checks the GitLab issue and merge request templates in .gitlab/.
//
// A template is a form that people fill in later, so a typo in it goes
// unnoticed until someone uses it. This check pins what each template must
// contain and catches the faults that make a template render badly:
//   - the set of files in each folder, and their names;
//   - the required headings, in order, and the required lines;
//   - the gate table of every merge request template (rows and columns);
//   - open HTML comments, unbalanced code fences, tabs, trailing spaces;
//   - paths, links and `scripts/check.sh` modes that do not exist;
//   - text left over from another project.
//
// Usage: node scripts/check-templates.mjs [repo-root]   (default: the repository root)
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const GATE_ROWS = [
  "Relevant tests / TDD",
  "Runtime / behavior",
  "Code review",
  "Security review",
  "Architecture review",
];
const PERFORMANCE_ROW = "Performance review";
const GATE_HEADER = "| Gate / Check | Status | Evidence |";
const IRREVERSIBLE = ["Irreversible After Merge"];
const GATES = ["Review Carefully", "Automated Gates & Verification"];

/** Required `##` headings in order, and extra lines every file must contain. */
export const SPEC = {
  "merge_request_templates": {
    "Default.md": {
      headings: ["Goal", "Description", "Implementation approach", "What this MR does", "Result", "How to review", "Validation", ...IRREVERSIBLE, "Checklist", "Related"],
      contains: [
        "**Task:**",
        "```mermaid",
        "scripts/check.sh full",
        "- [ ] A test for each rule the change adds or changes",
        "- [ ] `docs/guardrails.md` is not broken",
        "- [ ] A change to a command or an event updates `docs/ipc-contract.md`, the Rust DTOs and `apps/ui/src/ipc/types.ts` together",
        "- [ ] A fact in `docs/` that the change touched is updated",
        "- [ ] No model, audio file or database is committed",
        "- [ ] Every commit is signed off (DCO)",
      ],
    },
    "Feature.md": { headings: ["Summary", "Acceptance Criteria", "Implementation Flow", ...GATES, ...IRREVERSIBLE, "Risks / Deployment Notes"], gate: true, performance: true, status: true },
    "Bug_Fix.md": { headings: ["Bug / Root Cause / Fix", "Regression Proof", "Implementation Flow", ...GATES, ...IRREVERSIBLE, "Risks / Deployment Notes"], gate: true, performance: true, status: true },
    "Refactor.md": { headings: ["Summary", "Implementation Flow", ...GATES, ...IRREVERSIBLE], gate: true, performance: true, status: true },
    "Chore.md": { headings: ["Summary", "Compatibility Impact", "Review Carefully", "Automated Gates & Verification", ...IRREVERSIBLE, "Risks / Deployment Notes"], gate: true, performance: true, status: true },
    "Config_Infra.md": { headings: ["Summary", "Deployment / Rollback", "Implementation Flow", ...GATES, ...IRREVERSIBLE, "Risks / Deployment Notes"], gate: true, performance: true, status: true },
    "Docs.md": { headings: ["Summary", "Automated Gates & Verification"], gate: true, performance: false, status: true },
  },
  "issue_templates": {
    "Issue_Spec_Bug.md": {
      title: "Bug",
      headings: ["Summary / impact", "Environment and reproducibility", "Actual vs expected behavior", "Acceptance criteria", "Verification expectations", "Evidence and uncertainty", "Scope / exclusions", "Dependencies, risk and related issues", "Decisions and approvals"],
      contains: ["Do not report a security problem here. See SECURITY.md.", "macOS version", "remove any transcript text"],
    },
    "Issue_Spec_Feature.md": {
      title: "Feature",
      headings: ["Problem and desired outcome", "User journeys and behavior", "Scope boundaries", "Requirements", "Acceptance criteria", "Verification expectations", "Dependencies and decomposition", "Decisions / approval"],
      contains: ["docs/guardrails.md"],
    },
    "Issue_Spec_Investigation.md": {
      title: "Investigation",
      headings: ["Trigger and impact", "Investigation objective", "Current evidence", "Evidence plan and budget", "Exit criteria and expected outputs", "Verification expectations", "Decision package"],
    },
    "Issue_Spec_Task.md": {
      title: "Task",
      headings: ["Context / problem", "Outcome and scope", "Confirmed requirements", "Acceptance criteria", "Verification expectations", "Dependencies / constraints", "Implementation notes (non-binding)", "Definition of done", "Decisions and approvals"],
    },
    "Issue_Spec_Technical_Debt.md": {
      title: "Technical Debt",
      headings: ["Debt statement", "Impact and risk", "Target outcome", "Scope", "Acceptance criteria", "Verification expectations", "Alternatives and trade-offs", "Decisions / approval"],
    },
  },
};

// Text that belongs to another project. A template copied from one must not keep it.
const FOREIGN = ["claude-code", "Ticket-", "practice/", "todo-api", "npm start", "watchAll", "Asia/Ho_Chi_Minh"];
const CHECK_MODES = new Set(["quick", "full", "e2e", "supply"]);
const NAME = /^[A-Za-z0-9_]+\.md$/;
const ROOT_PATH = /^(?:\.claude|\.github|\.gitlab|docs|scripts|apps|crates)\/[\w./-]+$/;
const TOP_LEVEL = new Set(["LICENSE", "NOTICE", "deny.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]);

const FENCE_OPEN = /^\s*(`{3,}|~{3,})/;
const FENCE_CLOSE = /^\s*(`{3,}|~{3,})\s*$/;

/**
 * Splits a template the way GitLab renders it.
 *   visible: the text without `<!-- -->` comments (fenced code kept);
 *   prose:   visible, minus the lines inside code fences (the fence lines too).
 * A comment ends at the first `-->`. Comment markers inside a code fence or an
 * inline code span are plain text. Returns `problems` for a comment that is
 * never closed or is nested, and for a code fence that is not closed.
 */
function render(text) {
  const problems = [];
  const visible = [];
  const prose = [];
  let fence = null; // { char, length } of the open code fence
  let i = 0;
  let line = "";
  const endLine = () => {
    visible.push(line);
    if (fence) {
      // A fence closes on a line of the same character, at least as long as the opener.
      const close = FENCE_CLOSE.exec(line);
      if (close && close[1][0] === fence.char && close[1].length >= fence.length) fence = null;
    } else {
      const open = FENCE_OPEN.exec(line);
      if (open) fence = { char: open[1][0], length: open[1].length };
      else prose.push(line);
    }
    line = "";
  };
  while (i < text.length) {
    const ch = text[i];
    if (ch === "\n") { endLine(); i++; continue; }
    if (!fence && !FENCE_OPEN.test(line) && text.startsWith("<!--", i)) {
      const close = text.indexOf("-->", i + 4);
      if (close === -1) { problems.push("an HTML comment is not closed"); break; }
      if (text.slice(i + 4, close).includes("<!--")) problems.push("an HTML comment is nested in another");
      i = close + 3; // the comment, and any newlines in it, are dropped
      continue;
    }
    if (!fence && ch === "`") {
      let n = 0;
      while (text[i + n] === "`") n++;
      const run = "`".repeat(n);
      const lineEnd = text.indexOf("\n", i + n);
      const close = text.indexOf(run, i + n);
      // An inline code span closes on the same line; a fence opener (three backticks at the line start) is handled by endLine.
      if (n < 3 && close !== -1 && (lineEnd === -1 || close < lineEnd)) { line += text.slice(i, close + n); i = close + n; continue; }
      line += run; i += n; continue;
    }
    line += ch; i++;
  }
  if (line !== "") endLine();
  if (fence) problems.push("a code fence is not closed");
  return { visible: visible.join("\n"), prose: prose.join("\n"), problems };
}

const withoutInlineCode = (s) => s.replace(/`[^`\n]*`/g, "");

function checkFile(root, folder, name, rule) {
  const where = `.gitlab/${folder}/${name}`;
  const errors = [];
  const fail = (msg) => errors.push(`${where}: ${msg}`);
  const text = readFileSync(join(root, ".gitlab", folder, name), "utf8");

  if (text.trim() === "") return [`${where}: empty`];
  if (text.includes("\r")) fail("has CRLF line endings");
  if (!text.endsWith("\n") || text.endsWith("\n\n")) fail("must end with exactly one newline");
  if (text.includes("\t")) fail("has a tab");
  text.split("\n").forEach((line, i) => {
    if (/[ \t]+$/.test(line)) fail(`line ${i + 1} has trailing whitespace`);
  });

  // GitLab hides `<!-- -->` when it renders, so only `visible` and `prose` count.
  // A comment ends at the first `-->`: an arrow inside one (for example in a
  // Mermaid example) ends it early and shows the rest of the example as text.
  // An arrow in a visible code fence or an inline code span is fine.
  const { visible, prose, problems } = render(text);
  for (const problem of problems) fail(problem);
  for (const line of prose.split("\n")) {
    if (withoutInlineCode(line).includes("-->")) fail(`"-->" shows as text outside a code block: "${line.trim().slice(0, 50)}"; it probably ended a comment early`);
  }

  // Headings, in order. A heading in a comment or a code fence is not a heading.
  const found = [...prose.matchAll(/^## (.+?)\s*$/gm)].map((m) => m[1]);
  let at = 0;
  for (const heading of rule.headings) {
    const i = found.indexOf(heading, at);
    if (i === -1) fail(`missing heading "## ${heading}"${found.includes(heading) ? " (out of order)" : ""}`);
    else at = i + 1;
  }
  const extra = found.filter((h) => !rule.headings.includes(h));
  if (extra.length) fail(`unexpected heading(s): ${extra.join(", ")}`);
  const twice = found.filter((h, i) => found.indexOf(h) !== i);
  if (twice.length) fail(`duplicate heading(s): ${twice.join(", ")}`);

  if (rule.title !== undefined && !text.startsWith(`# [${rule.title}] `)) fail(`first line must start with "# [${rule.title}] "`);
  // Required text must be visible: outside comments and, except a fence marker, outside code fences.
  for (const needle of rule.contains ?? []) {
    if (!(needle.startsWith("```") ? visible : prose).includes(needle)) fail(`missing required text: ${needle}`);
  }
  if (rule.status && !/^\*\*Status:\*\* /m.test(prose)) fail("missing the **Status:** line");
  if (rule.status && !/^\*\*Task:\*\* /m.test(prose)) fail("missing the **Task:** line");

  if (rule.gate) {
    if (!/^\*\*Verified commit:\*\* /m.test(prose)) fail("missing the **Verified commit:** line");
    const rows = [...prose.matchAll(/^\| (.+?) \| .*\|$/gm)].map((m) => m[1]);
    const wanted = rule.performance ? [...GATE_ROWS, PERFORMANCE_ROW] : GATE_ROWS;
    for (const row of wanted) if (!rows.includes(row)) fail(`gate table has no "${row}" row`);
    if (!rule.performance && rows.includes(PERFORMANCE_ROW)) fail(`gate table must not have a "${PERFORMANCE_ROW}" row`);
  }

  // Every table row has the same number of cells as its header, and a separator line
  // with one dashed cell per column. `\|` is a pipe inside a cell, not a cell edge.
  const lines = prose.split("\n");
  const isRow = (l) => /^\|.*\|$/.test(l ?? "");
  const cells = (l) => l.split(/(?<!\\)\|/).length - 2;
  for (let i = 0; i < lines.length; i++) {
    if (!isRow(lines[i]) || (i > 0 && isRow(lines[i - 1]))) continue;
    const separator = lines[i + 1] ?? "";
    const dashed = /^\|(?:\s*:?-+:?\s*\|)+$/.test(separator);
    if (!dashed || cells(separator) !== cells(lines[i])) fail(`table "${lines[i].slice(0, 40)}": the header needs a separator line with one dashed cell per column`);
    for (let j = i; isRow(lines[j]); j++) {
      if (j !== i + 1 && cells(lines[j]) !== cells(lines[i])) fail(`table row "${lines[j].slice(0, 40)}" has a different number of cells than its header`);
    }
    if (rule.gate && /^\| Gate \/ Check \|/.test(lines[i]) && lines[i] !== GATE_HEADER) fail(`the gate table header must be "${GATE_HEADER}"`);
  }
  if (rule.gate && !prose.includes(GATE_HEADER)) fail(`missing the gate table header "${GATE_HEADER}"`);

  for (const word of FOREIGN) if (text.includes(word)) fail(`has text from another project: "${word}"`);

  // Paths, links and `scripts/check.sh` modes named in the visible text must exist.
  const exists = (p) => existsSync(join(root, p));
  for (const m of prose.matchAll(/`([^`\n]+)`/g)) {
    const code = m[1].trim();
    const check = code.match(/^scripts\/check\.sh(?: (\S+))?/);
    if (check && check[1] && /^[a-z]+$/.test(check[1]) && !CHECK_MODES.has(check[1])) fail(`unknown scripts/check.sh mode: ${check[1]}`);
    // `docs/foo.md:12` and `docs/foo.md#L3` name docs/foo.md; a glob or a placeholder names no single file.
    const path = code.split(/\s+/)[0].replace(/(?::\d+(?:-\d+)?|#L\d+.*)$/, "");
    const looksLikePath = ROOT_PATH.test(path) || TOP_LEVEL.has(path) || /^[A-Z][A-Z_]+\.md$/.test(path);
    if (looksLikePath && !/[*?{}<>]/.test(path) && !exists(path)) fail(`names a path that does not exist: ${path}`);
  }
  for (const m of prose.matchAll(/\]\(([^)\s]+)\)/g)) {
    const href = m[1].split("#")[0];
    if (href === "" || /^(?:https?:|mailto:|\/uploads\/)/.test(m[1])) continue;
    if (!exists(href.replace(/^\//, ""))) fail(`links to a file that does not exist: ${href}`);
  }
  return errors;
}

/** Returns a list of problems; empty when every template is as specified. */
export function checkTemplates(root) {
  const errors = [];
  for (const [folder, files] of Object.entries(SPEC)) {
    const dir = join(root, ".gitlab", folder);
    const present = existsSync(dir) ? readdirSync(dir).filter((f) => f !== ".DS_Store").sort() : [];
    for (const f of present) if (!NAME.test(f)) errors.push(`.gitlab/${folder}/${f}: name must match ${NAME} (GitLab shows it in the template list)`);
    for (const f of Object.keys(files)) if (!present.includes(f)) errors.push(`.gitlab/${folder}/${f}: file is missing`);
    for (const f of present) if (!(f in files)) errors.push(`.gitlab/${folder}/${f}: no rule for this file; add it to SPEC in scripts/check-templates.mjs`);
    for (const [f, rule] of Object.entries(files)) if (present.includes(f)) errors.push(...checkFile(root, folder, f, rule));
  }
  return errors;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), ".."));
  const errors = checkTemplates(root);
  const count = Object.values(SPEC).reduce((n, files) => n + Object.keys(files).length, 0);
  if (errors.length) {
    for (const e of errors) console.error(`check-templates: ${e}`);
    console.error(`check-templates: ${errors.length} problem(s)`);
    process.exit(1);
  }
  console.log(`check-templates: ${count} templates match the spec`);
}
