import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, Copy, Globe2, History, Loader2, RotateCcw, Search, Trash2 } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import {
  deleteVoiceTypingHistoryItem,
  deleteVoiceTypingHistoryItems,
  fetchVoiceTypingHistory,
  updateVoiceTypingHistoryText,
  type VoiceTypingHistoryItem,
} from "../../lib/dictationApi";
import { groupItemsByDate, toLocalDateValue } from "../../utils/dateGroups";
import { languages, getLocalizedLanguageLabel, toBcp47Locale } from "../../utils/languages";
import {
  readVoiceKeyboardMode,
  readVoiceKeyboardTargetLanguage,
  writeVoiceKeyboardTargetLanguage,
  writeVoiceKeyboardMode,
  type VoiceKeyboardMode,
} from "../../utils/voiceKeyboardPreferences";
import { DesktopConfirmDialog } from "./chat/DesktopConfirmDialog";

const PAGE_SIZE = 30;

function VoiceTypingSkeleton() {
  return (
    <div className="animate-pulse space-y-3" aria-hidden="true">
      {Array.from({ length: 5 }, (_, index) => (
        <div key={index} className="flex gap-5 rounded-2xl border border-[#303036] bg-[#151517] px-5 py-5">
          <span className="h-3 w-14 rounded bg-[#26262B]" />
          <span className="min-w-0 flex-1 space-y-2">
            <span className="block h-3.5 w-[78%] rounded bg-[#26262B]" />
            <span className="block h-3 w-[48%] rounded bg-[#202024]" />
          </span>
        </div>
      ))}
    </div>
  );
}

