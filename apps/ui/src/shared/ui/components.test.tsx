// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Button, Chip, Icon, KeyCap, Meter, Page, Section, Segmented } from "./index";

afterEach(cleanup);

// AC-08: Button variants and sizes (design system Button README).
describe("Button", () => {
  it("has the danger variant and the small size", () => {
    render(
      <>
        <Button variant="danger">Stop</Button>
        <Button size="sm">Open</Button>
        <Button variant="primary">Start</Button>
      </>,
    );
    expect(screen.getByRole("button", { name: "Stop" }).className).toContain("btn--danger");
    expect(screen.getByRole("button", { name: "Open" }).className).toContain("btn--sm");
    expect(screen.getByRole("button", { name: "Start" }).className).toContain("btn--primary");
  });

  it("states that a control is not available yet and disables it", () => {
    render(<Button unavailable>Quick translate</Button>);
    const button = screen.getByRole("button", { name: "Quick translate" });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    const described = button.getAttribute("aria-describedby");
    expect(described).toBeTruthy();
    expect(document.getElementById(described ?? "")?.textContent).toBe("Not available yet");
  });
});

// AC-09: Page and Section.
describe("Page and Section", () => {
  it("Page has one h1 and an optional lead line", () => {
    render(
      <Page title="Today" lead="Friday 20 November">
        <Section title="Tasks">x</Section>
      </Page>,
    );
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByText("Friday 20 November")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Tasks" })).toBeTruthy();
  });
});

// AC-10: Chip, Segmented, Meter, KeyCap.
describe("Chip", () => {
  it("renders every kind and shows a dot with its word for live", () => {
    for (const kind of ["live", "ok", "warning", "error", "acc"] as const) {
      const { container, unmount } = render(<Chip kind={kind}>word</Chip>);
      expect(container.querySelector(`.chip--${kind}`)).toBeTruthy();
      unmount();
    }
    const { container } = render(
      <Chip kind="live" dot>
        Live
      </Chip>,
    );
    expect(container.querySelector(".dot--live")).toBeTruthy();
    expect(container.textContent).toContain("Live");
  });
});

function Controlled() {
  const [value, setValue] = useState("both");
  return (
    <Segmented
      label="Mode"
      value={value}
      onChange={setValue}
      options={[
        { id: "listen", label: "Listen only" },
        { id: "speak", label: "Speak only" },
        { id: "both", label: "Both" },
      ]}
    />
  );
}

describe("Segmented", () => {
  it("marks one segment pressed and moves with the arrow keys", async () => {
    const user = userEvent.setup();
    render(<Controlled />);
    const group = screen.getByRole("group", { name: "Mode" });
    expect(group).toBeTruthy();
    expect(screen.getByRole("button", { name: "Both" }).getAttribute("aria-pressed")).toBe("true");
    screen.getByRole("button", { name: "Both" }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("button", { name: "Speak only" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Speak only" }));
    expect(group.querySelectorAll('[aria-pressed="true"]')).toHaveLength(1);
  });
});

describe("Meter", () => {
  it("exposes role=meter with its range and value", () => {
    render(<Meter label="Memory" value={40} />);
    const meter = screen.getByRole("meter", { name: "Memory" });
    expect(meter.getAttribute("aria-valuemin")).toBe("0");
    expect(meter.getAttribute("aria-valuemax")).toBe("100");
    expect(meter.getAttribute("aria-valuenow")).toBe("40");
    expect(meter.querySelector("i")?.style.width).toBe("40%");
  });

  it("keeps the value between 0 and 100", () => {
    const { rerender } = render(<Meter label="Memory" value={140} />);
    const meter = () => screen.getByRole("meter", { name: "Memory" });
    expect(meter().getAttribute("aria-valuenow")).toBe("100");
    expect(meter().querySelector("i")?.style.width).toBe("100%");
    rerender(<Meter label="Memory" value={-5} />);
    expect(meter().getAttribute("aria-valuenow")).toBe("0");
    expect(meter().querySelector("i")?.style.width).toBe("0%");
  });

  it("shows an empty track with no value when it is not available", () => {
    render(<Meter label="Memory" unavailable />);
    const meter = screen.getByRole("meter", { name: "Memory" });
    expect(meter.hasAttribute("aria-valuenow")).toBe(false);
    expect(meter.getAttribute("aria-disabled")).toBe("true");
    expect(meter.querySelector("i")).toBeNull();
  });
});

describe("KeyCap", () => {
  it("has a default size and a small size", () => {
    const { container } = render(
      <>
        <KeyCap>Ctrl+Option+T</KeyCap>
        <KeyCap size="sm">⌘K</KeyCap>
      </>,
    );
    const caps = container.querySelectorAll(".key");
    expect(caps).toHaveLength(2);
    expect(caps[0].className).not.toContain("key--sm");
    expect(caps[1].className).toContain("key--sm");
  });
});

describe("Icon", () => {
  it("is hidden from screen readers and takes its colour from the text", () => {
    const { container } = render(<Icon name="home" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("stroke")).toBe("currentColor");
  });
});
