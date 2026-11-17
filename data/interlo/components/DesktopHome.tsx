import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  CalendarClock,
  ChevronLeft,
  ChevronRight,
  Gauge,
  Link2,
  Languages,
  Loader2,
  RefreshCw,
  Settings as SettingsIcon,
  Unplug,
  Volume2,
} from "lucide-react";
import {
  beginGoogleCalendarConnect,
  disconnectGoogleCalendar,
  fetchGoogleCalendar,
  type CalendarPanelPayload,
} from "../../lib/googleCalendarApi";
import { useTranslation } from "../../hooks/useTranslation";
import { useAppSelector } from "../../redux/hooks";
import { toBcp47Locale } from "../../utils/languages";
import {
  type CalendarNoteContext,
  type LiveSessionLaunchPanel,
} from "./calendarNoteContext";
import { getUpcomingCalendarPagination } from "./calendarPagination";
import type { RecordingOpenContext } from "../../lib/recordingsApi";
import type { TranslateHistoryItem } from "../../lib/translateHistoryApi";
import {
  OperationalContext,
  RecentChats,
  VoiceKeyboardCard,
  Recordings,
  TranslatedFiles,
  type OpenChatHandler,
} from "./home-components";

interface DesktopHomeProps {
  firstName?: string;
  isLiveMeeting?: boolean;
  onOpenMeetingMemo: (calendarEvent: CalendarNoteContext) => void;
  onConfigureMeeting: (
    calendarEvent: CalendarNoteContext,
    panel: LiveSessionLaunchPanel,
  ) => void;
  onOpenChat: OpenChatHandler;
  onOpenVoiceTyping: () => void;
  onOpenWebApp: () => void;
  onViewAllRecordings: () => void;
  onStartRecordingsTutorial: () => void;
  onViewAllTranslatedFiles: () => void;
  onStartTranslation: () => void;
  onOpenTranslatedFile: (item: TranslateHistoryItem) => void;
  onOpenMeeting: (
    meetingId: string,
    title?: string,
    context?: RecordingOpenContext,
  ) => void;
}

interface DisplayUpcomingEvent {
  id: string;
  startsAt: number;
  day: string;
  date: number;
  month: string;
  title: string;
  time: string;
  live: boolean;
  participants: Array<{
    name: string;
    email: string | null;
    self: boolean;
  }>;
}

const MAX_UPCOMING_ITEMS = 2;
const CALENDAR_REFRESH_INTERVAL_MS = 5 * 60 * 1000;
const CONNECT_POLL_INTERVAL_MS = 3 * 1000;
const CONNECT_POLL_TIMEOUT_MS = 2 * 60 * 1000;

