import { memo, useLayoutEffect, useRef, useState } from "react";
import {
  INCOMING_SOURCE_ID,
  OUTGOING_SOURCE_ID,
  type SessionCaption,
} from "../../ipc/meetingSession";
import { useT, type MessageKey } from "../../shared/i18n";
import { Button } from "../../shared/ui";

const STATUS_KEY: Record<string, MessageKey> = {
  partial: "caption.status.partial",
  stable: "caption.status.stable",
  final: "caption.status.final",
  pending: "caption.status.pending",
  failed: "caption.status.failed",
};

/** Who spoke. The raw ids "incoming" and "outgoing" are internal and never shown. */
const SPEAKER_KEY: Record<string, MessageKey> = {
  [INCOMING_SOURCE_ID]: "caption.partner",
  [OUTGOING_SOURCE_ID]: "caption.you",
};

// Starting value: a little under one caption row, so a reader a few pixels short of the end
// still counts as being at the end.
const AT_BOTTOM_TOLERANCE_PX = 24;

/** Shown in the translation column of a final session line that has no translation. */
const NO_TRANSLATION = "\u2014";

const CaptionRow = memo(function CaptionRow({ caption: c }: { caption: SessionCaption }) {
  const t = useT();
  const speakerKey = SPEAKER_KEY[c.sourceId];
  const untranslated = c.delayMs !== undefined && c.sourceStatus === "final";
  const waiting = c.translationStatus ? t(STATUS_KEY[c.translationStatus]) : "…";
  return (
    <li className={`caption caption--${c.sourceStatus}`}>
      <div className="caption__src">
        <span className="caption__meta">
          {formatMs(c.startMs)} · {speakerKey ? t(speakerKey) : c.sourceId}
          {c.delayMs === undefined ? null : ` · ${t("caption.delay", { ms: c.delayMs })}`}
        </span>
        <span>{c.sourceText}</span>
        {c.sourceStatus !== "final" && (
          <span className="badge">{t(STATUS_KEY[c.sourceStatus])}</span>
        )}
      </div>
      <div className="caption__tgt">
        {c.targetText !== null ? (
          <span>{c.targetText}</span>
        ) : (
          <span className="muted">{untranslated ? NO_TRANSLATION : waiting}</span>
        )}
      </div>
    </li>
  );
});

/**
 * Two-column caption list: source on the left, translation on the right.
 * Partial source text is styled as provisional. A null target means no
 * translation matches the current source revision yet. A row renders again only
 * when its own caption object changes; the session keeps the other objects as they are.
 *
 * The list is a scrollable log region (`aria-live="off"`: a screen reader does not read each
 * update; the reader moves through it). While the reader is at the end, new lines scroll into
 * view. After the reader scrolls up, the list stays where it is and a "Latest" button appears.
 */
export function CaptionList({ captions }: { captions: readonly SessionCaption[] }) {
  const t = useT();
  const logRef = useRef<HTMLDivElement>(null);
  const atBottomRef = useRef(true);
  const [atBottom, setAtBottom] = useState(true);

  useLayoutEffect(() => {
    const log = logRef.current;
    if (log && atBottomRef.current) log.scrollTop = log.scrollHeight;
  }, [captions]);

  if (captions.length === 0) {
    return <p className="empty">{t("captions.waiting")}</p>;
  }

  const onScroll = () => {
    const log = logRef.current;
    if (!log) return;
    const atEnd = log.scrollHeight - log.scrollTop - log.clientHeight <= AT_BOTTOM_TOLERANCE_PX;
    atBottomRef.current = atEnd;
    setAtBottom(atEnd);
  };
  const jumpToLatest = () => {
    const log = logRef.current;
    if (!log) return;
    log.scrollTop = log.scrollHeight;
    atBottomRef.current = true;
    setAtBottom(true);
  };

  return (
    <div className="captions-wrap">
      <div
        ref={logRef}
        className="captions-log"
        role="log"
        aria-live="off"
        aria-label={t("captions.label")}
        tabIndex={0}
        onScroll={onScroll}
      >
        <ol className="captions">
          {captions.map((c) => (
            <CaptionRow key={c.segmentId} caption={c} />
          ))}
        </ol>
      </div>
      {atBottom ? null : (
        <Button className="captions-latest" onClick={jumpToLatest}>
          {t("captions.latest")}
        </Button>
      )}
    </div>
  );
}

export function formatMs(ms: number): string {
  const total = Math.floor(ms / 1000);
  const m = String(Math.floor(total / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return `${m}:${s}`;
}
