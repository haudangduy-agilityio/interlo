import { userFacingError } from '../../utils/userFacingError'
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ElementType,
} from "react";
import { createPortal } from "react-dom";
import {
  AlertCircle,
  ArrowUp,
  AudioLines,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Download,
  Languages,
  Link2,
  Loader2,
  Lock,
  MessageCircle,
  MessageSquarePlus,
  Pause,
  Play,
  RefreshCw,
  Search,
  SkipBack,
  SkipForward,
  Sparkles,
  Trash2,
  UsersRound,
  Unlink,
  X,
  Zap,
} from "lucide-react";
import WavesurferPlayer from "@wavesurfer/react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type WaveSurfer from "wavesurfer.js";
import NotionEditor from "../../pages/_components/NotionEditor";
import { resolveNotepadFormat } from "../../lib/notepadFormat";
import { tForLocale, useTranslation } from "../../hooks/useTranslation";
import { toBcp47Locale } from "../../utils/languages";
import {
  createRecordingComment,
  deleteRecordingComment,
  fetchRecordingDetail,
  RecordingRequestError,
  fetchRecordingComments,
  fetchRecordingPlayback,
  fetchRecordingTranscript,
  getOrCreateRecordingShareLink,
  regenerateRecordingNotes,
  retryRecordingNotes,
  revokeRecordingShareLink,
  restoreOriginalRecordingTranscript,
  shareRecording,
  translateRecordingNotes,
  translateRecordingTranscript,
  unblurRecording,
  updateRecordingComment,
  updateRecordingNoteSection,
  updateRecordingNotepad,
  updateRecordingSpeaker,
  updateRecordingTitle,
  updateRecordingTranscriptSection,
  type MeetingAssistantMessage,
  type MeetingNoteSection,
  type RecordingComment,
  type RecordingDetail,
  type RecordingNoteVersion,
  type RecordingSharePermission,
  type RecordingTranscriptSection,
} from "../../lib/recordingsApi";
import { DesktopMeetingPaywallOverlay } from "./DesktopMeetingPaywallOverlay";
import { DesktopMeetingAssistant } from "./DesktopMeetingAssistant";
import { TranscriptCleanupDialog } from "./TranscriptCleanupDialog";
import { RecentPeopleSuggestions } from "./RecentPeopleSuggestions";
import { createChat, createInvite, listChats, sendMessage } from "../../lib/chat/chatApi";
import type { ApiChat } from "../../lib/chat/apiTypes";

type RecordingDetailTab = "my-notes" | "enhanced" | "transcript";
type TranscriptCopyMode = "source" | "target" | "both";

const RECORDING_COMMENTS_ENABLED = true;
/** Diameter of the floating "comment on this selection" bubble, in px. */
const SELECTION_BUBBLE_SIZE = 32;
/** Fallback until the floating player/Ask stack is measured. Matches old `pb-32`. */
const RECORDING_DETAIL_BOTTOM_INSET_FALLBACK = 128;

type RecordingCommentAnchor =
  | {
      kind: "note";
      id: string;
      label: string;
      excerpt: string;
    }
  | {
      kind: "transcript";
      id: string;
      label: string;
      excerpt: string;
    };

export type DesktopMeetingAssistantLaunch = {
  meetingId: string;
  title: string;
  initialQuestion: string;
  initialDraft?: string;
  history?: MeetingAssistantMessage[];
  meetingChatId?: string;
  sessionId?: string;
};

interface DesktopRecordingDetailProps {
  meetingId: string;
  title: string;
  /** When true, keep skeletons and retry fetch until the meeting is ready (post-recording). */
  awaitReady?: boolean;
  initialTab?: RecordingDetailTab;
  onOpenAssistant?: (launch: DesktopMeetingAssistantLaunch) => void;
  onOpenPricing?: (referrer?: string) => void;
}

interface TabOption {
  id: RecordingDetailTab;
  label: string;
  icon?: ElementType;
}

interface AskPromptOption {
  id: "listQa" | "followUpEmail" | "afterMeeting";
  label: string;
}

const languageOptions = [
  ["en", "English"],
  ["ja", "Japanese"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
  ["pt", "Portuguese"],
  ["cn1", "Chinese"],
  ["kr", "Korean"],
  ["vi", "Vietnamese"],
] as const;

const htmlToText = (html: string) => {
  const el = document.createElement("div");
  el.innerHTML = html;
  return (el.innerText || el.textContent || "").trim();
};

const errorMessage = (error: unknown, fallback: string) =>
  userFacingError(error instanceof Error ? error.message : null, fallback);

const formatDate = (value: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));

const formatLongDate = (
  start: string,
  end: string | null,
  locale: string,
  nowLabel: string,
) => {
  const startDate = new Date(start);
  const endDate = end ? new Date(end) : null;
  const day = new Intl.DateTimeFormat(locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(startDate);
  const time = new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(startDate);
  const endTime = endDate
    ? new Intl.DateTimeFormat(locale, {
        hour: "numeric",
        minute: "2-digit",
      }).format(endDate)
    : nowLabel;
  return `${day} · ${time} – ${endTime}`;
};

const secondsFromMeetingStart = (meetingStart: string, sectionStart: string) =>
  Math.max(
    0,
    Math.round(
      (new Date(sectionStart).getTime() - new Date(meetingStart).getTime()) /
        1000,
    ),
  );

const formatTimecode = (seconds: number) => {
  if (!Number.isFinite(seconds) || seconds < 0) return "00:00:00";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  return [hours, minutes, secs]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
};

const inlineMarkdownFromNode = (node: Node): string => {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  if (!(node instanceof HTMLElement)) return "";

  const content = Array.from(node.childNodes)
    .map(inlineMarkdownFromNode)
    .join("");

  switch (node.tagName.toLowerCase()) {
    case "br":
      return "\n";
    case "strong":
    case "b":
      return `**${content}**`;
    case "em":
    case "i":
      return `*${content}*`;
    case "s":
    case "del":
      return `~~${content}~~`;
    case "code":
      return `\`${content}\``;
    case "a":
      return `[${content}](${node.getAttribute("href") ?? ""})`;
    default:
      return content;
  }
};

const markdownFromBlock = (node: Node): string => {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent?.trim() ?? "";
  if (!(node instanceof HTMLElement)) return "";

  const tag = node.tagName.toLowerCase();
  const inline = () =>
    Array.from(node.childNodes).map(inlineMarkdownFromNode).join("").trim();

  if (/^h[1-6]$/.test(tag)) {
    return `${"#".repeat(Number(tag[1]))} ${inline()}`;
  }
  if (tag === "p" || tag === "div") return inline();
  if (tag === "hr") return "---";
  if (tag === "pre") return `\`\`\`\n${node.textContent ?? ""}\n\`\`\``;
  if (tag === "blockquote") {
    return (node.innerText || node.textContent || "")
      .split("\n")
      .map((line) => `> ${line}`)
      .join("\n");
  }
  if (tag === "ul" || tag === "ol") {
    return Array.from(node.children)
      .filter((child) => child.tagName.toLowerCase() === "li")
      .map((child, index) => {
        const marker = tag === "ol" ? `${index + 1}.` : "-";
        const item = Array.from(child.childNodes)
          .filter(
            (itemNode) =>
              !(
                itemNode instanceof HTMLElement &&
                ["ul", "ol"].includes(itemNode.tagName.toLowerCase())
              ),
          )
          .map(inlineMarkdownFromNode)
          .join("")
          .trim();
        const nested = Array.from(child.children)
          .filter((nestedChild) =>
            ["ul", "ol"].includes(nestedChild.tagName.toLowerCase()),
          )
          .map(markdownFromBlock)
          .join("\n");
        return `${marker} ${item}${
          nested
            ? `\n${nested
                .split("\n")
                .map((line) => `  ${line}`)
                .join("\n")}`
            : ""
        }`;
      })
      .join("\n");
  }
  return inline();
};

const markdownFromEditable = (element: HTMLElement) =>
  Array.from(element.childNodes)
    .map(markdownFromBlock)
    .filter(Boolean)
    .join("\n\n")
    .trim();

function StructuredSection({
  heading,
  anchorId,
  content,
  editable,
  canComment,
  onComment,
  onSave,
}: {
  heading: string;
  anchorId: string;
  content: string;
  editable: boolean;
  canComment?: boolean;
  onComment?: () => void;
  onSave: (content: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [renderVersion, setRenderVersion] = useState(0);
  const [renderedContent, setRenderedContent] = useState(content);

  // Keep the Markdown element types stable across parent re-renders. Creating
  // these component functions inline makes React replace the rendered text
  // nodes whenever selectionDraft changes, which collapses a live selection
  // in Enhanced Notes to a caret.
  const markdownComponents = useMemo<Components>(
    () => ({
      h1: ({ node: _node, ...props }) => (
        <h1
          className="detail-note-heading mb-3 mt-6 text-[21px] font-semibold leading-7 first:mt-0"
          {...props}
        />
      ),
      h2: ({ node: _node, ...props }) => (
        <h2
          className="detail-note-heading mb-2 mt-5 text-[19px] font-semibold leading-7 first:mt-0"
          {...props}
        />
      ),
      h3: ({ node: _node, ...props }) => (
        <h3
          className="detail-note-heading mb-2 mt-4 text-[16px] font-semibold leading-7 first:mt-0"
          {...props}
        />
      ),
      p: ({ node: _node, ...props }) => (
        <p className="mb-3 last:mb-0" {...props} />
      ),
      ul: ({ node: _node, ...props }) => (
        <ul className="mb-3 list-disc space-y-1 pl-6 last:mb-0" {...props} />
      ),
      ol: ({ node: _node, ...props }) => (
        <ol
          className="mb-3 list-decimal space-y-1 pl-6 last:mb-0"
          {...props}
        />
      ),
    }),
    [],
  );

  useEffect(() => {
    setRenderedContent(content);
  }, [content]);

  return (
    <section className="mb-7 last:mb-0">
      <div className="group/comment-heading mb-2 flex items-center gap-2">
        <h2 className="detail-note-heading text-[21px] font-semibold tracking-[-0.01em]">
          {heading}
        </h2>
        {canComment && onComment && (
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={onComment}
            title={t("desktopRecordingDetail.comments.addOnSelection")}
            aria-label={t("desktopRecordingDetail.comments.addOnSelection")}
            className="recording-comment-affordance flex h-7 w-7 items-center justify-center rounded-md text-[#686871] opacity-0 transition hover:bg-[#202024] hover:text-[#D7D7DB] group-hover/comment-heading:opacity-100 focus-visible:opacity-100"
          >
            <MessageSquarePlus size={14} />
          </button>
        )}
      </div>
      {/* Chromium mutates the descendants of a contentEditable element as the
          user types. Remount the whole editable block after saving (or rolling
          back), rather than asking React to reconcile the mutated Markdown
          children. Reconciling those stale child nodes can throw a NotFoundError
          and take down the entire renderer. */}
      <div
        key={renderVersion}
        data-comment-base={anchorId}
        data-comment-kind="note"
        data-comment-label={heading}
        contentEditable={editable}
        suppressContentEditableWarning
        onBlur={(event) => {
          if (!editable) return;
          const next = markdownFromEditable(event.currentTarget);
          if (next && next !== renderedContent.trim()) {
            // Render the edited Markdown immediately. Persistence can finish in
            // the background, just like the personal-notes editor.
            setRenderedContent(next);
            setRenderVersion((version) => version + 1);
            void onSave(next).catch(() => {
              // The parent reports the save error. Re-mount the Markdown so an
              // unsaved contentEditable mutation does not remain on screen.
              setRenderedContent(content);
              setRenderVersion((version) => version + 1);
            });
          }
        }}
        onKeyDown={(event) => {
          if (!editable || event.key !== " ") return;
          const selection = window.getSelection();
          const anchor = selection?.anchorNode;
          const block =
            anchor instanceof HTMLElement
              ? anchor
              : anchor?.parentElement;
          const prefix = block?.textContent?.trim() ?? "";
          const headingMatch = prefix.match(/^(#{1,6})$/);
          if (
            !headingMatch ||
            !block ||
            block === event.currentTarget ||
            !event.currentTarget.contains(block)
          ) {
            return;
          }

          event.preventDefault();
          const level = headingMatch[1].length;
          const heading = document.createElement(`h${level}`);
          heading.appendChild(document.createElement("br"));
          if (level === 1) {
            heading.className =
              "detail-note-heading mb-3 mt-6 text-[21px] font-semibold leading-7";
          } else if (level === 2) {
            heading.className =
              "detail-note-heading mb-2 mt-5 text-[19px] font-semibold leading-7";
          } else if (level === 3) {
            heading.className =
              "detail-note-heading mb-2 mt-4 text-[16px] font-semibold leading-7";
          }
          block.replaceWith(heading);

          const range = document.createRange();
          range.setStart(heading, 0);
          range.collapse(true);
          selection?.removeAllRanges();
          selection?.addRange(range);
        }}
        className={`prose prose-invert max-w-none select-text text-[14px] leading-7 text-[#C9C9CF] outline-none ${
          editable ? "cursor-text" : ""
        }`}
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={markdownComponents}
        >
          {renderedContent}
        </ReactMarkdown>
      </div>
    </section>
  );
}

function GeneratingState({ message }: { message: string }) {
  return (
    <div
      className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center"
      role="status"
      aria-busy="true"
    >
      <Loader2 size={22} className="animate-spin text-[#A5A7FF]" />
      <p className="max-w-sm text-[13px] leading-5 text-[#C9C9CF]">{message}</p>
    </div>
  );
}

function NotesGenerationFailedState({
  message,
  retryLabel,
  busy,
  canRetry,
  onRetry,
}: {
  message: string;
  retryLabel: string;
  busy: boolean;
  canRetry: boolean;
  onRetry: () => void;
}) {
  return (
    <div
      className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center"
      role="alert"
    >
      <AlertCircle size={22} className="text-[#FF8F8F]" />
      <p className="max-w-sm text-[13px] leading-5 text-[#C9C9CF]">{message}</p>
      {canRetry && (
        <button
          type="button"
          disabled={busy}
          onClick={onRetry}
          className="mt-1 inline-flex h-9 items-center gap-2 rounded-full bg-[#5156FF] px-4 text-[12px] font-semibold text-white transition hover:bg-[#666AFF] disabled:cursor-not-allowed disabled:opacity-60"
        >
          {busy ? (
            <Loader2 size={14} className="animate-spin" />
          ) : (
            <RefreshCw size={14} />
          )}
          {retryLabel}
        </button>
      )}
    </div>
  );
}

const isGenerationErrorNote = (note: RecordingNoteVersion) =>
  note.sections.gist?.includes("Error generating") ?? false;

function EnhancedNotesEditor({
  note,
  editable,
  canComment,
  onComment,
  onSave,
  emptyMessage,
}: {
  note: RecordingNoteVersion | null;
  editable: boolean;
  canComment?: boolean;
  onComment?: (anchor: { id: string; label: string; excerpt: string }) => void;
  onSave: (section: MeetingNoteSection, content: string) => Promise<void>;
  emptyMessage?: string;
}) {
  const { t } = useTranslation();

  if (!note) {
    return (
      <div className="px-4 py-8 text-[13px] text-[#777780]">
        {emptyMessage ??
          t("desktopRecordingDetail.empty.aiNotesUnavailable")}
      </div>
    );
  }

  // Section headings follow the note language (not the app UI language),
  // so Japanese notes show 要旨/概要 even when the UI is English.
  const sectionLabel = (key: string) =>
    tForLocale(note.language, `desktopRecordingDetail.enhancedSections.${key}`);

  const sections: Array<[MeetingNoteSection, string, string | null]> = [
    ["gist", sectionLabel("gist"), note.sections.gist],
    ["summary", sectionLabel("summary"), note.sections.summary],
    ["keyPoints", sectionLabel("keyPoints"), note.sections.keyPoints],
    ["actionItems", sectionLabel("actionItems"), note.sections.actionItems],
    ["otherPoints", sectionLabel("otherPoints"), note.sections.otherPoints],
  ];

  return (
    <div className="px-4 pb-4 pt-2">
      {sections
        .filter(([, , content]) => Boolean(content?.trim()))
        .map(([section, label, content]) => (
          <StructuredSection
            key={section}
            heading={label}
            anchorId={`note:${note.timestamp}:${section}`}
            content={content!}
            editable={editable}
            canComment={Boolean(canComment && onComment)}
            onComment={() =>
              onComment?.({
                id: `note:${note.timestamp}:${section}`,
                label,
                excerpt: content!.trim().slice(0, 220),
              })
            }
            onSave={(next) => onSave(section, next)}
          />
        ))}
    </div>
  );
}

// Comment anchors are either a whole section/line ("note:123:gist") or a text
// selection inside one ("note:123:gist#40-88"). Everything that counts or
// highlights per section therefore matches on the base id.
const _commentAnchorBase = (anchorId: string) => anchorId.split("#")[0];

/** Nearest ancestor that actually scrolls, so we can centre inside it. */
function scrollableAncestor(element: HTMLElement | null): HTMLElement | null {
  let current: HTMLElement | null = element;
  while (current) {
    const overflowY = window.getComputedStyle(current).overflowY;
    if (
      (overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay") &&
      current.scrollHeight > current.clientHeight
    ) {
      return current;
    }
    current = current.parentElement;
  }
  return null;
}

/**
 * Centres the commented passage in the reading column. Centring the element
 * that contains the range is not the same thing: a transcript line or note
 * section can be many lines tall, which leaves the highlighted words parked at
 * an edge. This measures the range itself.
 */
function scrollRangeIntoView(range: Range | null) {
  if (!range) return;
  const target =
    range.startContainer instanceof HTMLElement
      ? range.startContainer
      : range.startContainer.parentElement;
  const container = scrollableAncestor(target);
  const rect = range.getBoundingClientRect();
  if (!container || (rect.width === 0 && rect.height === 0)) {
    target?.scrollIntoView({ block: "center", behavior: "smooth" });
    return;
  }
  const view = container.getBoundingClientRect();
  const delta =
    rect.top + rect.height / 2 - (view.top + view.height / 2);
  if (Math.abs(delta) < 2) return;
  container.scrollBy({ top: delta, behavior: "smooth" });
}

/**
 * Runs after the browser has laid out the frame the caller just triggered —
 * opening the comments rail narrows the reading column and reflows the text,
 * so measuring in the same tick scrolls to a position that no longer exists.
 */
function scrollRangeIntoViewAfterLayout(resolve: () => Range | null) {
  let inner = 0;
  const outer = window.requestAnimationFrame(() => {
    inner = window.requestAnimationFrame(() => scrollRangeIntoView(resolve()));
  });
  return () => {
    window.cancelAnimationFrame(outer);
    if (inner) window.cancelAnimationFrame(inner);
  };
}

/** The commentable block a range belongs to — start wins if the drag overruns it. */
function commentBaseFromRange(range: Range): HTMLElement | null {
  const startNode = range.startContainer;
  const startElement =
    startNode instanceof HTMLElement ? startNode : startNode.parentElement;
  const startBase = startElement?.closest<HTMLElement>("[data-comment-base]");
  if (startBase) return startBase;
  const ancestor = range.commonAncestorContainer;
  const ancestorElement =
    ancestor instanceof HTMLElement ? ancestor : ancestor.parentElement;
  return ancestorElement?.closest<HTMLElement>("[data-comment-base]") ?? null;
}

/** Clamp a live selection to the text that actually lives inside one comment base. */
function rangeClampedToCommentBase(range: Range, base: HTMLElement): Range {
  const clamped = range.cloneRange();
  if (!base.contains(clamped.startContainer)) {
    clamped.setStart(base, 0);
  }
  if (!base.contains(clamped.endContainer)) {
    clamped.setEnd(base, base.childNodes.length);
  }
  return clamped;
}

function rangeForCommentBase(baseId: string): Range | null {
  const element = document.querySelector<HTMLElement>(
    `[data-comment-base="${CSS.escape(baseId)}"]`,
  );
  if (!element) return null;
  const range = document.createRange();
  range.selectNodeContents(element);
  return range;
}

/** Character offset of a range's start within an element's text content. */
function textOffsetWithin(base: HTMLElement, range: Range) {
  const measure = document.createRange();
  measure.selectNodeContents(base);
  measure.setEnd(range.startContainer, range.startOffset);
  return measure.toString().length;
}

/**
 * Rebuilds the DOM range for a stored selection anchor so a comment written in
 * an earlier session still highlights the right words after a reload.
 */
function rangeForCommentAnchor(anchorId: string): Range | null {
  const [base, offsets] = anchorId.split("#");
  if (!offsets) return null;
  const [start, end] = offsets.split("-").map(Number);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return null;
  const element = document.querySelector<HTMLElement>(
    `[data-comment-base="${CSS.escape(base)}"]`,
  );
  if (!element) return null;

  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let consumed = 0;
  let started = false;
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const length = node.data.length;
    if (!started && consumed + length >= start) {
      range.setStart(node, start - consumed);
      started = true;
    }
    if (started && consumed + length >= end) {
      range.setEnd(node, end - consumed);
      return range;
    }
    consumed += length;
  }
  return null;
}

type RecordingCommentThread = {
  anchor: RecordingCommentAnchor;
  comments: RecordingComment[];
  /** A selection that has been made but not yet commented on. */
  pending?: boolean;
};

/**
 * Comments live in a rail beside the text, like the translated-file detail.
 * Selecting a thread highlights its passage; clicking a highlight selects the
 * thread — the two stay in sync in both directions.
 */
function RecordingCommentsRail({
  threads,
  loading,
  activeAnchorId,
  canComment,
  onSelectThread,
  onCancelDraft,
  onClose,
  onAdd,
  onToggleResolved,
  onDelete,
}: {
  threads: RecordingCommentThread[];
  loading: boolean;
  activeAnchorId: string | null;
  canComment: boolean;
  onSelectThread: (anchor: RecordingCommentAnchor) => void;
  /** Drops an uncommented draft thread and its highlight. */
  onCancelDraft: () => void;
  onClose: () => void;
  onAdd: (body: string) => Promise<void>;
  onToggleResolved: (commentId: string) => Promise<void>;
  onDelete: (commentId: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [showResolved, setShowResolved] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const cardRefs = useRef(new Map<string, HTMLElement>());

  // One line to start, growing with the text like a chat input.
  const autoGrow = (element: HTMLTextAreaElement) => {
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 160)}px`;
  };

  const openCount = threads.reduce(
    (total, thread) => total + thread.comments.filter((comment) => !comment.resolved).length,
    0,
  );
  const resolvedCount = threads.reduce(
    (total, thread) => total + thread.comments.filter((comment) => comment.resolved).length,
    0,
  );
  const visibleThreads = showResolved
    ? threads
    : threads
        .map((thread) => ({
          ...thread,
          comments: thread.comments.filter((comment) => !comment.resolved),
        }))
        .filter((thread) => thread.pending || thread.comments.length > 0);

  useEffect(() => {
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "auto";
    if (canComment && activeAnchorId) {
      window.setTimeout(() => composerRef.current?.focus(), 0);
    }
  }, [activeAnchorId, canComment]);

  // Follow the selection made in the text.
  useEffect(() => {
    if (!activeAnchorId) return;
    cardRefs.current
      .get(activeAnchorId)
      ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeAnchorId]);

  const submit = async () => {
    const body = draft.trim();
    if (!body || submitting) return;
    setSubmitting(true);
    try {
      await onAdd(body);
      setDraft("");
      composerRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <aside className="recording-comments-rail flex h-full w-[340px] shrink-0 flex-col border-l border-[#2A2A2F] bg-[#0E0E10]">
      <header className="flex shrink-0 items-center gap-3 border-b border-[#2A2A2F] px-4 py-3.5">
        <div className="recording-comment-rail-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[#292955] text-[#D6DCFF]">
          <MessageCircle size={16} />
        </div>
        <div className="min-w-0 flex-1">
          <h2 className="recording-comment-title text-[14px] font-semibold text-[#F1F1F3]">
            {t("desktopRecordingDetail.comments.title")}
          </h2>
          <p className="recording-comment-time mt-0.5 truncate text-[11px] text-[#85858E]">
            {openCount} open
            {resolvedCount > 0 ? ` · ${resolvedCount} resolved` : ""}
          </p>
        </div>
        {resolvedCount > 0 && (
          <button
            type="button"
            onClick={() => setShowResolved((value) => !value)}
            className="rounded-md px-2 py-1 text-[11px] text-[#777780] transition hover:bg-[#202024] hover:text-[#C9C9CF]"
          >
            {showResolved
              ? t("desktopRecordingDetail.comments.hideResolved")
              : t("desktopRecordingDetail.comments.showResolved")}
          </button>
        )}
        <button
          type="button"
          onClick={onClose}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[#85858E] transition hover:bg-[#202024] hover:text-white"
          aria-label={t("desktopRecordingDetail.comments.close")}
        >
          <X size={15} />
        </button>
      </header>

      {/* The floating Ask MeetingCopilot / player stack overlays the window; the
          parent sets --recording-detail-bottom-inset from its measured height. */}
      <div
        className="min-h-0 flex-1 overflow-y-auto px-3 pt-3"
        style={{
          paddingBottom: "var(--recording-detail-bottom-inset, 8rem)",
        }}
      >
        {loading && threads.length === 0 ? (
          <div className="flex items-center justify-center gap-2 py-12 text-[12px] text-[#777780]">
            <Loader2 size={14} className="animate-spin" />
            {t("desktopRecordingDetail.comments.loading")}
          </div>
        ) : visibleThreads.length === 0 ? (
          <div className="flex flex-col items-center px-5 py-12 text-center">
            <MessageSquarePlus size={21} className="text-[#62626C]" />
            <p className="mt-3 text-[12px] font-medium text-[#C9C9CF]">
              {t("desktopRecordingDetail.comments.startConversation")}
            </p>
            <p className="mt-1 text-[11px] leading-5 text-[#777780]">
              {t("desktopRecordingDetail.comments.emptyHint")}
            </p>
          </div>
        ) : (
          <div className="space-y-2.5">
            {visibleThreads.map((thread) => {
              const active = thread.anchor.id === activeAnchorId;
              return (
                <article
                  key={thread.anchor.id}
                  ref={(element) => {
                    if (element) cardRefs.current.set(thread.anchor.id, element);
                    else cardRefs.current.delete(thread.anchor.id);
                  }}
                  onClick={() => onSelectThread(thread.anchor)}
                  className={`recording-comment-card cursor-pointer rounded-xl border px-3 py-3 transition ${
                    active
                      ? "recording-comment-card--active border-[#5156FF] bg-[#191932]"
                      : "border-[#2A2A2F] bg-[#151517] hover:border-[#3A3A42]"
                  }`}
                >
                  <div className="recording-comment-eyebrow mb-1.5 flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-[#777780]">
                    {thread.anchor.kind === "transcript" ? (
                      <AudioLines size={11} />
                    ) : (
                      <Sparkles size={11} />
                    )}
                    <span className="min-w-0 flex-1 truncate">{thread.anchor.label}</span>
                    {/* A draft thread exists only to hold the highlight, so
                        dismissing the card drops the highlight with it. */}
                    {thread.pending && (
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          onCancelDraft();
                        }}
                        className="recording-comment-dismiss -mr-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[#686871] transition hover:bg-[#202024] hover:text-[#D7D7DB]"
                        title={t("common.cancel")}
                        aria-label={t("common.cancel")}
                      >
                        <X size={13} />
                      </button>
                    )}
                  </div>
                  <p className="recording-comment-quote line-clamp-2 border-l-2 border-[#5156FF]/60 pl-2 text-[12px] leading-5 text-[#B5B5BC]">
                    {thread.anchor.excerpt}
                  </p>

                  {thread.comments.length > 0 && (
                    <div className="mt-2.5 space-y-2.5">
                      {thread.comments.map((comment) => (
                        <div
                          key={comment.id}
                          className={comment.resolved ? "opacity-60" : ""}
                        >
                          <div className="flex items-center gap-2">
                            <div className="recording-comment-avatar flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#34343B] text-[10px] font-semibold text-[#E0E0E4]">
                              {comment.author.name.trim().charAt(0).toUpperCase() || "Y"}
                            </div>
                            <div className="min-w-0 flex-1">
                              <p className="recording-comment-author truncate text-[11px] font-semibold text-[#D7D7DB]">
                                {comment.author.name}
                              </p>
                              <p className="recording-comment-time text-[10px] text-[#777780]">
                                {new Intl.DateTimeFormat(undefined, {
                                  month: "short",
                                  day: "numeric",
                                  hour: "numeric",
                                  minute: "2-digit",
                                }).format(new Date(comment.createdAt))}
                              </p>
                            </div>
                            {/* Resolve and delete live here as icons, so the card
                                is just author, time, and the comment itself. */}
                            {canComment && (
                              <button
                                type="button"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void onToggleResolved(comment.id);
                                }}
                                className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md transition hover:bg-[#202024] ${
                                  comment.resolved
                                    ? "text-[#7BC98F] hover:text-[#9BD3AB]"
                                    : "text-[#686871] hover:text-[#D7D7DB]"
                                }`}
                                title={t(
                                  comment.resolved
                                    ? "desktopRecordingDetail.comments.reopen"
                                    : "desktopRecordingDetail.comments.resolve",
                                )}
                                aria-label={t(
                                  comment.resolved
                                    ? "desktopRecordingDetail.comments.reopen"
                                    : "desktopRecordingDetail.comments.resolve",
                                )}
                                aria-pressed={comment.resolved}
                              >
                                <Check size={14} strokeWidth={2.4} />
                              </button>
                            )}
                            {comment.canDelete && (
                              <button
                                type="button"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void onDelete(comment.id);
                                }}
                                className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-[#686871] transition hover:bg-[#2A1C1C] hover:text-[#D58B8B]"
                                title={t("desktopRecordingDetail.comments.delete")}
                                aria-label={t("desktopRecordingDetail.comments.delete")}
                              >
                                <Trash2 size={13} />
                              </button>
                            )}
                          </div>
                          <p className="recording-comment-body mt-1.5 whitespace-pre-wrap break-words text-[12px] leading-5 text-[#C9C9CF]">
                            {comment.body}
                          </p>
                        </div>
                      ))}
                    </div>
                  )}

                  {active && canComment && (
                    <div
                      className="recording-comment-composer mt-2.5"
                      onClick={(event) => event.stopPropagation()}
                    >
                      <textarea
                        ref={composerRef}
                        value={draft}
                        rows={1}
                        onChange={(event) => {
                          setDraft(event.target.value);
                          autoGrow(event.target);
                        }}
                        onKeyDown={(event) => {
                          if (event.key === "Escape") {
                            event.preventDefault();
                            onCancelDraft();
                            return;
                          }
                          if (event.key === "Enter" && !event.shiftKey) {
                            event.preventDefault();
                            void submit();
                          }
                        }}
                        placeholder={t("desktopRecordingDetail.comments.placeholder")}
                        className="recording-comment-input w-full resize-none overflow-hidden rounded-2xl border border-[#38383F] bg-[#111113] px-3.5 py-2 text-[12px] leading-5 text-[#F1F1F3] outline-none transition placeholder:text-[#62626C] focus:border-[#5156FF]"
                      />
                      <div className="mt-1.5 flex items-center justify-end gap-0.5">
                        <button
                          type="button"
                          onClick={onCancelDraft}
                          className="rounded-full px-2.5 py-1.5 text-[11px] font-semibold text-[#85858E] transition hover:bg-[#202024] hover:text-[#D7D7DB]"
                        >
                          {t("common.cancel")}
                        </button>
                        <button
                          type="button"
                          disabled={!draft.trim() || submitting}
                          onClick={() => void submit()}
                          className="recording-comment-submit flex h-7 items-center gap-1.5 rounded-full bg-[#5156FF] px-3.5 text-[11px] font-semibold text-white transition hover:bg-[#6266FF] disabled:cursor-not-allowed disabled:opacity-40"
                        >
                          {submitting && <Loader2 size={11} className="animate-spin" />}
                          {t("desktopRecordingDetail.comments.submit")}
                        </button>
                      </div>
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </div>

      {!canComment && (
        <p className="shrink-0 border-t border-[#2A2A2F] px-4 py-3 text-[10px] text-[#777780]">
          {t("desktopRecordingDetail.comments.viewOnly")}
        </p>
      )}
    </aside>
  );
}

