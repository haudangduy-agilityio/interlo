import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// D-55: the mute default is Ctrl+Option+M. Option+M alone types a character on some layouts.
const UI_ROOT = fileURLToPath(new URL("../", import.meta.url));
const SELF = "hotkeyText.test.ts";

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = `${dir}/${name}`;
    if (name === "node_modules" || name === "dist" || name === ".vitest") return [];
    if (statSync(path).isDirectory()) return files(path);
    return /\.(tsx?|css|html)$/.test(name) && name !== SELF ? [path] : [];
  });
}

describe("hotkey default text", () => {
  it("never shows the mute default as Option+M or a bare Option M key", () => {
    // A hotkey default needs Control and Option together, so a bare "Option+M" is always wrong.
    const bare = /(?<!Ctrl\+)(?<!Control\+)Option\+M\b|(?<!⌃)⌥M\b/;
    const hits = [...files(`${UI_ROOT}src`), ...files(`${UI_ROOT}e2e`)].filter((f) =>
      bare.test(readFileSync(f, "utf8")),
    );
    expect(hits).toEqual([]);
  });
});
