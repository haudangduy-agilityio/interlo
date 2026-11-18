import { useT, type MessageKey } from "../../shared/i18n";
import { Banner, Button, ErrorState, LoadingState, NotAvailable, Section } from "../../shared/ui";

export type QuickActionKind = "translate" | "fix-grammar" | "save-note";

export type PreviewState =
  | { status: "idle" }
  | { status: "working"; action: QuickActionKind }
  /** The shell has no command for this action yet. */
  | { status: "unavailable"; action: QuickActionKind; willDo: string }
  | { status: "done"; action: QuickActionKind; message: string }
  | { status: "error"; action: QuickActionKind; message: string };

export interface QuickActionsPanelProps {
  text: string;
  onTextChange: (text: string) => void;
  preview: PreviewState;
  onRun: (action: QuickActionKind) => void;
}

const ACTIONS: readonly { id: QuickActionKind; labelKey: MessageKey }[] = [
  { id: "translate", labelKey: "quick.translate" },
  { id: "fix-grammar", labelKey: "quick.fixGrammar" },
  { id: "save-note", labelKey: "quick.saveNote" },
];

function PreviewRegion({
  preview,
  onRun,
}: {
  preview: PreviewState;
  onRun: (a: QuickActionKind) => void;
}) {
  const t = useT();
  switch (preview.status) {
    case "idle":
      return <p className="muted">{t("quick.choose")}</p>;
    case "working":
      return <LoadingState label={t("quick.working")} />;
    case "unavailable":
      return <NotAvailable>{preview.willDo}</NotAvailable>;
    case "done":
      return <Banner kind="info">{preview.message}</Banner>;
    case "error":
      return <ErrorState message={preview.message} onRetry={() => onRun(preview.action)} />;
  }
}

/** Preview panel for quick text actions. The container decides what each action does. */
export function QuickActionsPanel({ text, onTextChange, preview, onRun }: QuickActionsPanelProps) {
  const t = useT();
  const busy = preview.status === "working";
  const empty = text.trim().length === 0;
  return (
    <div className="surface">
      <h1>{t("quick.title")}</h1>
      <Section title={t("quick.sectionText")}>
        <label className="field">
          <span>{t("quick.textLabel")}</span>
          <textarea value={text} rows={4} onChange={(e) => onTextChange(e.target.value)} />
        </label>
        <div className="controls">
          {ACTIONS.map((a) => (
            <Button key={a.id} disabled={busy || empty} onClick={() => onRun(a.id)}>
              {t(a.labelKey)}
            </Button>
          ))}
        </div>
      </Section>
      <Section title={t("quick.sectionPreview")}>
        <PreviewRegion preview={preview} onRun={onRun} />
      </Section>
    </div>
  );
}
