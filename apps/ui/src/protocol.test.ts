import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseEnvelope, type Envelope } from "./protocol";

// Same fixture the Rust tests use: fixtures/protocol/session_example.jsonl
const fixturePath = fileURLToPath(
  new URL("../../../fixtures/protocol/session_example.jsonl", import.meta.url),
);
const lines = readFileSync(fixturePath, "utf8")
  .split("\n")
  .filter((l) => l.trim().length > 0);

describe("protocol contract with meeting-core", () => {
  it("parses every fixture line", () => {
    const envs: Envelope[] = lines.map((l) => parseEnvelope(l));
    expect(envs).toHaveLength(9);
  });

  it("keeps sequence strictly increasing", () => {
    const seqs = lines.map((l) => parseEnvelope(l).sequence);
    for (let i = 1; i < seqs.length; i++) {
      expect(seqs[i]).toBeGreaterThan(seqs[i - 1]);
    }
  });

  it("rejects unknown event types", () => {
    const bad = JSON.stringify({
      ...JSON.parse(lines[0]),
      eventType: "transcript.segment.exploded",
    });
    expect(() => parseEnvelope(bad)).toThrow(/unknown eventType/);
  });

  it("rejects other schema versions", () => {
    const bad = JSON.stringify({ ...JSON.parse(lines[0]), schemaVersion: "9.9" });
    expect(() => parseEnvelope(bad)).toThrow(/unsupported schemaVersion/);
  });

  it("rejects non-integer sequence", () => {
    const bad = JSON.stringify({ ...JSON.parse(lines[0]), sequence: 1.5 });
    expect(() => parseEnvelope(bad)).toThrow(/sequence/);
  });
});