const TRANSCRIPT_VIEW_STORAGE_KEY = "MeetingCopilot_transcript_view";
const PLAYER_OPEN_STORAGE_KEY = "MeetingCopilot_player_open";
const PER_MEETING_PREFERENCE_LIMIT = 50;

/**
 * Per-meeting view preferences. They are per meeting rather than global because
 * recordings differ: some are translated and some are not, some are worth
 * listening to and some are read-only. One global flag would be wrong half the
 * time. Only the 50 most recent meetings are kept.
 */
function readMeetingPreference(
  storageKey: string,
  meetingId: string,
): boolean | null {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return null;
    const stored = JSON.parse(raw) as Record<string, boolean>;
    return typeof stored[meetingId] === "boolean" ? stored[meetingId] : null;
  } catch {
    return null;
  }
}

function writeMeetingPreference(
  storageKey: string,
  meetingId: string,
  value: boolean,
) {
  try {
    const raw = localStorage.getItem(storageKey);
    const stored = raw ? (JSON.parse(raw) as Record<string, boolean>) : {};
    // Re-insert so this meeting counts as the most recently used.
    delete stored[meetingId];
    stored[meetingId] = value;
    const keys = Object.keys(stored);
    const trimmed: Record<string, boolean> = {};
    for (const key of keys.slice(
      Math.max(0, keys.length - PER_MEETING_PREFERENCE_LIMIT),
    )) {
      trimmed[key] = stored[key];
    }
    localStorage.setItem(storageKey, JSON.stringify(trimmed));
  } catch {
    // Remembering a view is best-effort; never block the meeting on it.
  }
}

const readTranscriptView = (meetingId: string) =>
  readMeetingPreference(TRANSCRIPT_VIEW_STORAGE_KEY, meetingId);
const writeTranscriptView = (meetingId: string, bilingual: boolean) =>
  writeMeetingPreference(TRANSCRIPT_VIEW_STORAGE_KEY, meetingId, bilingual);

const readPlayerOpen = (meetingId: string) =>
  readMeetingPreference(PLAYER_OPEN_STORAGE_KEY, meetingId);
const writePlayerOpen = (meetingId: string, open: boolean) =>
  writeMeetingPreference(PLAYER_OPEN_STORAGE_KEY, meetingId, open);

const AWAIT_READY_TIMEOUT_MS = 2 * 60 * 1000;
const AWAIT_READY_INTERVAL_MS = 1500;

function RecordingDetailSkeleton({ title }: { title?: string }) {
  return (
    <div
      className="h-full bg-[#0B0B0D] text-[#C9C9CF]"
      role="status"
      aria-busy="true"
    >
      <div className="mx-auto w-full max-w-[1120px] animate-pulse px-10 pb-32 pt-4">
        <div className="flex items-center justify-between gap-6">
          {title?.trim() ? (
            <h1 className="min-w-0 font-quicksand text-[24px] font-medium leading-snug tracking-[-0.01em] text-[#F1F1F3]">
              {title}
            </h1>
          ) : (
            <div className="h-7 w-[42%] rounded-lg bg-[#26262B]" />
          )}
          <div className="h-8 w-20 shrink-0 rounded-lg bg-[#202024]" />
        </div>

        <div className="mt-3.5 flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-1 rounded-full border border-[#2A2A2F] bg-[#1A1A1D] p-0.5">
            <div className="h-7 w-20 rounded-full bg-[#26262B]" />
            <div className="h-7 w-24 rounded-full bg-[#202024]" />
            <div className="h-7 w-24 rounded-full bg-[#202024]" />
          </div>
          <div className="ml-auto h-7 w-16 rounded-full bg-[#202024]" />
        </div>

        <div className="mt-6 space-y-4 rounded-2xl border border-[#2A2A2F] bg-[#151517] p-5">
          <div className="h-3.5 w-28 rounded bg-[#26262B]" />
          <div className="space-y-2">
            <div className="h-3 w-full rounded bg-[#202024]" />
            <div className="h-3 w-[92%] rounded bg-[#202024]" />
            <div className="h-3 w-[78%] rounded bg-[#202024]" />
          </div>
          <div className="h-3.5 w-24 rounded bg-[#26262B]" />
          <div className="space-y-2">
            <div className="h-3 w-full rounded bg-[#202024]" />
            <div className="h-3 w-[88%] rounded bg-[#202024]" />
            <div className="h-3 w-[64%] rounded bg-[#202024]" />
          </div>
          <div className="h-3.5 w-32 rounded bg-[#26262B]" />
          <div className="space-y-2">
            <div className="h-3 w-[70%] rounded bg-[#202024]" />
            <div className="h-3 w-[55%] rounded bg-[#202024]" />
          </div>
        </div>
      </div>
    </div>
  );
}

