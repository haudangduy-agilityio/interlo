#!/usr/bin/env node
// Lists the moves in a diff that lower the quality bar: a new suppression, an ignored or skipped
// test, an unfinished stub, a deleted test file, or fewer assertions in a file.
// Only files under crates/, apps/ and fixtures/ are checked, and never vendored or generated
// folders. Only added or removed lines count, so code that was already there is never flagged.
//
// Rust (.rs) files get the Rust rules and TypeScript or JavaScript files get the vitest and
// Playwright rules. The two sets are kept apart because `.skip(` is a vitest call in one and
// an iterator method in the other.
//
// Usage: node floor-guard.mjs [--base <ref>]   (default base: HEAD, so the working tree
//        against the last commit, including staged and untracked files)
// Exit 0: clean. Exit 1: at least one finding. Exit 2: the guard could not run, so the changes
// were not checked (guard-weakened-bar.sh treats that as a block inside a repository).
// Reports the rule, the file and the matched fragment, never the whole line.
import { execFileSync } from 'node:child_process';

const SCOPES = ['crates/', 'apps/', 'fixtures/'];
const VENDORED = /(^|\/)(node_modules|target|dist|gen)\//;

const RUST_FILE = /\.rs$/;
const JS_FILE = /\.(mjs|cjs|[jt]sx?)$/;

