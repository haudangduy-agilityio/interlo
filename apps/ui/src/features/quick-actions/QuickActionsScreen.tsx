import { useState } from "react";
import { useToast } from "../../app/AppState";
import { useClient } from "../../ipc/ClientProvider";
import { toUiError } from "../../ipc/errors";
import { useT, type MessageKey } from "../../shared/i18n";
import { QuickActionsPanel, type PreviewState, type QuickActionKind } from "./QuickActionsPanel";

const WILL_DO: Record<Exclude<QuickActionKind, "save-note">, MessageKey> = {
  translate: "quick.willTranslate",
  "fix-grammar": "quick.willFix",
};

export function QuickActionsScreen() {
  const client = useClient();
  const toast = useToast();
  const t = useT();
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<PreviewState>({ status: "idle" });

  const run = (action: QuickActionKind) => {
    if (action !== "save-note") {
      setPreview({ status: "unavailable", action, willDo: t(WILL_DO[action]) });
      return;
    }
    setPreview({ status: "working", action });
    client
      .addNote({ text, tag: "quick" })
      .then(() => {
        setPreview({ status: "done", action, message: t("quick.saved") });
        toast(t("quick.savedToast"));
      })
      .catch((e: unknown) =>
        setPreview({ status: "error", action, message: toUiError(e).message }),
      );
  };

  return <QuickActionsPanel text={text} onTextChange={setText} preview={preview} onRun={run} />;
}
