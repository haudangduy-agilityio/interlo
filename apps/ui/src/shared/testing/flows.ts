import { screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/** Fill the "Before you start" panel so every per-meeting check passes, then confirm. */
export async function confirmStartPanel(user: UserEvent): Promise<void> {
  const panel = await screen.findByRole("region", { name: "Before you start" });
  await user.selectOptions(within(panel).getByRole("combobox", { name: "Meeting app" }), "browser");
  await user.click(within(panel).getByRole("checkbox", { name: "I use headphones" }));
  await user.click(
    within(panel).getByRole("checkbox", { name: "My meeting app microphone is set to Interlo" }),
  );
  await user.click(within(panel).getByRole("button", { name: "Start now" }));
}

/** Press Start on the Live screen, then confirm the panel. */
export async function startMeetingFromLive(user: UserEvent): Promise<void> {
  await user.click(await screen.findByRole("button", { name: "Start meeting" }));
  await confirmStartPanel(user);
}

/** Accept the licence in the open download dialog and confirm the download. */
export async function acceptLicenceAndDownload(user: UserEvent): Promise<void> {
  const dialog = await screen.findByRole("dialog", { name: /^Download / });
  await user.click(within(dialog).getByRole("checkbox", { name: "I accept this licence" }));
  await user.click(within(dialog).getByRole("button", { name: "Download" }));
}