function getLocalDateKey(value: number | Date) {
  const date = typeof value === "number" ? new Date(value) : value;
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function SectionHeading({ title, subtitle, action }: { title: string; subtitle?: string; action?: ReactNode }) {
  return (
    <header className="mb-3 flex items-end justify-between gap-3">
      <div className="min-w-0">
        <h2 className="truncate text-[20px] font-light tracking-[-0.02em] text-[#F1F1F3]">{title}</h2>
        {subtitle && <p className="mt-1 truncate text-[12px] text-[#777780]">{subtitle}</p>}
      </div>
      {action && <div className="flex shrink-0 items-center gap-2 pb-0.5">{action}</div>}
    </header>
  );
}

function ComingUp({
  isLiveMeeting = false,
  onOpenMeetingMemo,
  onConfigureMeeting,
}: Pick<DesktopHomeProps, "isLiveMeeting" | "onOpenMeetingMemo" | "onConfigureMeeting">) {
  const { t, currentLanguage } = useTranslation();
  const recorderSessionActive = useAppSelector(
    (state) => state.recorder.isRecording || state.recorder.paused,
  );
  const tRef = useRef(t);
  tRef.current = t;
  const locale = toBcp47Locale(currentLanguage);
  const [firstEventIndex, setFirstEventIndex] = useState(0);
  const [calendar, setCalendar] = useState<CalendarPanelPayload | null>(null);
  const [loading, setLoading] = useState(true);
  const [validating, setValidating] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settingsMenuEventId, setSettingsMenuEventId] = useState<string | null>(null);
  const [hoveredEventId, setHoveredEventId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const hasActiveSession = isLiveMeeting || recorderSessionActive;
  const today = new Date(now);
  const todayKey = getLocalDateKey(today);
  const todayLabel = {
    date: today.getDate(),
    month: new Intl.DateTimeFormat(locale, { month: "long" }).format(today),
    weekday: new Intl.DateTimeFormat(locale, { weekday: "short" }).format(today),
  };

  useEffect(() => {
    if (!hasActiveSession) return;
    setSettingsMenuEventId(null);
  }, [hasActiveSession]);

  useEffect(() => {
    if (!settingsMenuEventId) return;
    const closeMenu = () => setSettingsMenuEventId(null);
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenu();
    };
    window.addEventListener("click", closeMenu);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [settingsMenuEventId]);

  const loadCalendar = useCallback(async (silent = false) => {
    if (silent) setValidating(true);
    else setLoading(true);
    try {
      const next = await fetchGoogleCalendar();
      setCalendar(next);
      setError(null);
      if (next.connection === "connected") setConnecting(false);
    } catch (requestError) {
      setError(
        requestError instanceof Error
          ? requestError.message
          : tRef.current("desktopHome.comingUp.loadError"),
      );
    } finally {
      setLoading(false);
      setValidating(false);
    }
  }, []);

  useEffect(() => {
    void loadCalendar();
    const refresh = window.setInterval(
      () => void loadCalendar(true),
      CALENDAR_REFRESH_INTERVAL_MS,
    );
    const clock = window.setInterval(() => setNow(Date.now()), 60 * 1000);
    const refreshOnFocus = () => void loadCalendar(true);
    window.addEventListener("focus", refreshOnFocus);
    return () => {
      window.clearInterval(refresh);
      window.clearInterval(clock);
      window.removeEventListener("focus", refreshOnFocus);
    };
  }, [loadCalendar]);

  useEffect(() => {
    if (!window.ipc?.on) return;
    const cleanup = window.ipc.on(
      "calendar-oauth-complete",
      (payload?: { status?: string }) => {
        setConnecting(false);
        setError(null);
        if (payload?.status && payload.status !== "connected") {
          setError(tRef.current("desktopHome.comingUp.connectError"));
        }
        void loadCalendar(true);
      },
    );
    return () => {
      cleanup?.();
    };
  }, [loadCalendar]);

  useEffect(() => {
    if (!connecting) return;
    const poll = window.setInterval(
      () => void loadCalendar(true),
      CONNECT_POLL_INTERVAL_MS,
    );
    const timeout = window.setTimeout(() => {
      setConnecting(false);
      setError(tRef.current("desktopHome.comingUp.connectTimeout"));
    }, CONNECT_POLL_TIMEOUT_MS);
    return () => {
      window.clearInterval(poll);
      window.clearTimeout(timeout);
    };
  }, [connecting, loadCalendar]);

  const events = useMemo<DisplayUpcomingEvent[]>(() => {
    if (calendar?.connection !== "connected") return [];
    return calendar.events.flatMap((event) => {
      const start = new Date(event.start);
      const end = new Date(event.end);
      if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) {
        return [];
      }
      const startTime = start.toLocaleTimeString(locale, {
        hour: "numeric",
        minute: "2-digit",
      });
      const endTime = end.toLocaleTimeString(locale, {
        hour: "numeric",
        minute: "2-digit",
      });
      return [{
        id: event.id,
        startsAt: start.getTime(),
        day: start.toLocaleDateString(locale, { weekday: "short" }),
        date: start.getDate(),
        month: start.toLocaleDateString(locale, { month: "long" }),
        title: event.title.trim() || t("desktopHome.comingUp.untitledEvent"),
        time: `${startTime} – ${endTime}`,
        live: start.getTime() <= now && end.getTime() >= now,
        participants: event.participants.map((participant) => ({
          name: participant.name,
          email: participant.email,
          self: participant.self,
        })),
      }];
    }).sort((a, b) => a.startsAt - b.startsAt);
  }, [calendar, locale, now, t]);

  const eventSignature = events.map((event) => event.id).join("|");
  useEffect(() => {
    setFirstEventIndex(0);
  }, [eventSignature]);

  const hasEventsToday = events.some(
    (event) => getLocalDateKey(event.startsAt) === todayKey,
  );
  const getPagination = (index: number) => getUpcomingCalendarPagination({
    eventCount: events.length,
    firstEventIndex: index,
    hasEventsToday,
    maxItems: MAX_UPCOMING_ITEMS,
  });
  const {
    showNoEventsToday,
    visibleEventLimit,
    canShowPreviousEvent,
    canShowNextEvent,
  } = getPagination(firstEventIndex);
  const orderedEvents = events.slice(
    firstEventIndex,
    firstEventIndex + visibleEventLimit,
  );

  const connect = async () => {
    setError(null);
    try {
      const authorizationUrl = await beginGoogleCalendarConnect();
      setConnecting(true);
      if (window.electronAPI?.openExternal) {
        window.electronAPI.openExternal(authorizationUrl);
      } else {
        window.open(authorizationUrl, "_blank", "noopener,noreferrer");
      }
    } catch (requestError) {
      setConnecting(false);
      setError(
        requestError instanceof Error
          ? requestError.message
          : t("desktopHome.comingUp.connectError"),
      );
    }
  };

  const disconnect = async () => {
    if (!window.confirm(t("desktopHome.comingUp.disconnectConfirm"))) return;
    setDisconnecting(true);
    setError(null);
    try {
      await disconnectGoogleCalendar();
      setCalendar({ connection: "disconnected", events: [] });
      setFirstEventIndex(0);
      setConnecting(false);
    } catch (requestError) {
      setError(
        requestError instanceof Error
          ? requestError.message
          : t("desktopHome.comingUp.disconnectError"),
      );
    } finally {
      setDisconnecting(false);
    }
  };

  const connected = calendar?.connection === "connected";
  const reconnectRequired = calendar?.connection === "reauthorization_required";
  const headingActions = connected ? (
    <>
      {validating && (
        <Loader2
          size={13}
          className="animate-spin text-[#777780]"
          aria-label={t("desktopHome.comingUp.refreshingAria")}
        />
      )}
      <button
        type="button"
        onClick={() => void disconnect()}
        disabled={disconnecting}
        className="rounded-md p-1 text-[#777780] hover:bg-[#202024] hover:text-white disabled:opacity-50"
        aria-label={t("desktopHome.comingUp.disconnectAria")}
      >
        {disconnecting ? <Loader2 size={15} className="animate-spin" /> : <Unplug size={15} />}
      </button>
      {events.length > 1 && (
        <>
          <button
            type="button"
            onClick={() => setFirstEventIndex((current) => getPagination(current).previousEventIndex)}
            disabled={!canShowPreviousEvent}
            className="rounded-md p-1 text-[#777780] hover:bg-[#202024] hover:text-white disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-[#777780]"
            aria-label={t("desktopHome.comingUp.previousEventsAria")}
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            onClick={() => setFirstEventIndex((current) => getPagination(current).nextEventIndex)}
            disabled={!canShowNextEvent}
            className="rounded-md p-1 text-[#777780] hover:bg-[#202024] hover:text-white disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-[#777780]"
            aria-label={t("desktopHome.comingUp.nextEventsAria")}
          >
            <ChevronRight size={16} />
          </button>
        </>
      )}
    </>
  ) : undefined;

  return (
    <section data-product-tour="connect-calendar">
      <SectionHeading
        title={t("desktopHome.comingUp.title")}
        subtitle={
          reconnectRequired && calendar.accountEmail
            ? calendar.accountEmail
            : undefined
        }
        action={headingActions}
      />
      {loading && !calendar ? (
        <div
          className="desktop-home-card min-h-[118px] overflow-hidden rounded-[20px] border border-[#2D2D32] bg-[#151517] px-4 py-3 shadow-[0_8px_30px_rgba(0,0,0,0.12)]"
          role="status"
          aria-label={t("desktopHome.comingUp.loadingAria")}
        >
          {[0, 1].map((item) => (
            <div key={item} className="flex animate-pulse items-center gap-3 py-2">
              <div className="h-9 w-14 rounded-lg bg-[#202024]" />
              <div className="h-8 w-0.5 rounded-full bg-[#292955]" />
              <div className="flex-1 space-y-2">
                <div className="h-3 w-2/3 rounded bg-[#25252A]" />
                <div className="h-2.5 w-1/3 rounded bg-[#202024]" />
              </div>
            </div>
          ))}
        </div>
      ) : error && !calendar ? (
        <div className="desktop-home-card desktop-home-error flex min-h-[118px] items-center gap-4 rounded-[20px] border border-[#3D2D30] bg-[#151517] px-5 py-4 shadow-[0_8px_30px_rgba(0,0,0,0.12)] max-[600px]:flex-col max-[600px]:items-start">
          <CalendarClock size={26} strokeWidth={1.6} className="shrink-0 text-[#B45D43]" aria-hidden="true" />
          <div className="min-w-0 flex-1 max-[600px]:w-full">
            <p className="text-[13px] font-medium text-[#D7D7DB]">{t("desktopHome.comingUp.title")}</p>
            <p className="mt-1 truncate text-[12px] text-[#A0A0A8]">{error}</p>
          </div>
          <button
            type="button"
            onClick={() => void loadCalendar()}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-[#3B3B43] bg-[#202024] px-3.5 py-2 text-[11px] font-semibold text-[#F1F1F3] hover:border-[#5156FF] hover:bg-[#292955] max-[600px]:self-end"
          >
            <RefreshCw size={11} /> {t("desktopHome.comingUp.retry")}
          </button>
        </div>
      ) : !connected ? (
        <div className="desktop-home-card flex min-h-[118px] overflow-hidden rounded-[20px] border border-[#2D2D32] bg-[#151517] shadow-[0_8px_30px_rgba(0,0,0,0.12)]">
          <div className="flex w-[106px] shrink-0 items-start gap-1.5 px-4 py-4">
            <span className="font-quicksand text-[31px] font-light leading-none text-[#F1F1F3]">{todayLabel.date}</span>
            <span className="pt-0.5 text-[11px] leading-[1.3] text-[#85858E]">
              <span className="text-[#D7D7DB]">{todayLabel.month}</span>
              <br />
              {todayLabel.weekday}
            </span>
          </div>

          <div className="m-2.5 ml-0 flex min-w-0 flex-1 items-center justify-center gap-3 rounded-[14px] border border-dashed border-[#34343A] px-4 py-3 text-left">
            <CalendarClock size={23} strokeWidth={1.6} className="shrink-0 text-[#777780]" aria-hidden="true" />
            <div className="min-w-0">
              <h3 className="text-[14px] font-medium text-[#D7D7DB]">
                {reconnectRequired
                  ? t("desktopHome.comingUp.reconnectTitle")
                  : t("desktopHome.comingUp.linkTitle")}
              </h3>
              <p className="mt-1 text-[11px] leading-5 text-[#85858E]">
                {error || (reconnectRequired
                  ? t("desktopHome.comingUp.reconnectDescription")
                  : t("desktopHome.comingUp.linkDescription"))}
              </p>
            </div>
            <button
              type="button"
              onClick={() => void connect()}
              disabled={connecting}
              className="shrink-0 rounded-full border border-[#3B3B43] bg-[#202024] px-3.5 py-2 text-[11px] font-semibold text-[#F1F1F3] transition hover:border-[#5156FF] hover:bg-[#292955] hover:text-white disabled:cursor-wait disabled:opacity-60"
            >
              {connecting
                ? t("desktopHome.comingUp.waitingForGoogle")
                : reconnectRequired
                  ? t("desktopHome.comingUp.reconnect")
                  : t("desktopHome.comingUp.linkCalendar")}
            </button>
          </div>
        </div>
      ) : (
        <div className="desktop-home-card overflow-visible rounded-[20px] border border-[#2D2D32] bg-[#151517] shadow-[0_8px_30px_rgba(0,0,0,0.12)]">
          {showNoEventsToday && (
            <div
              className={`flex min-h-[58px] w-full items-center gap-3.5 px-4 py-2.5 text-left ${events.length > 0 ? "rounded-t-[19px] border-b border-dashed border-[#2D2D32]" : "rounded-[19px]"}`}
            >
              <div className="flex w-[96px] shrink-0 items-center gap-2">
                <span className="font-quicksand text-[26px] font-light leading-none text-[#F1F1F3]">{todayLabel.date}</span>
                <span className="text-[11px] leading-[1.3] text-[#85858E]">
                  {todayLabel.month}<br />{todayLabel.weekday}
                </span>
              </div>
              <div className="h-8 w-0.5 shrink-0 rounded-full bg-[#5156FF]" />
              <p className="truncate text-[14px] font-medium leading-5 text-[#85858E]">
                {t("desktopHome.comingUp.noMeetings")}
              </p>
            </div>
          )}
          {orderedEvents.map((event, index) => {
            const previousEvent = orderedEvents[index - 1];
            const showEventDate = !previousEvent
              || getLocalDateKey(previousEvent.startsAt) !== getLocalDateKey(event.startsAt);
            const noteContext: CalendarNoteContext = {
              id: event.id,
              title: event.title,
              dateLabel: `${event.month} ${event.date}, ${event.day}`,
              time: event.time,
              startsAt: event.startsAt,
              calendarEventId: event.id,
              participants: event.participants,
            };

            return (
              <div
                key={event.id}
                onMouseEnter={() => setHoveredEventId(event.id)}
                onMouseLeave={() => setHoveredEventId((current) => current === event.id ? null : current)}
                className={`group flex min-h-[58px] w-full items-center gap-3.5 px-4 py-2.5 text-left transition-colors ${hoveredEventId === event.id ? "bg-[#202024]" : ""} ${index === 0 && !showNoEventsToday ? "rounded-t-[19px]" : ""} ${index === orderedEvents.length - 1 ? "rounded-b-[19px]" : ""} ${index < orderedEvents.length - 1 ? "border-b border-dashed border-[#2D2D32]" : ""}`}
              >
              <button
                type="button"
                onClick={() => onOpenMeetingMemo(noteContext)}
                className="flex min-w-0 flex-1 items-center gap-3 text-left"
                aria-label={t("desktopHome.comingUp.startNoteAria", { title: event.title })}
              >
                <div className="flex w-[96px] shrink-0 items-center gap-2">
                  {showEventDate && (
                    <>
                      <span className="font-quicksand text-[26px] font-light leading-none text-[#F1F1F3]">{event.date}</span>
                      <span className="text-[11px] leading-[1.3] text-[#85858E]">{event.month}<br />{event.day}</span>
                    </>
                  )}
                </div>
                <div className="h-8 w-0.5 shrink-0 rounded-full bg-[#5156FF]" />
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center gap-1.5">
                    <p className="truncate text-[14px] font-medium leading-5 text-[#D7D7DB]">{event.title}</p>
                    {event.live && (
                      <span
                        className="h-1.5 w-1.5 shrink-0 rounded-full bg-[#67D44B]"
                        title={t("desktopTopBar.chrome.sessionLive")}
                        aria-label={t("desktopTopBar.chrome.sessionLive")}
                      />
                    )}
                  </div>
                  <p className="mt-0.5 truncate text-[12px] leading-5 text-[#85858E]">{event.time}</p>
                </div>
              </button>
              <div className="relative shrink-0">
                <button
                  type="button"
                  disabled={hasActiveSession}
                  onClick={(clickEvent) => {
                    clickEvent.stopPropagation();
                    setSettingsMenuEventId((current) =>
                      current === event.id ? null : event.id,
                    );
                  }}
                  className={`flex h-8 w-8 items-center justify-center text-[#777780] transition hover:text-[#5156FF] focus-visible:pointer-events-auto focus-visible:opacity-100 focus-visible:outline-none focus-visible:text-[#5156FF] disabled:cursor-not-allowed ${settingsMenuEventId === event.id ? "pointer-events-auto opacity-100" : "pointer-events-none opacity-0 group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100"}`}
                  aria-label={t("desktopUi.calendar.configureAria", { title: event.title })}
                  aria-expanded={settingsMenuEventId === event.id}
                  aria-haspopup="menu"
                  title={hasActiveSession
                    ? t("desktopUi.calendar.settingsBlocked")
                    : t("desktopUi.calendar.settingsTitle")}
                >
                  <SettingsIcon size={17} strokeWidth={1.8} aria-hidden="true" />
                </button>

                {settingsMenuEventId === event.id && !hasActiveSession && (
                  <div
                    role="menu"
                    aria-label={t("desktopUi.calendar.settingsForAria", { title: event.title })}
                    onClick={(clickEvent) => clickEvent.stopPropagation()}
                    className="meeting-start-mode-menu absolute right-0 top-[calc(100%+8px)] z-50 w-[300px] rounded-[16px] border border-[#34343A] bg-[#202024] p-1.5 shadow-[0_18px_48px_rgba(0,0,0,0.55)]"
                  >
                    {([
                      {
                        panel: "language" as const,
                        label: t("desktopUi.calendar.language"),
                        description: t("desktopUi.calendar.languageDescription"),
                        icon: <Languages size={14} aria-hidden="true" />,
                      },
                      {
                        panel: "quality" as const,
                        label: t("desktopUi.calendar.quality"),
                        description: t("desktopUi.calendar.qualityDescription"),
                        icon: <Gauge size={14} aria-hidden="true" />,
                      },
                      {
                        panel: "voice" as const,
                        label: t("desktopTopBar.modes.voiceToVoice.label"),
                        description: t("desktopTopBar.modes.voiceToVoice.description"),
                        icon: <Volume2 size={14} aria-hidden="true" />,
                      },
                      {
                        panel: "sharing" as const,
                        label: t("desktopTopBar.modes.share.label"),
                        description: t("desktopTopBar.modes.share.description"),
                        icon: <Link2 size={14} aria-hidden="true" />,
                      },
                    ] satisfies Array<{
                      panel: LiveSessionLaunchPanel;
                      label: string;
                      description: string;
                      icon: ReactNode;
                    }>).map((option) => (
                      <button
                        key={option.panel}
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setSettingsMenuEventId(null);
                          onConfigureMeeting(noteContext, option.panel);
                        }}
                        className="meeting-start-mode-option flex w-full items-start gap-3 rounded-[11px] px-3 py-2.5 text-left transition hover:bg-[#303036] focus-visible:bg-[#303036] focus-visible:outline-none"
                      >
                        <span className="mt-0.5 rounded-lg bg-[#5156FF]/15 p-1.5 text-[#8F92FF]">
                          {option.icon}
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="block text-[13px] font-semibold text-[#F1F1F3]">
                            {option.label}
                          </span>
                          <span className="mt-0.5 block text-[11px] leading-5 text-[#85858E]">
                            {option.description}
                          </span>
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
            );
          })}
        </div>
      )}
      {connected && calendar.accountEmail && (
        <p
          className="mt-2 truncate pr-1 text-right text-[12px] text-[#777780]"
          title={t("desktopHome.comingUp.googleCalendarSubtitle", { email: calendar.accountEmail })}
        >
          {t("desktopHome.comingUp.googleCalendarSubtitle", { email: calendar.accountEmail })}
        </p>
      )}
    </section>
  );
}

function RecentFiles({
  onViewAllRecordings,
  onStartRecordingsTutorial,
  onViewAllTranslatedFiles,
  onStartTranslation,
  onOpenTranslatedFile,
  onOpenMeeting,
}: Pick<DesktopHomeProps, "onViewAllRecordings" | "onStartRecordingsTutorial" | "onViewAllTranslatedFiles" | "onStartTranslation" | "onOpenTranslatedFile" | "onOpenMeeting">) {
  return (
    <section className="space-y-9">
      <Recordings
        onViewAllRecordings={onViewAllRecordings}
        onStartRecordingsTutorial={onStartRecordingsTutorial}
        onOpenMeeting={onOpenMeeting}
      />
      <TranslatedFiles
        onViewAllTranslatedFiles={onViewAllTranslatedFiles}
        onStartTranslation={onStartTranslation}
        onOpenTranslatedFile={onOpenTranslatedFile}
      />
    </section>
  );
}

export function DesktopHome({ firstName, isLiveMeeting, onOpenMeetingMemo, onConfigureMeeting, onOpenChat, onOpenVoiceTyping, onOpenWebApp, onViewAllRecordings, onStartRecordingsTutorial, onViewAllTranslatedFiles, onStartTranslation, onOpenTranslatedFile, onOpenMeeting }: DesktopHomeProps) {
  const [scrollIndicatorVisible, setScrollIndicatorVisible] = useState(false);
  const scrollIndicatorTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (scrollIndicatorTimerRef.current) {
      clearTimeout(scrollIndicatorTimerRef.current);
    }
  }, []);

  const showScrollIndicator = () => {
    setScrollIndicatorVisible(true);
    if (scrollIndicatorTimerRef.current) {
      clearTimeout(scrollIndicatorTimerRef.current);
    }
    scrollIndicatorTimerRef.current = setTimeout(
      () => setScrollIndicatorVisible(false),
      700,
    );
  };

  return (
    <div
      onScroll={showScrollIndicator}
      className={`activity-scrollbar h-full overflow-y-auto bg-[#0B0B0D] text-[#F5F5F7] ${
        scrollIndicatorVisible ? "activity-scrollbar--visible" : ""
      }`}
    >
      <div className="mx-auto w-full max-w-[1120px] px-6 pb-10 pt-3">
        {/* 60 / 40 split of the free space, written as the ratio itself. */}
        <div className="grid items-start gap-6 min-[700px]:grid-cols-[minmax(0,0.6fr)_minmax(260px,0.4fr)]">
          <div className="contents min-[700px]:block min-[700px]:min-w-0 min-[700px]:space-y-7">
            <div className="order-1 min-w-0">
              <ComingUp
                isLiveMeeting={isLiveMeeting}
                onOpenMeetingMemo={onOpenMeetingMemo}
                onConfigureMeeting={onConfigureMeeting}
              />
            </div>
            {/* Stacked layout order: Coming up → Your context → Recent chats →
                Recordings, with the voice keyboard card closing the column. */}
            <div className="order-4 min-w-0">
              <RecentFiles
                onViewAllRecordings={onViewAllRecordings}
                onStartRecordingsTutorial={onStartRecordingsTutorial}
                onViewAllTranslatedFiles={onViewAllTranslatedFiles}
                onStartTranslation={onStartTranslation}
                onOpenTranslatedFile={onOpenTranslatedFile}
                onOpenMeeting={onOpenMeeting}
              />
            </div>
          </div>
          <div className="contents min-[700px]:block min-[700px]:min-w-0 min-[700px]:space-y-7">
            <div className="order-2 min-w-0">
              <OperationalContext firstName={firstName} onOpenWebApp={onOpenWebApp} />
            </div>
            <div className="order-3 min-w-0">
              <RecentChats
                onOpenChat={onOpenChat}
                onOpenVoiceTyping={onOpenVoiceTyping}
              />
            </div>
          </div>
        </div>
        <div className="mt-5 min-[700px]:hidden">
          <VoiceKeyboardCard onOpenVoiceTyping={onOpenVoiceTyping} />
        </div>
      </div>
    </div>
  );
}
