import { describe, expect, it, vi } from "vitest";
import { translate } from "./i18n";

// A stand-in dictionary with a marker, not Vietnamese copy: one key has a value, one is empty,
// and every other key is missing.
vi.mock("./messages.vi", () => ({
  vi: { "nav.home": "[vi] Home", "nav.live": "" },
}));

describe("translate with a half-filled dictionary", () => {
  it("uses the text of the language when the key has one", () => {
    expect(translate("vi", "nav.home")).toBe("[vi] Home");
  });

  it("falls back to English for an empty value and for a missing key", () => {
    expect(translate("vi", "nav.live")).toBe("Live meeting");
    expect(translate("vi", "nav.meetings")).toBe("Meetings");
  });

  it("fills in {name} values in the fallback text too", () => {
    expect(translate("vi", "setup.downloadNamed", { name: "X" })).toBe("Download X");
  });
});