export function DesktopRecordingDetail({
  meetingId,
  title,
  awaitReady = false,
  initialTab = "enhanced",
  onOpenAssistant,
  onOpenPricing,
}: DesktopRecordingDetailProps) {
  const { t, currentLanguage } = useTranslation();
  const locale = useMemo(() => toBcp47Locale(currentLanguage), [currentLanguage]);
  const tRef = useRef(t);
  tRef.current = t;
  const askPrompts: AskPromptOption[] = useMemo(
    () => [
      { id: "listQa", label: t("desktopRecordingDetail.ask.prompts.listQa") },
      {
        id: "followUpEmail",
        label: t("desktopRecordingDetail.ask.prompts.followUpEmail"),
      },
      {
        id: "afterMeeting",
        label: t("desktopRecordingDetail.ask.prompts.afterMeeting"),
      },
    ],
    [t],
  );
  const [detail, setDetail] = useState<RecordingDetail | null>(null);
  const tabs: TabOption[] = useMemo(
    () => {
      const allTabs: TabOption[] = [
        { id: "my-notes", label: t("desktopRecordingDetail.tabs.myNotes") },
        {
          id: "enhanced",
          label: t("desktopRecordingDetail.tabs.enhanced"),
          icon: Sparkles,
        },
        { id: "transcript", label: t("desktopRecordingDetail.tabs.transcript") },
      ];
      if (detail?.access === "shared") {
        return allTabs.filter((tab) => tab.id !== "my-notes");
      }
      return allTabs;
    },
    [detail?.access, t],
  );
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [activeTab, setActiveTab] =
    useState<RecordingDetailTab>(initialTab);
  const [copied, setCopied] = useState<"notes" | "transcript" | null>(null);
  const [copyMenuOpen, setCopyMenuOpen] = useState(false);
  const [copyHighlight, setCopyHighlight] =
    useState<TranscriptCopyMode | null>(null);
  const copyMenuRef = useRef<HTMLDivElement>(null);
  const transcriptListRef = useRef<HTMLDivElement>(null);
  // Anchor whose passage should be scrolled to once its tab has rendered.
  const pendingScrollAnchorRef = useRef<string | null>(null);
  /** Notes/transcript body — watched so comment highlights survive re-renders. */
  const detailBodyRef = useRef<HTMLDivElement>(null);
  const floatStackRef = useRef<HTMLDivElement>(null);
  const [bottomInset, setBottomInset] = useState(
    RECORDING_DETAIL_BOTTOM_INSET_FALLBACK,
  );
  const transcriptSearchInputRef = useRef<HTMLInputElement>(null);
  const submittedTranscriptSearchRef = useRef("");
  const [transcriptSearch, setTranscriptSearch] = useState("");
  const [activeTranscriptMatch, setActiveTranscriptMatch] = useState(0);
  const [askInput, setAskInput] = useState("");
  const [assistantLaunch, setAssistantLaunch] = useState<
    (DesktopMeetingAssistantLaunch & { nonce: number }) | null
  >(null);
  // Restored per meeting below; false is only the pre-hydration default.
  const [playerOpen, setPlayerOpen] = useState(false);
  /**
   * Opens the player once a recording turns out to exist — unless this meeting
   * has a remembered choice, which wins. Without this an explicitly collapsed
   * player springs back open on the next load.
   */
  const applyRememberedPlayerState = (mediaAvailable: boolean) => {
    const remembered = readPlayerOpen(meetingId);
    setPlayerOpen((current) => (mediaAvailable ? remembered ?? true : current));
  };
  const [playing, setPlaying] = useState(false);
  const [myNotesContent, setMyNotesContent] = useState("");
  const notepadFormat = resolveNotepadFormat(detail?.notepadHtml ?? "", detail?.notepadFormat);
  const lastSavedNotepad = useRef("");
  const latestNotepad = useRef("");
  const canSaveNotepad = useRef(false);
  const [rows, setRows] = useState<RecordingTranscriptSection[]>([]);
  const [comments, setComments] = useState<RecordingComment[]>([]);
  const [commentAnchor, setCommentAnchor] = useState<RecordingCommentAnchor | null>(null);
  const [commentsRailOpen, setCommentsRailOpen] = useState(false);
  const [commentsLoading, setCommentsLoading] = useState(false);
  const [commentAnchorRange, setCommentAnchorRange] = useState<Range | null>(null);
  // Pending text selection: the "Comment" bubble that follows a highlight.
  const [selectionDraft, setSelectionDraft] = useState<{
    anchor: RecordingCommentAnchor;
    range: Range;
    left: number;
    top: number;
  } | null>(null);
  const [transcriptLoading, setTranscriptLoading] = useState(false);
  const [metaOpen, setMetaOpen] = useState(false);
  const metaRef = useRef<HTMLDivElement>(null);
  const [enhancedMenuOpen, setEnhancedMenuOpen] = useState(false);
  const enhancedMenuRef = useRef<HTMLDivElement>(null);
  const [showLanguages, setShowLanguages] = useState(false);
  const [transcriptMenuOpen, setTranscriptMenuOpen] = useState(false);
  const transcriptMenuRef = useRef<HTMLDivElement>(null);
  const [showTranscriptLanguages, setShowTranscriptLanguages] = useState(false);
  const [transcriptCleanupOpen, setTranscriptCleanupOpen] = useState(false);
  const [bilingualView, setBilingualView] = useState(false);
  const storedTranscriptViewRef = useRef<boolean | null>(null);
  const chooseTranscriptView = useCallback(
    (bilingual: boolean) => {
      setBilingualView(bilingual);
      storedTranscriptViewRef.current = bilingual;
      writeTranscriptView(meetingId, bilingual);
    },
    [meetingId],
  );
  const hadTranscriptTranslations = useRef(false);
  const [selectedNoteTimestamp, setSelectedNoteTimestamp] = useState<
    string | null
  >(null);
  const [actionLoading, setActionLoading] = useState<
    "regenerate" | "retry-notes" | "translate" | "translate-transcript" | "restore-transcript" | "share" | "unblur" | null
  >(null);
  useEffect(() => {
    if (!statusMessage || actionLoading) return;
    const timer = window.setTimeout(() => {
      setStatusMessage((current) => current === statusMessage ? null : current);
    }, 20_000);
    return () => window.clearTimeout(timer);
  }, [actionLoading, statusMessage]);
  const [shareOpen, setShareOpen] = useState(false);
  const [shareMode, setShareMode] = useState<"people" | "chat" | "link">("people");
  const [shareInput, setShareInput] = useState("");
  const [shareSuggestionsOpen, setShareSuggestionsOpen] = useState(false);
  const [sharePermission, setSharePermission] =
    useState<RecordingSharePermission>("view");
  const [createShareChat, setCreateShareChat] = useState(true);
  const [shareChats, setShareChats] = useState<ApiChat[]>([]);
  const [shareChatsLoaded, setShareChatsLoaded] = useState(false);
  const [shareChatsLoading, setShareChatsLoading] = useState(false);
  const [shareChatsRequest, setShareChatsRequest] = useState(0);
  const [shareChatQuery, setShareChatQuery] = useState("");
  const [selectedShareChatId, setSelectedShareChatId] = useState<string | null>(null);
  const [shareChatSending, setShareChatSending] = useState(false);
  const [shareChatPermission, setShareChatPermission] =
    useState<RecordingSharePermission>("view");
  const [newShareChatOpen, setNewShareChatOpen] = useState(false);
  const [newShareChatTitle, setNewShareChatTitle] = useState("");
  const [newShareChatEmails, setNewShareChatEmails] = useState("");
  const [creatingShareChat, setCreatingShareChat] = useState(false);
  const [shareChatError, setShareChatError] = useState<string | null>(null);
  const [shareLinkLoading, setShareLinkLoading] = useState(false);
  const [shareLinkCopied, setShareLinkCopied] = useState(false);
  const [waveSurfer, setWaveSurfer] = useState<WaveSurfer | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioPart, setAudioPart] = useState(0);
  const [audioPlaylist, setAudioPlaylist] = useState<string[]>([]);
  const [audioPlaylistIndex, setAudioPlaylistIndex] = useState(0);
  const [audioTime, setAudioTime] = useState(0);
  const [audioDuration, setAudioDuration] = useState(0);
  const [waveformReady, setWaveformReady] = useState(false);
  const [waveformError, setWaveformError] = useState(false);
  const pendingSeekRef = useRef<number | null>(null);

  useEffect(() => {
    if (!shareOpen || shareMode !== "chat" || shareChatsLoaded) {
      return;
    }
    let cancelled = false;
    setShareChatsLoading(true);
    setShareChatError(null);
    void listChats("all")
      .then((nextChats) => {
        if (cancelled) return;
        setShareChats(nextChats.filter((chat) => !chat.isArchived));
        setShareChatsLoaded(true);
      })
      .catch((error) => {
        if (cancelled) return;
        setShareChatError(errorMessage(error, t("desktopChat.shell.couldNotLoadChats")));
      })
      .finally(() => {
        if (!cancelled) setShareChatsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [shareChatsLoaded, shareChatsRequest, shareMode, shareOpen]);

  const loadComments = useCallback(async () => {
    setCommentsLoading(true);
    try {
      const next = await fetchRecordingComments(meetingId);
      setComments(next);
    } catch (error) {
      setComments([]);
      setStatusMessage(errorMessage(error, t("desktopUi.recording.commentLoadFailed")));
    } finally {
      setCommentsLoading(false);
    }
  }, [meetingId]);

  useEffect(() => {
    setComments([]);
    setCommentAnchor(null);
    setCommentsRailOpen(false);
    setCommentAnchorRange(null);
    setSelectionDraft(null);
  }, [meetingId]);

  useEffect(() => {
    if (!RECORDING_COMMENTS_ENABLED || !detail) return;
    void loadComments();
  }, [detail?.id, loadComments]);

  const loadDetail = useCallback(
    async (silent = false): Promise<boolean> => {
      if (!silent) setLoading(true);
      try {
        const next = await fetchRecordingDetail(meetingId);
        setDetail(next);
        setLoadError(null);
        setMyNotesContent((current) => {
          if (silent && current !== lastSavedNotepad.current) return current;
          lastSavedNotepad.current = next.notepadHtml;
          return next.notepadHtml;
        });
        setSelectedNoteTimestamp((current) =>
          current && next.notes.some((note) => note.timestamp === current)
            ? current
            : next.notes[0]?.timestamp ?? null,
        );
        applyRememberedPlayerState(next.media.available);
        if (!silent) setLoading(false);
        return true;
      } catch (error) {
        if (error instanceof RecordingRequestError && error.status === 404) {
          // The meeting no longer exists on the server (e.g. an empty session
          // was discarded after stop). Clear the stale detail so the polling
          // loop stops and the "not available" screen replaces the eternal
          // "generating notes" spinner.
          setDetail(null);
          setLoadError(
            tRef.current("desktopRecordingDetail.status.meetingRemoved"),
          );
          if (!silent) setLoading(false);
          return false;
        }
        setLoadError(
          errorMessage(
            error,
            tRef.current("desktopRecordingDetail.status.couldNotLoadMeeting"),
          ),
        );
        if (!silent) setLoading(false);
        return false;
      }
    },
    [meetingId],
  );

  const loadTranscript = useCallback(
    async (silent = false) => {
      if (!silent) setTranscriptLoading(true);
      try {
        const result = await fetchRecordingTranscript(meetingId, rows);
        if (!result.unchanged) setRows(result.data);
      } catch (error) {
        setStatusMessage(
          errorMessage(
            error,
            tRef.current("desktopRecordingDetail.status.couldNotLoadTranscript"),
          ),
        );
      } finally {
        if (!silent) setTranscriptLoading(false);
      }
    },
    [meetingId, rows],
  );

  useEffect(() => {
    setActiveTab(initialTab);
    setDetail(null);
    setRows([]);
    const remembered = readTranscriptView(meetingId);
    storedTranscriptViewRef.current = remembered;
    setBilingualView(remembered ?? false);
    setPlayerOpen(readPlayerOpen(meetingId) ?? false);
    hadTranscriptTranslations.current = false;
    setTranscriptMenuOpen(false);
    setShowTranscriptLanguages(false);
    setSelectedNoteTimestamp(null);
    setAudioUrl(null);
    setAudioPart(0);
    setAudioPlaylist([]);
    setAudioPlaylistIndex(0);
    setLoadError(null);
    setLoading(true);

    if (!awaitReady) {
      void loadDetail();
      return;
    }

    let cancelled = false;
    const startedAt = Date.now();

    const wait = (ms: number) =>
      new Promise<void>((resolve) => {
        window.setTimeout(resolve, ms);
      });

    const pollUntilReady = async () => {
      while (!cancelled && Date.now() - startedAt < AWAIT_READY_TIMEOUT_MS) {
        try {
          const next = await fetchRecordingDetail(meetingId);
          if (cancelled) return;
          setDetail(next);
          setLoadError(null);
          lastSavedNotepad.current = next.notepadHtml;
          setMyNotesContent(next.notepadHtml);
          setSelectedNoteTimestamp(next.notes[0]?.timestamp ?? null);
          applyRememberedPlayerState(next.media.available);
          setLoading(false);
          return;
        } catch {
          await wait(AWAIT_READY_INTERVAL_MS);
        }
      }

      if (cancelled) return;
      setLoading(false);
      setLoadError(
        tRef.current("desktopRecordingDetail.status.couldNotLoadMeeting"),
      );
    };

    void pollUntilReady();
    return () => {
      cancelled = true;
    };
  }, [meetingId, awaitReady, initialTab, loadDetail]);

  const notesGenerationIsActive =
    detail?.notesGeneration?.status === "queued" ||
    detail?.notesGeneration?.status === "generating" ||
    detail?.notesGeneration?.status === "retrying";
  const notesGenerationIsTerminal =
    detail?.notesGeneration?.status === "failed" ||
    detail?.notesGeneration?.status === "cancelled";
  const detailNeedsRefresh = Boolean(
    detail && (
      notesGenerationIsActive ||
      (!notesGenerationIsTerminal && detail.status !== "ready")
    ),
  );
  useEffect(() => {
    if (!detailNeedsRefresh) return;
    let cancelled = false;
    let timer: number | undefined;

    // Schedule the next request only after the current one completes. This
    // prevents overlapping requests and keeps polling stable while each fresh
    // response replaces the detail object.
    const poll = async () => {
      await loadDetail(true);
      if (!cancelled) timer = window.setTimeout(poll, 3000);
    };

    timer = window.setTimeout(poll, AWAIT_READY_INTERVAL_MS);
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [detailNeedsRefresh, loadDetail]);

  useEffect(() => {
    const refreshOnFocus = () => {
      void loadDetail(true);
      if (activeTab === "transcript") void loadTranscript(true);
    };
    window.addEventListener("focus", refreshOnFocus);
    return () => window.removeEventListener("focus", refreshOnFocus);
  }, [activeTab, loadDetail, loadTranscript]);

  useEffect(() => {
    if (activeTab !== "transcript") return;
    void loadTranscript();
  }, [activeTab, meetingId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (
      activeTab !== "transcript" ||
      !detail ||
      detail.status === "ready"
    ) {
      return;
    }
    const timer = window.setInterval(() => void loadTranscript(true), 3000);
    return () => window.clearInterval(timer);
  }, [activeTab, detail, loadTranscript]);

  useEffect(() => {
    if (detail?.access === "shared" && activeTab === "my-notes") {
      setActiveTab("enhanced");
    }
  }, [detail?.access, activeTab]);

  useEffect(() => {
    latestNotepad.current = myNotesContent;
    canSaveNotepad.current = Boolean(detail?.permissions.editNotepad);
    if (
      !detail?.permissions.editNotepad ||
      myNotesContent === lastSavedNotepad.current
    ) {
      return;
    }
    const timer = window.setTimeout(async () => {
      try {
        const result = await updateRecordingNotepad(
          meetingId,
          myNotesContent,
        );
        lastSavedNotepad.current = result.notepadHtml;
        setStatusMessage(t("desktopRecordingDetail.status.notesSaved"));
      } catch (error) {
        setStatusMessage(
          errorMessage(
            error,
            t("desktopRecordingDetail.status.couldNotSaveNotes"),
          ),
        );
      }
    }, 750);
    return () => window.clearTimeout(timer);
  }, [detail?.permissions.editNotepad, meetingId, myNotesContent, t]);

  useEffect(
    () => () => {
      if (
        canSaveNotepad.current &&
        latestNotepad.current !== lastSavedNotepad.current
      ) {
        void updateRecordingNotepad(meetingId, latestNotepad.current);
      }
    },
    [meetingId],
  );

  useEffect(() => {
    if (!copyMenuOpen) return;
    const close = (event: MouseEvent) => {
      if (!copyMenuRef.current?.contains(event.target as Node)) {
        setCopyMenuOpen(false);
        setCopyHighlight(null);
      }
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [copyMenuOpen]);

  useEffect(() => {
    if (!metaOpen) return;
    const close = (event: MouseEvent) => {
      if (!metaRef.current?.contains(event.target as Node)) setMetaOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [metaOpen]);

  useEffect(() => {
    if (!enhancedMenuOpen) return;
    const close = (event: MouseEvent) => {
      if (!enhancedMenuRef.current?.contains(event.target as Node)) {
        setEnhancedMenuOpen(false);
        setShowLanguages(false);
      }
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [enhancedMenuOpen]);

  useEffect(() => {
    if (!transcriptMenuOpen) return;
    const close = (event: MouseEvent) => {
      if (!transcriptMenuRef.current?.contains(event.target as Node)) {
        setTranscriptMenuOpen(false);
        setShowTranscriptLanguages(false);
      }
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [transcriptMenuOpen]);

  useEffect(() => {
    const hasTranslations = rows.some((row) => Boolean(row.target.trim()));
    if (
      hasTranslations &&
      !hadTranscriptTranslations.current &&
      storedTranscriptViewRef.current === null
    ) {
      setBilingualView(true);
    }
    hadTranscriptTranslations.current = hasTranslations;
  }, [rows]);

  useEffect(() => {
    if (!detail?.media.available || audioUrl) return;
    let cancelled = false;
    void fetchRecordingPlayback(meetingId, audioPart)
      .then((playback) => {
        if (!cancelled) {
          const playlist = playback.playlistUrls?.length
            ? playback.playlistUrls
            : [playback.url];
          setAudioPlaylist(playlist);
          setAudioPlaylistIndex(0);
          setAudioUrl(playlist[0]);
        }
      })
      .catch((error) =>
        setStatusMessage(
          errorMessage(
            error,
            t("desktopRecordingDetail.status.couldNotLoadRecording"),
          ),
        ),
      );
    return () => {
      cancelled = true;
    };
  }, [audioPart, audioUrl, detail?.media.available, meetingId, t]);

  useEffect(() => {
    if (!waveSurfer) return;
    if (playing) {
      void waveSurfer.play().catch(() => setPlaying(false));
    } else {
      waveSurfer.pause();
    }
  }, [playing, waveSurfer]);

  useEffect(() => {
    setWaveSurfer(null);
    setAudioTime(0);
    setAudioDuration(0);
    setWaveformReady(false);
    setWaveformError(false);
  }, [audioUrl]);

  const usableNotes = detail?.notes.filter((note) => !isGenerationErrorNote(note)) ?? [];
  const notesGenerationFailed = Boolean(
    usableNotes.length === 0 &&
      (detail?.notesGeneration?.status === "failed" ||
        detail?.notes.some(isGenerationErrorNote)),
  );
  const selectedNote =
    usableNotes.find((note) => note.timestamp === selectedNoteTimestamp) ??
    usableNotes[0] ??
    null;
  const hasTranscriptTranslations = rows.some((row) => Boolean(row.target.trim()));
  const showBilingualTranscript =
    bilingualView && hasTranscriptTranslations;
  const normalizedTranscriptSearch = transcriptSearch.trim().toLocaleLowerCase();
  const transcriptMatchIndices = useMemo(() => {
    if (!normalizedTranscriptSearch) return [];
    return rows.reduce<number[]>((matches, row, index) => {
      const searchableText = showBilingualTranscript
        ? `${row.source}\n${row.target}`
        : row.source;
      if (searchableText.toLocaleLowerCase().includes(normalizedTranscriptSearch)) {
        matches.push(index);
      }
      return matches;
    }, []);
  }, [normalizedTranscriptSearch, rows, showBilingualTranscript]);
  const activeTranscriptRowIndex = transcriptMatchIndices.length > 0
    ? transcriptMatchIndices[Math.min(activeTranscriptMatch, transcriptMatchIndices.length - 1)]
    : undefined;
  const transcriptGroups = useMemo(
    () =>
      rows.reduce<
        Array<{
          speaker: string;
          entries: Array<{ row: RecordingTranscriptSection; index: number }>;
        }>
      >((groups, row, index) => {
        const previousGroup = groups[groups.length - 1];
        if (previousGroup?.speaker === row.speaker) {
          previousGroup.entries.push({ row, index });
        } else {
          groups.push({ speaker: row.speaker, entries: [{ row, index }] });
        }
        return groups;
      }, []),
    [rows],
  );

  useEffect(() => {
    setActiveTranscriptMatch(0);
  }, [normalizedTranscriptSearch, showBilingualTranscript]);

  useEffect(() => {
    if (activeTab !== "transcript") return;
    const focusTranscriptSearch = (event: KeyboardEvent) => {
      if (event.key.toLocaleLowerCase() !== "f" || (!event.metaKey && !event.ctrlKey)) {
        return;
      }
      event.preventDefault();
      transcriptSearchInputRef.current?.focus();
      transcriptSearchInputRef.current?.select();
    };
    window.addEventListener("keydown", focusTranscriptSearch);
    return () => window.removeEventListener("keydown", focusTranscriptSearch);
  }, [activeTab]);

  const scrollToTranscriptRow = useCallback((rowIndex: number) => {
    transcriptListRef.current
      ?.querySelector(`[data-transcript-row-index="${rowIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, []);

  useEffect(() => {
    if (activeTranscriptRowIndex === undefined) return;
    scrollToTranscriptRow(activeTranscriptRowIndex);
  }, [activeTranscriptRowIndex, scrollToTranscriptRow]);

  const moveTranscriptMatch = (direction: -1 | 1) => {
    if (transcriptMatchIndices.length === 0) return;
    setActiveTranscriptMatch((current) =>
      (current + direction + transcriptMatchIndices.length) % transcriptMatchIndices.length,
    );
  };

  const renderTranscriptSearchHighlight = (text: string) => {
    if (!normalizedTranscriptSearch) return text;
    const escapedSearch = transcriptSearch.trim().replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const parts = text.split(new RegExp(`(${escapedSearch})`, "gi"));
    return parts.map((part, index) =>
      part.toLocaleLowerCase() === normalizedTranscriptSearch ? (
        <mark
          key={`${part}-${index}`}
          className="transcript-search-mark rounded-sm bg-[#5156FF]/45 px-0.5 text-inherit"
        >
          {part}
        </mark>
      ) : part,
    );
  };

  const displayTitle =
    detail?.title.trim() ||
    title.trim() ||
    t("desktopRecordingDetail.common.untitledMeeting");
  const displayParticipants = useMemo(() => {
    if (!detail) return [];

    const normalizedOwnerEmail = detail.owner.email?.trim().toLowerCase() ?? null;
    const normalizedOwnerName = detail.owner.name.trim().toLowerCase();

    const others = detail.participants
      .filter((person) => {
        const personEmail = person.email?.trim().toLowerCase() ?? null;
        if (normalizedOwnerEmail && personEmail === normalizedOwnerEmail) return false;
        if (person.name.trim().toLowerCase() === normalizedOwnerName) return false;
        return true;
      })
      .map((person) => ({
        key: person.email ?? person.name,
        name: person.name,
        email: person.email,
        me: person.me,
      }));

    return [
      {
        key: `owner:${detail.owner.id}`,
        name: detail.owner.name,
        email: detail.owner.email,
        me: detail.access === "owner",
      },
      ...others,
    ];
  }, [detail]);
  // Every unresolved comment written on a text selection, with the DOM range
  // it points at — the highlight is the affordance, so these are painted
  // whether or not their thread is open.
  // One card per anchor, newest anchor first, with the in-progress selection on
  // top so the composer is where the eye already is.
  const commentThreads = useMemo<RecordingCommentThread[]>(() => {
    const byAnchor = new Map<string, RecordingCommentThread>();
    for (const comment of comments) {
      const existing = byAnchor.get(comment.anchorId);
      if (existing) {
        existing.comments.push(comment);
        continue;
      }
      byAnchor.set(comment.anchorId, {
        anchor: {
          kind: comment.anchorType,
          id: comment.anchorId,
          label: comment.anchorLabel,
          excerpt: comment.anchorExcerpt,
        },
        comments: [comment],
      });
    }
    const threads = Array.from(byAnchor.values());
    if (commentAnchor && !byAnchor.has(commentAnchor.id)) {
      threads.unshift({ anchor: commentAnchor, comments: [], pending: true });
    }
    return threads;
  }, [commentAnchor, comments]);

  const storedHighlightComments = useMemo(
    () => comments.filter((comment) => !comment.resolved && comment.anchorId.includes("#")),
    [comments],
  );
  const totalOpenComments = comments.filter((comment) => !comment.resolved).length;
  const openComments = useCallback(
    (anchor: RecordingCommentAnchor, range?: Range | null) => {
      setCommentsRailOpen(true);
      setCommentAnchor(anchor);
      // A note comment can be selected while the transcript is showing (and the
      // reverse), so switch to the tab that actually renders the passage. The
      // scroll then waits for that tab to mount — see the effect below.
      const anchorTab: RecordingDetailTab =
        anchor.kind === "transcript" ? "transcript" : "enhanced";
      if (activeTab !== anchorTab) {
        setActiveTab(anchorTab);
        setCommentAnchorRange(null);
        pendingScrollAnchorRef.current = anchor.id;
        return;
      }
      // A selection carries its own range; a stored anchor is re-found from the
      // offsets in its id so the quoted words highlight again.
      const resolved = range ?? rangeForCommentAnchor(anchor.id);
      setCommentAnchorRange(resolved);
      // Re-resolve after the rail has opened: the reading column reflows, so
      // the range captured a moment ago points at stale coordinates.
      scrollRangeIntoViewAfterLayout(
        () => rangeForCommentAnchor(anchor.id) ?? resolved,
      );
    },
    [activeTab],
  );

  // Finishes a navigation that had to wait for the other tab to render. Retries
  // on each render while pending, because the transcript may still be loading.
  useEffect(() => {
    const anchorId = pendingScrollAnchorRef.current;
    if (!anchorId) return;
    const frame = window.requestAnimationFrame(() => {
      const resolved = rangeForCommentAnchor(anchorId);
      if (!resolved) return;
      pendingScrollAnchorRef.current = null;
      setCommentAnchorRange(resolved);
      scrollRangeIntoViewAfterLayout(
        () => rangeForCommentAnchor(anchorId) ?? resolved,
      );
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab, comments, rows, selectedNoteTimestamp, showBilingualTranscript]);

  // Selecting text inside a note section or transcript line offers to comment
  // on exactly that passage. Do not update React state during a pointer drag:
  // Enhanced Notes renders Markdown, and a re-render while Chromium is moving
  // the range can collapse the selection to a caret. Capture once the pointer
  // is released, or after a non-pointer selection settles.
  useEffect(() => {
    if (!RECORDING_COMMENTS_ENABLED || !detail?.permissions.comment) {
      setSelectionDraft(null);
      return;
    }
    let frame = 0;
    let settleTimer = 0;
    let pointerSelecting = false;
    const captureSelection = () => {
      if (settleTimer) {
        window.clearTimeout(settleTimer);
        settleTimer = 0;
      }
      if (frame) window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        const selection = window.getSelection();
        if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
          setSelectionDraft(null);
          return;
        }
        const liveRange = selection.getRangeAt(0);
        const ancestor = liveRange.commonAncestorContainer;
        const ancestorElement =
          ancestor instanceof HTMLElement ? ancestor : ancestor.parentElement;
        if (
          ancestorElement?.closest(
            ".recording-comments-rail, .selection-comment-bubble",
          )
        ) {
          return;
        }
        const base = commentBaseFromRange(liveRange);
        const baseId = base?.dataset.commentBase;
        if (!base || !baseId) {
          setSelectionDraft(null);
          return;
        }
        const range = rangeClampedToCommentBase(liveRange, base);
        const text = range.toString().trim();
        if (!text) {
          setSelectionDraft(null);
          return;
        }
        const start = textOffsetWithin(base, range);
        // The union box of a multi-line range ends at the bottom of the last
        // line and the right edge of the widest one, which parks the bubble far
        // from where the selection actually ends. Anchor to the last line box
        // instead — that is where the cursor let go.
        const rects = Array.from(range.getClientRects()).filter(
          (candidate) => candidate.width > 0 || candidate.height > 0,
        );
        const rect = rects[rects.length - 1] ?? range.getBoundingClientRect();
        const kind = base.dataset.commentKind === "transcript" ? "transcript" : "note";
        setSelectionDraft({
          anchor: {
            kind,
            id: `${baseId}#${start}-${start + text.length}`,
            // The API requires a label; sections and rows always provide one,
            // but fall back to the generic anchor name just in case.
            label:
              base.dataset.commentLabel?.trim() ||
              t(
                kind === "transcript"
                  ? "desktopRecordingDetail.comments.anchorTranscript"
                  : "desktopRecordingDetail.comments.anchorNote",
              ),
            excerpt: text.slice(0, 220),
          },
          range: range.cloneRange(),
          // Sits immediately after the last character, vertically centred on
          // that line — a Docs/Notion bubble, not a marker parked below the
          // paragraph. SELECTION_BUBBLE_SIZE keeps it inside the viewport.
          left: Math.min(
            rect.right + 6,
            window.innerWidth - SELECTION_BUBBLE_SIZE - 8,
          ),
          top: Math.max(
            8,
            Math.min(
              rect.top + rect.height / 2 - SELECTION_BUBBLE_SIZE / 2,
              window.innerHeight - SELECTION_BUBBLE_SIZE - 8,
            ),
          ),
        });
      });
    };
    const beginPointerSelection = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      pointerSelecting = Boolean(target?.closest?.("[data-comment-base]"));
      if (settleTimer) {
        window.clearTimeout(settleTimer);
        settleTimer = 0;
      }
    };
    const finishPointerSelection = () => {
      const shouldCapture = pointerSelecting;
      pointerSelecting = false;
      if (shouldCapture) captureSelection();
    };
    const captureKeyboardSelection = () => captureSelection();
    const captureSettledSelection = () => {
      if (pointerSelecting) return;
      if (settleTimer) window.clearTimeout(settleTimer);
      settleTimer = window.setTimeout(captureSelection, 80);
    };
    document.addEventListener("pointerdown", beginPointerSelection, true);
    document.addEventListener("pointerup", finishPointerSelection);
    document.addEventListener("pointercancel", finishPointerSelection);
    document.addEventListener("keyup", captureKeyboardSelection);
    document.addEventListener("selectionchange", captureSettledSelection);
    return () => {
      document.removeEventListener("pointerdown", beginPointerSelection, true);
      document.removeEventListener("pointerup", finishPointerSelection);
      document.removeEventListener("pointercancel", finishPointerSelection);
      document.removeEventListener("keyup", captureKeyboardSelection);
      document.removeEventListener("selectionchange", captureSettledSelection);
      if (settleTimer) window.clearTimeout(settleTimer);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [detail?.permissions.comment, t]);

  // Paint every commented passage, so a highlight is the thing you look for
  // and click — not a badge at the end of the row.
  useEffect(() => {
    const highlights = (
      CSS as unknown as { highlights?: Map<string, unknown> }
    ).highlights;
    const HighlightCtor = (
      window as unknown as { Highlight?: new (...ranges: Range[]) => unknown }
    ).Highlight;
    if (!highlights || !HighlightCtor) return;

    const paint = () => {
      const ranges = storedHighlightComments
        .map((comment) => rangeForCommentAnchor(comment.anchorId))
        .filter((range): range is Range => Boolean(range));
      if (ranges.length === 0) {
        highlights.delete("MeetingCopilot-comment-stored");
        return;
      }
      highlights.set("MeetingCopilot-comment-stored", new HighlightCtor(...ranges));
    };
    paint();

    // A Range holds the text nodes it was built from. Any re-render of the
    // notes or transcript replaces those nodes, which silently detaches the
    // range and the highlight vanishes — the deps below cannot catch every
    // such render, so re-anchor from the stored offsets whenever the DOM under
    // the text actually changes.
    let frame = 0;
    const schedulePaint = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        paint();
      });
    };
    const observer = new MutationObserver(schedulePaint);
    const target = detailBodyRef.current;
    if (target) {
      observer.observe(target, {
        subtree: true,
        childList: true,
        characterData: true,
      });
    }

    return () => {
      observer.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
      highlights.delete("MeetingCopilot-comment-stored");
    };
    // Re-anchor whenever the rendered text changes underneath the offsets.
  }, [
    storedHighlightComments,
    activeTab,
    selectedNoteTimestamp,
    selectedNote,
    rows,
    showBilingualTranscript,
  ]);

  // Clicking inside a highlight opens that passage's thread.
  useEffect(() => {
    if (!RECORDING_COMMENTS_ENABLED) return;
    const openHighlightAtPoint = (event: MouseEvent) => {
      // Ignore the click that ends a drag-selection.
      if (!window.getSelection()?.isCollapsed) return;
      const target = event.target as HTMLElement | null;
      const base = target?.closest?.<HTMLElement>("[data-comment-base]");
      const baseId = base?.dataset.commentBase;
      if (!base || !baseId) return;
      const caret = (
        document as unknown as {
          caretRangeFromPoint?: (x: number, y: number) => Range | null;
        }
      ).caretRangeFromPoint?.(event.clientX, event.clientY);
      if (!caret) return;
      const offset = textOffsetWithin(base, caret);
      const hit = storedHighlightComments.find((comment) => {
        const [commentBase, offsets] = comment.anchorId.split("#");
        if (commentBase !== baseId) return false;
        const [start, end] = offsets.split("-").map(Number);
        return offset >= start && offset < end;
      });
      if (!hit) return;
      openComments(
        {
          kind: hit.anchorType,
          id: hit.anchorId,
          label: hit.anchorLabel,
          excerpt: hit.anchorExcerpt,
        },
        rangeForCommentAnchor(hit.anchorId),
      );
    };
    document.addEventListener("click", openHighlightAtPoint);
    return () => document.removeEventListener("click", openHighlightAtPoint);
  }, [openComments, storedHighlightComments]);

  // The open thread's passage is painted more strongly than the rest.
  useEffect(() => {
    const highlights = (
      CSS as unknown as { highlights?: Map<string, unknown> }
    ).highlights;
    const HighlightCtor = (
      window as unknown as { Highlight?: new (...ranges: Range[]) => unknown }
    ).Highlight;
    if (!highlights || !HighlightCtor) return;
    // The anchor id carries the character offsets, so the passage can always be
    // found again from the DOM. Prefer that over the captured Range: a Range
    // dies the moment its text nodes are replaced (any notes re-render) or the
    // native selection collapses, which is what made the draft highlight blink
    // out while its bubble stayed on screen.
    const anchorId = commentAnchor?.id ?? selectionDraft?.anchor.id ?? null;
    const capturedRange = commentAnchorRange ?? selectionDraft?.range ?? null;
    const resolveRange = () =>
      (anchorId ? rangeForCommentAnchor(anchorId) : null) ?? capturedRange;

    const range = resolveRange();
    if (!range) {
      highlights.delete("MeetingCopilot-comment-anchor");
      return;
    }
    const paint = (live: Range) => {
      const active = new HighlightCtor(live) as { priority?: number };
      // Outrank the stored-comment highlight where they overlap.
      active.priority = 1;
      highlights.set("MeetingCopilot-comment-anchor", active);
    };
    paint(range);

    let frame = 0;
    const observer = new MutationObserver(() => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        const next = resolveRange();
        if (next) paint(next);
      });
    });
    const target = detailBodyRef.current;
    if (target) {
      observer.observe(target, {
        subtree: true,
        childList: true,
        characterData: true,
      });
    }

    return () => {
      observer.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
      highlights.delete("MeetingCopilot-comment-anchor");
    };
  }, [
    commentAnchor?.id,
    commentAnchorRange,
    selectionDraft,
    // A note section re-renders its Markdown on save; re-anchor against the
    // fresh DOM rather than the discarded nodes.
    selectedNote,
    activeTab,
    selectedNoteTimestamp,
  ]);
  const addComment = async (body: string) => {
    if (!commentAnchor) return;
    try {
      const created = await createRecordingComment(meetingId, {
        anchorType: commentAnchor.kind,
        anchorId: commentAnchor.id,
        anchorLabel: commentAnchor.label,
        anchorExcerpt: commentAnchor.excerpt,
        body,
      });
      setComments((current) => [...current, created]);
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.commentAddFailed")));
      throw error;
    }
  };
  const toggleCommentResolved = async (commentId: string) => {
    const comment = comments.find((item) => item.id === commentId);
    if (!comment) return;
    try {
      const updated = await updateRecordingComment(
        meetingId,
        commentId,
        !comment.resolved,
      );
      setComments((current) =>
        current.map((item) => (item.id === commentId ? updated : item)),
      );
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.commentUpdateFailed")));
    }
  };
  const deleteComment = async (commentId: string) => {
    try {
      await deleteRecordingComment(meetingId, commentId);
      setComments((current) =>
        current.filter((comment) => comment.id !== commentId),
      );
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.commentDeleteFailed")));
    }
  };
  const hasRecording = Boolean(detail?.media.available);
  const showPaywall = Boolean(detail?.paywalled);

  useLayoutEffect(() => {
    const element = floatStackRef.current;
    if (!element) return;
    const update = () => {
      const height = Math.ceil(element.getBoundingClientRect().height);
      if (height > 0) setBottomInset(height);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [detail, playerOpen, statusMessage, hasRecording, showPaywall]);
  const noteAsText = (note: RecordingNoteVersion | null) =>
    note
      ? Object.entries(note.sections)
          .filter(([, value]) => Boolean(value))
          .map(([section, value]) => `## ${section}\n${value}`)
          .join("\n\n")
      : "";

  const openPricing = (referrer: "notes" | "transcript") => {
    onOpenPricing?.(referrer);
    if (!onOpenPricing && typeof window !== "undefined") {
      window.electronAPI?.openExternal(
        `https://app.MeetingCopilot.io/pricing?referrer=${referrer}`,
      );
    }
  };

  const handleUnblur = async () => {
    if (actionLoading === "unblur") return;
    setActionLoading("unblur");
    try {
      const next = await unblurRecording(meetingId);
      setDetail(next);
      setLoadError(null);
      setMyNotesContent(next.notepadHtml);
      lastSavedNotepad.current = next.notepadHtml;
      setSelectedNoteTimestamp(next.notes[0]?.timestamp ?? null);
      setActiveTab("enhanced");
      applyRememberedPlayerState(next.media.available);
      setRows([]);
      if (activeTab === "transcript") {
        void loadTranscript();
      }
      setStatusMessage(null);
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.paywall.unblurError"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const copyText = async (
    type: "notes" | "transcript",
    mode: TranscriptCopyMode = "both",
  ) => {
    if (showPaywall) return;
    const text =
      type === "notes"
        ? activeTab === "my-notes"
          ? (notepadFormat === "markdown" ? myNotesContent : htmlToText(myNotesContent))
          : noteAsText(selectedNote)
        : rows
            .map((row) => {
              const heading = `### ${row.speaker} - ${formatTimecode(
                detail
                  ? secondsFromMeetingStart(detail.timestamp, row.start)
                  : 0,
              )}`;
              if (mode === "source") return `${heading}\n${row.source}`;
              if (mode === "target") return `${heading}\n${row.target}`;
              return `${heading}\n${row.source}\n\n${row.target}`;
            })
            .join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(type);
      setCopyMenuOpen(false);
      window.setTimeout(() => setCopied(null), 1600);
    } catch {
      setStatusMessage(t("desktopRecordingDetail.status.couldNotCopy"));
    }
  };

  const saveTitle = async (nextTitle: string) => {
    if (!detail?.permissions.editTitle || nextTitle.trim() === detail.title) return;
    try {
      const result = await updateRecordingTitle(meetingId, nextTitle);
      setDetail((current) =>
        current ? { ...current, title: result.title } : current,
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotUpdateTitle"),
        ),
      );
    }
  };

  const saveEnhancedSection = async (
    section: MeetingNoteSection,
    content: string,
  ) => {
    if (!selectedNote) return;
    try {
      await updateRecordingNoteSection(
        meetingId,
        selectedNote.timestamp,
        section,
        content,
      );
      setDetail((current) =>
        current
          ? {
              ...current,
              notes: current.notes.map((note) =>
                note.timestamp === selectedNote.timestamp
                  ? {
                      ...note,
                      sections: { ...note.sections, [section]: content },
                    }
                  : note,
              ),
            }
          : current,
      );
      setStatusMessage(
        t("desktopRecordingDetail.status.enhancedNotesSaved"),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotSaveEnhancedNotes"),
        ),
      );
      throw error;
    }
  };

  const regenerate = async () => {
    if (!detail?.permissions.regenerate || actionLoading) return;
    setActionLoading("regenerate");
    setEnhancedMenuOpen(false);
    try {
      const note = await regenerateRecordingNotes(
        meetingId,
        detail?.access === "shared" ? selectedNote?.language : undefined,
      );
      await loadDetail(true);
      setSelectedNoteTimestamp(note.timestamp);
      setActiveTab("enhanced");
      setStatusMessage(
        t("desktopRecordingDetail.status.newMeetingNotesCreated"),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotCreateMeetingNotes"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const retryNotes = async () => {
    if (!detail?.notesGeneration?.canRetry || actionLoading) return;
    setActionLoading("retry-notes");
    try {
      await retryRecordingNotes(meetingId);
      await loadDetail(true);
      setStatusMessage(t("desktopRecordingDetail.status.notesRetryQueued"));
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotRetryNotes"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const translate = async (language: string) => {
    if (!selectedNote || !detail?.permissions.translate || actionLoading) return;
    setActionLoading("translate");
    setEnhancedMenuOpen(false);
    setShowLanguages(false);
    try {
      const result = await translateRecordingNotes(
        meetingId,
        selectedNote.timestamp,
        language,
      );
      await loadDetail(true);
      setSelectedNoteTimestamp(result.note.timestamp);
      setActiveTab("enhanced");
      setStatusMessage(
        result.reused
          ? t("desktopRecordingDetail.status.existingTranslationSelected")
          : t("desktopRecordingDetail.status.notesTranslated"),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotTranslateNotes"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const translateTranscript = async (language: string) => {
    if (!detail?.permissions.translate || actionLoading) return;
    setActionLoading("translate-transcript");
    setTranscriptMenuOpen(false);
    setShowTranscriptLanguages(false);
    setStatusMessage(
      t("desktopRecordingDetail.status.translatingTranscript"),
    );
    try {
      const result = await translateRecordingTranscript(meetingId, language);
      await loadTranscript();
      chooseTranscriptView(true);
      setStatusMessage(
        t("desktopRecordingDetail.status.transcriptTranslated", {
          count: result.translatedSections,
        }),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotTranslateTranscript"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const handleTranscriptCleanupApplied = async (sectionCount: number) => {
    await Promise.all([loadTranscript(), loadDetail(true)]);
    setStatusMessage(t("desktopUi.recording.transcriptCleaned", { count: sectionCount }));
  };

  const restoreOriginalTranscript = async () => {
    if (
      detail?.status !== "ready" ||
      !detail.permissions.editTranscript ||
      !detail.canRestoreOriginalTranscript ||
      actionLoading
    ) return;
    if (!window.confirm(t("desktopUi.recording.restoreConfirm"))) {
      return;
    }
    setActionLoading("restore-transcript");
    setTranscriptMenuOpen(false);
    setStatusMessage(t("desktopUi.recording.restoringOriginal"));
    try {
      const result = await restoreOriginalRecordingTranscript(meetingId);
      await Promise.all([loadTranscript(), loadDetail(true)]);
      chooseTranscriptView(false);
      setStatusMessage(t("desktopUi.recording.originalRestored", {
        count: result.restoredSections,
      }));
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.restoreOriginalFailed")));
    } finally {
      setActionLoading(null);
    }
  };

  const submitShare = async () => {
    if (showPaywall) return;
    const emails = shareInput
      .split(/[\s,;]+/)
      .map((email) => email.trim())
      .filter(Boolean);
    if (emails.length === 0 || actionLoading) return;
    setActionLoading("share");
    try {
      const result = await shareRecording(
        meetingId,
        emails,
        sharePermission,
        createShareChat,
      );
      setDetail((current) =>
        current
          ? {
              ...current,
              sharedTo: result.recipients,
              sharedEditors: result.sharedEditors ?? current.sharedEditors ?? [],
              sharedCommenters: result.sharedCommenters ?? current.sharedCommenters ?? [],
            }
          : current,
      );
      setShareInput("");
      setShareOpen(false);
      setStatusMessage(
        result.failed.length
          ? t(
              result.failed.length === 1
                ? "desktopRecordingDetail.share.sharedPartialSingular"
                : "desktopRecordingDetail.share.sharedPartialPlural",
              { count: result.failed.length },
            )
          : t(
              result.sharedCount === 1
                ? "desktopRecordingDetail.share.sharedSuccessSingular"
                : "desktopRecordingDetail.share.sharedSuccessPlural",
              { count: result.sharedCount },
            ),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotShareMeeting"),
        ),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const shareMeetingToExistingChat = async (
    targetChatId: string | null = selectedShareChatId,
    chatTitle?: string,
    pendingEmails: string[] = [],
  ) => {
    if (!detail || !targetChatId || shareChatSending) return;
    const chat = shareChats.find((candidate) => candidate.id === targetChatId);
    const title = chatTitle || chat?.title;
    if (!chat && !chatTitle) return;

    const rawPreview =
      selectedNote?.sections.gist?.trim() ||
      selectedNote?.sections.summary?.trim() ||
      "Meeting notes are ready to view.";
    const preview = rawPreview.replace(/\s+/g, " ").slice(0, 160);
    const sharedHref = `/meeting/shared/${detail.id}`;

    setShareChatSending(true);
    setShareChatError(null);
    try {
      // Grant the chosen permission to everyone in the chat as of now. `notify: false`
      // keeps this silent — the notes card in the thread is the notification, so no
      // invite email is sent and no AI credit is spent.
      if (detail.permissions.share) {
        await shareRecording(detail.id, pendingEmails, shareChatPermission, false, {
          chatId: targetChatId,
          notify: false,
        });
      }
      await sendMessage(targetChatId, {
        content: [
          `Meeting notes shared for ${detail.id}`,
          detail.title,
          preview,
          `View notes: ${sharedHref}`,
        ].join("\n"),
        messageType: "meeting_notes",
        metadata: {
          meetingId: detail.id,
          title: detail.title,
          preview,
          sharedHref,
          ownerHref: `/meeting/${detail.id}`,
          meetingOwnerId: detail.owner.id,
        },
        clientMessageId: `meeting-notes-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        sourceLanguage: "en",
      });
      setShareOpen(false);
      setSelectedShareChatId(null);
      setShareChatQuery("");
      setStatusMessage(t("desktopUi.recording.sharedNotesTo", { title }));
    } catch (error) {
      setShareChatError(errorMessage(error, t("desktopUi.recording.shareToChatFailed")));
    } finally {
      setShareChatSending(false);
    }
  };

  const createShareChatAndShare = async () => {
    const title = newShareChatTitle.trim();
    const emails = newShareChatEmails
      .split(/[,\s;]+/)
      .map((value) => value.trim().toLowerCase())
      .filter((value) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value));
    if (!title || emails.length === 0 || creatingShareChat) return;
    setCreatingShareChat(true);
    setShareChatError(null);
    try {
      const timezone =
        typeof Intl !== "undefined" ? Intl.DateTimeFormat().resolvedOptions().timeZone : "UTC";
      const created = await createChat({
        kind: "group",
        title,
        ownerLanguage: "en",
        ownerTimezone: timezone,
        participants: [],
      });
      await Promise.all(
        emails.map((email) =>
          createInvite(created.id, {
            inviteeEmail: email,
            timezone,
            chatTitle: title,
            chatKind: "group",
          }),
        ),
      );
      setShareChats((current) => [created, ...current]);
      setNewShareChatOpen(false);
      setNewShareChatTitle("");
      setNewShareChatEmails("");
      // Invitees are not members until they accept, so grant to their emails directly.
      await shareMeetingToExistingChat(created.id, title, emails);
    } catch (error) {
      setShareChatError(errorMessage(error, t("desktopUi.recording.createChatFailed")));
    } finally {
      setCreatingShareChat(false);
    }
  };

  const appendShareRecipient = (email: string) => {
    const emails = shareInput
      .split(/[\s,;]+/)
      .map((value) => value.trim().toLowerCase())
      .filter(Boolean);
    if (!emails.includes(email.toLowerCase())) emails.push(email.toLowerCase());
    setShareInput(`${emails.join(", ")}, `);
    setShareSuggestionsOpen(false);
  };

  const updateSharePermission = async (
    email: string,
    permission: RecordingSharePermission,
  ) => {
    if (actionLoading) return;
    setActionLoading("share");
    try {
      const result = await shareRecording(meetingId, [email], permission, false);
      setDetail((current) =>
        current
          ? {
              ...current,
              sharedTo: result.recipients,
              sharedEditors: result.sharedEditors ?? current.sharedEditors ?? [],
              sharedCommenters: result.sharedCommenters ?? current.sharedCommenters ?? [],
            }
          : current,
      );
      setStatusMessage(
        permission === "edit"
          ? t("desktopUi.recording.permissionCanEdit", { email })
          : permission === "comment"
            ? t("desktopUi.recording.permissionCanComment", { email })
            : t("desktopUi.recording.permissionViewOnly", { email }),
      );
    } catch (error) {
      setStatusMessage(
        errorMessage(error, t("desktopUi.recording.updatePermissionFailed")),
      );
    } finally {
      setActionLoading(null);
    }
  };

  const copyShareLink = async () => {
    if (!detail?.permissions.share || shareLinkLoading) return;
    setShareLinkLoading(true);
    try {
      const result = detail.shareLink
        ? { url: detail.shareLink }
        : await getOrCreateRecordingShareLink(meetingId);
      setDetail((current) => current ? { ...current, shareLink: result.url } : current);
      await navigator.clipboard.writeText(result.url);
      setShareLinkCopied(true);
      window.setTimeout(() => setShareLinkCopied(false), 5000);
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.copyShareLinkFailed")));
    } finally {
      setShareLinkLoading(false);
    }
  };

  const revokeShareLink = async () => {
    if (!detail?.permissions.share || shareLinkLoading) return;
    setShareLinkLoading(true);
    try {
      await revokeRecordingShareLink(meetingId);
      setDetail((current) => current ? { ...current, shareLink: null } : current);
      setShareLinkCopied(false);
    } catch (error) {
      setStatusMessage(errorMessage(error, t("desktopUi.recording.disableShareLinkFailed")));
    } finally {
      setShareLinkLoading(false);
    }
  };

  const updateTranscript = async (
    index: number,
    field: "speaker" | "source" | "target",
    value: string,
  ) => {
    if (!detail?.permissions.editTranscript) return;
    const current = rows[index];
    const normalized = value.trim();
    if (!current || !normalized || current[field] === normalized) return;
    const previousRows = rows;
    if (field === "speaker") {
      setRows((items) =>
        items.map((row) =>
          row.speaker === current.speaker
            ? { ...row, speaker: normalized }
            : row,
        ),
      );
    } else {
      setRows((items) =>
        items.map((row, rowIndex) =>
          rowIndex === index ? { ...row, [field]: normalized } : row,
        ),
      );
    }
    try {
      if (field === "speaker") {
        await updateRecordingSpeaker(
          meetingId,
          current.speaker,
          normalized,
        );
      } else {
        await updateRecordingTranscriptSection(
          meetingId,
          current.sectionId,
          field,
          normalized,
        );
      }
      setStatusMessage(t("desktopRecordingDetail.status.transcriptSaved"));
    } catch (error) {
      setRows(previousRows);
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotSaveTranscript"),
        ),
      );
    }
  };

  const seekTo = (seconds: number) => {
    const requestedTime = Math.max(0, seconds);
    setPlayerOpen(true);
    setPlaying(true);
    writePlayerOpen(meetingId, true);

    if (!waveSurfer) {
      pendingSeekRef.current = requestedTime;
      return;
    }
    const duration = waveSurfer.getDuration();
    if (!Number.isFinite(duration) || duration <= 0) {
      pendingSeekRef.current = requestedTime;
      return;
    }
    const nextTime = Math.min(duration, requestedTime);
    pendingSeekRef.current = null;
    waveSurfer.seekTo(nextTime / duration);
    setAudioTime(nextTime);
  };

  const loadAudioPart = (part: number, autoplay = true) => {
    setAudioPart(part);
    setAudioUrl(null);
    setAudioPlaylist([]);
    setAudioPlaylistIndex(0);
    setAudioTime(0);
    setAudioDuration(0);
    setPlaying(autoplay);
  };

  const downloadRecording = async () => {
    try {
      const playback = await fetchRecordingPlayback(
        meetingId,
        audioPart,
        "attachment",
      );
      window.electronAPI?.openExternal(playback.url);
    } catch (error) {
      setStatusMessage(
        errorMessage(
          error,
          t("desktopRecordingDetail.status.couldNotDownloadRecording"),
        ),
      );
    }
  };

  const openCalendar = () => {
    if (!detail) return;
    const date = new Date(detail.timestamp);
    const path = `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
    window.electronAPI?.openExternal(
      `https://calendar.google.com/calendar/u/0/r/day/${path}`,
    );
  };

  const submitAsk = () => {
    const question = askInput.trim();
    if (!question || !detail?.permissions.ask) return;
    setPlayerOpen(false);
    setAssistantLaunch({
      meetingId,
      title: displayTitle,
      initialQuestion: question,
      nonce: Date.now(),
    });
    setAskInput("");
  };

  const openAssistantDraft = (draft = "") => {
    if (!detail?.permissions.ask) return;
    setPlayerOpen(false);
    setAssistantLaunch((current) => current ?? {
      meetingId,
      title: displayTitle,
      initialQuestion: "",
      initialDraft: draft,
      nonce: Date.now(),
    });
    setAskInput("");
  };

  if (loading && !detail) {
    return <RecordingDetailSkeleton title={title} />;
  }

  if (loadError && !detail) {
    return (
      <div className="flex h-full items-center justify-center bg-[#0B0B0D] px-8 text-center">
        <div>
          <AlertCircle className="mx-auto text-[#B45D43]" size={24} />
          <p className="mt-3 text-[13px] text-[#C9C9CF]">{loadError}</p>
          <button
            type="button"
            onClick={() => void loadDetail()}
            className="mt-4 rounded-full border border-[#38383F] px-4 py-2 text-[12px] text-[#D7D7DB]"
          >
            {t("desktopRecordingDetail.common.tryAgain")}
          </button>
        </div>
      </div>
    );
  }

  if (!detail) return null;

  // Some capture clients historically wrote internal speaker identifiers such
  // as "camera-subtitle" into sharedTo. They are routing metadata, not people.
  const shareRecipientEmails = detail.sharedTo.filter((value) => {
    const normalized = value.trim();
    return /^[^\s@]+@[^\s@]+$/.test(normalized);
  });

  const ghostCopyButton = (type: "notes" | "transcript", className = "") => (
    <button
      type="button"
      disabled={showPaywall}
      onClick={() => {
        if (showPaywall) return;
        void copyText(type);
      }}
      className={`flex h-8 items-center gap-1.5 rounded-full px-3 text-[12px] font-medium text-[#85858E] transition hover:bg-[#202024] hover:text-white disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-[#85858E] ${className}`}
    >
      {copied === type ? <Check size={13} /> : <Copy size={13} />}
      {copied === type
        ? t("desktopRecordingDetail.copy.copied")
        : t("desktopRecordingDetail.copy.label")}
    </button>
  );

  return (
    <div
      className="desktop-recording-detail relative flex h-full bg-[#0B0B0D] text-[#C9C9CF]"
      style={
        {
          "--recording-detail-bottom-inset": `${bottomInset}px`,
        } as CSSProperties
      }
    >
      <div className="h-full min-w-0 flex-1 overflow-y-auto">
        <div
          className="mx-auto w-full max-w-[1120px] px-10 pt-4"
          style={{
            paddingBottom: "var(--recording-detail-bottom-inset, 8rem)",
          }}
        >
          <div className="flex items-center justify-between gap-6">
            <h1
              contentEditable={detail.permissions.editTitle}
              suppressContentEditableWarning
              onBlur={(event) => void saveTitle(event.currentTarget.innerText)}
              className={`min-w-0 font-quicksand text-[24px] font-medium leading-snug tracking-[-0.01em] text-[#F1F1F3] outline-none ${
                detail.permissions.editTitle
                  ? "rounded px-1 focus:bg-[#151517]"
                  : ""
              }`}
            >
              {displayTitle}
            </h1>
            <div className="flex shrink-0 items-center gap-2">
              {RECORDING_COMMENTS_ENABLED && (
                <button
                  type="button"
                  disabled={showPaywall}
                  onClick={() => setCommentsRailOpen((open) => !open)}
                  aria-pressed={commentsRailOpen}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-[#303036] bg-[#151517] px-3 text-[12px] font-medium text-[#A0A0A8] transition hover:bg-[#202024] hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
                  aria-label={t("desktopTranslatedFileComments.openCount", { count: totalOpenComments })}
                >
                  <MessageCircle size={13} />
                  {t("desktopRecordingDetail.comments.title")}
                  {totalOpenComments > 0 && (
                    <span className="inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-[#5156FF] px-1 text-[10px] font-semibold leading-none text-white">
                      {totalOpenComments}
                    </span>
                  )}
                </button>
              )}
              <div className="detail-share relative flex h-7 shrink-0 gap-px">
                <button
                  type="button"
                  disabled={!detail.permissions.share || showPaywall}
                  onClick={() => {
                    if (showPaywall) return;
                    setCreateShareChat(true);
                    setShareMode("people");
                    setShareChatError(null);
                    setShareOpen(true);
                  }}
                  className="detail-share-main flex items-center gap-1.5 px-2.5 text-[11px] font-semibold transition disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <Lock size={11} />
                  {t("desktopRecordingDetail.share.button")}
                </button>
                <button
                  type="button"
                  disabled={!detail.permissions.share || showPaywall || shareLinkLoading}
                  onClick={() => void copyShareLink()}
                  className="detail-share-link-trigger flex w-7 items-center justify-center transition disabled:cursor-not-allowed disabled:opacity-50"
                  aria-label={t("desktopRecordingDetail.share.copyLinkLabel")}
                  aria-describedby={!shareLinkCopied ? "copy-share-link-tooltip" : undefined}
                >
                  {shareLinkLoading ? <Loader2 size={12} className="animate-spin" /> : shareLinkCopied ? <Check size={12} /> : <Link2 size={12} />}
                </button>
                {!shareLinkCopied && (
                  <div
                    id="copy-share-link-tooltip"
                    role="tooltip"
                    className="detail-share-tooltip pointer-events-none absolute right-0 top-9 z-30 whitespace-nowrap rounded-lg px-2.5 py-1.5 text-[10px] font-medium shadow-lg"
                  >
                    {t("desktopRecordingDetail.share.copyLink")}
                  </div>
                )}
                {shareLinkCopied && (
                  <div
                    role="status"
                    aria-live="polite"
                    className="detail-share-feedback absolute right-0 top-9 z-30 w-max min-w-[170px] rounded-xl px-3 py-2 text-left shadow-lg"
                  >
                    <span className="block text-[11px] font-semibold leading-4">
                      {t("desktopRecordingDetail.share.linkCopied")}
                    </span>
                    <span className="block text-[10px] font-medium leading-4 opacity-75">
                      {t("desktopRecordingDetail.share.linkCopiedHint")}
                    </span>
                  </div>
                )}
              </div>
            </div>
          </div>

          <div className="mt-3.5 flex flex-wrap items-center gap-2">
            <div
              className="flex items-center rounded-full border border-[#2A2A2F] bg-[#1A1A1D] p-0.5"
              role="tablist"
              aria-label={t("desktopRecordingDetail.tabs.noteViewAria")}
            >
              {tabs.map(({ id, label, icon: Icon }) => {
                const active = activeTab === id;
                const isEnhanced = id === "enhanced";
                const isTranscript = id === "transcript";
                const tabButton = (
                  <button
                    type="button"
                    role="tab"
                    aria-selected={active}
                    onClick={() => {
                      if (isEnhanced && active) {
                        setEnhancedMenuOpen((open) => !open);
                        setTranscriptMenuOpen(false);
                      } else if (isTranscript && active) {
                        setTranscriptMenuOpen((open) => !open);
                        setEnhancedMenuOpen(false);
                      } else {
                        setActiveTab(id);
                        setEnhancedMenuOpen(false);
                        setTranscriptMenuOpen(false);
                      }
                    }}
                    className={`flex h-[30px] items-center gap-1.5 rounded-full px-3 text-[12px] font-medium transition ${
                      active
                        ? "bg-[#2A2A2A] text-[#F1F1F3] shadow-sm"
                        : "text-[#85858E] hover:text-white"
                    }`}
                  >
                    {Icon && <Icon size={12} />}
                    {label}
                    {(isEnhanced || isTranscript) && (
                      <ChevronDown
                        size={11}
                        className={`text-[#777780] transition-transform ${
                          (isEnhanced && enhancedMenuOpen) ||
                          (isTranscript && transcriptMenuOpen)
                            ? "rotate-180"
                            : ""
                        }`}
                      />
                    )}
                  </button>
                );
                if (!isEnhanced && !isTranscript) {
                  return <span key={id}>{tabButton}</span>;
                }
                if (isTranscript) {
                  return (
                    <div key={id} ref={transcriptMenuRef} className="relative">
                      {tabButton}
                      {transcriptMenuOpen && (
                        <div
                          role="menu"
                          className="absolute right-0 top-10 z-30 w-64 rounded-2xl border border-[#303036] bg-[#151517] p-1.5 shadow-[0_14px_34px_rgba(0,0,0,0.38)]"
                        >
                          <button
                            type="button"
                            disabled={
                              !showBilingualTranscript &&
                              !hasTranscriptTranslations
                            }
                            onClick={() => {
                              chooseTranscriptView(!bilingualView);
                              setTranscriptMenuOpen(false);
                            }}
                            className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#202024] hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
                          >
                            <RefreshCw size={13} />
                            {showBilingualTranscript
                              ? t("desktopRecordingDetail.menus.monolingualView")
                              : t("desktopRecordingDetail.menus.bilingualView")}
                          </button>
                          <button
                            type="button"
                            disabled={!detail.permissions.translate || Boolean(actionLoading)}
                            onClick={() =>
                              setShowTranscriptLanguages((show) => !show)
                            }
                            className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#292955] hover:text-[#D6DCFF] disabled:opacity-40"
                          >
                            {actionLoading === "translate-transcript" ? (
                              <Loader2 size={13} className="animate-spin" />
                            ) : (
                              <Languages size={13} />
                            )}
                            {t("desktopRecordingDetail.menus.translateTranscript")}
                          </button>
                          {showTranscriptLanguages && (
                            <div className="mt-1 grid max-h-44 grid-cols-2 gap-1 overflow-y-auto border-t border-[#2A2A2F] pt-1">
                              {languageOptions.map(([code, label]) => (
                                <button
                                  key={code}
                                  type="button"
                                  onClick={() => void translateTranscript(code)}
                                  className="rounded-lg px-2 py-1.5 text-left text-[10px] text-[#85858E] hover:bg-[#202024] hover:text-white"
                                >
                                  {label}
                                </button>
                              ))}
                            </div>
                          )}
                          <div className="mt-1 border-t border-[#2A2A2F] pt-1">
                            <button
                              type="button"
                              disabled={
                                detail.status !== "ready" ||
                                !detail.permissions.curateTranscript ||
                                Boolean(actionLoading)
                              }
                              onClick={() => {
                                setTranscriptMenuOpen(false);
                                setTranscriptCleanupOpen(true);
                              }}
                              className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#292955] hover:text-[#D6DCFF] disabled:cursor-not-allowed disabled:opacity-40"
                            >
                              <Sparkles size={13} />
                              {t("desktopRecordingDetail.transcript.cleanUp")}
                            </button>
                            {detail.canRestoreOriginalTranscript && (
                              <button
                                type="button"
                                disabled={
                                  detail.status !== "ready" ||
                                  !detail.permissions.editTranscript ||
                                  Boolean(actionLoading)
                                }
                                onClick={() => void restoreOriginalTranscript()}
                                className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#202024] hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
                              >
                                {actionLoading === "restore-transcript" ? (
                                  <Loader2 size={13} className="animate-spin" />
                                ) : (
                                  <RefreshCw size={13} />
                                )}
                                {t("desktopUi.recording.restoreOriginal")}
                              </button>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  );
                }
                return (
                  <div key={id} ref={enhancedMenuRef} className="relative">
                    {tabButton}
                    {enhancedMenuOpen && (
                      <div
                        role="menu"
                        className="absolute left-0 top-10 z-30 w-64 rounded-2xl border border-[#303036] bg-[#151517] p-1.5 shadow-[0_14px_34px_rgba(0,0,0,0.38)]"
                      >
                        {usableNotes.length > 1 && (
                          <div className="mb-1 border-b border-[#2A2A2F] pb-1">
                            {usableNotes.map((note) => (
                              <button
                                key={note.timestamp}
                                type="button"
                                onClick={() => {
                                  setSelectedNoteTimestamp(note.timestamp);
                                  setEnhancedMenuOpen(false);
                                }}
                                className={`flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-[11px] transition hover:bg-[#202024] ${
                                  note.timestamp === selectedNote?.timestamp
                                    ? "text-[#D6DCFF]"
                                    : "text-[#85858E]"
                                }`}
                              >
                                <span>{note.language.toUpperCase()} · {note.type}</span>
                                {note.timestamp === selectedNote?.timestamp && (
                                  <Check size={12} />
                                )}
                              </button>
                            ))}
                          </div>
                        )}
                        <button
                          type="button"
                          disabled={!detail.permissions.regenerate || Boolean(actionLoading)}
                          onClick={() => void regenerate()}
                          className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#292955] hover:text-[#D6DCFF] disabled:opacity-40"
                        >
                          {actionLoading === "regenerate" ? (
                            <Loader2 size={13} className="animate-spin" />
                          ) : (
                            <RefreshCw size={13} />
                          )}
                          {t("desktopRecordingDetail.menus.createNotesAgain")}
                        </button>
                        <button
                          type="button"
                          disabled={!detail.permissions.translate || !selectedNote || Boolean(actionLoading)}
                          onClick={() => setShowLanguages((show) => !show)}
                          className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[12px] text-[#A0A0A8] transition hover:bg-[#292955] hover:text-[#D6DCFF] disabled:opacity-40"
                        >
                          {actionLoading === "translate" ? (
                            <Loader2 size={13} className="animate-spin" />
                          ) : (
                            <Languages size={13} />
                          )}
                          {t("desktopRecordingDetail.menus.translateNotes")}
                        </button>
                        {showLanguages && (
                          <div className="mt-1 grid max-h-44 grid-cols-2 gap-1 overflow-y-auto border-t border-[#2A2A2F] pt-1">
                            {languageOptions.map(([code, label]) => (
                              <button
                                key={code}
                                type="button"
                                onClick={() => void translate(code)}
                                className="rounded-lg px-2 py-1.5 text-left text-[10px] text-[#85858E] hover:bg-[#202024] hover:text-white"
                              >
                                {label}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>

            <div ref={metaRef} className="relative">
              <button
                type="button"
                onClick={() => setMetaOpen((open) => !open)}
                className="flex h-8 items-center gap-1.5 rounded-full border border-[#2A2A2F] bg-[#151517] px-3 text-[12px] text-[#A0A0A8] transition hover:bg-[#202024]"
              >
                <CalendarDays size={13} className="text-[#777780]" />
                {formatDate(detail.timestamp, locale)}
                <UsersRound size={13} className="ml-1 text-[#777780]" />
                {displayParticipants.length}
              </button>
              {metaOpen && (
                <div className="absolute left-0 top-10 z-30 w-80 rounded-2xl border border-[#303036] bg-[#151517] shadow-[0_14px_34px_rgba(0,0,0,0.38)]">
                  <div className="p-4">
                    <div className="flex items-center gap-2.5">
                      <span
                        className={`h-2.5 w-2.5 shrink-0 rounded-[3px] ${
                          detail.status === "live"
                            ? "bg-[#67D44B]"
                            : "bg-[#5156FF]"
                        }`}
                      />
                      <span className="min-w-0 truncate text-[13px] font-semibold text-[#F1F1F3]">
                        {displayTitle}
                      </span>
                    </div>
                    <p className="mt-1 pl-[22px] text-[11.5px] text-[#A0A0A8]">
                      {formatLongDate(
                        detail.timestamp,
                        detail.endTime,
                        locale,
                        t("desktopRecordingDetail.meta.now"),
                      )}
                    </p>
                    <div className="mt-3 pl-[22px]">
                      <p className="text-[11px] text-[#777780]">
                        {t("desktopRecordingDetail.meta.owner")}
                      </p>
                      <p className="mt-0.5 truncate text-[12.5px] text-[#C9C9CF]">
                        {detail.owner.name}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={openCalendar}
                      className="mt-3.5 flex h-9 w-full items-center justify-center gap-2 rounded-full border border-[#38383F] text-[12px] font-medium text-[#D7D7DB] transition hover:bg-[#202024]"
                    >
                      <CalendarDays size={13} className="text-[#85858E]" />
                      {t("desktopRecordingDetail.meta.openInGoogleCalendar")}
                    </button>
                  </div>
                  <div className="border-t border-[#2A2A2F] px-4 py-3">
                    <p className="text-[11px] font-semibold text-[#777780]">
                      {t("desktopRecordingDetail.meta.participants")}
                    </p>
                    <ul className="mt-2 space-y-2.5">
                      {displayParticipants.map((person) => (
                        <li
                          key={person.key}
                          className="flex items-center gap-2.5"
                        >
                          <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[#292955] text-[11px] font-semibold text-[#A5A7FF]">
                            {person.name[0]?.toUpperCase()}
                          </span>
                          <div className="min-w-0">
                            <p className="truncate text-[12.5px] text-[#D7D7DB]">
                              {person.name}
                              {person.me && (
                                <span className="text-[#777780]">
                                  {t("desktopRecordingDetail.meta.meSuffix")}
                                </span>
                              )}
                            </p>
                            {person.email && person.email !== person.name && (
                              <p className="truncate text-[11px] text-[#777780]">
                                {person.email}
                              </p>
                            )}
                          </div>
                        </li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}
            </div>

            {activeTab === "transcript" ? (
              <div className="ml-auto flex min-w-0 items-center gap-2">
                <div className="flex h-8 w-[220px] items-center gap-1 rounded-full border border-[#303036] bg-[#151517] px-2.5 text-[#85858E] transition-colors focus-within:border-[#5156FF]/60 focus-within:text-[#C9C9CF]">
                  <Search size={13} className="shrink-0" aria-hidden="true" />
                  <input
                    ref={transcriptSearchInputRef}
                    type="search"
                    value={transcriptSearch}
                    onChange={(event) => {
                      submittedTranscriptSearchRef.current = "";
                      setTranscriptSearch(event.target.value);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        if (
                          submittedTranscriptSearchRef.current !== normalizedTranscriptSearch
                        ) {
                          submittedTranscriptSearchRef.current = normalizedTranscriptSearch;
                          setActiveTranscriptMatch(0);
                          const firstMatch = transcriptMatchIndices[0];
                          if (firstMatch !== undefined) {
                            scrollToTranscriptRow(firstMatch);
                          }
                        } else {
                          moveTranscriptMatch(event.shiftKey ? -1 : 1);
                        }
                      } else if (event.key === "Escape") {
                        submittedTranscriptSearchRef.current = "";
                        setTranscriptSearch("");
                      }
                    }}
                    placeholder={`${t("desktopSidebar.search.label")} ${t("desktopRecordingDetail.tabs.transcript").toLocaleLowerCase()}`}
                    aria-label={`${t("desktopSidebar.search.label")} ${t("desktopRecordingDetail.tabs.transcript")}`}
                    className="desktop-transcript-search-input min-w-0 flex-1 bg-transparent text-[11px] text-[#D7D7DB] outline-none placeholder:text-[#66666E] [&::-webkit-search-cancel-button]:hidden"
                  />
                  {normalizedTranscriptSearch && (
                    <>
                      <span className="shrink-0 text-[10px] tabular-nums text-[#777780]">
                        {transcriptMatchIndices.length === 0
                          ? "0/0"
                          : `${Math.min(activeTranscriptMatch, transcriptMatchIndices.length - 1) + 1}/${transcriptMatchIndices.length}`}
                      </span>
                      <button
                        type="button"
                        disabled={transcriptMatchIndices.length === 0}
                        onClick={() => moveTranscriptMatch(-1)}
                        className="rounded p-0.5 hover:bg-[#292955] hover:text-[#D6DCFF] disabled:opacity-30"
                        aria-label={t("desktopRecordingDetail.transcript.previousMatch")}
                      >
                        <ChevronUp size={12} />
                      </button>
                      <button
                        type="button"
                        disabled={transcriptMatchIndices.length === 0}
                        onClick={() => moveTranscriptMatch(1)}
                        className="rounded p-0.5 hover:bg-[#292955] hover:text-[#D6DCFF] disabled:opacity-30"
                        aria-label={t("desktopRecordingDetail.transcript.nextMatch")}
                      >
                        <ChevronDown size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          submittedTranscriptSearchRef.current = "";
                          setTranscriptSearch("");
                          transcriptSearchInputRef.current?.focus();
                        }}
                        className="rounded p-0.5 hover:bg-[#202024] hover:text-white"
                        aria-label={t("desktopRecordingDetail.transcript.clearSearch")}
                      >
                        <X size={12} />
                      </button>
                    </>
                  )}
                </div>
                <div ref={copyMenuRef} className="relative shrink-0">
                  <button
                    type="button"
                    disabled={showPaywall}
                    onClick={() => {
                      if (showPaywall) return;
                      setCopyMenuOpen((open) => !open);
                    }}
                    className="flex h-8 items-center gap-1.5 rounded-full px-3 text-[12px] font-medium text-[#85858E] transition hover:bg-[#202024] hover:text-white disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-[#85858E]"
                  >
                    {copied === "transcript" ? (
                      <Check size={13} />
                    ) : (
                      <Copy size={13} />
                    )}
                    {copied === "transcript"
                      ? t("desktopRecordingDetail.copy.copied")
                      : t("desktopRecordingDetail.copy.label")}
                    <ChevronDown size={12} />
                  </button>
                  {copyMenuOpen && !showPaywall && (
                    <div className="absolute right-0 top-10 z-30 w-52 overflow-hidden rounded-xl border border-[#303036] bg-[#19191C] p-1.5 shadow-[0_14px_34px_rgba(0,0,0,0.38)]">
                      {([
                        [
                          "source",
                          t("desktopRecordingDetail.copy.originalTranscript"),
                        ],
                        [
                          "target",
                          t("desktopRecordingDetail.copy.translatedTranscript"),
                        ],
                        ["both", t("desktopRecordingDetail.copy.both")],
                      ] as const).map(([mode, label]) => (
                        <button
                          key={mode}
                          type="button"
                          onMouseEnter={() => setCopyHighlight(mode)}
                          onMouseLeave={() => setCopyHighlight(null)}
                          onClick={() => void copyText("transcript", mode)}
                          className="flex w-full items-center rounded-lg px-3 py-2 text-left text-[11px] text-[#A0A0A8] transition hover:bg-[#292955] hover:text-[#D6DCFF]"
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            ) : (
              ghostCopyButton("notes", "ml-auto shrink-0")
            )}
          </div>

          {/* Observed for DOM churn so comment highlights re-anchor after any
              re-render of the notes or transcript. */}
          <div ref={detailBodyRef} className="mt-4">
            {activeTab === "my-notes" && (
              <div className="detail-notes-pad -mx-4 min-h-[320px]">
                {detail.permissions.editNotepad ? (
                  <NotionEditor
                    key={meetingId}
                    content={myNotesContent}
                    format={notepadFormat}
                    setContent={setMyNotesContent}
                    isActive
                  />
                ) : notepadFormat === "markdown" ? (
                  <div className="ProseMirror px-4 py-2 text-[14px] leading-7 text-[#C9C9CF]">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {myNotesContent || t("desktopRecordingDetail.empty.noPersonalNotes")}
                    </ReactMarkdown>
                  </div>
                ) : (
                  <div
                    className="ProseMirror px-4 py-2 text-[14px] leading-7 text-[#C9C9CF]"
                    dangerouslySetInnerHTML={{
                      __html:
                        myNotesContent ||
                        `<p>${t("desktopRecordingDetail.empty.noPersonalNotes")}</p>`,
                    }}
                  />
                )}
              </div>
            )}

            {activeTab === "enhanced" && (
              <div className="detail-notes-pad relative -mx-4 min-h-[320px]">
                {showPaywall ? (
                  <DesktopMeetingPaywallOverlay
                    variant="notes"
                    aiUsage={detail.aiUsage}
                    busy={actionLoading === "unblur"}
                    canUnblur={detail.permissions.unblur}
                    onUpgrade={() => openPricing("notes")}
                    onUseCredit={() => void handleUnblur()}
                  />
                ) : notesGenerationFailed ? (
                  <NotesGenerationFailedState
                    message={t("desktopRecordingDetail.status.notesGenerationFailed")}
                    retryLabel={t("desktopRecordingDetail.status.tryNotesAgain")}
                    busy={actionLoading === "retry-notes"}
                    canRetry={detail.notesGeneration?.canRetry === true}
                    onRetry={() => void retryNotes()}
                  />
                ) : !selectedNote && detail.status !== "ready" ? (
                  <GeneratingState
                    message={t(
                      detail.notesGeneration?.status === "queued"
                        ? "desktopRecordingDetail.status.notesQueued"
                        : detail.notesGeneration?.status === "retrying"
                          ? "desktopRecordingDetail.status.notesRetrying"
                          : "desktopRecordingDetail.status.generatingAiNotes",
                    )}
                  />
                ) : (
                  <EnhancedNotesEditor
                    key={selectedNote?.timestamp ?? "empty"}
                    note={selectedNote}
                    editable={detail.permissions.editEnhancedNotes}
                    canComment={
                      RECORDING_COMMENTS_ENABLED &&
                      detail.permissions.comment &&
                      !showPaywall
                    }
                    onComment={(anchor) => {
                      setSelectionDraft(null);
                      window.getSelection()?.removeAllRanges();
                      openComments(
                        { kind: "note", ...anchor },
                        rangeForCommentBase(anchor.id),
                      );
                    }}
                    onSave={saveEnhancedSection}
                  />
                )}
              </div>
            )}

            {activeTab === "transcript" && (
              <div ref={transcriptListRef} className="relative min-h-[240px] space-y-6">
                {showPaywall ? (
                  <DesktopMeetingPaywallOverlay
                    variant="transcript"
                    aiUsage={detail.aiUsage}
                    busy={actionLoading === "unblur"}
                    canUnblur={detail.permissions.unblur}
                    onUpgrade={() => openPricing("transcript")}
                    onUseCredit={() => void handleUnblur()}
                  />
                ) : rows.length === 0 ? (
                  detail.status !== "ready" || transcriptLoading ? (
                    <GeneratingState
                      message={
                        transcriptLoading && detail.status === "ready"
                          ? t("desktopRecordingDetail.status.loadingTranscript")
                          : t("desktopRecordingDetail.status.generatingTranscript")
                      }
                    />
                  ) : (
                    <p className="py-8 text-[12px] text-[#777780]">
                      {t("desktopRecordingDetail.empty.transcriptUnavailable")}
                    </p>
                  )
                ) : (
                  transcriptGroups.map((group) => {
                    const firstEntry = group.entries[0];
                    if (!firstEntry) return null;
                    return (
                      <article
                        key={firstEntry.row.sectionId}
                        className="space-y-1"
                      >
                        <div className="desktop-transcript-meta flex items-center pb-1 pt-1">
                          <span
                            contentEditable={detail.permissions.editTranscript}
                            suppressContentEditableWarning
                            onBlur={(event) =>
                              void updateTranscript(
                                firstEntry.index,
                                "speaker",
                                event.currentTarget.innerText,
                              )
                            }
                            className="desktop-transcript-editable -ml-0.5 rounded-md border border-[#303036] bg-[#19191C] px-2 py-0.5 text-[13px] font-semibold text-[#D7D7DB] outline-none transition-colors focus:bg-[#1A1A1D] focus:text-[#D7D7DB]"
                          >
                            {group.speaker}
                          </span>
                        </div>
                        <div className="space-y-1">
                          {group.entries.map(({ row, index }) => {
                            const offset = secondsFromMeetingStart(
                              detail.timestamp,
                              row.start,
                            );
                            return (
                              <div
                                key={row.sectionId}
                                data-transcript-row-index={index}
                                className={`desktop-transcript-segment group/transcript relative grid grid-cols-[52px_minmax(0,1fr)] items-start gap-1 rounded-xl transition-colors ${
                                  activeTranscriptRowIndex === index
                                    ? "transcript-search-row--active bg-[#292955]/55 ring-1 ring-[#5156FF]/50"
                                    : transcriptMatchIndices.includes(index)
                                      ? "transcript-search-row--match bg-[#202024]"
                                      : ""
                                }`}
                              >
                                <div className="relative flex h-9 items-center gap-1 text-[11px] text-[#777780]">
                                  <button
                                    type="button"
                                    onClick={() => hasRecording && seekTo(offset)}
                                    disabled={!hasRecording}
                                    className="recording-transcript-timecode rounded px-0.5 tabular-nums outline-none transition-colors hover:text-[#5156FF] focus-visible:ring-2 focus-visible:ring-[#5156FF]/40 disabled:cursor-default"
                                    aria-label={
                                      hasRecording
                                        ? t("desktopUi.recording.playTranscriptFrom", {
                                            time: formatTimecode(offset),
                                          })
                                        : undefined
                                    }
                                  >
                                    {formatTimecode(offset)}
                                  </button>
                                </div>
                                <div
                                  className={`desktop-transcript-line rounded-xl px-3 py-1 transition-colors group-hover/transcript:bg-[#151517] ${
                                    showBilingualTranscript
                                      ? "grid grid-cols-2 gap-10"
                                      : "block"
                                  }`}
                                >
                                  <p
                                    data-comment-base={`transcript:${row.sectionId}`}
                                    data-comment-kind="transcript"
                                    data-comment-label={`${group.speaker} · ${formatTimecode(offset)}`}
                                    contentEditable={detail.permissions.editTranscript}
                                    suppressContentEditableWarning
                                    onBlur={(event) =>
                                      void updateTranscript(
                                        index,
                                        "source",
                                        event.currentTarget.innerText,
                                      )
                                    }
                                    className={`desktop-transcript-editable desktop-transcript-source -mx-2 select-text rounded-lg px-2 text-[14px] leading-7 outline-none transition-colors focus:bg-[#1A1A1D] ${
                                      copyHighlight === "source" ||
                                      copyHighlight === "both"
                                        ? "bg-[#292955] text-[#D6DCFF]"
                                        : "text-[#C9C9CF]"
                                    }`}
                                  >
                                    {renderTranscriptSearchHighlight(row.source)}
                                  </p>
                                  {showBilingualTranscript && (
                                    <p
                                      // Its own anchor base: the translation has
                                      // different text and different offsets from
                                      // the source, so they cannot share one.
                                      data-comment-base={`transcript:${row.sectionId}:target`}
                                      data-comment-kind="transcript"
                                      data-comment-label={`${group.speaker} · ${formatTimecode(offset)}`}
                                      contentEditable={detail.permissions.editTranscript}
                                      suppressContentEditableWarning
                                      onBlur={(event) =>
                                        void updateTranscript(
                                          index,
                                          "target",
                                          event.currentTarget.innerText,
                                        )
                                      }
                                      className={`desktop-transcript-editable desktop-transcript-target -mx-2 select-text rounded-lg px-2 text-[14px] font-semibold leading-7 outline-none transition-colors focus:bg-[#1A1A1D] ${
                                        copyHighlight === "target" ||
                                        copyHighlight === "both"
                                          ? "bg-[#30233F] text-[#E3C7FF]"
                                          : "text-[#A0A0A8]"
                                      }`}
                                    >
                                      {renderTranscriptSearchHighlight(row.target)}
                                    </p>
                                  )}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      </article>
                    );
                  })
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      <div
        ref={floatStackRef}
        className="ask-float-fade pointer-events-none absolute inset-x-0 bottom-0 flex justify-center px-8 pb-6 pt-10"
      >
        <div className="pointer-events-auto flex w-full max-w-[1040px] flex-col gap-2">
          {statusMessage && (
            <div
              role="status"
              aria-live="polite"
              className="ask-float-bar flex min-h-9 w-full items-center gap-2 rounded-xl px-3 py-2 text-[11px] leading-5 text-[#A0A0A8]"
            >
              {actionLoading && (
                <Loader2
                  size={13}
                  className="shrink-0 animate-spin text-[#85858E]"
                  aria-hidden="true"
                />
              )}
              <span className="min-w-0 whitespace-normal break-words">
                {statusMessage}
              </span>
              <button
                type="button"
                onClick={() => setStatusMessage(null)}
                className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[#777780] transition hover:bg-[#202024] hover:text-[#D7D7DB]"
                aria-label={t("desktopRecordingDetail.dismissNotification")}
              >
                <X size={13} aria-hidden="true" />
              </button>
            </div>
          )}
          {hasRecording && !showPaywall && playerOpen && (
            <div className="ask-float-bar rounded-2xl px-3 py-2">
              <div className="relative h-8 w-full overflow-hidden">
                {audioUrl ? (
                  <>
                    {!waveformReady && (
                      <div className="absolute inset-0 z-10 flex items-center justify-center gap-2 text-[10px] text-[#777780]">
                        {!waveformError && (
                          <Loader2 size={12} className="animate-spin" />
                        )}
                        {waveformError
                          ? t("desktopRecordingDetail.player.waveformUnavailable")
                          : t("desktopRecordingDetail.player.loadingWaveform")}
                      </div>
                    )}
                    <div className={waveformReady ? "opacity-100" : "opacity-0"}>
                      <WavesurferPlayer
                        key={audioUrl}
                        height={30}
                        waveColor="#777780"
                        progressColor="#F1F1F3"
                        cursorColor="#5156FF"
                        cursorWidth={1}
                        barWidth={2}
                        barGap={2}
                        barRadius={2}
                        normalize
                        url={audioUrl}
                        onReady={(instance) => {
                          setWaveSurfer(instance);
                          setWaveformReady(true);
                          setWaveformError(false);
                          const duration = instance.getDuration();
                          const safeDuration =
                            Number.isFinite(duration) && duration > 0
                              ? duration
                              : 0;
                          setAudioDuration(safeDuration);
                          const pendingSeek = pendingSeekRef.current;
                          if (pendingSeek !== null && safeDuration > 0) {
                            const nextTime = Math.min(safeDuration, pendingSeek);
                            pendingSeekRef.current = null;
                            instance.seekTo(nextTime / safeDuration);
                            setAudioTime(nextTime);
                          } else {
                            setAudioTime(0);
                          }
                          if (playing || pendingSeek !== null) {
                            void instance.play();
                          }
                        }}
                        onTimeupdate={(instance) => {
                          const current = instance.getCurrentTime();
                          setAudioTime(Number.isFinite(current) ? current : 0);
                        }}
                        onPlay={() => setPlaying(true)}
                        onPause={() => setPlaying(false)}
                        onFinish={() => {
                          if (audioPlaylistIndex + 1 < audioPlaylist.length) {
                            const nextIndex = audioPlaylistIndex + 1;
                            setAudioPlaylistIndex(nextIndex);
                            setAudioUrl(audioPlaylist[nextIndex]);
                            setPlaying(true);
                          } else if (audioPart + 1 < detail.media.partCount) {
                            loadAudioPart(audioPart + 1);
                          } else {
                            setPlaying(false);
                            setAudioTime(0);
                          }
                        }}
                        onError={() => {
                          setWaveformReady(false);
                          setWaveformError(true);
                          setStatusMessage(
                            t(
                              "desktopRecordingDetail.status.couldNotLoadWaveform",
                            ),
                          );
                        }}
                      />
                    </div>
                  </>
                ) : (
                  <div className="h-[30px] w-full animate-pulse rounded bg-[#202024]" />
                )}
              </div>
              <div className="mt-1.5 flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => seekTo(audioTime - 10)}
                  className="flex h-7 w-7 items-center justify-center rounded-full bg-[#202024] text-[#C9C9CF] transition hover:bg-[#25252A]"
                >
                  <SkipBack size={12} />
                </button>
                <button
                  type="button"
                  disabled={!audioUrl}
                  onClick={() => setPlaying((value) => !value)}
                  className="flex h-8 w-8 items-center justify-center rounded-full bg-[#5156FF] text-white transition hover:bg-[#6266FF] disabled:opacity-50"
                >
                  {playing ? (
                    <Pause size={13} />
                  ) : (
                    <Play size={13} className="ml-0.5" />
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => seekTo(audioTime + 10)}
                  className="flex h-7 w-7 items-center justify-center rounded-full bg-[#202024] text-[#C9C9CF] transition hover:bg-[#25252A]"
                >
                  <SkipForward size={12} />
                </button>
                <span className="ml-1 text-[10px] tabular-nums text-[#777780]">
                  {formatTimecode(audioTime)} / {formatTimecode(audioDuration)}
                  {detail.media.partCount > 1
                    ? ` · ${t("desktopRecordingDetail.player.part", {
                        current: audioPart + 1,
                        total: detail.media.partCount,
                      })}`
                    : ""}
                </span>
                <button
                  type="button"
                  onClick={() => void downloadRecording()}
                  className="ml-auto flex h-7 items-center gap-1.5 rounded-lg border border-[#38383F] px-2.5 text-[11px] font-medium text-[#D7D7DB] transition hover:bg-[#202024]"
                >
                  <Download size={12} />
                  {t("desktopRecordingDetail.player.download")}
                </button>
              </div>
            </div>
          )}

          <div className="flex items-end gap-2">
            {hasRecording && !showPaywall && (
              <button
                type="button"
                onClick={() =>
                  setPlayerOpen((value) => {
                    // Remembered per meeting: some recordings get listened to
                    // every time they are opened, others never.
                    writePlayerOpen(meetingId, !value);
                    return !value;
                  })
                }
                className="ask-float-bar flex h-12 w-12 shrink-0 items-center justify-center gap-0.5 rounded-full text-[#F1F1F3] transition"
              >
                <AudioLines size={16} />
                <ChevronUp
                  size={12}
                  className={`text-[#85858E] transition-transform ${
                    playerOpen ? "rotate-180" : ""
                  }`}
                />
              </button>
            )}
            <div className="meeting-assistant-composer-shell ask-float-bar flex min-w-0 flex-1 items-center gap-1.5 rounded-full py-1.5 pl-3 pr-2">
              <input
                value={askInput}
                disabled={!detail.permissions.ask}
                onChange={(event) => setAskInput(event.target.value)}
                onFocus={() => openAssistantDraft(askInput)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") submitAsk();
                }}
                placeholder={
                  detail.permissions.ask
                    ? t("desktopRecordingDetail.ask.placeholder")
                    : t("desktopRecordingDetail.ask.unavailablePlaceholder")
                }
                className="ask-float-input h-9 min-w-0 flex-1 bg-transparent px-2 text-[13.5px] text-[#F1F1F3] outline-none disabled:opacity-50"
              />
              {askInput.trim() ? (
                <button
                  type="button"
                  onClick={submitAsk}
                  className="meeting-assistant-send-button flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-white"
                  aria-label={t("desktopChat.common.send")}
                >
                  <ArrowUp size={16} strokeWidth={2} />
                </button>
              ) : (
                detail.permissions.ask &&
                askPrompts.map(({ id, label }) => (
                  <button
                    key={id}
                    type="button"
                    onClick={() => openAssistantDraft(label)}
                    className="meeting-assistant-suggestion-pill flex h-9 shrink-0 items-center gap-1.5 rounded-full border border-[#38383F] px-3 text-[12px] font-medium text-[#D7D7DB] transition hover:bg-[#202024]"
                  >
                    <Zap size={11} className="text-[#85858E]" />
                    {label}
                  </button>
                ))
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Portalled to <body>: the coordinates come from getClientRects(), which
          are viewport-relative, but a `fixed` element resolves against the
          nearest ancestor with a transform/filter/contain rather than the
          viewport. Rendering in place pushed the bubble away from the text by
          whatever that ancestor's offset happened to be. */}
      {RECORDING_COMMENTS_ENABLED &&
        selectionDraft &&
        selectionDraft.anchor.id !== commentAnchor?.id &&
        typeof document !== "undefined" &&
        createPortal(
          <button
            type="button"
            style={{
              left: selectionDraft.left,
              top: selectionDraft.top,
              width: SELECTION_BUBBLE_SIZE,
              height: SELECTION_BUBBLE_SIZE,
            }}
            // Keep the selection alive: mousedown would otherwise collapse it
            // before the click handler runs.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              openComments(selectionDraft.anchor, selectionDraft.range);
              setSelectionDraft(null);
            }}
            title={t("desktopRecordingDetail.comments.addOnSelection")}
            aria-label={t("desktopRecordingDetail.comments.addOnSelection")}
            className="selection-comment-bubble fixed z-[3000] flex items-center justify-center rounded-full border border-[#3A3A42] bg-[#1B1B1F] text-[#D7D7DB] shadow-[0_10px_28px_rgba(0,0,0,0.45)] transition hover:border-[#5156FF] hover:text-white"
          >
            <MessageSquarePlus size={15} />
          </button>,
          document.body,
        )}

      {RECORDING_COMMENTS_ENABLED && commentsRailOpen && (
        <RecordingCommentsRail
          threads={commentThreads}
          loading={commentsLoading}
          activeAnchorId={commentAnchor?.id ?? null}
          canComment={Boolean(detail?.permissions.comment)}
          onSelectThread={(anchor) => openComments(anchor)}
          onCancelDraft={() => {
            setSelectionDraft(null);
            setCommentAnchor(null);
            setCommentAnchorRange(null);
            window.getSelection()?.removeAllRanges();
          }}
          onClose={() => {
            setCommentsRailOpen(false);
            setCommentAnchor(null);
            setCommentAnchorRange(null);
          }}
          onAdd={addComment}
          onToggleResolved={toggleCommentResolved}
          onDelete={deleteComment}
        />
      )}

      {transcriptCleanupOpen && (
        <TranscriptCleanupDialog
          meetingId={meetingId}
          meetingStart={detail.timestamp}
          onClose={() => setTranscriptCleanupOpen(false)}
          onApplied={handleTranscriptCleanupApplied}
        />
      )}

      {shareOpen && (
        <div
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 px-4 py-3 sm:px-6 sm:py-5"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setShareOpen(false);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") setShareOpen(false);
          }}
        >
          <div
            className={`detail-share-dialog w-full max-w-md rounded-2xl border border-[#303036] bg-[#151517] p-4 shadow-2xl sm:p-5 ${
              shareMode === "chat"
                ? "flex h-[calc(100%-1.5rem)] max-h-[820px] flex-col overflow-hidden sm:h-[calc(100%-2.5rem)]"
                : "max-h-[calc(100%-1.5rem)] overflow-y-auto sm:max-h-[calc(100%-2.5rem)]"
            }`}
            role="dialog"
            aria-modal="true"
            aria-labelledby="share-meeting-notes-title"
          >
            <div className="flex shrink-0 items-center justify-between">
              <div>
                <h2 id="share-meeting-notes-title" className="detail-share-dialog-title text-[15px] font-semibold text-[#F1F1F3]">
                  {t("desktopRecordingDetail.share.title")}
                </h2>
                <p className="detail-share-dialog-muted mt-1 text-[11px] text-[#777780]">
                  {shareMode === "people"
                    ? t("desktopRecordingDetail.share.body")
                    : shareMode === "chat"
                      ? t("desktopRecordingDetail.share.chatBody")
                      : t("desktopRecordingDetail.share.linkBody")}
                </p>
              </div>
              <button
                type="button"
                onClick={() => setShareOpen(false)}
                className="detail-share-dialog-close rounded-full p-1 text-[#777780] hover:bg-[#202024] hover:text-white"
                aria-label={t("desktopRecordingDetail.share.close")}
              >
                <X size={16} />
              </button>
            </div>
            <div
              className="detail-share-tabs mt-4 grid shrink-0 grid-cols-3 rounded-xl border border-[#38383F] bg-[#0F0F11] p-1"
              role="tablist"
              aria-label={t("desktopRecordingDetail.share.method")}
            >
              {(["people", "chat", "link"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  role="tab"
                  aria-selected={shareMode === mode}
                  onClick={() => {
                    setShareMode(mode);
                    setShareChatError(null);
                  }}
                  className={`detail-share-tab rounded-lg px-3 py-2 text-[11px] font-semibold capitalize transition ${
                    shareMode === mode
                      ? "detail-share-tab--selected bg-[#5156FF] text-white"
                      : "text-[#A0A0A8] hover:bg-[#202024] hover:text-white"
                  }`}
                >
                  {mode === "people" ? "email" : mode}
                </button>
              ))}
            </div>

            {shareMode === "people" && <>
            {shareRecipientEmails.length > 0 && (
              <div className="mt-4 space-y-1.5">
                {shareRecipientEmails.map((email) => (
                  <div
                    key={email}
                    className="detail-share-recipient flex items-center justify-between gap-3 rounded-xl bg-[#202024] px-3 py-2"
                  >
                    <span className="detail-share-recipient-email min-w-0 truncate text-[10px] text-[#A0A0A8]">
                      {email}
                    </span>
                    <select
                      value={(detail.sharedEditors ?? []).includes(email)
                        ? "edit"
                        : (detail.sharedCommenters ?? []).includes(email)
                          ? "comment"
                          : "view"}
                      disabled={actionLoading === "share"}
                      onChange={(event) =>
                        void updateSharePermission(
                          email,
                          event.target.value as RecordingSharePermission,
                        )
                      }
                      aria-label={t("desktopRecordingDetail.share.permissionFor", { email })}
                      className="detail-share-permission-select h-7 shrink-0 rounded-lg border border-[#38383F] bg-[#151517] px-2 text-[10px] text-[#D7D7DB] outline-none focus:border-[#5156FF]"
                    >
                      <option value="view">{t("desktopRecordingDetail.share.permissions.view")}</option>
                      <option value="comment">{t("desktopRecordingDetail.share.permissions.comment")}</option>
                      <option value="edit">{t("desktopRecordingDetail.share.permissions.edit")}</option>
                    </select>
                  </div>
                ))}
              </div>
            )}
            <div className="relative mt-4">
              <textarea
                autoFocus
                value={shareInput}
                onFocus={() => setShareSuggestionsOpen(true)}
                onBlur={() => setShareSuggestionsOpen(false)}
                onChange={(event) => {
                  setShareInput(event.target.value);
                  setShareSuggestionsOpen(true);
                }}
                placeholder={t("desktopRecordingDetail.share.emailPlaceholder")}
                className="detail-share-recipient-input min-h-[92px] w-full resize-none rounded-xl border border-[#38383F] bg-[#0F0F11] p-3 text-[12px] text-[#F1F1F3] outline-none focus:border-[#5156FF]"
              />
              <RecentPeopleSuggestions
                open={shareSuggestionsOpen}
                query={shareInput.split(/[\s,;]+/).filter(Boolean).pop() ?? ""}
                excludeEmails={[
                  ...shareRecipientEmails,
                  ...shareInput.split(/[\s,;]+/).filter(Boolean),
                ]}
                onSelect={(person) => appendShareRecipient(person.email)}
              />
            </div>
            <div className="detail-share-permission-group mt-3 grid grid-cols-3 rounded-xl border border-[#38383F] bg-[#0F0F11] p-1" role="radiogroup" aria-label={t("desktopRecordingDetail.share.permissionForNew")}>
              {(["view", "comment", "edit"] as const).map((permission) => (
                <button
                  key={permission}
                  type="button"
                  role="radio"
                  aria-checked={sharePermission === permission}
                  onClick={() => setSharePermission(permission)}
                  className={`detail-share-permission-option rounded-lg px-3 py-2 text-[11px] font-semibold transition ${
                    sharePermission === permission
                      ? "detail-share-permission-option--selected bg-[#5156FF] text-white"
                      : "text-[#A0A0A8] hover:bg-[#202024] hover:text-white"
                  }`}
                >
                  {t(`desktopRecordingDetail.share.permissions.${permission}`)}
                </button>
              ))}
            </div>
            <p className="detail-share-dialog-muted mt-1.5 text-[11px] leading-4 text-[#777780]">
              {t(`desktopRecordingDetail.share.permissionHints.${sharePermission}`)}
            </p>
            <label className="detail-share-chat-option mt-3 flex min-h-12 cursor-pointer items-start gap-2 rounded-lg border border-[#303036] bg-[#0F0F11] px-3 py-2.5">
              <input
                type="checkbox"
                checked={createShareChat}
                onChange={(event) => setCreateShareChat(event.target.checked)}
                className="detail-share-chat-checkbox mt-0.5 h-3.5 w-3.5 shrink-0 accent-[#6D28D9]"
              />
              <span className="min-w-0">
                <span className="detail-share-dialog-title block text-[11px] font-semibold text-[#E7E7EA]">
                  {t("desktopRecordingDetail.share.createGroupChat")}
                </span>
                <span className="detail-share-dialog-muted mt-0.5 block text-[10px] leading-4 text-[#777780]">
                  {t("desktopRecordingDetail.share.createGroupChatHint")}
                </span>
              </span>
            </label>
            <button
              type="button"
              disabled={!shareInput.trim() || actionLoading === "share"}
              onClick={() => void submitShare()}
              className="detail-share-submit mt-3 flex h-9 w-full items-center justify-center gap-2 rounded-full bg-[#5156FF] text-[12px] font-semibold text-white disabled:opacity-50"
            >
              {actionLoading === "share" && (
                <Loader2 size={13} className="animate-spin" />
              )}
              {t("desktopRecordingDetail.share.button")}
            </button>
            </>}

            {shareMode === "chat" && (
              <div className="mt-4 flex min-h-0 flex-1 flex-col">
                {detail.permissions.share ? (
                  <div className="mb-3">
                    <p className="detail-share-dialog-muted text-[10px] font-semibold uppercase tracking-wide text-[#777780]">
                      {t("desktopRecordingDetail.share.chatPermissionLabel")}
                    </p>
                    <div
                      className="detail-share-permission-group mt-2 grid grid-cols-3 rounded-xl border border-[#38383F] bg-[#0F0F11] p-1"
                      role="radiogroup"
                      aria-label={t("desktopRecordingDetail.share.chatPermissionLabel")}
                    >
                      {(["view", "comment", "edit"] as const).map((permission) => (
                        <button
                          key={permission}
                          type="button"
                          role="radio"
                          aria-checked={shareChatPermission === permission}
                          onClick={() => setShareChatPermission(permission)}
                          className={`detail-share-permission-option whitespace-nowrap rounded-lg px-3 py-2 text-[11px] font-semibold transition ${
                            shareChatPermission === permission
                              ? "detail-share-permission-option--selected bg-[#5156FF] text-white"
                              : "text-[#A0A0A8] hover:bg-[#202024] hover:text-white"
                          }`}
                        >
                          {t(`desktopRecordingDetail.share.permissions.${permission}`)}
                        </button>
                      ))}
                    </div>
                    <p className="detail-share-dialog-muted mt-1.5 text-[11px] leading-4 text-[#777780]">
                      {t("desktopRecordingDetail.share.chatPermissionNote")}
                    </p>
                  </div>
                ) : null}
                <label className="relative block">
                  <Search
                    size={14}
                    className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[#8C8C96]"
                  />
                  <input
                    autoFocus
                    value={shareChatQuery}
                    onChange={(event) => setShareChatQuery(event.target.value)}
                    placeholder={t("desktopRecordingDetail.share.searchChats")}
                    className="detail-share-recipient-input h-10 w-full rounded-xl border border-[#38383F] bg-[#0F0F11] pl-9 pr-3 text-[12px] text-[#F1F1F3] outline-none focus:border-[#5156FF]"
                  />
                </label>
                <div className="mt-3 min-h-0 flex-1 overflow-y-auto pr-1">
                  {detail.permissions.share ? (
                    newShareChatOpen ? (
                      <div className="detail-share-link-card mb-2 rounded-xl border border-[#303036] bg-[#0F0F11] p-3">
                        <p className="detail-share-dialog-title text-[11px] font-semibold text-[#E7E7EA]">
                          {t("desktopRecordingDetail.share.newGroupChat")}
                        </p>
                        <input
                          value={newShareChatTitle}
                          onChange={(event) => setNewShareChatTitle(event.target.value)}
                          placeholder={t("desktopRecordingDetail.share.newGroupChatNamePlaceholder")}
                          className="detail-share-recipient-input mt-2 h-9 w-full rounded-lg border border-[#38383F] bg-[#0F0F11] px-3 text-[12px] text-[#F1F1F3] outline-none focus:border-[#5156FF]"
                        />
                        <input
                          value={newShareChatEmails}
                          onChange={(event) => setNewShareChatEmails(event.target.value)}
                          placeholder={t("desktopRecordingDetail.share.emailPlaceholder")}
                          className="detail-share-recipient-input mt-2 h-9 w-full rounded-lg border border-[#38383F] bg-[#0F0F11] px-3 text-[12px] text-[#F1F1F3] outline-none focus:border-[#5156FF]"
                        />
                        <div className="mt-2 flex items-center gap-2">
                          <button
                            type="button"
                            disabled={
                              !newShareChatTitle.trim() ||
                              !newShareChatEmails.trim() ||
                              creatingShareChat ||
                              shareChatSending
                            }
                            onClick={() => void createShareChatAndShare()}
                            className="detail-share-submit flex h-8 flex-1 items-center justify-center gap-2 rounded-full bg-[#5156FF] px-3 text-[11px] font-semibold text-white disabled:opacity-50"
                          >
                            {creatingShareChat || shareChatSending ? (
                              <Loader2 size={13} className="animate-spin" />
                            ) : (
                              <UsersRound size={13} />
                            )}
                            {t("desktopRecordingDetail.share.createChatAndShare")}
                          </button>
                          <button
                            type="button"
                            onClick={() => setNewShareChatOpen(false)}
                            className="detail-share-link-button h-8 rounded-full border border-[#38383F] px-3 text-[11px] text-[#D7D7DB]"
                          >
                            {t("common.cancel")}
                          </button>
                        </div>
                      </div>
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          setNewShareChatOpen(true);
                          setNewShareChatTitle(shareChatQuery.trim() || detail.title);
                        }}
                        className="detail-share-chat-row mb-1 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition hover:bg-[#202024]"
                      >
                        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[#24242A] text-[#A5A7FF]">
                          <UsersRound size={14} />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="detail-share-dialog-title block truncate text-[11px] font-semibold">
                            {t("desktopRecordingDetail.share.newGroupChat")}
                          </span>
                          <span className="detail-share-dialog-muted mt-0.5 block text-[10px]">
                            {t("desktopRecordingDetail.share.newGroupChatHint")}
                          </span>
                        </span>
                      </button>
                    )
                  ) : null}
                  {shareChatsLoading ? (
                    <div className="detail-share-dialog-muted flex items-center justify-center gap-2 py-10 text-[11px]">
                      <Loader2 size={14} className="animate-spin" /> {t("desktopRecordingDetail.share.loadingChats")}
                    </div>
                  ) : shareChatError && !shareChatsLoaded ? (
                    <div className="py-8 text-center">
                      <p className="text-[11px] text-[#F07178]">{shareChatError}</p>
                      <button
                        type="button"
                        onClick={() => {
                          setShareChatError(null);
                          setShareChatsLoaded(false);
                          setShareChatsRequest((current) => current + 1);
                        }}
                        className="detail-share-link-button mt-3 rounded-lg border border-[#38383F] px-3 py-1.5 text-[10px]"
                      >
                        {t("common.tryAgain")}
                      </button>
                    </div>
                  ) : shareChats.filter((chat) =>
                      chat.title.toLocaleLowerCase().includes(shareChatQuery.trim().toLocaleLowerCase()),
                    ).length === 0 ? (
                    <p className="detail-share-dialog-muted py-10 text-center text-[11px]">{t("desktopRecordingDetail.share.noChats")}</p>
                  ) : (
                    shareChats
                      .filter((chat) =>
                        chat.title.toLocaleLowerCase().includes(shareChatQuery.trim().toLocaleLowerCase()),
                      )
                      .map((chat) => {
                        const selected = selectedShareChatId === chat.id;
                        return (
                          <button
                            key={chat.id}
                            type="button"
                            onClick={() => setSelectedShareChatId(chat.id)}
                            className={`detail-share-chat-row mb-1 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition ${
                              selected ? "detail-share-chat-row--selected bg-[#292955]" : "hover:bg-[#202024]"
                            }`}
                          >
                            <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${selected ? "bg-[#5156FF] text-white" : "bg-[#24242A] text-[#A5A7FF]"}`}>
                              {selected ? <Check size={14} /> : <MessageCircle size={14} />}
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="detail-share-dialog-title block truncate text-[11px] font-semibold">{chat.title}</span>
                              <span className="detail-share-dialog-muted mt-0.5 block text-[10px]">
                                {chat.memberCount ?? chat.members?.length ?? 0} members
                              </span>
                            </span>
                          </button>
                        );
                      })
                  )}
                </div>
                {shareChatError && shareChatsLoaded && (
                  <p className="mt-2 text-[11px] text-[#F07178]">{shareChatError}</p>
                )}
                <button
                  type="button"
                  disabled={!selectedShareChatId || shareChatSending}
                  onClick={() => void shareMeetingToExistingChat()}
                  className="detail-share-submit mt-3 flex h-9 w-full shrink-0 items-center justify-center gap-2 rounded-full bg-[#5156FF] text-[12px] font-semibold text-white disabled:opacity-50"
                >
                  {shareChatSending ? <Loader2 size={13} className="animate-spin" /> : <MessageCircle size={13} />}
                  {t(shareChatSending ? "desktopRecordingDetail.share.sharing" : "desktopRecordingDetail.share.shareToChat")}
                </button>
              </div>
            )}

            {shareMode === "link" && (
            <div className="detail-share-link-section mt-4">
              <div className="detail-share-link-card rounded-xl border border-[#303036] bg-[#0F0F11] p-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <p className="detail-share-dialog-title text-[12px] font-semibold text-[#E7E7EA]">{t("desktopRecordingDetail.share.linkTitle")}</p>
                    <p className="detail-share-dialog-muted mt-0.5 text-[11px] leading-4 text-[#777780]">{t("desktopRecordingDetail.share.linkDescription")}</p>
                  </div>
                  {detail.shareLink && (
                    <button
                      type="button"
                      disabled={shareLinkLoading}
                      onClick={() => void revokeShareLink()}
                      className="detail-share-link-secondary flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[10px] font-medium text-[#A0A0A8] hover:bg-[#202024] hover:text-white disabled:opacity-50"
                    >
                      <Unlink size={11} /> {t("desktopRecordingDetail.share.disableLink")}
                    </button>
                  )}
                </div>
                <button
                  type="button"
                  disabled={shareLinkLoading}
                  onClick={() => void copyShareLink()}
                  className="detail-share-link-button mt-2.5 flex h-9 w-full items-center justify-center gap-2 rounded-lg border border-[#38383F] bg-[#1A1A1D] text-[11px] font-semibold text-[#E7E7EA] transition hover:border-[#5156FF] hover:bg-[#202024] disabled:opacity-50"
                >
                  {shareLinkLoading ? <Loader2 size={13} className="animate-spin" /> : shareLinkCopied ? <Check size={13} /> : <Link2 size={13} />}
                  {t(shareLinkCopied ? "desktopRecordingDetail.share.linkCopied" : detail.shareLink ? "desktopRecordingDetail.share.copyLink" : "desktopRecordingDetail.share.createCopyLink")}
                </button>
              </div>
            </div>
            )}
          </div>
        </div>
      )}

      {assistantLaunch && (
        <DesktopMeetingAssistant
          key={`${assistantLaunch.meetingId}:${assistantLaunch.nonce}`}
          launch={assistantLaunch}
          overlay
          suggestions={askPrompts.map(({ label }) => label)}
          onClose={() => setAssistantLaunch(null)}
          onMaximize={
            onOpenAssistant
              ? (nextLaunch) => {
                  setAssistantLaunch(null);
                  onOpenAssistant(nextLaunch);
                }
              : undefined
          }
        />
      )}
    </div>
  );
}
