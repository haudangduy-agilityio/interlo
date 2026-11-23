// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { formatDateTime } from "./format";
import { LanguageProvider, localeFor, translate, useLanguage, useT } from "./i18n";
import { en } from "./messages.en";
import { vi } from "./messages.vi";

afterEach(cleanup);

function Probe() {
  const { language, setLanguage } = useLanguage();
  const t = useT();
  return (
    <div>
      <p>language:{language}</p>
      <p>text:{t("nav.home")}</p>
      <button onClick={() => setLanguage("vi")}>to vi</button>
      <button onClick={() => setLanguage("en")}>to en</button>
    </div>
  );
}

describe("translate", () => {
  it("gives the English text and fills in {name} values", () => {
    expect(translate("en", "nav.home")).toBe("Today");
    expect(translate("en", "live.latency", { p50: 820, p95: 1450 })).toBe(
      "Latency: median 820 ms, 95th percentile 1450 ms.",
    );
  });

  it("leaves a placeholder in place when no value is given for it", () => {
    expect(translate("en", "setup.downloadNamed", {})).toBe("Download {name}");
  });

  it("the Vietnamese dictionary has exactly the keys of the English one", () => {
    expect(Object.keys(vi).sort()).toEqual(Object.keys(en).sort());
  });
});

describe("LanguageProvider", () => {
  it("sets <html lang> to the language, and changes it when the language changes", () => {
    render(
      <LanguageProvider>
        <Probe />
      </LanguageProvider>,
    );
    expect(document.documentElement.lang).toBe("en");
    act(() => screen.getByRole("button", { name: "to vi" }).click());
    expect(document.documentElement.lang).toBe("vi");
    expect(screen.getByText("language:vi")).toBeTruthy();
    act(() => screen.getByRole("button", { name: "to en" }).click());
    expect(document.documentElement.lang).toBe("en");
  });

  it("starts in the language it is given", () => {
    render(
      <LanguageProvider initial="vi">
        <Probe />
      </LanguageProvider>,
    );
    expect(document.documentElement.lang).toBe("vi");
  });

  it("speaks English without a provider", () => {
    render(<Probe />);
    expect(screen.getByText("language:en")).toBeTruthy();
    expect(screen.getByText("text:Today")).toBeTruthy();
  });
});

describe("dates follow the interface language", () => {
  const ms = Date.UTC(2026, 10, 16, 9, 30, 0);

  it("formats with the locale of each language", () => {
    expect(localeFor("en")).toBe("en-US");
    expect(localeFor("vi")).toBe("vi-VN");
    expect(formatDateTime(ms, "vi")).toBe(
      new Date(ms).toLocaleString("vi-VN", { dateStyle: "medium", timeStyle: "short" }),
    );
    expect(formatDateTime(ms, "vi")).not.toBe(formatDateTime(ms, "en"));
  });

  it("uses English when no language is given", () => {
    expect(formatDateTime(ms)).toBe(formatDateTime(ms, "en"));
  });
});