// Rust: a lint switched off, a test ignored, a stub left in place. `#[expect(` is not listed:
// it fails the build once the lint stops firing, so it cannot hide a problem for long.
const RUST_SUPPRESSIONS = /#!?\[allow\(/;
const RUST_SKIPS = /#\[ignore\b/;
const RUST_STUBS = /\b(todo|unimplemented)!\s*\(/;
const RUST_ASSERTION = /\b(debug_)?assert(_eq|_ne)?!/;

// TypeScript and JavaScript: vitest and Playwright.
const JS_SUPPRESSIONS = /eslint-disable|@ts-ignore|@ts-nocheck|istanbul ignore/;
const JS_SKIPS = /\.(skip|only|fixme)\(|\bxit\(|\bxtest\(|\bxdescribe\(|\.todo\(/;
const JS_STUBS = /catch\s*(\(\w*\))?\s*\{\s*\}/;
const JS_TEST_FILE = /(^|\/)[^/]*\.(test|spec)\.[jt]sx?$/;
const JS_ASSERTION = /\b(assert|expect)\b/;

// A Rust test file lives in a crate's tests/ folder, or is named tests.rs or *_test.rs.
const RUST_TEST_FILE = /(^|\/)(tests\/[^/]*\.rs|tests\.rs|[^/]*_test\.rs)$/;
const TEST_FILE = (f) => RUST_TEST_FILE.test(f) || JS_TEST_FILE.test(f);

function inScope(f) {
  return SCOPES.some((scope) => f.startsWith(scope)) && !VENDORED.test(f);
}

const base = process.argv.includes('--base') ? process.argv[process.argv.indexOf('--base') + 1] : 'HEAD';

// Fixed output options, so neither the user's git config nor a .gitattributes file can change
// what the parser sees: no rename detection (a moved test file shows as a deletion), no external
// diff or textconv filter, every file treated as text (a `-diff` or `binary` attribute would
// otherwise print "Binary files differ" and no lines), no colour, fixed a/ and b/ prefixes, and
// raw non-ASCII file names.
const DIFF_FLAGS = [
  '--no-renames',
  '--no-ext-diff',
  '--no-textconv',
  '--text',
  '--no-color',
  '--src-prefix=a/',
  '--dst-prefix=b/',
  '--unified=0',
];
const MAX_OUTPUT = 256 * 1024 * 1024;

const git = (args, { diffExit = false } = {}) => {
  try {
    return execFileSync('git', ['-c', 'core.quotePath=false', ...args], {
      encoding: 'utf8',
      maxBuffer: MAX_OUTPUT,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (e) {
    // `git diff --no-index` exits 1 for any new file, which is the normal case.
    return diffExit && e.status === 1 && typeof e.stdout === 'string' ? e.stdout : null;
  }
};
const bail = (message) => {
  console.error(`floor-guard: ${message}`);
  process.exit(2);
};

const top = git(['rev-parse', '--show-toplevel'])?.trim();
if (!top) bail('not inside a git work tree');
process.chdir(top);

const mergeBase = git(['merge-base', base, 'HEAD'])?.trim();
if (!mergeBase) bail(`no merge base against ${base}`);

const tracked = git(['diff', ...DIFF_FLAGS, mergeBase, '--']);
if (tracked === null) bail(`could not diff against ${mergeBase}`);

// Deleted files come from the status list, not the patch: an empty file leaves no patch header.
const statusList = git(['diff', '--name-status', '-z', '--no-renames', mergeBase, '--']);
if (statusList === null) bail(`could not list changed files against ${mergeBase}`);
const statusFields = statusList.split('\0');
const deleted = [];
for (let i = 0; i + 1 < statusFields.length; i += 2) {
  // T is a type change: a test file replaced by a symlink or a submodule has lost its content.
  if (statusFields[i] === 'D' || statusFields[i] === 'T') deleted.push(statusFields[i + 1]);
}

const untrackedList = git(['ls-files', '--others', '--exclude-standard', '-z']);
if (untrackedList === null) bail('could not list untracked files');
const untracked = untrackedList
  .split('\0')
  .filter(inScope)
  .map((f) => {
    const d = git(['diff', '--no-index', ...DIFF_FLAGS, '/dev/null', f], { diffExit: true });
    if (d === null) bail(`could not diff untracked file ${f}`);
    return d;
  })
  .join('\n');

// `---` and `+++` are file headers only between a `diff` line and its first `@@`; inside a
// hunk every line is content, so an added `++i` (shown as `+++i`) is still a change.
const added = [];
const removed = [];
let file = '';
let oldFile = '';
let inHeader = false;
// A header path is C-quoted when the name holds a double quote, a backslash or a control
// character, and ends with a tab when it holds a space.
const ESCAPES = { a: 7, b: 8, t: 9, n: 10, v: 11, f: 12, r: 13, '"': 34, '\\': 92 };
const unquote = (s) => {
  if (!s.startsWith('"')) return s;
  const body = s.slice(1, s.lastIndexOf('"'));
  const bytes = [];
  for (let i = 0; i < body.length; i++) {
    if (body[i] !== '\\') bytes.push(...Buffer.from(body[i]));
    else if (/[0-7]/.test(body[i + 1])) {
      bytes.push(parseInt(body.slice(i + 1, i + 4), 8));
      i += 3;
    } else bytes.push(ESCAPES[body[++i]] ?? body.charCodeAt(i));
  }
  return Buffer.from(bytes).toString('utf8');
};
const pathOf = (s) => unquote(s.replace(/\t$/, '')).replace(/^[ab]\//, '');
for (const line of `${tracked}\n${untracked}`.split('\n')) {
  if (line.startsWith('diff ')) inHeader = true;
  else if (line.startsWith('@@')) inHeader = false;
  else if (inHeader) {
    if (line.startsWith('--- ')) oldFile = pathOf(line.slice(4));
    else if (line.startsWith('+++ ')) {
      const newFile = pathOf(line.slice(4));
      file = newFile === '/dev/null' ? oldFile : newFile;
    }
  } else if (line.startsWith('+')) added.push({ file, text: line.slice(1) });
  else if (line.startsWith('-')) removed.push({ file, text: line.slice(1) });
}

const findings = [];
const flag = (rule, f, detail) => findings.push(`[${rule}] ${f}: ${detail}`);

for (const { file: f, text } of added.filter((l) => inScope(l.file))) {
  let rules = [];
  if (RUST_FILE.test(f)) {
    rules = [
      ['silenced-checker', RUST_SUPPRESSIONS],
      ['test-made-easier', RUST_SKIPS],
      ['unfinished-work', RUST_STUBS],
    ];
  } else if (JS_FILE.test(f)) {
    rules = [
      ['silenced-checker', JS_SUPPRESSIONS],
      ['test-made-easier', JS_SKIPS],
      ['unfinished-work', JS_STUBS],
    ];
  }
  for (const [rule, pattern] of rules) {
    const match = text.match(pattern);
    if (match) flag(rule, f, match[0].trim());
  }
}

for (const f of deleted.filter((d) => inScope(d) && TEST_FILE(d))) flag('test-deleted', f, 'file deleted');

// A file that kept fewer assertions than it had. Replacing one assertion with another is a
// rewrite, so only a net loss counts. Rust assertions count in every .rs file, because tests
// sit in `#[cfg(test)]` modules next to the code. JavaScript assertions count in test files.
const assertionOf = (f) => (RUST_FILE.test(f) ? RUST_ASSERTION : JS_TEST_FILE.test(f) ? JS_ASSERTION : null);
const keptFiles = new Set(
  [...added, ...removed].map((l) => l.file).filter((f) => inScope(f) && assertionOf(f) && !deleted.includes(f)),
);
for (const f of keptFiles) {
  const pattern = assertionOf(f);
  const count = (lines) => lines.filter((l) => l.file === f && pattern.test(l.text)).length;
  const lost = count(removed) - count(added);
  if (lost > 0) flag('assertion-removed', f, `${lost} fewer assertion line(s)`);
}

if (findings.length === 0) process.exit(0);
console.error(`floor-guard: ${findings.length} finding(s) that lower the bar:`);
for (const f of findings) console.error(`  ${f}`);
process.exit(1);
