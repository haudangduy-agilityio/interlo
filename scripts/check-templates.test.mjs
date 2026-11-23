// Proves that scripts/check-templates.mjs catches each fault it claims to.
// Each case breaks a copy of the templates in one way and expects a matching error.
// Run: node --test scripts/check-templates.test.mjs
import assert from "node:assert/strict";
import { cpSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { checkTemplates } from "./check-templates.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
const made = [];

/** A scratch root: a copy of .gitlab/ (the part under test) and links to everything else. */
function scratch() {
  const root = mkdtempSync(join(tmpdir(), "templates-"));
  made.push(root);
  for (const name of readdirSync(REPO)) {
    if (name === ".gitlab" || name === ".git" || name === "target" || name === "node_modules") continue;
    symlinkSync(join(REPO, name), join(root, name));
  }
  cpSync(join(REPO, ".gitlab"), join(root, ".gitlab"), { recursive: true });
  return root;
}

/** Breaks one file with `change` (text in, text out) and returns the errors. */
function broken(folder, file, change) {
  const root = scratch();
  const path = join(root, ".gitlab", folder, file);
  writeFileSync(path, change(readFileSync(path, "utf8")));
  return checkTemplates(root);
}
const mr = (file, change) => broken("merge_request_templates", file, change);
const issue = (file, change) => broken("issue_templates", file, change);
const has = (errors, text) => assert.ok(errors.some((e) => e.includes(text)), `expected "${text}" in:\n${errors.join("\n")}`);

after(() => made.forEach((d) => rmSync(d, { recursive: true, force: true })));

describe("check-templates", () => {
  it("passes on the templates in the repository", () => {
    assert.deepEqual(checkTemplates(REPO), []);
  });

  it("reports a missing file and a file with no rule", () => {
    const root = scratch();
    rmSync(join(root, ".gitlab/merge_request_templates/Docs.md"));
    writeFileSync(join(root, ".gitlab/issue_templates/Extra.md"), "# x\n");
    writeFileSync(join(root, ".gitlab/issue_templates/Extra one.md"), "# x\n");
    const errors = checkTemplates(root);
    has(errors, "Docs.md: file is missing");
    has(errors, "Extra.md: no rule for this file");
    has(errors, "Extra one.md: name must match");
  });

  it("reports a missing heading and a heading out of order", () => {
    has(mr("Bug_Fix.md", (t) => t.replace("## Regression Proof\n", "")), 'missing heading "## Regression Proof"');
    has(
      mr("Feature.md", (t) => t.replace("## Summary", "## Acceptance Criteria X").replace("## Acceptance Criteria\n", "## Summary\n").replace("## Acceptance Criteria X", "## Acceptance Criteria")),
      "out of order",
    );
  });

  it("reports an unexpected heading", () => {
    has(mr("Chore.md", (t) => t + "\n## Notes\n"), "unexpected heading(s): Notes");
  });

  it("reports trailing whitespace, a tab and CRLF", () => {
    has(mr("Docs.md", (t) => t.replace("## Summary", "## Summary ")), "trailing whitespace");
    has(mr("Docs.md", (t) => t.replace("- <!--", "-\t<!--")), "has a tab");
    has(mr("Docs.md", (t) => t.replace(/\n/g, "\r\n")), "CRLF");
  });

  it("reports an arrow inside a comment that ends the comment early (the Mermaid case)", () => {
    // The arrow ends the comment inside its own code block: the fence is left open.
    const inFence = (t) => t.replace("## Result", "<!--\n```mermaid\nflowchart LR\n  A --> B\n```\n-->\n\n## Result");
    has(mr("Default.md", inFence), "code fence is not closed");
    // The arrow ends a plain comment: the words after it show up as text.
    const plain = (t) => t.replace("## Result", "<!-- note --> more text -->\n\n## Result");
    has(mr("Default.md", plain), "outside a code block");
  });

  it("accepts an arrow in a visible code block, and comment markers in code", () => {
    // Default.md holds a visible Mermaid example with `-->`; the shipped file passes.
    assert.ok(readFileSync(join(REPO, ".gitlab/merge_request_templates/Default.md"), "utf8").includes("A[Entry point] --> B[Changed step]"));
    assert.deepEqual(checkTemplates(REPO), []);
    assert.deepEqual(mr("Docs.md", (t) => t + "\nWrite `<!--` to start a comment and `-->` to end it.\n"), []);
    assert.deepEqual(mr("Docs.md", (t) => t + "\n```text\n<!-- start\n```\n"), []);
  });

  it("does not count a heading, a required line or a gate table that is hidden or only in code", () => {
    has(mr("Docs.md", (t) => t.replace("## Summary\n", "```\n## Summary\n```\n")), 'missing heading "## Summary"');
    has(mr("Docs.md", (t) => t.replace("## Summary\n", "~~~\n## Summary\n~~~\n")), 'missing heading "## Summary"');
    has(mr("Docs.md", (t) => t.replace("## Summary\n", "<!--\n## Summary\n-->\n")), 'missing heading "## Summary"');
    has(mr("Default.md", (t) => t.replace("- [ ] Every commit is signed off (DCO)", "<!-- - [ ] Every commit is signed off (DCO) -->")), "signed off (DCO)");
    has(issue("Issue_Spec_Bug.md", (t) => t.replace("Do not report a security problem here. See SECURITY.md.", "<!-- Do not report a security problem here. See SECURITY.md. -->")), "security problem");
    has(mr("Chore.md", (t) => t.replace("**Verified commit:**", "<!-- **Verified commit:**").replace("<!-- SHA -->`", "<!-- SHA -->` -->")), "Verified commit");
    has(mr("Chore.md", (t) => t.replace(/^\| Security review.*$/m, (row) => "```\n" + row + "\n```")), 'no "Security review" row');
  });

  it("tracks code fences by character and length", () => {
    // A 3-backtick line does not close a 4-backtick fence, so the heading stays hidden and the fence stays open.
    const open4 = (t) => t.replace("## Summary\n", "````\n```\n## Summary\n");
    has(mr("Docs.md", open4), "code fence is not closed");
    // Backticks inside a valid tilde fence are text; the fence closes on its own marker.
    assert.deepEqual(mr("Docs.md", (t) => t + "\n~~~md\n```\nnot a fence\n```\n~~~\n"), []);
    // A longer closing line is fine.
    assert.deepEqual(mr("Docs.md", (t) => t + "\n```text\nx\n`````\n"), []);
  });

  it("pins the gate table columns and the separator line", () => {
    has(mr("Docs.md", (t) => t.replace(/^\|---\|---\|---\|$/m, "|:::|:::|:::|")), "dashed cell per column");
    has(mr("Docs.md", (t) => t.replace(/ \| <!-- (CI link|report|evidence|status) -->/g, "")), "different number of cells");
    has(mr("Docs.md", (t) => t.replace("| Gate / Check | Status | Evidence |", "| Gate / Check | Result | Evidence |")), "gate table header");
    assert.deepEqual(mr("Docs.md", (t) => t + "\n| a | b |\n|---|---|\n| x \\| y | z |\n"), []);
  });

  it("reports a duplicate heading", () => {
    has(mr("Docs.md", (t) => t.replace("## Summary\n", "## Summary\n\n## Summary\n")), "duplicate heading(s): Summary");
  });

  it("reports a comment that is never closed", () => {
    has(mr("Docs.md", (t) => t + "\n<!-- left open\n"), "not closed");
  });

  it("reports a missing gate row, a stray performance row and a missing verified commit", () => {
    has(mr("Chore.md", (t) => t.replace(/^\| Security review.*\n/m, "")), 'no "Security review" row');
    has(mr("Docs.md", (t) => t.replace("| Architecture review", "| Performance review | x | y |\n| Architecture review")), "must not have");
    has(mr("Refactor.md", (t) => t.replace("**Verified commit:**", "**Commit:**")), "Verified commit");
  });

  it("reports a table row with the wrong number of cells and a missing separator", () => {
    has(mr("Feature.md", (t) => t.replace("| Code review | ", "| Code review | extra | ")), "different number of cells");
    has(mr("Feature.md", (t) => t.replace(/^\|---\|---\|---\|\n/m, "")), "separator");
  });

  it("reports text from another project", () => {
    has(mr("Default.md", (t) => t + "\nSee claude-code.\n"), 'another project: "claude-code"');
  });

  it("reports a path or a check.sh mode that does not exist", () => {
    has(mr("Default.md", (t) => t + "\nSee `docs/not-a-file.md`.\n"), "does not exist: docs/not-a-file.md");
    has(mr("Default.md", (t) => t.replace("`scripts/check.sh full`", "`scripts/check.sh everything`")), "unknown scripts/check.sh mode");
    has(mr("Default.md", (t) => t + "\nSee `docs/not-a-file.md:12`.\n"), "does not exist: docs/not-a-file.md");
    has(mr("Default.md", (t) => t + "\nSee [x](docs/not-a-file.md#top).\n"), "links to a file that does not exist: docs/not-a-file.md");
    has(mr("Default.md", (t) => t + "\nSee `.claude/hooks/nope.sh` and `deny.tomll`.\n"), "does not exist: .claude/hooks/nope.sh");
    assert.deepEqual(mr("Default.md", (t) => t + "\nSee `docs/guardrails.md:12`, `docs/*.md`, [x](docs/guardrails.md#top), `deny.toml`.\n"), []);
  });

  it("does not treat NOTICE as a repository path (it is generated at release, not tracked)", () => {
    const root = scratch();
    rmSync(join(root, "NOTICE"), { force: true });
    const path = join(root, ".gitlab", "merge_request_templates", "Default.md");
    writeFileSync(path, readFileSync(path, "utf8") + "\nThird-party notices: `NOTICE`.\n");
    assert.deepEqual(checkTemplates(root), []);
  });

  it("reports a dropped Interlo rule in the default merge request template", () => {
    has(mr("Default.md", (t) => t.replace("- [ ] Every commit is signed off (DCO)\n", "")), "signed off (DCO)");
  });

  it("reports a dropped security notice and a wrong title tag in issue templates", () => {
    has(issue("Issue_Spec_Bug.md", (t) => t.replace("Do not report a security problem here. See SECURITY.md.\n", "")), "security problem");
    has(issue("Issue_Spec_Task.md", (t) => t.replace("# [Task]", "# [Chore]")), 'must start with "# [Task] "');
  });
});
