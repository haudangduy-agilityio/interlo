// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Tabs } from "./index";

afterEach(cleanup);

const TABS = [
  { id: "a", label: "Alpha" },
  { id: "b", label: "Beta" },
  { id: "c", label: "Gamma" },
] as const;

function Harness() {
  const [active, setActive] = useState<"a" | "b" | "c">("a");
  return (
    <>
      <Tabs label="Sections" tabs={TABS} active={active} onChange={setActive} />
      <button>After</button>
    </>
  );
}

const tab = (name: string) => screen.getByRole("tab", { name });

describe("Tabs", () => {
  it("only the active tab is in the Tab order", () => {
    render(<Harness />);
    expect(tab("Alpha").tabIndex).toBe(0);
    expect(tab("Beta").tabIndex).toBe(-1);
    expect(tab("Gamma").tabIndex).toBe(-1);
  });

  it("Tab leaves the tab strip after one stop", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.tab();
    expect(document.activeElement).toBe(tab("Alpha"));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "After" }));
  });

  it("ArrowRight and ArrowLeft move focus and select, and wrap around", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    tab("Alpha").focus();

    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(tab("Beta"));
    expect(tab("Beta").getAttribute("aria-selected")).toBe("true");
    expect(tab("Beta").tabIndex).toBe(0);
    expect(tab("Alpha").tabIndex).toBe(-1);

    await user.keyboard("{ArrowRight}{ArrowRight}");
    expect(document.activeElement).toBe(tab("Alpha"));

    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(tab("Gamma"));
    expect(tab("Gamma").getAttribute("aria-selected")).toBe("true");
  });

  it("Home and End go to the first and the last tab", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    tab("Alpha").focus();
    await user.keyboard("{End}");
    expect(document.activeElement).toBe(tab("Gamma"));
    await user.keyboard("{Home}");
    expect(document.activeElement).toBe(tab("Alpha"));
  });

  it("a click still selects a tab", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(tab("Beta"));
    expect(tab("Beta").getAttribute("aria-selected")).toBe("true");
  });
});
