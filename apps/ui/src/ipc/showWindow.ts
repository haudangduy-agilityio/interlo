import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriShell } from "./tauriTransport";

export interface ShellWindow {
  show(): Promise<void>;
}

export interface ShowWindowDeps {
  inShell: () => boolean;
  getWindow: () => ShellWindow;
}

const realDeps: ShowWindowDeps = { inShell: isTauriShell, getWindow: getCurrentWindow };

/**
 * The shell starts the main window hidden, so the owner never sees the empty native
 * window colour before the first screen is drawn. The page shows it once it has mounted.
 * A refusal is ignored: the shell shows the window itself after a short delay.
 */
export async function showShellWindow(deps: ShowWindowDeps = realDeps): Promise<void> {
  if (!deps.inShell()) return;
  try {
    await deps.getWindow().show();
  } catch {
    // The shell's own fallback shows the window.
  }
}
