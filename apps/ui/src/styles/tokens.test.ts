import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// AC-01, AC-02, AC-04: the token file equals the tables of docs/design.md, the text pairs hold
// 4.5:1 in both themes, and the theme blocks are wired as the design says.
const here = fileURLToPath(new URL(".", import.meta.url));
const design = readFileSync(`${here}../../../../docs/design.md`, "utf8");
const css = readFileSync(`${here}tokens.css`, "utf8");

type Values = Record<string, string>;

/** Prettier spaces the commas inside rgba(); the design tables do not. Compare without that. */
const norm = (value: string): string =>
  value
    .trim()
    .replace(/\s*,\s*/g, ",")
    .replace(/\s+/g, " ");

/** The declarations (`--name: value;`) of one block of CSS text. */
function declarations(block: string): Values {
  const out: Values = {};
  for (const m of block.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) out[m[1]] = norm(m[2]);
  const scheme = /(?:^|\s)color-scheme:\s*([a-z]+);/.exec(block);
  if (scheme) out["color-scheme"] = scheme[1];
  return out;
}

/** The text between the braces that open at `marker`. */
function blockAfter(text: string, marker: string): string {
  const at = text.indexOf(marker);
  if (at < 0) throw new Error(`missing block: ${marker}`);
  const open = text.indexOf("{", at);
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    if (text[i] === "}" && --depth === 0) return text.slice(open + 1, i);
  }
  throw new Error(`unclosed block: ${marker}`);
}

const light = declarations(blockAfter(css, ":root {"));
const darkBySystem = declarations(
  blockAfter(
    blockAfter(css, "@media (prefers-color-scheme: dark)"),
    ':root:not([data-theme="light"])',
  ),
);
const darkByAttribute = declarations(blockAfter(css, ':root[data-theme="dark"]'));

interface Row {
  name: string;
  cells: string[];
}
function rowsOf(section: string): Row[] {
  const at = design.indexOf(section);
  if (at < 0) throw new Error(`missing section: ${section}`);
  const next = design.indexOf("\n##", at + section.length);
  const text = design.slice(at, next < 0 ? undefined : next);
  const rows: Row[] = [];
  for (const line of text.split("\n")) {
    const m = /^\| `([a-z0-9-]+)` \|(.*)\|\s*$/.exec(line);
    if (!m) continue;
    rows.push({ name: m[1], cells: m[2].split("|").map((c) => c.trim()) });
  }
  return rows;
}
const tick = (cell: string): string => norm(cell.replace(/^`|`$/g, ""));

const COLOUR_SECTIONS = [
  "### Colour: the app",
  "### Colour: overlays and the subtitle bar",
  "### Colour: the desktop shell",
];

describe("tokens equal docs/design.md (AC-01)", () => {
  it("has every colour in light and in dark", () => {
    const rows = COLOUR_SECTIONS.flatMap(rowsOf);
    expect(rows.length).toBeGreaterThan(30);
    for (const row of rows) {
      expect(light[row.name], `light ${row.name}`).toBe(tick(row.cells[0]));
      expect(darkBySystem[row.name], `dark (system) ${row.name}`).toBe(tick(row.cells[1]));
      expect(darkByAttribute[row.name], `dark (attribute) ${row.name}`).toBe(tick(row.cells[1]));
    }
  });

  it("has every shadow, in light and in dark where the value differs", () => {
    const rows = rowsOf("### Shadow");
    expect(rows.map((r) => r.name)).toContain("shadow-window");
    for (const row of rows) {
      const cell = row.cells[0];
      const split = /^light: `(.+?)`; dark: `(.+?)`$/.exec(cell);
      if (split) {
        expect(light[row.name], `light ${row.name}`).toBe(norm(split[1]));
        expect(darkBySystem[row.name], `dark ${row.name}`).toBe(norm(split[2]));
        expect(darkByAttribute[row.name], `dark attr ${row.name}`).toBe(norm(split[2]));
      } else {
        expect(light[row.name], row.name).toBe(tick(cell));
      }
    }
  });

  it("has every radius and every type size", () => {
    const radii = rowsOf("### Corner radius");
    const sizes = rowsOf("### Type sizes");
    expect(radii).toHaveLength(12);
    expect(sizes).toHaveLength(14);
    for (const row of [...radii, ...sizes]) {
      expect(light[row.name], row.name).toBe(tick(row.cells[0]));
    }
  });

  it("names the three font families", () => {
    for (const name of ["font-display", "font-text", "font-mono"]) {
      expect(light[name], name).toBeTruthy();
    }
  });
});

function channel(v: number): number {
  const c = v / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}
function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  return (
    0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255)
  );
}
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// Text on surface pairs named by the usage notes of the design system.
const PAIRS: readonly [text: string, on: string][] = [
  ["text", "bg"],
  ["text", "card"],
  ["text", "side"],
  ["mute", "bg"],
  ["mute", "card"],
  ["mute", "sunk"],
  ["acc-ink", "acc"],
  ["acc", "acc-soft"],
  ["acc", "bg"],
  ["acc", "card"],
  ["live", "live-soft"],
  ["live", "card"],
  ["bad", "bad-soft"],
  ["bad", "card"],
  ["warn", "warn-soft"],
  ["ok", "card"],
];

describe("contrast (AC-02)", () => {
  for (const [theme, values] of [
    ["light", light],
    ["dark", darkBySystem],
  ] as const) {
    it(`every text pair holds 4.5:1 in ${theme}`, () => {
      for (const [text, on] of PAIRS) {
        const ratio = contrast(values[text], values[on]);
        expect(ratio, `${text} on ${on} (${ratio.toFixed(2)})`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});

describe("theme wiring (AC-04)", () => {
  it("sets color-scheme for light and for both dark blocks", () => {
    expect(light["color-scheme"]).toBe("light");
    expect(darkBySystem["color-scheme"]).toBe("dark");
    expect(darkByAttribute["color-scheme"]).toBe("dark");
  });

  it("applies dark by the system only when the page is not forced to light", () => {
    expect(css).toContain("@media (prefers-color-scheme: dark)");
    expect(css).toContain(':root:not([data-theme="light"])');
  });
});
