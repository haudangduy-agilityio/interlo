import { useEffect, useState } from "react";
import { useAppState, useToast } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { toUiError } from "../../ipc/errors";
import { isPaused } from "../../ipc/meetingSession";
import { useSession } from "../../ipc/SessionProvider";
import type { Direction, MeetingMode } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { useActionFailure } from "../../shared/useActionFailure";
import { StartPanel, toStartArgs, useStartForm } from "../start";
import { LiveView } from "./LiveView";

export function LiveScreen() {
  const client = useClient();
  const { session, start, stop, setMute, setPaused, dismissBanner, clearStartFailure } =
    useSession();
  const { state: app, dispatch } = useAppState();
  const toast = useToast();
  const t = useT();
  const [mode, setMode] = useState<MeetingMode>("both");
  const { banner, fail, clear } = useActionFailure();
  const startForm = useStartForm();

  const setupBlocked = session.startFailure?.kind === "setup-incomplete";
  const settingsOpen = app.settingsOpen;

  // A failed start belongs to this visit. Leaving the screen forgets it.
  useEffect(() => clearStartFailure, [clearStartFailure]);

  // The setup message goes away once setup is ready, for example after a model download
  // in the Settings sheet, which opens over this screen.
  useEffect(() => {
    if (!setupBlocked || settingsOpen) return;
    let cancelled = false;
    client.checkSetup().then(
      (status) => {
        if (!cancelled && status.canStart) clearStartFailure();
      },
      () => {
        // The check failed: keep the message, since setup is still unconfirmed.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [setupBlocked, settingsOpen, client, clearStartFailure]);

  const confirmStart = async () => {
    const outcome = await start(toStartArgs(mode, startForm.form));
    // A refused per-meeting check keeps the panel open, with its fix steps.
    if (outcome.kind !== "checks-failed") startForm.close();
  };

  const checksFailure =
    session.startFailure?.kind === "checks-failed" ? session.startFailure : null;

  const run = (action: () => Promise<unknown>, done?: () => void) => {
    clear();
    action().then(done, (e: unknown) => {
      toast(toUiError(e).message, "error");
      fail(e);
    });
  };

  return (
    <LiveView
      session={session}
      mode={mode}
      error={banner}
      onModeChange={setMode}
      panel={
        startForm.open ? (
          <StartPanel
            form={startForm.form}
            onChange={startForm.change}
            failure={checksFailure}
            busy={session.status === "starting"}
            onConfirm={() => void confirmStart()}
            onCancel={() => {
              clearStartFailure();
              startForm.close();
            }}
          />
        ) : null
      }
      onStart={startForm.begin}
      onStop={() => run(stop)}
      onToggleMute={(direction: Direction) => {
        const muted = direction === "out" ? session.mutedOut : session.mutedIn;
        run(() => setMute(direction, !muted));
      }}
      onTogglePause={() => {
        const paused = isPaused(session);
        run(
          () => setPaused(!paused),
          () => toast(paused ? t("live.resumedToast") : t("live.pausedToast")),
        );
      }}
      onOpenSetup={() => dispatch({ type: "navigate", screen: "setup" })}
      onDismissBanner={dismissBanner}
    />
  );
}