export function DesktopVoiceTyping() {
  const { t, currentLanguage } = useTranslation();
  const locale = toBcp47Locale(currentLanguage);
  const editableElementsRef = useRef<Record<string, HTMLParagraphElement | null>>({});
  const [items, setItems] = useState<VoiceTypingHistoryItem[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [expandedIds, setExpandedIds] = useState<string[]>([]);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [savingKeys, setSavingKeys] = useState<string[]>([]);
  const [editErrorKey, setEditErrorKey] = useState<string | null>(null);
  const [deletingIds, setDeletingIds] = useState<string[]>([]);
  const [deleteErrorId, setDeleteErrorId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<VoiceTypingHistoryItem | null>(null);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [bulkDeleteOpen, setBulkDeleteOpen] = useState(false);
  const [bulkDeleteError, setBulkDeleteError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState(false);
  const [voiceKeyboardMode, setVoiceKeyboardMode] = useState<VoiceKeyboardMode>(
    readVoiceKeyboardMode,
  );

  const [targetLanguage, setTargetLanguage] = useState(readVoiceKeyboardTargetLanguage);

  const voiceKeyboardOptions = useMemo(
    () => [
      {
        value: "direct" as const,
        label: t("desktopSettings.general.voiceKeyboard.directLabel"),
        description: t("desktopSettings.general.voiceKeyboard.directDescription"),
      },
      {
        value: "curate" as const,
        label: t("desktopSettings.general.voiceKeyboard.curateLabel"),
        description: t("desktopSettings.general.voiceKeyboard.curateDescription"),
      },
      {
        value: "translate" as const,
        label: t("desktopVoiceTyping.translateLabel"),
        description: t("desktopVoiceTyping.translateDescription"),
      },
    ],
    [t],
  );

  const updateVoiceKeyboardMode = (mode: VoiceKeyboardMode) => {
    setVoiceKeyboardMode(mode);
    writeVoiceKeyboardMode(mode);
  };

  const loadInitial = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(false);
    try {
      const result = await fetchVoiceTypingHistory({ limit: PAGE_SIZE }, signal);
      setItems(result.items);
      setNextCursor(result.nextCursor);
    } catch (loadError) {
      if ((loadError as { name?: string })?.name === "AbortError") return;
      setItems([]);
      setNextCursor(null);
      setError(true);
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void loadInitial(controller.signal);
    return () => controller.abort();
  }, [loadInitial]);

  const loadMore = async () => {
    if (!nextCursor || loadingMore) return;
    setLoadingMore(true);
    setError(false);
    try {
      const result = await fetchVoiceTypingHistory({ cursor: nextCursor, limit: PAGE_SIZE });
      setItems((current) => {
        const knownIds = new Set(current.map((item) => item.id));
        return [...current, ...result.items.filter((item) => !knownIds.has(item.id))];
      });
      setNextCursor(result.nextCursor);
    } catch {
      setError(true);
    } finally {
      setLoadingMore(false);
    }
  };

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visibleItems = useMemo(
    () => normalizedQuery
      ? items.filter((item) => (
          item.formattedText.toLocaleLowerCase().includes(normalizedQuery)
          || item.transcript.toLocaleLowerCase().includes(normalizedQuery)
        ))
      : items,
    [items, normalizedQuery],
  );
  const visibleItemIds = useMemo(
    () => visibleItems.map((item) => item.id),
    [visibleItems],
  );
  const allVisibleSelected = visibleItemIds.length > 0
    && visibleItemIds.every((id) => selectedIds.includes(id));

  useEffect(() => {
    const visibleIds = new Set(visibleItemIds);
    setSelectedIds((current) => current.filter((id) => visibleIds.has(id)));
  }, [visibleItemIds]);
  const groups = useMemo(
    () => groupItemsByDate(
      visibleItems,
      (item) => toLocalDateValue(item.createdAt),
      locale,
      t("desktopVoiceTyping.today"),
    ),
    [locale, t, visibleItems],
  );
  const timeFormatter = useMemo(
    () => new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }),
    [locale],
  );

  const toggleOriginal = (id: string) => {
    setExpandedIds((current) => current.includes(id)
      ? current.filter((itemId) => itemId !== id)
      : [...current, id]);
  };

  const copyText = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      window.setTimeout(() => setCopiedKey((current) => current === key ? null : current), 1500);
    } catch {
      setCopiedKey(null);
    }
  };

  const copyEditableText = (key: string, fallback: string) => {
    const visibleText = editableElementsRef.current[key]?.innerText.trim();
    return copyText(key, visibleText || fallback);
  };

  const saveEditedText = async (
    item: VoiceTypingHistoryItem,
    element: HTMLParagraphElement,
    field: "formattedText" | "transcript",
  ) => {
    const text = element.innerText.trim();
    const previousText = item[field];
    const editKey = `${item.id}:${field}`;
    if (!text || text === previousText.trim()) {
      element.innerText = previousText;
      return;
    }

    setEditErrorKey(null);
    setSavingKeys((current) => current.includes(editKey) ? current : [...current, editKey]);
    try {
      const updated = await updateVoiceTypingHistoryText(item.id, { [field]: text });
      setItems((current) => current.map((historyItem) => (
        historyItem.id === item.id ? updated : historyItem
      )));
      element.innerText = updated[field];
    } catch {
      element.innerText = previousText;
      setEditErrorKey(editKey);
    } finally {
      setSavingKeys((current) => current.filter((key) => key !== editKey));
    }
  };

  const deleteItem = async (item: VoiceTypingHistoryItem) => {
    if (deletingIds.includes(item.id)) return;

    setDeleteErrorId(null);
    setDeletingIds((current) => [...current, item.id]);
    try {
      await deleteVoiceTypingHistoryItem(item.id);
      setItems((current) => current.filter((historyItem) => historyItem.id !== item.id));
      setExpandedIds((current) => current.filter((itemId) => itemId !== item.id));
      setCopiedKey((current) => current?.startsWith(`${item.id}:`) ? null : current);
      setEditErrorKey((current) => current?.startsWith(`${item.id}:`) ? null : current);
      setDeleteTarget(null);
    } catch {
      setDeleteErrorId(item.id);
    } finally {
      setDeletingIds((current) => current.filter((itemId) => itemId !== item.id));
    }
  };

  const exitSelectionMode = () => {
    setSelectionMode(false);
    setSelectedIds([]);
    setBulkDeleteOpen(false);
    setBulkDeleteError(false);
  };

  const toggleSelectedItem = (id: string) => {
    setSelectedIds((current) => current.includes(id)
      ? current.filter((itemId) => itemId !== id)
      : [...current, id]);
  };

  const toggleSelectAll = () => {
    setSelectedIds(allVisibleSelected ? [] : visibleItemIds);
  };

  const deleteSelectedItems = async () => {
    if (selectedIds.length === 0 || deletingIds.length > 0) return;
    const ids = [...selectedIds];
    const idSet = new Set(ids);
    setBulkDeleteError(false);
    setDeletingIds(ids);
    try {
      await deleteVoiceTypingHistoryItems(ids);
      setItems((current) => current.filter((item) => !idSet.has(item.id)));
      setExpandedIds((current) => current.filter((id) => !idSet.has(id)));
      setCopiedKey((current) => current && idSet.has(current.split(":", 1)[0]!) ? null : current);
      setEditErrorKey((current) => current && idSet.has(current.split(":", 1)[0]!) ? null : current);
      exitSelectionMode();
    } catch {
      setBulkDeleteError(true);
    } finally {
      setDeletingIds([]);
    }
  };

  return (
    <div className="h-full overflow-hidden bg-[#0B0B0D] text-[#F5F5F7]">
      <main className="h-full overflow-y-auto">
        <div className="mx-auto w-full max-w-[980px] px-7 pb-16 pt-7">
          <header className="flex flex-wrap items-start justify-between gap-5 border-b border-[#2D2D32] pb-6">
            <div>
              <h1 className="text-[28px] font-light tracking-[-0.025em] text-[#F1F1F3]">
                {t("desktopVoiceTyping.title")}
              </h1>
              <p className="mt-1.5 text-sm text-[#8D8D96]">{t("desktopVoiceTyping.description")}</p>
              <p className="mt-2 flex items-center gap-1.5 text-xs text-[#777780]">
                <History size={13} />
                {t("desktopVoiceTyping.privacy")}
              </p>
            </div>
            <div className="grid w-full max-w-[440px] gap-4">
              <div className="min-w-0">
                <p
                  id="voice-typing-mode-label"
                  className="voice-typing-setting-label mb-2 text-left text-xs font-medium leading-4 text-[#8D8D96]"
                >
                  {t("desktopSettings.general.voiceKeyboard.modeAria")}
                </p>
                <div
                  role="radiogroup"
                  aria-labelledby="voice-typing-mode-label"
                  aria-describedby="voice-typing-mode-description"
                  className="voice-typing-mode grid min-h-[42px] w-full max-w-[480px] grid-cols-3 gap-1 rounded-xl border border-[#303036] bg-[#0B0B0D] p-1"
                >
                  {voiceKeyboardOptions.map((option) => {
                    const selected = voiceKeyboardMode === option.value;
                    return (
                      <button
                        key={option.value}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        title={option.description}
                        onClick={() => updateVoiceKeyboardMode(option.value)}
                        className={`voice-typing-mode-option min-w-0 rounded-lg px-2 py-1.5 text-xs font-semibold leading-5 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[#8589FF] ${
                          selected
                            ? "voice-typing-mode-option--selected bg-[#292955] text-[#F1F1F3]"
                            : "text-[#8D8D96] hover:bg-[#202024] hover:text-[#D7D7DB]"
                        }`}
                      >
                        {option.label}
                      </button>
                    );
                  })}
                </div>
                <p id="voice-typing-mode-description" className="voice-typing-mode-description mt-2 max-w-[480px] text-xs leading-5 text-[#777780]">
                  {voiceKeyboardOptions.find((option) => option.value === voiceKeyboardMode)?.description}
                </p>
              </div>
              {voiceKeyboardMode === "translate" ? (
                <label className="voice-typing-setting-label block min-w-0 text-left text-xs font-medium leading-4 text-[#8D8D96]">
                  {t("desktopVoiceTyping.outputLanguage")}
                  <span className="relative mt-2 block">
                    <Globe2 size={15} aria-hidden="true" className="voice-typing-language-icon pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[#85858E]" />
                    <select
                      value={targetLanguage}
                      onChange={(event) => {
                        setTargetLanguage(event.target.value);
                        writeVoiceKeyboardTargetLanguage(event.target.value);
                      }}
                      className="voice-typing-language-select block h-[42px] w-full cursor-pointer appearance-none rounded-xl border border-[#3A3A40] bg-[#202024] pl-9 pr-9 text-sm font-normal text-[#D7D7DB] outline-none transition-colors hover:border-[#66666F] focus-visible:border-[#8589FF] focus-visible:ring-2 focus-visible:ring-[#8589FF]/25"
                    >
                      {languages.map((language) => (
                        <option key={language.key} value={language.key}>
                          {getLocalizedLanguageLabel(language, currentLanguage)}
                        </option>
                      ))}
                    </select>
                    <ChevronDown size={15} aria-hidden="true" className="voice-typing-language-icon pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[#85858E]" />
                  </span>
                </label>
              ) : null}
            </div>
          </header>

          <div className="mt-5 flex items-center gap-2.5">
            <label className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-xl border border-[#303036] bg-[#151517] px-3 text-[#777780]">
              <Search size={14} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("desktopVoiceTyping.search")}
                className="min-w-0 flex-1 border-0 bg-transparent text-xs text-[#D7D7DB] outline-none focus:ring-0 placeholder:text-[#66666F]"
              />
            </label>
            {!loading && items.length > 0 && !selectionMode ? (
              <button
                type="button"
                onClick={() => setSelectionMode(true)}
                className="desktop-history-select-button inline-flex h-9 shrink-0 items-center rounded-xl border px-3 text-xs font-semibold transition-colors"
              >
                {t("desktopVoiceTyping.select")}
              </button>
            ) : null}
          </div>

          <div className="mt-7">
            {selectionMode ? (
              <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[#303036] bg-[#151517] px-4 py-3">
                <label className="flex items-center gap-2 text-xs text-[#D7D7DB]">
                  <input
                    type="checkbox"
                    checked={allVisibleSelected}
                    onChange={toggleSelectAll}
                    disabled={visibleItemIds.length === 0}
                    className="h-4 w-4 rounded border-[#3A3A40] bg-[#111113] text-[#5156FF] focus:ring-[#5156FF]"
                  />
                  {t("desktopVoiceTyping.selectAll")}
                </label>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={exitSelectionMode}
                    className="desktop-selection-neutral-button rounded-lg px-3 py-2 text-xs font-semibold"
                  >
                    {t("desktopVoiceTyping.cancel")}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setBulkDeleteError(false);
                      setBulkDeleteOpen(true);
                    }}
                    disabled={selectedIds.length === 0}
                    className={`rounded-lg px-3 py-2 text-xs font-semibold shadow transition ${
                      selectedIds.length === 0
                        ? "desktop-selection-disabled-button cursor-not-allowed"
                        : "chat-btn-danger"
                    }`}
                  >
                    {t("desktopVoiceTyping.deleteSelected", { count: selectedIds.length })}
                  </button>
                </div>
              </div>
            ) : null}
            {loading ? (
              <VoiceTypingSkeleton />
            ) : error && items.length === 0 ? (
              <div className="rounded-2xl border border-[#303036] bg-[#151517] px-6 py-14 text-center">
                <p className="text-sm font-medium text-[#D7D7DB]">{t("desktopVoiceTyping.error")}</p>
                <button
                  type="button"
                  onClick={() => void loadInitial()}
                  className="mt-4 rounded-xl bg-[#5156FF] px-4 py-2 text-xs font-semibold text-white hover:opacity-90"
                >
                  {t("desktopVoiceTyping.retry")}
                </button>
              </div>
            ) : groups.length === 0 ? (
              <div className="rounded-2xl border border-[#303036] bg-[#151517] px-6 py-14 text-center">
                <p className="text-sm font-medium text-[#D7D7DB]">
                  {query.trim() ? t("desktopVoiceTyping.noResults") : t("desktopVoiceTyping.empty")}
                </p>
                <p className="mt-1 text-xs text-[#777780]">
                  {query.trim() ? t("desktopVoiceTyping.noResultsHint") : t("desktopVoiceTyping.emptyHint")}
                </p>
              </div>
            ) : (
              <>
                {groups.map((group, groupIndex) => (
                  <section key={group.dateValue} className={groupIndex ? "mt-9" : ""}>
                    <h2 className="mb-3 text-xs font-semibold uppercase tracking-[0.12em] text-[#777780]">
                      {group.label}
                    </h2>
                    <div className="overflow-hidden rounded-2xl border border-[#303036] bg-[#151517]">
                      {group.items.map((item) => {
                        const isCurated = item.mode === "curate";
                        const isTranslated = item.mode === "translate";
                        const canShowOriginal = isCurated || isTranslated;
                        const expanded = expandedIds.includes(item.id);
                        const finalEditKey = `${item.id}:formattedText`;
                        const originalEditKey = `${item.id}:transcript`;
                        const saving = savingKeys.includes(finalEditKey);
                        const originalSaving = savingKeys.includes(originalEditKey);
                        const deleting = deletingIds.includes(item.id);
                        const selected = selectedIds.includes(item.id);
                        return (
                          <article
                            key={item.id}
                            className={`border-b border-[#2C2C31] last:border-b-0 ${selected ? "bg-[#292955]/40" : ""}`}
                          >
                            <div className="flex items-start gap-5 px-5 py-5">
                              {selectionMode ? (
                                <input
                                  type="checkbox"
                                  checked={selected}
                                  onChange={() => toggleSelectedItem(item.id)}
                                  aria-label={t("desktopVoiceTyping.selectItem")}
                                  className="mt-0.5 h-4 w-4 shrink-0 rounded border-[#3A3A40] bg-[#111113] text-[#5156FF] focus:ring-[#5156FF]"
                                />
                              ) : null}
                              <time className="w-[72px] shrink-0 pt-0.5 text-xs text-[#777780]">
                                {timeFormatter.format(new Date(item.createdAt))}
                              </time>
                              <div className="min-w-0 flex-1">
                                <p
                                  ref={(element) => {
                                    editableElementsRef.current[finalEditKey] = element;
                                  }}
                                  contentEditable={!selectionMode && !saving && !deleting}
                                  suppressContentEditableWarning
                                  role="textbox"
                                  aria-multiline="true"
                                  aria-label={t("desktopVoiceTyping.editAria")}
                                  title={t("desktopVoiceTyping.editHint")}
                                  onFocus={() => setEditErrorKey(null)}
                                  onKeyDown={(event) => {
                                    if (event.key === "Escape") {
                                      event.currentTarget.innerText = item.formattedText;
                                      event.currentTarget.blur();
                                    }
                                    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                                      event.preventDefault();
                                      event.currentTarget.blur();
                                    }
                                  }}
                                  onBlur={(event) => void saveEditedText(item, event.currentTarget, "formattedText")}
                                  className={`voice-typing-entry-text -m-2 cursor-text whitespace-pre-wrap rounded-lg p-2 text-sm leading-6 outline-none transition ${saving ? "opacity-60" : ""}`}
                                >
                                  {item.formattedText}
                                </p>
                              </div>
                              {!selectionMode ? (
                              <div className="-mt-3 flex shrink-0 flex-col items-end gap-0.5">
                                <div className="flex items-center gap-1">
                                  <button
                                    type="button"
                                    onClick={() => void copyEditableText(finalEditKey, item.formattedText)}
                                    aria-label={t("desktopVoiceTyping.copy")}
                                    title={t("desktopVoiceTyping.copy")}
                                    className="flex h-9 w-9 items-center justify-center rounded-lg text-[#85858E] hover:bg-[#202024] hover:text-white"
                                  >
                                    {copiedKey === finalEditKey ? <Check size={16} /> : <Copy size={16} />}
                                  </button>
                                  {canShowOriginal ? (
                                    <button
                                      type="button"
                                      onClick={() => toggleOriginal(item.id)}
                                      aria-expanded={expanded}
                                      className="flex h-9 items-center gap-1.5 rounded-lg px-2.5 text-xs font-medium text-[#A5A7FF] hover:bg-[#202024]"
                                    >
                                      <RotateCcw size={14} />
                                      {expanded ? t("desktopVoiceTyping.hideOriginal") : t("desktopVoiceTyping.viewOriginal")}
                                      <ChevronDown size={14} className={`transition-transform ${expanded ? "rotate-180" : ""}`} />
                                    </button>
                                  ) : null}
                                  <button
                                    type="button"
                                    onMouseDown={(event) => event.preventDefault()}
                                    onClick={() => {
                                      setDeleteErrorId(null);
                                      setDeleteTarget(item);
                                    }}
                                    disabled={deleting}
                                    aria-label={t("desktopVoiceTyping.delete")}
                                    title={t("desktopVoiceTyping.delete")}
                                    className="voice-typing-delete-button flex h-9 w-9 items-center justify-center rounded-lg disabled:cursor-wait disabled:opacity-60"
                                  >
                                    {deleting ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
                                  </button>
                                </div>
                                <div className={`flex flex-wrap items-center justify-start gap-1.5 ${canShowOriginal ? "w-[calc(100%-40px)]" : "w-full"}`}>
                                  <span className={`file-tag rounded-md border px-1.5 py-px text-[10px] font-semibold ${
                                    isCurated
                                      ? "border-[#5156FF]/40 bg-[#292955] text-[#A5A7FF]"
                                      : "border-[#38383F] bg-[#202024] text-[#A0A0A8]"
                                  }`}>
                                    {isTranslated
                                      ? t("desktopVoiceTyping.translated")
                                      : isCurated ? t("desktopVoiceTyping.curated") : t("desktopVoiceTyping.direct")}
                                    {isTranslated && item.targetLanguage ? ` · ${getLocalizedLanguageLabel(
                                      languages.find((language) => language.key === item.targetLanguage)
                                        ?? { key: item.targetLanguage, value: item.targetLanguage }, currentLanguage,
                                    )}` : ""}
                                  </span>
                                  <span className="text-[11px] text-[#777780]">
                                    {t("desktopVoiceTyping.duration", { seconds: item.durationSeconds })}
                                  </span>
                                  {saving ? (
                                    <span role="status" className="text-[11px] text-[#777780]">
                                      {t("desktopVoiceTyping.saving")}
                                    </span>
                                  ) : null}
                                  {editErrorKey === finalEditKey ? (
                                    <span role="alert" className="text-[11px] text-red-400">
                                      {t("desktopVoiceTyping.saveError")}
                                    </span>
                                  ) : null}
                                  {deleteErrorId === item.id ? (
                                    <span role="alert" className="text-[11px] text-red-400">
                                      {t("desktopVoiceTyping.deleteError")}
                                    </span>
                                  ) : null}
                                </div>
                              </div>
                              ) : null}
                            </div>
                            {!selectionMode && expanded && canShowOriginal ? (
                              <div className="relative border-t border-[#2C2C31] bg-[#202024] px-5 py-3 pl-[117px]">
                                <div className="mb-1">
                                  <p className="text-[10px] font-bold uppercase tracking-[0.12em] text-[#85858E]">
                                    {t("desktopVoiceTyping.original")}
                                  </p>
                                  <button
                                    type="button"
                                    onClick={() => void copyEditableText(originalEditKey, item.transcript)}
                                    aria-label={t("desktopVoiceTyping.copyOriginal")}
                                    title={t("desktopVoiceTyping.copyOriginal")}
                                    className="absolute right-5 top-2 flex h-7 w-7 items-center justify-center rounded-lg text-[#85858E] hover:bg-[#151517] hover:text-white"
                                  >
                                    {copiedKey === originalEditKey ? <Check size={15} /> : <Copy size={15} />}
                                  </button>
                                </div>
                                <p
                                  ref={(element) => {
                                    editableElementsRef.current[originalEditKey] = element;
                                  }}
                                  contentEditable={!originalSaving && !deleting}
                                  suppressContentEditableWarning
                                  role="textbox"
                                  aria-multiline="true"
                                  aria-label={t("desktopVoiceTyping.editOriginalAria")}
                                  title={t("desktopVoiceTyping.editHint")}
                                  onFocus={() => setEditErrorKey(null)}
                                  onKeyDown={(event) => {
                                    if (event.key === "Escape") {
                                      event.currentTarget.innerText = item.transcript;
                                      event.currentTarget.blur();
                                    }
                                    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                                      event.preventDefault();
                                      event.currentTarget.blur();
                                    }
                                  }}
                                  onBlur={(event) => void saveEditedText(item, event.currentTarget, "transcript")}
                                  className={`voice-typing-original-text -m-1.5 cursor-text whitespace-pre-wrap rounded-lg p-1.5 text-sm leading-5 outline-none transition ${originalSaving ? "opacity-60" : ""}`}
                                >
                                  {item.transcript}
                                </p>
                                {originalSaving ? (
                                  <p role="status" className="mt-1 text-[11px] text-[#777780]">
                                    {t("desktopVoiceTyping.saving")}
                                  </p>
                                ) : null}
                                {editErrorKey === originalEditKey ? (
                                  <p role="alert" className="mt-1 text-[11px] text-red-400">
                                    {t("desktopVoiceTyping.saveError")}
                                  </p>
                                ) : null}
                              </div>
                            ) : null}
                          </article>
                        );
                      })}
                    </div>
                  </section>
                ))}

                {nextCursor ? (
                  <div className="mt-7 text-center">
                    <button
                      type="button"
                      onClick={() => void loadMore()}
                      disabled={loadingMore}
                      className="rounded-xl border border-[#38383F] bg-[#151517] px-4 py-2.5 text-xs font-semibold text-[#D7D7DB] hover:bg-[#202024] disabled:cursor-wait disabled:opacity-60"
                    >
                      {loadingMore ? t("desktopVoiceTyping.loadingMore") : t("desktopVoiceTyping.loadMore")}
                    </button>
                  </div>
                ) : null}
                {error && items.length > 0 ? (
                  <p className="mt-4 text-center text-xs text-red-400">{t("desktopVoiceTyping.loadMoreError")}</p>
                ) : null}
              </>
            )}
          </div>
        </div>
      </main>
      {deleteTarget ? (
        <DesktopConfirmDialog
          title={t("desktopVoiceTyping.deleteTitle")}
          description={(
            <>
              <p>
                {t("desktopVoiceTyping.deleteDescription")} {t("desktopVoiceTyping.deleteIrreversible")}
              </p>
              {deleteErrorId === deleteTarget.id ? (
                <p role="alert" className="mt-2 text-xs text-red-400">
                  {t("desktopVoiceTyping.deleteError")}
                </p>
              ) : null}
            </>
          )}
          confirmLabel={deletingIds.includes(deleteTarget.id)
            ? t("desktopVoiceTyping.deleting")
            : t("desktopVoiceTyping.deleteButton")}
          cancelLabel={t("desktopVoiceTyping.cancel")}
          danger
          onConfirm={() => void deleteItem(deleteTarget)}
          onCancel={() => {
            if (deletingIds.includes(deleteTarget.id)) return;
            setDeleteTarget(null);
            setDeleteErrorId(null);
          }}
        />
      ) : null}
      {bulkDeleteOpen ? (
        <DesktopConfirmDialog
          title={t("desktopVoiceTyping.bulkDeleteTitle")}
          description={(
            <>
              <p>
                {t("desktopVoiceTyping.bulkDeleteDescription", { count: selectedIds.length })}{" "}
                {t("desktopVoiceTyping.deleteIrreversible")}
              </p>
              {bulkDeleteError ? (
                <p role="alert" className="mt-2 text-xs text-red-400">
                  {t("desktopVoiceTyping.bulkDeleteError")}
                </p>
              ) : null}
            </>
          )}
          confirmLabel={deletingIds.length > 0
            ? t("desktopVoiceTyping.deleting")
            : t("desktopVoiceTyping.deleteButton")}
          cancelLabel={t("desktopVoiceTyping.cancel")}
          danger
          onConfirm={() => void deleteSelectedItems()}
          onCancel={() => {
            if (deletingIds.length > 0) return;
            setBulkDeleteOpen(false);
            setBulkDeleteError(false);
          }}
        />
      ) : null}
    </div>
  );
}
