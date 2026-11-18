import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { toUiError } from "./errors";
import { IPC_ERROR_CODES } from "./types";

describe("toUiError", () => {
  it("keeps the message of a plain failure", () => {
    expect(toUiError(new Error("Disk read failed."))).toEqual({
      code: "UNKNOWN",
      message: "Disk read failed.",
    });
    expect(toUiError("Shell crashed.")).toEqual({ code: "UNKNOWN", message: "Shell crashed." });
  });

  it("keeps a contract error unchanged", () => {
    const error = { code: "NO_MEETING", message: "No meeting is running." };
    expect(toUiError(error)).toEqual(error);
  });

  it("keeps the message when an UNKNOWN error is normalized a second time", () => {
    const once = toUiError(new Error("Disk read failed."));
    expect(toUiError(once)).toEqual(once);
  });

  it("falls back to a generic message for an unusable value", () => {
    expect(toUiError(42)).toEqual({ code: "UNKNOWN", message: "Something went wrong." });
    expect(toUiError({ code: "UNKNOWN" })).toEqual({
      code: "UNKNOWN",
      message: "Something went wrong.",
    });
  });
});

describe("details of START_CHECK_FAILED", () => {
  const failed = {
    code: "START_CHECK_FAILED",
    message: "Confirm before starting: headphones in use.",
    details: ["headphones"],
  };

  it("keeps the failed check ids", () => {
    expect(toUiError(failed)).toEqual(failed);
  });

  it("keeps them when the error is normalized a second time", () => {
    expect(toUiError(toUiError(failed))).toEqual(failed);
  });

  it("leaves details out when the shell sends none, a bad list, or another code", () => {
    expect(toUiError({ ...failed, details: undefined })).toEqual({
      code: "START_CHECK_FAILED",
      message: failed.message,
    });
    expect(toUiError({ ...failed, details: [1, "headphones"] })).toEqual({
      code: "START_CHECK_FAILED",
      message: failed.message,
    });
    expect(toUiError({ code: "NO_MEETING", message: "m", details: ["headphones"] })).toEqual({
      code: "NO_MEETING",
      message: "m",
    });
  });
});

describe("error codes", () => {
  it("toUiError keeps every code the contract names", () => {
    for (const code of IPC_ERROR_CODES) {
      expect(toUiError({ code, message: "m" })).toEqual({ code, message: "m" });
    }
    expect(toUiError({ code: "SOMETHING_ELSE", message: "m" }).code).toBe("UNKNOWN");
  });

  it("the code list is written once, in types.ts", () => {
    const files = ["errors.ts", "client.ts", "startOutcome.ts", "ClientProvider.tsx"];
    for (const name of files) {
      const source = readFileSync(new URL(`./${name}`, import.meta.url), "utf8");
      expect({ name, listsCodes: source.includes('"MEETING_RUNNING"') }).toEqual({
        name,
        listsCodes: false,
      });
    }
  });

  it("exports no helper that nothing uses", () => {
    const tauri = readFileSync(new URL("./tauriTransport.ts", import.meta.url), "utf8");
    const errors = readFileSync(new URL("./errors.ts", import.meta.url), "utf8");
    expect(tauri).not.toContain("hasTauriGlobal");
    expect(errors).not.toContain("export function isIpcError");
  });
});
