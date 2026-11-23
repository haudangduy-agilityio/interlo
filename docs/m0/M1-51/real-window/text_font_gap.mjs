// Prints which of the typed code points the bundled text font files do not hold (from the log of
// type_telex.py), one hex value per line. Usage: node text_font_gap.mjs <fonts dir> <telex.log>
import { readFileSync } from "node:fs";
import { codepoints } from "./woff2cmap.mjs";
const [dir, logPath] = process.argv.slice(2);
const line = readFileSync(logPath, "utf8").split("\n").find((l) => l.includes(" codepoints=U+")) ?? "";
const typed = [...line.matchAll(/U\+([0-9A-F]{4})/g)].map((m) => parseInt(m[1], 16));
const have = new Set();
for (const sub of ["latin", "latin-ext"]) for (const c of codepoints(`${dir}/text-${sub}.woff2`)) have.add(c);
const missing = typed.filter((c) => !have.has(c));
console.error(`typed=${typed.length} held_by_text_font=${typed.length - missing.length} fallback_needed=${missing.length}`);
for (const c of missing) console.log(c.toString(16));
