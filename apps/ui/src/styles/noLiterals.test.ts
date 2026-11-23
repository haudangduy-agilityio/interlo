import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// AC-03: a colour, radius, shadow or type size is a token. Only the token file holds literal values.
const SRC = fileURLToPath(new URL("../", import.meta.url));
const SELF = "noLiterals.test.ts";
const TOKEN_FILE = "styles/tokens.css";

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = `${dir}${name}`;
    if (statSync(path).isDirectory()) return files(`${path}/`);
    return /\.(tsx?|css)$/.test(name) ? [path] : [];
  });
}

const HEX = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})(?![0-9a-zA-Z_-])/;
const FUNCTION_COLOUR = /\b(?:rgb|rgba|hsl|hsla)\(/;
const SHAPE_DECLARATION = /(?:border-radius|box-shadow|font-size)\s*:\s*([^;}]+)[;}]/g;
const ALLOWED_SHAPE = /^(?:var\(--[a-z0-9-]+\)|0|none|inherit)(?: (?:var\(--[a-z0-9-]+\)|0))*$/;

interface Source {
  path: string;
  text: string;
}

/** Every source file except the token file and this test. */
function sourceFiles(): Source[] {
  return files(SRC)
    .filter((p) => !p.endsWith(TOKEN_FILE) && !p.endsWith(SELF))
    .map((path) => ({ path, text: readFileSync(path, "utf8") }));
}

function literalProblems(sources: Source[]): string[] {
  const problems: string[] = [];
  for (const { path, text } of sources) {
    const rel = path.replace(SRC, "");
    if (/\.test\.tsx?$/.test(rel)) continue;
    if (HEX.test(text)) problems.push(`${rel}: hex colour`);
    if (FUNCTION_COLOUR.test(text)) problems.push(`${rel}: rgb or hsl colour`);
    if (rel.endsWith(".css")) {
      for (const m of text.matchAll(SHAPE_DECLARATION)) {
        const value = m[1].trim().replace(/\s+/g, " ");
        const parts = value.split(/,(?![^(]*\))/).map((p) => p.trim());
        if (!parts.every((p) => ALLOWED_SHAPE.test(p))) problems.push(`${rel}: ${m[0].trim()}`);
      }
    }
  }
  return problems;
}

describe("tokens only (AC-03)", () => {
  it("finds no literal colour, radius, shadow or type size outside the token file", () => {
    expect(literalProblems(sourceFiles())).toEqual([]);
  });

  it("fails on a planted hex colour, an rgba colour and a literal radius", () => {
    const planted = [
      { path: `${SRC}a.css`, text: ".a { color: #fff; }" },
      { path: `${SRC}b.css`, text: ".b { background: rgba(0, 0, 0, 0.5); }" },
      { path: `${SRC}c.css`, text: ".c { border-radius: 50%; font-size: 13px; }" },
    ];
    expect(literalProblems(planted)).toHaveLength(4);
  });
});
