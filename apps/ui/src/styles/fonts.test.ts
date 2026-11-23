import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// AC-05 (local files), AC-06 (attribution; real names kept out of code).
const UI = fileURLToPath(new URL("../../", import.meta.url));
const REPO = `${UI}../../`;
const fontsCss = readFileSync(`${UI}src/styles/fonts.css`, "utf8");
const techStack = readFileSync(`${REPO}docs/tech-stack.md`, "utf8");

/** The family names of the three rows of docs/tech-stack.md. They are read from there, not repeated here. */
const familyRows = [...techStack.matchAll(/^\| (display|text|mono)\s+\| (.+?)\s+\| SIL OFL/gm)].map(
  (m) => ({ role: m[1], name: m[2].trim() }),
);
const familyNames = familyRows.map((r) => r.name);

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = `${dir}/${name}`;
    if (statSync(path).isDirectory()) return files(path);
    return /\.(tsx?|css|html)$/.test(name) ? [path] : [];
  });
}

describe("font faces (AC-05)", () => {
  it("declares the three aliases", () => {
    for (const alias of ["interlo-display", "interlo-text", "interlo-mono"]) {
      expect(fontsCss).toContain(`font-family: "${alias}"`);
    }
  });

  it("points every src at an existing local woff2 file and never at a URL", () => {
    const sources = [...fontsCss.matchAll(/src:\s*url\("([^"]+)"\)\s*format\("woff2"\)/g)].map(
      (m) => m[1],
    );
    expect(sources.length).toBeGreaterThanOrEqual(8);
    for (const src of sources) {
      expect(src, src).toMatch(/^\.\.\/assets\/fonts\/[a-z-]+\.woff2$/);
      expect(existsSync(`${UI}src/styles/${src}`), src).toBe(true);
    }
    expect(fontsCss).not.toMatch(/https?:/);
  });

  it("keeps every font file above the size the build inlines as a data URL", () => {
    // The CSP has no data: source for fonts, so an inlined font would not load.
    for (const name of readdirSync(`${UI}src/assets/fonts`)) {
      expect(statSync(`${UI}src/assets/fonts/${name}`).size, name).toBeGreaterThan(4096);
    }
  });
});

describe("attribution (AC-06)", () => {
  it("lists the three families in docs/tech-stack.md with their licence and where the text sits", () => {
    expect(familyRows.map((r) => r.role)).toEqual(["display", "text", "mono"]);
    expect(techStack).toContain("sits beside the font files");
  });

  it("keeps each licence text, with its author's copyright line, beside the font files", () => {
    for (const { role, name } of familyRows) {
      const text = readFileSync(`${UI}src/assets/fonts/${role}-OFL.txt`, "utf8");
      expect(text.split("\n")[0], role).toMatch(/^Copyright \d{4} The .+ Project Authors/);
      expect(text.split("\n")[0], role).toContain(name);
      expect(text, role).toContain("SIL OPEN FONT LICENSE Version 1.1");
    }
  });

  it("keeps the real family names out of the UI source and index.html", () => {
    const places = [...files(`${UI}src`), `${UI}index.html`].filter(
      (p) => !p.endsWith("fonts.test.ts"),
    );
    for (const path of places) {
      const text = readFileSync(path, "utf8").toLowerCase();
      for (const name of familyNames)
        expect(text, `${name} in ${path}`).not.toContain(name.toLowerCase());
    }
  });
});
