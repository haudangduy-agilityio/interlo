// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Button, Dialog } from "./index";

afterEach(cleanup);

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <Button onClick={() => setOpen(true)}>Open</Button>
      <Button>Behind</Button>
      {open ? (
        <Dialog title="Sample" onClose={() => setOpen(false)}>
          <Button>First</Button>
          <Button>Last</Button>
        </Dialog>
      ) : null}
    </div>
  );
}

async function openDialog() {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Open" }));
  return user;
}

const focused = () => document.activeElement?.textContent;

describe("Dialog focus trap", () => {
  it("Tab from the last control goes to the first control of the dialog, not to the page", async () => {
    const user = await openDialog();
    // The dialog itself has focus first; its controls are Close, First, Last.
    await user.tab();
    await user.tab();
    await user.tab();
    expect(focused()).toBe("Last");
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close dialog" }));
    expect(focused()).not.toBe("Behind");
  });

  it("Shift+Tab from the first control goes to the last control", async () => {
    const user = await openDialog();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close dialog" }));
    await user.tab({ shift: true });
    expect(focused()).toBe("Last");
  });

  it("Shift+Tab with focus on the dialog itself goes to the last control", async () => {
    const user = await openDialog();
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    await user.tab({ shift: true });
    expect(focused()).toBe("Last");
  });

  it("moves focus back into the dialog when it lands outside", async () => {
    await openDialog();
    screen.getByRole("button", { name: "Behind" }).focus();
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  it("still gives focus back to the opener after close", async () => {
    const user = await openDialog();
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Open" }));
  });
});
