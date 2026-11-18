// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Banner, Button, Dialog } from "./index";

afterEach(cleanup);

function Harness({ onWindowKey }: { onWindowKey?: (code: string) => void }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!onWindowKey) return;
    const listener = (e: KeyboardEvent) => onWindowKey(e.code);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [onWindowKey]);
  return (
    <div>
      <Button onClick={() => setOpen(true)}>Open</Button>
      {open ? (
        <Dialog title="Sample title" onClose={() => setOpen(false)}>
          <p>Body text.</p>
          <Button>Inside</Button>
        </Dialog>
      ) : null}
    </div>
  );
}

describe("Dialog", () => {
  it("is a modal dialog named by its title", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Open" }));
    const dialog = screen.getByRole("dialog", { name: "Sample title" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("closes on Escape without the key reaching a window listener, and gives focus back", async () => {
    const user = userEvent.setup();
    const onWindowKey = vi.fn();
    render(<Harness onWindowKey={onWindowKey} />);
    const opener = screen.getByRole("button", { name: "Open" });
    await user.click(opener);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
    expect(onWindowKey).not.toHaveBeenCalledWith("Escape");
  });

  it("closes with the Close button", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Open" }));
    await user.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("Banner details", () => {
  it("puts the details inside the same alert as the message", () => {
    const html = renderToStaticMarkup(
      <Banner
        kind="error"
        details={
          <ul aria-label="Fix steps">
            <li>Do this.</li>
          </ul>
        }
      >
        Message text.
      </Banner>,
    );
    expect(html).toMatch(/role="alert".*Message text\..*<ul aria-label="Fix steps">/);
  });

  it("renders as before when there are no details", () => {
    const html = renderToStaticMarkup(<Banner kind="info">Hello.</Banner>);
    expect(html).toBe(
      '<div class="banner banner--info" role="status"><p class="banner__text">Hello.</p></div>',
    );
  });
});
