import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useAppState } from "../../app/AppState";
import { useSetupStatus } from "../../app/SetupStatusProvider";
import { toUiError } from "../../ipc/errors";
import { useSession } from "../../ipc/SessionProvider";
import type { MeetingMode, SetupStatus } from "../../ipc/types";
import { useT } from "../../shared/i18n";
import { Dialog } from "../../shared/ui";
import { StartPanelBody } from "./StartPanel";
import { toStartArgs, useStartForm } from "./startForm";

interface StartApi {
  /**
   * Press Start from any screen. Not ready: Setup opens with a toast (J1, S-04). Ready: the
   * "Before you start" dialog opens with `mode` selected. A setup check that cannot run shows a
   * toast and opens nothing.
   */
  requestStart: (mode?: MeetingMode) => Promise<void>;
}

const StartContext = createContext<StartApi | null>(null);

/** Owns the "Before you start" dialog, so the top bar and Home start a meeting the same way. */
export function StartProvider({ children }: { children: ReactNode }) {
  const { dispatch } = useAppState();
  const { state: setup, refresh } = useSetupStatus();
  const { session, start, clearStartFailure } = useSession();
  const t = useT();
  const startForm = useStartForm();
  const { begin } = startForm;
  const [mode, setMode] = useState<MeetingMode>("both");

  const toast = useCallback(
    (message: string, kind: "info" | "error" = "info") =>
      dispatch({ type: "showToast", message, kind }),
    [dispatch],
  );

  const requestStart = useCallback(
    async (chosen: MeetingMode = "both") => {
      let status: SetupStatus;
      if (setup.status === "done") {
        status = setup.data;
      } else {
        try {
          status = await refresh();
        } catch (e) {
          toast(t("start.checkFailed", { message: toUiError(e).message }), "error");
          return;
        }
      }
      if (!status.canStart) {
        dispatch({ type: "navigate", screen: "setup" });
        toast(t("start.finishSetup", { groups: status.missingGroups.join(", ") }));
        return;
      }
      clearStartFailure();
      setMode(chosen);
      begin();
    },
    [setup, refresh, toast, t, dispatch, clearStartFailure, begin],
  );

  // A failed start belongs to the dialog that showed it. Closing it forgets the failure.
  const { close } = startForm;
  const closeDialog = useCallback(() => {
    clearStartFailure();
    close();
  }, [clearStartFailure, close]);
  useEffect(() => clearStartFailure, [clearStartFailure]);

  const confirm = async () => {
    const outcome = await start(toStartArgs(mode, startForm.form));
    if (outcome.kind === "started") {
      close();
      dispatch({ type: "navigate", screen: "live" });
    }
  };

  const api = useMemo(() => ({ requestStart }), [requestStart]);
  return (
    <StartContext.Provider value={api}>
      {children}
      {startForm.open ? (
        <Dialog title={t("start.title")} onClose={closeDialog}>
          <StartPanelBody
            form={startForm.form}
            onChange={startForm.change}
            failure={session.startFailure}
            busy={session.status === "starting"}
            mode={mode}
            onModeChange={setMode}
            onConfirm={() => void confirm()}
            onCancel={closeDialog}
          />
        </Dialog>
      ) : null}
    </StartContext.Provider>
  );
}

export function useStart(): StartApi {
  const api = useContext(StartContext);
  if (!api) throw new Error("useStart needs a StartProvider");
  return api;
}
