import { DesktopBackNavigation, type DesktopBackTarget } from "./DesktopBackNavigation";
import { LearningSync } from './LearningSync';
import { useCallback, useEffect, useRef, useState } from "react";
import { BookOpen, ChevronLeft, FileText, Gift, Home, Languages, MessageCircle, Mic2, Search, Sparkles, UsersRound, X } from "lucide-react";
import { DesktopLearning } from './DesktopLearning';
import { useTranslation } from "../../hooks/useTranslation";
import { useAppDispatch, useAppSelector } from "../../redux/hooks";
import {
  setBothModeUplinkTranslatedLanguage,
  setEnabled,
  setExtraTranslatedLanguages,
  setSpokenLanguage,
  setSpokenLanguageModel,
  setTranslatedLanguage,
  setTranslationMode,
} from "../../redux/slices/recorderSlice";
import { setRefetchUser } from "../../redux/slices/userSlice";
import {
  getLiveQuotaState,
  isLiveQuotaExhausted,
  minutesUpgradeDismissKey,
  type LiveSessionQuotaMode,
} from "../../lib/minutesQuota";
import {
  clearChatBadge,
  DESKTOP_OPEN_CHAT_FROM_NOTIFICATION,
  setChatFocusState,
  type ChatNotificationClickPayload,
} from "../../lib/chat/notifications";
import { DesktopChatNotificationProvider } from "../../Providers/DesktopChatNotificationProvider";
import { DesktopMeetingNotificationProvider } from "../../Providers/DesktopMeetingNotificationProvider";
import AudioRecorder from "../../pages/_components/AudioRecorder";
import type { PostMeetingDetailTab } from "../../utils/postMeetingDestination";
import { EXIT_TRANSCRIPT_PRESENTATION_EVENT } from "../../utils/transcriptPresentation";
import {
  fetchRecordings,
  type RecordingOpenContext,
  type RecordingSessionSource,
} from "../../lib/recordingsApi";
import { DesktopChat } from "./DesktopChat";
import { DesktopHome } from "./DesktopHome";
import {
  DesktopRecordingDetail,
  type DesktopMeetingAssistantLaunch,
} from "./DesktopRecordingDetail";
import { DesktopMeetingAssistant } from "./DesktopMeetingAssistant";
import { DesktopRecordings } from "./DesktopRecordings";
import { DesktopPeople } from "./DesktopPeople";
import { DesktopPersonDetail } from "./DesktopPersonDetail";
import { DesktopVoiceTyping } from "./DesktopVoiceTyping";
import { DesktopSidebar, type DesktopView } from "./DesktopSidebar";
import {
  DesktopSettingsModal,
  type SettingsSection,
} from "./DesktopSettingsModal";
import { DesktopTopBar } from "./DesktopTopBar";
import {
  DesktopProductTour,
  PRODUCT_TOUR_STEPS,
  RECORDINGS_TOUR_RANGE,
} from "./DesktopProductTour";
import { DesktopMinutesUpgradeCoachmark } from "./DesktopMinutesUpgradeCoachmark";
import {
  DesktopTranslate,
  detectTranslateModeForFile,
  type OpenFileTranslationRequest,
  type OpenTextTranslationRequest,
} from "./DesktopTranslate";
import { TranslatedFileDetail } from "./translate-components/TranslatedFileDetail";
import type { TranslateHistoryItem } from "../../lib/translateHistoryApi";
import type { WorkspacePerson } from "../../lib/peopleApi";
import { DesktopTeam, TeamTutorial } from "./team-components";
import { DesktopReferral } from "./referral-components";
import { DesktopPricing, DesktopPurchaseFeedbackModal, useDesktopPricingCopy } from "./pricing-components";
import { DesktopConfirmDialog } from "./chat/DesktopConfirmDialog";
import {
  checkpointActiveCalendarLiveSessionPreferences,
  clearCalendarNoteContext,
  finishActiveCalendarLiveSessionPreferences,
  readCalendarNoteContext,
  setCalendarNoteContext,
  type CalendarNoteContext,
  type LiveSessionLaunchPanel,
  type LiveSessionPanelRequest,
} from "./calendarNoteContext";
import {
  clearLanguageChangeLiveSessionReload,
  clearChatShareLaunch,
  consumeRestoreRecordingViewOnReload,
  readLanguageChangeLiveSessionReload,
  writeChatShareLaunch,
  type ChatShareLaunchPayload,
} from "../../lib/liveTranslationInvitationStorage";
import {
  readPendingSharedJoin,
  shouldRestoreRecordingViewOnBoot,
} from "../../utils/sharedJoinModelSync";
import {
  applyDesktopZoomPercent,
  DESKTOP_ZOOM_DEFAULT,
  DESKTOP_ZOOM_STEP,
  readDesktopZoomPercent,
} from "../../utils/desktopZoomPreferences";
import {
  readSidebarCollapsedPreference,
  writeSidebarCollapsedPreference,
} from "../../utils/sidebarPreference";
import {
  parseDesktopContentOpenPayload,
  translatedFileItemFromDeepLink,
} from "../../lib/contentDeepLink";
import {
  DESKTOP_OPEN_PRICING_EVENT,
  type DesktopOpenPricingDetail,
} from "../../lib/desktopPricingNavigation";
import type { DesktopPermissionSetupReason } from "./DesktopPermissionSetup";
import {
  getDesktopPermissionStatus,
  hasMissingLiveAudioPermission,
  hasMissingRequiredDesktopPermission,
  hasMissingVoiceKeyboardPermission,
} from "./desktopPermissionStatus";
import {
  DESKTOP_THEME_STORAGE_KEY,
  readInitialDesktopTheme,
} from "../../utils/themePreference";

type ChatLaunchTarget = {
  chatId: string;
  threadParentId?: string | null;
};

const webAppUrl = "https://app.MeetingCopilot.io/";
const PRODUCT_TOUR_STORAGE_VERSION = "v2";
const PERMISSION_SETUP_STORAGE_VERSION = "v1";
const SIDEBAR_PREFERENCE_VIEWS = new Set<DesktopView>([
  "learning",
  "home",
  "recordings",
  "translate",
]);
const SIDEBAR_FORCED_COLLAPSE_VIEWS = new Set<DesktopView>(["chat", "pricing"]);

// The collapse chevron overlays the window's top-left, next to the native
// traffic lights; it opts out of the surrounding drag regions.
const noDrag = { WebkitAppRegion: "no-drag" } as React.CSSProperties;

const readStoredRecorderPreferences = () => {
  let config: Record<string, unknown> = {};
  try {
    config = JSON.parse(localStorage.getItem("MeetingCopilot_config") || "{}") as Record<string, unknown>;
  } catch {
    // Ignore malformed legacy preferences and retain the current Redux state.
  }
  const translationMode = localStorage.getItem("MeetingCopilot_translation_mode");
  return { config, translationMode };
};

// macOS-style sidebar toggle. lucide's PanelLeft only draws an outline, so this
// custom glyph actually fills the left panel: a small pill hint when collapsed,
// and the left third filled ("open") when the sidebar is showing.
function SidebarToggleIcon({ open }: { open: boolean }) {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3" y="5" width="18" height="14" rx="3" />
      {open ? (
        <path d="M9 5 H6 A3 3 0 0 0 3 8 V16 A3 3 0 0 0 6 19 H9 Z" fill="currentColor" stroke="none" />
      ) : (
        <rect x="5.2" y="8.5" width="2.6" height="7" rx="1.3" fill="currentColor" stroke="none" />
      )}
    </svg>
  );
}

export function DesktopShell() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const { user } = useAppSelector((state) => state.user);
  const { enabled: translationEnabled } = useAppSelector((state) => state.recorder);
  const liveQuota = getLiveQuotaState({
    translationEnabled,
    translationUsage: user?.translationUsage ?? 0,
    translationLimit: user?.translationLimit ?? null,
    transcriptionUsage: user?.transcriptionUsage ?? 0,
    transcriptionLimit: user?.transcriptionLimit ?? null,
  });
  const liveQuotaRef = useRef(liveQuota);
  liveQuotaRef.current = liveQuota;

  const blockLiveSessionIfExhausted = useCallback((mode: LiveSessionQuotaMode = "current") => {
    if (!isLiveQuotaExhausted(liveQuotaRef.current, mode)) return false;
    setForceMinutesUpgrade(true);
    return true;
  }, []);
  const [languageChangeReload, setLanguageChangeReload] = useState(
    () => readLanguageChangeLiveSessionReload(),
  );
  const syncRecorderPreferencesFromStorage = useCallback(() => {
    const { config, translationMode } = readStoredRecorderPreferences();
    if (typeof config.translationEnabled === "boolean") dispatch(setEnabled(config.translationEnabled));
    if (typeof config.spokenLanguage === "string") dispatch(setSpokenLanguage(config.spokenLanguage));
    if (typeof config.spokenLanguageModel === "string") dispatch(setSpokenLanguageModel(config.spokenLanguageModel));
    if (typeof config.translatedLanguage === "string") dispatch(setTranslatedLanguage(config.translatedLanguage));
    if (typeof config.bothModeUplinkTranslatedLanguage === "string") {
      dispatch(setBothModeUplinkTranslatedLanguage(config.bothModeUplinkTranslatedLanguage));
    }
    if (Array.isArray(config.extraTranslatedLanguages)) {
      dispatch(setExtraTranslatedLanguages(config.extraTranslatedLanguages));
    }
    if (translationMode === "single" || translationMode === "fast" || translationMode === "multi") {
      dispatch(setTranslationMode(translationMode));
    }
  }, [dispatch]);

  useEffect(() => {
    const checkpoint = () => {
      checkpointActiveCalendarLiveSessionPreferences();
    };
    window.addEventListener("beforeunload", checkpoint);
    return () => window.removeEventListener("beforeunload", checkpoint);
  }, []);
  const pricingCopy = useDesktopPricingCopy();
  const [activeView, setActiveView] = useState<DesktopView>(() =>
    shouldRestoreRecordingViewOnBoot({
      languageChangeReload,
      pendingSharedJoin: readPendingSharedJoin(),
      restoreRecordingView: consumeRestoreRecordingViewOnReload(),
    })
      ? "recording"
      : "home"
  );
  const viewHistoryRef = useRef<DesktopView[]>([]);
  const [nestedBack, setNestedBack] = useState<DesktopBackTarget | null>(null);
  const [pricingReferrer, setPricingReferrer] = useState<string | undefined>("desktop-app");
  const [pricingRefreshKey, setPricingRefreshKey] = useState(0);
  const [teamBillingRequest, setTeamBillingRequest] = useState(0);
  const [pricingInitialViewMode, setPricingInitialViewMode] = useState<
    "individual" | "teams-enterprise" | "events" | undefined
  >();
  const [showPricingTeamTutorial, setShowPricingTeamTutorial] = useState(false);
  const [purchaseFeedbackToken, setPurchaseFeedbackToken] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(
    () => readSidebarCollapsedPreference(),
  );
  const [sidebarExpansionAllowed, setSidebarExpansionAllowed] = useState(true);
  const [searchOpen, setSearchOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [permissionSetupOpen, setPermissionSetupOpen] = useState(false);
  const [permissionSetupResolved, setPermissionSetupResolved] = useState(false);
  const [permissionSetupReason, setPermissionSetupReason] =
    useState<DesktopPermissionSetupReason>("onboarding");
  const permissionSetupOpenTimerRef = useRef<number | null>(null);
  const [productTourStep, setProductTourStep] = useState<number | null>(null);
  const [productTourNeedsSetup, setProductTourNeedsSetup] = useState(true);
  const [forceMinutesUpgrade, setForceMinutesUpgrade] = useState(false);
  const [recordingMenuOpen, setRecordingMenuOpen] = useState(false);
  const [_minutesUpgradeDismissedKey, setMinutesUpgradeDismissedKey] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    try {
      return sessionStorage.getItem("MeetingCopilot_minutes_upgrade_dismissed");
    } catch {
      return null;
    }
  });
  const [productTourRange, setProductTourRange] = useState<{
    firstStepIndex: number;
    lastStepIndex: number;
  } | null>(null);
  const recordingsTourPreviousTranslationRef = useRef(translationEnabled);
  const productTourTarget = productTourStep === null
    ? undefined
    : PRODUCT_TOUR_STEPS[productTourStep]?.target ?? undefined;
  const productTourAutoStartCheckedRef = useRef(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("ai-notes");
  const [transcribeWorkspaceRequest, setTranscribeWorkspaceRequest] = useState(0);
  const [translateWorkspaceRequest, setTranslateWorkspaceRequest] = useState(0);
  const [translateFilesRequest, setTranslateFilesRequest] = useState(0);
  const [translateHistoryRequest, setTranslateHistoryRequest] = useState(0);
  const [translateTextRequest, setTranslateTextRequest] =
    useState<OpenTextTranslationRequest | null>(null);
  const [dockFileRequest, setDockFileRequest] =
    useState<OpenFileTranslationRequest | null>(null);
  const [selectedTranslatedFile, setSelectedTranslatedFile] =
    useState<TranslateHistoryItem | null>(null);
  const [translatedHistoryRefreshKey, setTranslatedHistoryRefreshKey] = useState(0);
  const [selectedMeeting, setSelectedMeeting] = useState<{
    id: string;
    title: string;
    awaitReady?: boolean;
    initialTab?: PostMeetingDetailTab;
  } | null>(null);
  const [selectedPerson, setSelectedPerson] = useState<WorkspacePerson | null>(null);
  const [meetingAssistantLaunch, setMeetingAssistantLaunch] =
    useState<DesktopMeetingAssistantLaunch | null>(null);
  const [chatLaunch, setChatLaunch] = useState<ChatLaunchTarget | null>(null);
  // Keep AudioRecorder mounted for the whole live session so navigating to
  // chat/translate/home does not disconnect the socket and end the meeting.
  const [isLiveMeeting, setIsLiveMeeting] = useState(false);
  const [preparedLiveSession, setPreparedLiveSession] = useState<{
    meetingId: string;
    shareCode: string;
    reservationKey?: string;
  } | null>(() => languageChangeReload
    ? {
        meetingId: languageChangeReload.meetingId,
        shareCode: languageChangeReload.shareCode,
        reservationKey: languageChangeReload.reservationKey,
      }
    : null);
  const [showAdhocLinkExitWarning, setShowAdhocLinkExitWarning] = useState(false);
  const [availableLiveSession, setAvailableLiveSession] = useState<{
    id: string;
    title: string;
    sessionSource?: RecordingSessionSource;
  } | null>(null);
  // The recorder starts automatically after its socket/auth setup completes.
  const [recordingStartRequested, setRecordingStartRequested] = useState(
    languageChangeReload?.wasRecording === true,
  );
  const [recordingInitialTab, setRecordingInitialTab] = useState<"Transcript" | "Notes">(
    translationEnabled ? "Transcript" : "Notes",
  );
  const [recordingPanelRequest, setRecordingPanelRequest] =
    useState<LiveSessionPanelRequest | null>(null);
  const [recordingCalendarContext, setRecordingCalendarContext] =
    useState<CalendarNoteContext | null>(() => readCalendarNoteContext());
  const [adhocSessionKey, setAdhocSessionKey] = useState(0);
  const recordingPanelRequestIdRef = useRef(0);
  // Assume macOS until we can check: the shell now owns the window chrome, so on
  // mac the native traffic lights overlay the sidebar's top-left and we reserve a
  // draggable strip for them; on Windows the top bar renders its own controls.
  const [isMac, setIsMac] = useState(true);
  const [isTranscriptMaximized, setIsTranscriptMaximized] = useState(false);

  const handleLiveMeetingChange = useCallback((isLive: boolean, _meetingId: string | null) => {
    setIsLiveMeeting(isLive);
    if (isLive) {
      clearLanguageChangeLiveSessionReload();
      setRecordingStartRequested(false);
    }
  }, []);

  const openSettingsAtMainWindowSize = useCallback((section: SettingsSection) => {
    if (permissionSetupOpenTimerRef.current !== null) {
      window.clearTimeout(permissionSetupOpenTimerRef.current);
      permissionSetupOpenTimerRef.current = null;
    }
    setSearchOpen(false);
    setSettingsSection(section);
    if (activeView === "recording") {
      setSettingsOpen(false);
      window.ipc?.send("window-view-mode", { mode: "main", forceRestore: true });
      // Keep the live view mounted, but wait for the native main-window bounds
      // animation before displaying Settings.
      permissionSetupOpenTimerRef.current = window.setTimeout(() => {
        permissionSetupOpenTimerRef.current = null;
        setSettingsOpen(true);
      }, 800);
      return;
    }
    setSettingsOpen(true);
  }, [activeView]);

  const closeSettings = useCallback(() => {
    if (permissionSetupOpenTimerRef.current !== null) {
      window.clearTimeout(permissionSetupOpenTimerRef.current);
      permissionSetupOpenTimerRef.current = null;
    }
    setSettingsOpen(false);
    if (activeView === "recording") {
      window.ipc?.send("window-view-mode", { mode: "recording", forceRestore: true });
    }
  }, [activeView]);

  const openPermissionSetup = useCallback((reason: DesktopPermissionSetupReason) => {
    setPermissionSetupReason(reason);
    setPermissionSetupOpen(true);
    openSettingsAtMainWindowSize("setup");
  }, [openSettingsAtMainWindowSize]);

  useEffect(() => () => {
    if (permissionSetupOpenTimerRef.current !== null) {
      window.clearTimeout(permissionSetupOpenTimerRef.current);
    }
  }, []);

  const ensureLiveAudioPermissions = useCallback(async () => {
    try {
      const status = await getDesktopPermissionStatus();
      if (!hasMissingLiveAudioPermission(status)) return true;
    } catch {
      // Fail closed when the permission state cannot be confirmed.
    }
    openPermissionSetup("live-audio");
    return false;
  }, [openPermissionSetup]);

  useEffect(() => {
    if (!languageChangeReload) return;
    clearLanguageChangeLiveSessionReload();
    setLanguageChangeReload(null);
  }, [languageChangeReload]);

  useEffect(() => {
    setIsMac(typeof navigator !== "undefined" && navigator.platform.toLowerCase().includes("mac"));
  }, []);

  const transcriptPresentationMode = activeView === "recording" && isTranscriptMaximized;
  const exitTranscriptPresentation = useCallback(() => {
    window.dispatchEvent(new Event(EXIT_TRANSCRIPT_PRESENTATION_EVENT));
  }, []);

  useEffect(() => {
    window.ipc?.send("transcript-presentation-mode", transcriptPresentationMode);
  }, [transcriptPresentationMode]);

  useEffect(() => () => {
    window.ipc?.send("transcript-presentation-mode", false);
  }, []);

  useEffect(() => {
    applyDesktopZoomPercent(readDesktopZoomPercent(), { persist: false, notify: false });

    const handleZoomShortcut = (event: KeyboardEvent) => {
      const platform = navigator.platform.toLowerCase();
      const modifierPressed = platform.includes("mac") ? event.metaKey : event.ctrlKey;
      if (!modifierPressed || event.altKey) return;

      const currentZoom = readDesktopZoomPercent();
      let nextZoom: number | null = null;
      if (event.key === "+" || event.key === "=") nextZoom = currentZoom + DESKTOP_ZOOM_STEP;
      else if (event.key === "-") nextZoom = currentZoom - DESKTOP_ZOOM_STEP;
      else if (event.key === "0") nextZoom = DESKTOP_ZOOM_DEFAULT;
      if (nextZoom === null) return;

      event.preventDefault();
      applyDesktopZoomPercent(nextZoom);
    };

    window.addEventListener("keydown", handleZoomShortcut);
    return () => window.removeEventListener("keydown", handleZoomShortcut);
  }, []);

  useEffect(() => {
    let cancelled = false;

    const refreshLiveSession = () => {
      void fetchRecordings({ page: 1, limit: 5, liveOnly: true })
        .then((result) => {
          if (cancelled) return;
          const session = result.data.find((recording) => recording.live) ?? null;
          setAvailableLiveSession(session
            ? {
                id: session.id,
                title: session.title,
                sessionSource: session.sessionSource,
              }
            : null);
        })
        .catch(() => {
          // Keep the last known session during a transient server failure.
        });
    };

    refreshLiveSession();
    const timer = window.setInterval(refreshLiveSession, 15_000);
    window.addEventListener("focus", refreshLiveSession);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      window.removeEventListener("focus", refreshLiveSession);
    };
  }, [user?.id]);

  const openMeetingSession = useCallback((
    calendarEvent?: CalendarNoteContext,
    initialTab?: "Transcript" | "Notes",
    panel?: LiveSessionLaunchPanel,
  ) => {
    if (calendarEvent) {
      clearChatShareLaunch();
      setCalendarNoteContext(calendarEvent);
      setRecordingCalendarContext(calendarEvent);
      syncRecorderPreferencesFromStorage();
      setRecordingInitialTab(initialTab ?? "Notes");
    } else {
      if (clearCalendarNoteContext()) syncRecorderPreferencesFromStorage();
      setRecordingCalendarContext(null);
      if (!isLiveMeeting) {
        setPreparedLiveSession(null);
        clearChatShareLaunch();
        setAdhocSessionKey((key) => key + 1);
      }
      setRecordingInitialTab(initialTab ?? (translationEnabled ? "Transcript" : "Notes"));
    }

    setRecordingPanelRequest(panel
      ? { id: ++recordingPanelRequestIdRef.current, panel }
      : null);

    if (activeView !== "recording") {
      viewHistoryRef.current.push(activeView);
    }
    setActiveView("recording");
    setSearchOpen(false);
    setRecordingStartRequested(false);
  }, [activeView, isLiveMeeting, syncRecorderPreferencesFromStorage, translationEnabled]);

  useEffect(() => {
    if (activeView === "recording" || isLiveMeeting) return;

    // A calendar session can be prepared (for example, with a share link)
    // before recording starts. Leaving that screen must still restore the
    // global Home preferences; keep the event context mounted so returning to
    // the prepared session can reapply its profile.
    if (preparedLiveSession) {
      if (finishActiveCalendarLiveSessionPreferences()) syncRecorderPreferencesFromStorage();
      return;
    }

    if (clearCalendarNoteContext()) syncRecorderPreferencesFromStorage();
    setRecordingCalendarContext(null);
  }, [activeView, isLiveMeeting, preparedLiveSession, syncRecorderPreferencesFromStorage]);

  const requestRecordingStart = useCallback(async (
    calendarEvent?: CalendarNoteContext,
    initialTab?: "Transcript" | "Notes",
  ) => {
    if (blockLiveSessionIfExhausted()) return;
    if (!await ensureLiveAudioPermissions()) return;
    openMeetingSession(calendarEvent, initialTab);
    setRecordingStartRequested(true);
  }, [blockLiveSessionIfExhausted, ensureLiveAudioPermissions, openMeetingSession]);

  useEffect(() => {
    const cleanup = window.ipc?.on("tray-command", (command: unknown) => {
      setSearchOpen(false);

      switch (command) {
        case "quick-recording":
        case "open-live-session":
          if (blockLiveSessionIfExhausted()) break;
          openMeetingSession();
          break;
        case "open-home":
          setMeetingAssistantLaunch(null);
          setChatLaunch(null);
          setActiveView("home");
          break;
        case "open-chat":
          setMeetingAssistantLaunch(null);
          setChatLaunch(null);
          setActiveView("chat");
          break;
        case "translate-files":
          setTranslateFilesRequest((request) => request + 1);
          setActiveView("translate");
          break;
        case "settings":
          setActiveView("home");
          setSettingsSection("ai-notes");
          setSettingsOpen(true);
          break;
        case "open-pricing":
          setPricingReferrer("voice-typing-limit");
          setActiveView("pricing");
          break;
      }
    });

    return cleanup;
  }, [blockLiveSessionIfExhausted, openMeetingSession]);

  useEffect(() => {
    if (!window.ipc?.on || !window.ipc.readDockFile) return;
    return window.ipc.on("dock-file-opened", (rawPayload: unknown) => {
      const payload = rawPayload as {
        token?: unknown;
        name?: unknown;
        type?: unknown;
      };
      if (typeof payload?.token !== "string") return;

      void window.ipc.readDockFile(payload.token)
        .then((opened) => {
          const fileBuffer = new ArrayBuffer(opened.data.byteLength);
          new Uint8Array(fileBuffer).set(opened.data);
          const file = new File([fileBuffer], opened.name, {
            type: opened.type || "application/octet-stream",
          });
          const mode = detectTranslateModeForFile(file);
          if (!mode) {
            window.alert(t("desktopUi.translate.unsupportedFile", { filename: opened.name }));
            return;
          }
          setDockFileRequest({ id: Date.now(), file, mode });
          changeViewRef.current("translate");
        })
        .catch((error) => {
          window.alert(
            error instanceof Error
              ? error.message
              : t("desktopUi.translate.openDroppedFileFailed"),
          );
        });
    });
  }, [t]);

  // Light/dark theme: applied globally via <html data-theme>, remembered across sessions.
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [themePreferenceLoaded, setThemePreferenceLoaded] = useState(false);

  useEffect(() => {
    setTheme(readInitialDesktopTheme());
    setThemePreferenceLoaded(true);
  }, []);

  useEffect(() => {
    if (!themePreferenceLoaded) return;

    document.documentElement.dataset.theme = theme;
    localStorage.setItem(DESKTOP_THEME_STORAGE_KEY, theme);
    // Keep the minimized recording widget (a separate window) in sync.
    window.ipc?.send("app-theme", theme);

    // Re-tint the opacity-controlled app root to match the theme.
    const savedOpacity = parseInt(localStorage.getItem("backgroundOpacity") ?? "", 10);
    const appElement = document.querySelector("[data-app-background]") as HTMLElement | null;
    if (appElement && !Number.isNaN(savedOpacity)) {
      appElement.style.backgroundColor = theme === "light"
        ? `rgba(246, 246, 248, ${savedOpacity / 100})`
        : `rgba(31, 31, 31, ${savedOpacity / 100})`;
    }
  }, [theme, themePreferenceLoaded]);

  useEffect(() => {
    const appElement = document.querySelector<HTMLElement>("[data-app-background]");
    if (!appElement) return;

    if (activeView === "recording") {
      appElement.dataset.liveSessionActive = "true";
    } else {
      delete appElement.dataset.liveSessionActive;
    }

    return () => {
      delete appElement.dataset.liveSessionActive;
    };
  }, [activeView]);

  // Home and recording keep independent window bounds. Tell the main process
  // which view is active so it can restore that view's size and coordinates.
  const previousWindowViewRef = useRef(activeView);
  useEffect(() => {
    if (typeof window !== "undefined") {
      const previousView = previousWindowViewRef.current;
      const mode = activeView === "recording" ? "recording" : "main";
      window.ipc?.send("window-view-mode", {
        mode,
        // Session completion can update several renderer states at once. Force
        // the saved Home bounds back on the native window even if the main
        // process already handled an earlier "main" transition.
        forceRestore: previousView === "recording" && mode === "main",
      });
      previousWindowViewRef.current = activeView;
    }
  }, [activeView]);

  // Drive the Electron mini recorder: show it while a meeting is live but the
  // full recording UI is not on screen (user browsing chat/translate/home).
  useEffect(() => {
    if (typeof window === "undefined") return;
    window.ipc?.send("recording-state", {
      meetingUiVisible: activeView === "recording",
    });
  }, [activeView]);

  useEffect(() => {
    setChatFocusState({ activeView });
  }, [activeView]);

  useEffect(() => {
    const syncFocus = () => {
      const windowFocused =
        typeof document !== "undefined" &&
        document.visibilityState === "visible" &&
        (typeof document.hasFocus !== "function" || document.hasFocus());
      setChatFocusState({ windowFocused });
      if (windowFocused) {
        clearChatBadge();
      }
    };
    syncFocus();
    window.addEventListener("focus", syncFocus);
    window.addEventListener("blur", syncFocus);
    document.addEventListener("visibilitychange", syncFocus);
    return () => {
      window.removeEventListener("focus", syncFocus);
      window.removeEventListener("blur", syncFocus);
      document.removeEventListener("visibilitychange", syncFocus);
    };
  }, []);

  useEffect(() => {
    const syncSidebarToWindow = () => {
      const expansionAllowed = window.innerWidth >= 900;
      setSidebarExpansionAllowed(expansionAllowed);
      if (!expansionAllowed) {
        setSidebarCollapsed(true);
      }
    };

    syncSidebarToWindow();
    window.addEventListener("resize", syncSidebarToWindow);

    const handleShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchOpen(true);
      }
      if (event.key === "Escape") setSearchOpen(false);
    };

    window.addEventListener("keydown", handleShortcut);
    return () => {
      window.removeEventListener("resize", syncSidebarToWindow);
      window.removeEventListener("keydown", handleShortcut);
    };
  }, []);

  useEffect(() => {
    if (!sidebarExpansionAllowed || SIDEBAR_FORCED_COLLAPSE_VIEWS.has(activeView)) {
      setSidebarCollapsed(true);
      return;
    }
    if (SIDEBAR_PREFERENCE_VIEWS.has(activeView)) {
      setSidebarCollapsed(readSidebarCollapsedPreference());
    }
  }, [activeView, sidebarExpansionAllowed]);

  const openExternal = (url = webAppUrl) => {
    if (typeof window !== "undefined") {
      window.electronAPI?.openExternal(url);
    }
  };

  const openSettings = (section: SettingsSection = "ai-notes") => {
    openSettingsAtMainWindowSize(section);
  };

  const openRecordingSettings = (
    launch?: ChatShareLaunchPayload,
    quotaMode: LiveSessionQuotaMode = launch?.shareCode ? "translation" : "current",
  ) => {
    if (blockLiveSessionIfExhausted(quotaMode)) return;
    setSettingsOpen(false);
    if (launch?.shareCode) {
      writeChatShareLaunch(launch);
    }
    if (!isLiveMeeting) {
      if (clearCalendarNoteContext()) syncRecorderPreferencesFromStorage();
      setRecordingCalendarContext(null);
      if (!launch?.shareCode) {
        setPreparedLiveSession(null);
        if (!launch) clearChatShareLaunch();
      }
      setAdhocSessionKey((key) => key + 1);
    }
    changeView("recording");
  };

  const changeView = (view: DesktopView) => {
    if (SIDEBAR_FORCED_COLLAPSE_VIEWS.has(view)) setSidebarCollapsed(true);

    if (view === activeView) {
      setSearchOpen(false);
      return;
    }

    if (view === "home") viewHistoryRef.current = [];
    else viewHistoryRef.current.push(activeView);

    if (view === "chat") setMeetingAssistantLaunch(null);
    if (view !== "chat") setChatLaunch(null);
    setActiveView(view);
    setSearchOpen(false);
  };

  const openVoiceTyping = async () => {
    try {
      const status = await getDesktopPermissionStatus();
      if (!hasMissingVoiceKeyboardPermission(status)) {
        changeView("voice-typing");
        return;
      }
    } catch {
      // Route to Setup when the permission state cannot be verified.
    }
    openPermissionSetup("voice-keyboard");
  };

  const showProductTourStep = useCallback((stepIndex: number) => {
    const step = PRODUCT_TOUR_STEPS[stepIndex];
    if (!step) return;
    const { target } = step;

    const showsSettings = target === "settings-panel" || target === "permission-setup";
    setSettingsOpen(showsSettings);
    if (target === "settings-panel") setSettingsSection("general");
    else if (target === "permission-setup") setSettingsSection("setup");
    setSearchOpen(false);
    setMeetingAssistantLaunch(null);
    setChatLaunch(null);
    viewHistoryRef.current = [];

    if (target === "recordings-page") setActiveView("recordings");
    else if (target === "translate-page") setActiveView("translate");
    else if (target === "chat-page") setActiveView("chat");
    else if (target === "people") setActiveView("people");
    else setActiveView("home");

    dispatch(setEnabled(step.id !== "noteTaking"));

    const collapseSidebar = target === "chat-page";
    setSidebarCollapsed(collapseSidebar);
    if (!collapseSidebar) writeSidebarCollapsedPreference(false);
    setProductTourStep(stepIndex);
  }, [dispatch]);

  const startProductTour = useCallback(() => {
    setProductTourRange(null);
    showProductTourStep(0);
  }, [showProductTourStep]);

  const startRecordingsTutorial = useCallback(() => {
    recordingsTourPreviousTranslationRef.current = translationEnabled;
    setProductTourRange(RECORDINGS_TOUR_RANGE);
    showProductTourStep(RECORDINGS_TOUR_RANGE.firstStepIndex);
  }, [showProductTourStep, translationEnabled]);

  const closeProductTour = useCallback((_completed: boolean) => {
    const isRecordingsTutorial = productTourRange !== null;
    setProductTourStep(null);
    setProductTourRange(null);
    setSettingsOpen(false);
    dispatch(setEnabled(
      isRecordingsTutorial
        ? recordingsTourPreviousTranslationRef.current
        : true,
    ));
    setSidebarCollapsed(false);
    writeSidebarCollapsedPreference(false);
    if (!isRecordingsTutorial && user?.id) {
      localStorage.setItem(
        `MeetingCopilot_product_tour_${PRODUCT_TOUR_STORAGE_VERSION}_${user.id}`,
        "seen",
      );
    }
  }, [dispatch, productTourRange, user?.id]);

  useEffect(() => {
    if (productTourRange !== null
      || productTourStep !== PRODUCT_TOUR_STEPS.length - 1) return;

    let cancelled = false;
    void getDesktopPermissionStatus()
      .then((status) => {
        if (!cancelled) setProductTourNeedsSetup(hasMissingRequiredDesktopPermission(status));
      })
      .catch(() => {
        if (!cancelled) setProductTourNeedsSetup(true);
      });
    return () => {
      cancelled = true;
    };
  }, [productTourRange, productTourStep]);

  const finishProductTour = useCallback(async (completed: boolean) => {
    if (!completed || productTourRange !== null) {
      closeProductTour(completed);
      return;
    }

    let needsSetup = true;
    try {
      const status = await getDesktopPermissionStatus();
      needsSetup = hasMissingRequiredDesktopPermission(status);
    } catch {
      // Route to Setup when readiness cannot be verified.
    }

    closeProductTour(true);
    if (needsSetup) openPermissionSetup("onboarding");
  }, [closeProductTour, openPermissionSetup, productTourRange]);

  const abortRecordingsTutorialAfterInteraction = useCallback(() => {
    setProductTourStep(null);
    setProductTourRange(null);
    setSettingsOpen(false);
    setSidebarCollapsed(false);
    writeSidebarCollapsedPreference(false);
  }, []);

  useEffect(() => {
    let cancelled = false;
    if (!user?.id) {
      setPermissionSetupOpen(false);
      setPermissionSetupResolved(false);
      return () => {
        cancelled = true;
      };
    }
    if (permissionSetupOpenTimerRef.current !== null) {
      return () => {
        cancelled = true;
      };
    }
    if (activeView !== "home" || isLiveMeeting || preparedLiveSession) {
      setPermissionSetupOpen(false);
      setPermissionSetupResolved(false);
      return () => {
        cancelled = true;
      };
    }
    const storageKey = `MeetingCopilot_permission_setup_${PERMISSION_SETUP_STORAGE_VERSION}_${user.id}`;
    const sessionKey = `${storageKey}_session`;
    const completed = localStorage.getItem(storageKey) === "completed";
    const dismissedThisSession = sessionStorage.getItem(sessionKey) === "dismissed";
    setPermissionSetupResolved(false);

    const productTourStorageKey = `MeetingCopilot_product_tour_${PRODUCT_TOUR_STORAGE_VERSION}_${user.id}`;
    if (!localStorage.getItem(productTourStorageKey)) {
      // New users see the product tour first. Its final action routes them to
      // Setup when any required permission is still missing.
      setPermissionSetupOpen(false);
      setPermissionSetupResolved(true);
      return () => {
        cancelled = true;
      };
    }

    void getDesktopPermissionStatus()
      .then((status) => {
        if (cancelled) return;
        const shouldOpenSetup = !dismissedThisSession
          && hasMissingRequiredDesktopPermission(status);
        setPermissionSetupOpen(shouldOpenSetup);
        if (shouldOpenSetup) {
          setPermissionSetupReason("onboarding");
          setSettingsSection("setup");
          setSettingsOpen(true);
        }
      })
      .catch(() => {
        if (cancelled) return;
        const shouldOpenSetup = !completed
          && !dismissedThisSession;
        setPermissionSetupOpen(shouldOpenSetup);
        if (shouldOpenSetup) {
          setPermissionSetupReason("onboarding");
          setSettingsSection("setup");
          setSettingsOpen(true);
        }
      })
      .finally(() => {
        if (cancelled) return;
        setPermissionSetupResolved(true);
        productTourAutoStartCheckedRef.current = false;
      });

    return () => {
      cancelled = true;
    };
  }, [activeView, isLiveMeeting, preparedLiveSession, user?.id]);

  const closePermissionSetup = useCallback((completed: boolean) => {
    if (user?.id) {
      const storageKey = `MeetingCopilot_permission_setup_${PERMISSION_SETUP_STORAGE_VERSION}_${user.id}`;
      if (completed) localStorage.setItem(storageKey, "completed");
      else sessionStorage.setItem(`${storageKey}_session`, "dismissed");
    }
    setPermissionSetupOpen(false);
  }, [user?.id]);

  const handlePermissionSetupClose = useCallback((completed: boolean) => {
    closePermissionSetup(completed);
    closeSettings();
  }, [closePermissionSetup, closeSettings]);

  useEffect(() => {
    if (!user?.id || productTourAutoStartCheckedRef.current) return;
    if (!permissionSetupResolved || permissionSetupOpen) return;
    if (activeView !== "home" || isLiveMeeting || preparedLiveSession) return;
    productTourAutoStartCheckedRef.current = true;

    const storageKey = `MeetingCopilot_product_tour_${PRODUCT_TOUR_STORAGE_VERSION}_${user.id}`;
    if (localStorage.getItem(storageKey)) return;
    const timer = window.setTimeout(startProductTour, 600);
    return () => window.clearTimeout(timer);
  }, [activeView, isLiveMeeting, permissionSetupOpen, permissionSetupResolved, preparedLiveSession, startProductTour, user?.id]);

  const openPricing = (referrer?: string) => {
    if (user?.isTeamAdmin) {
      setTeamBillingRequest(request => request + 1);
      changeView("team");
      return;
    }
    setPricingReferrer(referrer ?? "desktop-app");
    setPricingInitialViewMode(undefined);
    setShowPricingTeamTutorial(false);
    changeView("pricing");
  };

  const replacePricingWithTeamManagement = () => {
    // Team admins cannot use individual pricing. Replace that history entry so
    // Back returns to the page they visited before Pricing instead of entering
    // a Pricing → Team Management redirect loop.
    setTeamBillingRequest(request => request + 1);
    setActiveView("team");
    setSearchOpen(false);
  };
  const openPricingRef = useRef(openPricing);
  openPricingRef.current = openPricing;

  useEffect(() => {
    const handleOpenPricing = (event: Event) => {
      const pricingEvent = event as CustomEvent<DesktopOpenPricingDetail>;
      event.preventDefault();
      openPricingRef.current(pricingEvent.detail?.referrer);
    };

    window.addEventListener(DESKTOP_OPEN_PRICING_EVENT, handleOpenPricing);
    return () => window.removeEventListener(DESKTOP_OPEN_PRICING_EVENT, handleOpenPricing);
  }, []);

  const openChat = (chatId?: string, threadParentId?: string | null) => {
    setChatLaunch(chatId ? { chatId, threadParentId } : null);
    changeView("chat");
  };
  const openChatRef = useRef(openChat);
  openChatRef.current = openChat;
  const changeViewRef = useRef(changeView);
  changeViewRef.current = changeView;

  useEffect(() => {
    const cleanup = window.ipc?.on(
      "selection-translation-opened",
      (rawPayload: unknown) => {
        const payload = rawPayload as Partial<OpenTextTranslationRequest>;
        if (
          typeof payload?.sourceText !== "string" ||
          typeof payload?.translatedText !== "string" ||
          typeof payload?.targetLanguage !== "string"
        ) {
          return;
        }
        setTranslateTextRequest({
          id: Date.now(),
          sourceText: payload.sourceText,
          translatedText: payload.translatedText,
          targetLanguage: payload.targetLanguage,
          explanationText: typeof payload.explanationText === "string"
            ? payload.explanationText
            : undefined,
        });
        changeViewRef.current("translate");
      },
    );
    return cleanup;
  }, []);

  useEffect(() => {
    const handleOpenFromNotification = (event: Event) => {
      const detail = (event as CustomEvent<ChatNotificationClickPayload>).detail;
      if (!detail?.chatId) return;
      openChatRef.current(detail.chatId, detail.threadParentId ?? undefined);
    };
    window.addEventListener(
      DESKTOP_OPEN_CHAT_FROM_NOTIFICATION,
      handleOpenFromNotification,
    );
    return () => {
      window.removeEventListener(
        DESKTOP_OPEN_CHAT_FROM_NOTIFICATION,
        handleOpenFromNotification,
      );
    };
  }, []);

  // Mini recorder click (and other restore paths) should jump back to the live meeting UI.
  useEffect(() => {
    const cleanup = window.ipc?.on("focus-recording-view", () => {
      changeViewRef.current("recording");
    });
    return cleanup;
  }, []);

  // Stripe Checkout return via MeetingCopilot://payment-success / checkout-canceled
  useEffect(() => {
    const cleanupSuccess = window.ipc?.on("payment-success", (rawPayload: unknown) => {
      const payload = rawPayload as { sessionId?: string | null } | undefined;
      const sessionId =
        typeof payload?.sessionId === "string" && payload.sessionId.length > 0
          ? payload.sessionId
          : null;

      openPricingRef.current("desktop-app");
      setPricingRefreshKey((key) => key + 1);
      dispatch(setRefetchUser());
      if (sessionId) {
        setPurchaseFeedbackToken(sessionId);
      }
      // Webhook may lag a few seconds behind the deep-link return
      window.setTimeout(() => {
        setPricingRefreshKey((key) => key + 1);
        dispatch(setRefetchUser());
      }, 2500);
    });

    const cleanupCanceled = window.ipc?.on("checkout-canceled", () => {
      openPricingRef.current("desktop-app");
      setPricingRefreshKey((key) => key + 1);
    });

    const cleanupContentOpen = window.ipc?.on("open-shared-content", (rawPayload: unknown) => {
      const payload = parseDesktopContentOpenPayload(rawPayload);
      if (!payload) return;
      setSearchOpen(false);
      if (payload.content === "meeting") {
        setSelectedMeeting({ id: payload.id, title: payload.title });
        setActiveView("recording-detail");
        return;
      }
      setSelectedTranslatedFile(translatedFileItemFromDeepLink(payload));
      setActiveView("translated-file-detail");
    });

    // Tell the main process it is safe to deliver a callback captured during a
    // cold launch. Listener registration must happen before this handshake.
    window.ipc?.send("desktop-shell-protocol-ready", null);

    return () => {
      cleanupSuccess?.();
      cleanupCanceled?.();
      cleanupContentOpen?.();
    };
  }, [dispatch]);

  const goBackImmediately = () => {
    if (activeView === "chat" && meetingAssistantLaunch) {
      setMeetingAssistantLaunch(null);
    }
    if (activeView === "chat") setChatLaunch(null);
    setActiveView(viewHistoryRef.current.pop() ?? "home");
    setSearchOpen(false);
  };

  const goBack = () => {
    if (nestedBack) { nestedBack.onBack(); return; }
    const willLosePreparedAdhocLink = activeView === "recording"
      && Boolean(preparedLiveSession?.shareCode)
      && !recordingCalendarContext
      && !isLiveMeeting
      && !recordingStartRequested;
    if (willLosePreparedAdhocLink) {
      setShowAdhocLinkExitWarning(true);
      return;
    }
    goBackImmediately();
  };

  const discardPreparedAdhocLinkAndGoBack = () => {
    setShowAdhocLinkExitWarning(false);
    setPreparedLiveSession(null);
    clearChatShareLaunch();
    setAdhocSessionKey((key) => key + 1);
    goBackImmediately();
  };

  const startRecording = (calendarEvent?: CalendarNoteContext) => {
    requestRecordingStart(calendarEvent);
  };

  const openTranscribeWorkspace = () => {
    setTranscribeWorkspaceRequest((request) => request + 1);
    changeView("recordings");
  };

  const openMeetingDetail = (
    meetingId: string,
    title?: string,
    options?: { awaitReady?: boolean; replaceCurrentView?: boolean; initialTab?: PostMeetingDetailTab },
  ) => {
    setSelectedMeeting({
      id: meetingId,
      title: title?.trim() || "Untitled meeting",
      awaitReady: options?.awaitReady === true,
      initialTab: options?.initialTab,
    });
    if (options?.replaceCurrentView) {
      // Ending a live session is a terminal transition. Keep the screen that
      // launched the recorder as Back's destination; never put the ended
      // recorder itself into navigation history.
      setActiveView("recording-detail");
      setSearchOpen(false);
      return;
    }
    changeView("recording-detail");
  };

  const openTranslatedFile = (item: TranslateHistoryItem) => {
    if (item.kind === "text" && item.sourceText && item.translatedText) {
      setTranslateTextRequest({
        id: Date.now(),
        sourceText: item.sourceText,
        translatedText: item.translatedText,
        targetLanguage: item.targetLanguage,
        explanationText: item.explanation ?? undefined,
      });
      changeView("translate");
      return;
    }
    setSelectedTranslatedFile(item);
    changeView("translated-file-detail");
  };

  const openRecordingListItem = (
    meetingId: string,
    title?: string,
    context?: RecordingOpenContext,
  ) => {
    if (!context?.live) {
      openMeetingDetail(meetingId, title);
      return;
    }

    if (context.sessionSource !== "chrome-extension") {
      if (isLiveMeeting) {
        changeView("recording");
        return;
      }

      // A stale desktop session can remain marked live after it is deleted or
      // interrupted. Keep it in-app so the detail view can show the unavailable
      // state instead of unexpectedly launching the web application.
      openMeetingDetail(meetingId, title);
      return;
    }

    // Keep Chrome-extension sessions in the desktop app too. The recording
    // detail surface already renders and polls the live/pending state, matching
    // the web meeting page without unexpectedly launching a browser window.
    openMeetingDetail(meetingId, title);
  };

  const returnToLiveSession = () => {
    if (isLiveMeeting || recordingStartRequested) {
      if (recordingCalendarContext) setCalendarNoteContext(recordingCalendarContext);
      changeView("recording");
      return;
    }
    if (!availableLiveSession) return;
    if (recordingCalendarContext) setCalendarNoteContext(recordingCalendarContext);
    openRecordingListItem(
      availableLiveSession.id,
      availableLiveSession.title,
      {
        live: true,
        sessionSource: availableLiveSession.sessionSource,
      },
    );
  };

  const openMeetingAssistant = (launch: DesktopMeetingAssistantLaunch) => {
    viewHistoryRef.current.push(activeView);
    setMeetingAssistantLaunch(launch);
    setActiveView("chat");
    setSearchOpen(false);
  };

  const previousView = viewHistoryRef.current.at(-1);
  const previousViewDetails = previousView
    ? {
        home: { label: t("desktopSidebar.nav.home"), icon: Home },
        recordings: { label: t("desktopSidebar.nav.recordings"), icon: Mic2 },
        recording: { label: t("desktopSidebar.nav.recording"), icon: Mic2 },
        "recording-detail": { label: t("desktopRecordingDetail.tabs.transcript"), icon: FileText },
        people: { label: t("desktopSidebar.features.people"), icon: UsersRound },
        "person-detail": { label: t("desktopSidebar.features.people"), icon: UsersRound },
        "translated-file-detail": { label: t("desktopTranslate.history.title"), icon: FileText },
        "voice-typing": { label: t("desktopVoiceTyping.title"), icon: Mic2 },
        chat: { label: t("desktopSidebar.nav.chat"), icon: MessageCircle },
        translate: { label: t("desktopSidebar.nav.translate"), icon: Languages },
        learning: { label: t("desktopSidebar.nav.learning"), icon: BookOpen },
        team: { label: t("desktopSidebar.nav.team"), icon: UsersRound },
        pricing: { label: t("desktopPricing.shell.title"), icon: Sparkles },
        referral: { label: t("desktopReferral.title"), icon: Gift },
      }[previousView]
    : undefined;
  // Desktop windows keep the sidebar toggle visible at every width. The
  // content can compact independently; the desktop chrome should not vanish.
  const showSidebarToggle = !transcriptPresentationMode;
  const minutesUpgradeSnapshot = {
    translationUsage: user?.translationUsage ?? 0,
    transcriptionUsage: user?.transcriptionUsage ?? 0,
    translationLimit: user?.translationLimit ?? null,
    transcriptionLimit: user?.transcriptionLimit ?? null,
  };
  const minutesUpgradeKey = minutesUpgradeDismissKey(user?.id, liveQuota, minutesUpgradeSnapshot);
  const showMinutesUpgradeCoachmark = Boolean(user)
    && forceMinutesUpgrade
    && !recordingMenuOpen
    && !permissionSetupOpen
    && productTourStep === null
    && !isLiveMeeting
    && activeView !== "recording"
    && activeView !== "pricing"
    && liveQuota.variant !== "ok";

  return (
    <DesktopBackNavigation.Provider value={setNestedBack}>
    <div className={`desktop-ui relative flex h-full w-full overflow-hidden transition-colors duration-300 ${
      // During a recording session the shell is transparent down to the
      // [data-app-background] root so the Background Opacity setting shows through.
      activeView === "recording" ? "min-h-0 bg-transparent" : "min-h-[620px] bg-[#0B0B0D]"
    }`}>
      <DesktopChatNotificationProvider />
      <LearningSync userId={user?.id} />
      <DesktopMeetingNotificationProvider
        onOpenMeeting={openMeetingSession}
        onStartMeeting={requestRecordingStart}
        onOpenNotificationSettings={() => openSettings("notifications")}
      />
      {/* Full-height sidebar: its right border runs to the very top of the
          window, and its content starts just below the traffic lights. */}
      {activeView !== "recording" && (
        <DesktopSidebar
          activeView={activeView}
          collapsed={sidebarCollapsed}
          email={user?.email}
          firstName={user?.firstName}
          lastName={user?.lastName}
          tier={user?.currentTier}
          onChangeView={changeView}
          onOpenExternal={openExternal}
          onOpenSettings={openSettings}
          onManageTeam={() => { setTeamBillingRequest(0); changeView("team"); }}
          onOpenSearch={() => setSearchOpen(true)}
          onOpenPricing={() => openPricing("desktop-app")}
          theme={theme}
          onToggleTheme={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
          productTourTarget={productTourTarget}
          onStartProductTour={startProductTour}
        />
      )}

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <DesktopTopBar
          backDestination={nestedBack ? { label: nestedBack.label, icon: BookOpen } : activeView !== "home" && activeView !== "recording" ? previousViewDetails : undefined}
          onGoBack={activeView !== "home" && activeView !== "recording" ? goBack : undefined}
          onStartRecording={() => startRecording()}
          onUploadFile={openTranscribeWorkspace}
          onOpenLiveSession={(quotaMode) => openRecordingSettings(undefined, quotaMode)}
          onOpenMeetingNotesSettings={() => openSettings("ai-notes")}
          translationSessionBlocked={liveQuota.translationExhausted}
          transcriptionSessionBlocked={liveQuota.transcriptionExhausted}
          onLiveSessionBlocked={() => setForceMinutesUpgrade(true)}
          onRecordingMenuOpenChange={setRecordingMenuOpen}
          isLiveMeeting={
            activeView !== "recording" &&
            (isLiveMeeting || recordingStartRequested || Boolean(availableLiveSession))
          }
          onReturnToMeeting={returnToLiveSession}
          // With the compact sidebar, the top bar starts only 56px from the
          // window edge. Reserve a slot for the sidebar toggle so back controls
          // begin after it instead of merging with it. The expanded sidebar
          // already provides that horizontal clearance itself.
          reserveTrafficLights={isMac && (sidebarCollapsed || activeView === "recording") && !transcriptPresentationMode}
          showWindowControls={!isMac && !transcriptPresentationMode}
          onExitTranscriptPresentation={transcriptPresentationMode
            ? exitTranscriptPresentation
            : undefined}
          showCapture={activeView !== "recording"}
          productTourTarget={productTourTarget}
        />

        <div className="relative min-h-0 flex-1">
          {/* Translation jobs are global workspace state. Keep the translator
              mounted while another view is open so uploads, processing, and
              results survive navigation just like the live meeting session. */}
          <div
            data-product-tour="translate-page"
            className={activeView === "translate" ? "animate-view-fade h-full min-h-0" : "hidden"}
            aria-hidden={activeView !== "translate"}
          >
            <DesktopTranslate
              openTranslateRequest={translateWorkspaceRequest}
              openFilesRequest={translateFilesRequest}
              openHistoryRequest={translateHistoryRequest}
              openFileRequest={dockFileRequest}
              openTextRequest={translateTextRequest}
              onOpenPricing={() => openPricing("desktop-app")}
              onOpenTranslatedFile={openTranslatedFile}
              externalHistoryRefreshKey={translatedHistoryRefreshKey}
            />
          </div>

          {/* Other pages remain keyed so the fade replays on every switch.
              AudioRecorder also lives outside this node so neither long-running
              workspace is remounted by ordinary page navigation. */}
          {activeView !== "recording" && activeView !== "translate" && (
            <div key={activeView} className="animate-view-fade h-full min-h-0">
              {activeView === "learning" && <DesktopLearning key={user?.id} userId={user?.id} />}
              {activeView === "home" && (
                <DesktopHome
                  firstName={user?.firstName}
                  isLiveMeeting={
                    isLiveMeeting ||
                    recordingStartRequested ||
                    Boolean(availableLiveSession)
                  }
                  onOpenMeetingMemo={openMeetingSession}
                  onConfigureMeeting={(calendarEvent, panel) =>
                    openMeetingSession(calendarEvent, "Notes", panel)
                  }
                  onOpenChat={openChat}
                  onOpenVoiceTyping={() => void openVoiceTyping()}
                  onOpenWebApp={() => openExternal()}
                  onViewAllRecordings={() => changeView("recordings")}
                  onStartRecordingsTutorial={startRecordingsTutorial}
                  onViewAllTranslatedFiles={() => {
                    setTranslateHistoryRequest((request) => request + 1);
                    changeView("translate");
                  }}
                  onStartTranslation={() => {
                    setTranslateWorkspaceRequest((request) => request + 1);
                    changeView("translate");
                  }}
                  onOpenTranslatedFile={openTranslatedFile}
                  onOpenMeeting={openRecordingListItem}
                />
              )}
              {activeView === "recordings" && (
                <div data-product-tour="recordings-page" className="h-full min-h-0">
                  <DesktopRecordings
                    maximizeTranscribeRequest={transcribeWorkspaceRequest}
                    onOpenMeeting={openRecordingListItem}
                    onOpenPricing={() => openPricing("desktop-app")}
                    onStartRecordingsTutorial={startRecordingsTutorial}
                  />
                </div>
              )}
              {activeView === "people" && (
                <div data-product-tour="people-page" className="h-full min-h-0">
                  <DesktopPeople
                    onOpenPerson={(person) => {
                      setSelectedPerson(person);
                      changeView("person-detail");
                    }}
                  />
                </div>
              )}
              {activeView === "person-detail" && selectedPerson && (
                <DesktopPersonDetail
                  selectedPerson={selectedPerson}
                  onOpenMeeting={(meetingId, title) => openMeetingDetail(meetingId, title)}
                  onOpenChat={(chatId) => openChat(chatId)}
                  onOpenTranslatedFile={openTranslatedFile}
                />
              )}
              {activeView === "recording-detail" && selectedMeeting && (
                <DesktopRecordingDetail
                  meetingId={selectedMeeting.id}
                  title={selectedMeeting.title}
                  awaitReady={selectedMeeting.awaitReady}
                  initialTab={selectedMeeting.initialTab}
                  onOpenAssistant={openMeetingAssistant}
                  onOpenPricing={(referrer) => openPricing(referrer ?? "desktop-app")}
                />
              )}
              {activeView === "chat" && (
                meetingAssistantLaunch
                  ? <DesktopMeetingAssistant launch={meetingAssistantLaunch} />
                  : (
                    <div data-product-tour="chat-page" className="h-full min-h-0">
                      <DesktopChat
                        launchTarget={chatLaunch}
                        onLaunchConsumed={() => setChatLaunch(null)}
                        onOpenMeeting={openMeetingDetail}
                        onOpenLiveSession={openRecordingSettings}
                      />
                    </div>
                  )
              )}
              {activeView === "translated-file-detail" && selectedTranslatedFile && (
                <TranslatedFileDetail
                  item={selectedTranslatedFile}
                  onRenamed={(item) => {
                    setSelectedTranslatedFile(item);
                    setTranslatedHistoryRefreshKey((key) => key + 1);
                  }}
                  onRetranslated={(item) => {
                    setSelectedTranslatedFile(item);
                    setTranslatedHistoryRefreshKey((key) => key + 1);
                  }}
                />
              )}
              {activeView === "voice-typing" && (
                <div data-product-tour="voice-keyboard" className="h-full min-h-0">
                  <DesktopVoiceTyping />
                </div>
              )}
              {activeView === "team" && user?.isTeamAdmin && (
                <DesktopTeam onOpenExternal={openExternal} billingRequest={teamBillingRequest} />
              )}
              {activeView === "referral" && <DesktopReferral />}
              {activeView === "pricing" && (
                showPricingTeamTutorial ? (
                  <TeamTutorial
                    context="pricing"
                    onBack={() => setShowPricingTeamTutorial(false)}
                  />
                ) : (
                  <DesktopPricing
                    key={pricingRefreshKey}
                    referrer={pricingReferrer}
                    initialViewMode={pricingInitialViewMode}
                    onOpenExternal={openExternal}
                    isTeamAdmin={user?.isTeamAdmin === true}
                    onOpenTeamManagement={replacePricingWithTeamManagement}
                    onOpenTeamTutorial={() => {
                      setPricingInitialViewMode("teams-enterprise");
                      setShowPricingTeamTutorial(true);
                    }}
                    onUpgraded={() => dispatch(setRefetchUser())}
                    onRequestPurchaseFeedback={setPurchaseFeedbackToken}
                  />
                )
              )}
            </div>
          )}
          {(activeView === "recording" || isLiveMeeting || preparedLiveSession) && (
            <div
              className={
                activeView === "recording"
                  ? "h-full min-h-0 overflow-hidden bg-transparent"
                  : "hidden"
              }
              aria-hidden={activeView !== "recording"}
            >
              <AudioRecorder
                key={recordingCalendarContext?.calendarEventId || recordingCalendarContext?.id || `adhoc-live-session-${adhocSessionKey}`}
                autoStart={recordingStartRequested}
                initialTab={recordingInitialTab}
                panelRequest={recordingPanelRequest}
                calendarContext={recordingCalendarContext}
                initialShareInvitation={languageChangeReload}
                onLiveMeetingChange={handleLiveMeetingChange}
                onTranscriptMaximizedChange={setIsTranscriptMaximized}
                onShareInvitationChange={setPreparedLiveSession}
                onPermissionSetupRequired={() => openPermissionSetup("live-audio")}
                onRecordingDiscarded={() => {
                  setIsLiveMeeting(false);
                  setPreparedLiveSession(null);
                  setAvailableLiveSession(null);
                  setRecordingStartRequested(false);
                  setActiveView("home");
                  setSearchOpen(false);
                }}
                onRecordingComplete={(meetingId, title, initialTab) => {
                  setIsLiveMeeting(false);
                  setPreparedLiveSession(null);
                  setAvailableLiveSession((session) =>
                    session?.id === meetingId ? null : session,
                  );
                  openMeetingDetail(meetingId, title, {
                    awaitReady: true,
                    replaceCurrentView: true,
                    initialTab,
                  });
                }}
              />
            </div>
          )}

          {showMinutesUpgradeCoachmark && liveQuota.variant !== "ok" && (
            <DesktopMinutesUpgradeCoachmark
              variant={liveQuota.variant}
              onDismiss={() => {
                try {
                  sessionStorage.setItem("MeetingCopilot_minutes_upgrade_dismissed", minutesUpgradeKey);
                } catch {
                  // Ignore storage failures; dismiss still lasts for this session in memory.
                }
                setMinutesUpgradeDismissedKey(minutesUpgradeKey);
                setForceMinutesUpgrade(false);
              }}
              onUpgrade={() => {
                setForceMinutesUpgrade(false);
                openPricing("minutes-exhausted");
              }}
            />
          )}

          {!permissionSetupOpen && productTourStep !== null && (
            <DesktopProductTour
              stepIndex={productTourStep}
              onStepChange={showProductTourStep}
              onClose={(completed) => void finishProductTour(completed)}
              firstStepIndex={productTourRange?.firstStepIndex}
              lastStepIndex={productTourRange?.lastStepIndex}
              allowTargetInteraction={productTourRange !== null}
              onTargetInteraction={abortRecordingsTutorialAfterInteraction}
              skipCloses={productTourRange !== null}
              needsSetupOnFinish={productTourRange === null && productTourNeedsSetup}
            />
          )}
        </div>
      </div>

      {/* Collapse / back chevron pinned to the window's top-left, right of the
          traffic lights — its position never changes with the layout. */}
      {/* Height tuned so the icon's vertical center (~19px) sits level with the
          native macOS traffic lights (trafficLightPosition y:11). */}
      {showSidebarToggle && <div style={noDrag} className={`absolute left-0 top-0 z-40 flex h-[38px] items-center ${isMac ? "pl-[78px]" : "pl-3"}`}>
        <button
          type="button"
          onClick={() => {
            if (activeView === "recording") {
              goBack();
              return;
            }
            setSidebarCollapsed((collapsed) => {
              const next = !collapsed;
              writeSidebarCollapsedPreference(next);
              return next;
            });
          }}
          className="flex h-7 w-7 items-center justify-center rounded-lg text-[#D8D8DC] transition-colors hover:bg-white/10 hover:text-white"
          aria-label={activeView === "recording"
            ? t("desktopSidebar.chrome.backToPreviousPage")
            : sidebarCollapsed
              ? t("desktopSidebar.chrome.expandNavigation")
              : t("desktopSidebar.chrome.collapseNavigation")}
        >
          {activeView === "recording" ? <ChevronLeft size={17} /> : <SidebarToggleIcon open={!sidebarCollapsed} />}
        </button>
      </div>}

      {searchOpen && (
        <div className="absolute inset-0 z-50 flex items-start justify-center bg-black/60 px-5 pt-[12vh] backdrop-blur-[4px]" onMouseDown={() => setSearchOpen(false)}>
          <div className="w-full max-w-[560px] overflow-hidden rounded-2xl border border-[#38383F] bg-[#17171A] shadow-[0_24px_70px_rgba(0,0,0,0.55)]" onMouseDown={(event) => event.stopPropagation()}>
            <div className="flex h-14 items-center gap-3 border-b border-[#2C2C31] px-4">
              <Search size={18} className="text-[#85858F]" />
              <input
                autoFocus
                placeholder={t("desktopSidebar.search.palettePlaceholder")}
                className="min-w-0 flex-1 appearance-none border-0 bg-[#17171A] text-sm text-[#F0F0F2] outline-none placeholder:text-[#777780] focus:bg-[#17171A]"
                style={{
                  backgroundColor: theme === "light" ? "#FFFFFF" : "#17171A",
                  color: theme === "light" ? "#111827" : "#F0F0F2",
                  WebkitTextFillColor: theme === "light" ? "#111827" : "#F0F0F2",
                  colorScheme: theme,
                }}
              />
              <button type="button" onClick={() => setSearchOpen(false)} className="flex h-7 w-7 items-center justify-center rounded-lg text-[#8B8B95] hover:bg-[#25252A]" aria-label={t("desktopSidebar.search.closeSearch")}><X size={15} /></button>
            </div>
            <div className="p-2">
              <p className="px-3 pb-2 pt-1 text-[10px] font-bold uppercase tracking-[0.12em] text-[#A0A0A8]">{t("desktopSidebar.search.quickNavigation")}</p>
              {[
                { view: "home" as const, icon: Home },
                { view: "recordings" as const, icon: Mic2 },
                { view: "chat" as const, icon: MessageCircle },
                { view: "translate" as const, icon: Languages },
                { view: "learning" as const, icon: BookOpen },
              ].map(({ view, icon: Icon }) => (
                <button key={view} type="button" onClick={() => changeView(view)} className="flex w-full items-center gap-3 rounded-xl p-3 text-left hover:bg-[#24242A]">
                  <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-[#292955] text-[#A5A7FF]"><Icon size={17} /></span>
                  <span className="flex-1">
                    <span className="block text-sm font-bold text-[#F0F0F2]">{t(`desktopSidebar.nav.${view}`)}</span>
                    <span className="block text-xs text-[#8D8D96]">{t(`desktopSidebar.search.descriptions.${view}`)}</span>
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {showAdhocLinkExitWarning && (
        <DesktopConfirmDialog
          title={t("desktopSidebar.adhocLinkExit.title")}
          description={(
            <div className="space-y-2">
              <p>{t("desktopSidebar.adhocLinkExit.permanentIntro")}</p>
              <ul className="list-disc space-y-1 pl-5">
                <li>{t("desktopSidebar.adhocLinkExit.createCalendarEvent")}</li>
                <li>{t("desktopSidebar.adhocLinkExit.generateFromEvent")}</li>
              </ul>
            </div>
          )}
          cancelLabel={t("desktopSidebar.adhocLinkExit.stay")}
          confirmLabel={t("desktopSidebar.adhocLinkExit.leave")}
          danger
          dismissOnBackdrop={false}
          onCancel={() => setShowAdhocLinkExitWarning(false)}
          onConfirm={discardPreparedAdhocLinkAndGoBack}
        />
      )}

      <DesktopSettingsModal
        open={settingsOpen}
        initialSection={settingsSection}
        onClose={() => {
          closeSettings();
          if (permissionSetupOpen) closePermissionSetup(false);
        }}
        onOpenExternal={openExternal}
        onOpenRecordingUI={openRecordingSettings}
        onPermissionSetupClose={handlePermissionSetupClose}
        permissionSetupOnboarding={permissionSetupOpen}
        permissionSetupReason={permissionSetupReason}
        onOpenPricing={openPricing}
        productTourActive={productTourTarget === "settings-panel"
          || productTourTarget === "permission-setup"}
      />

      {purchaseFeedbackToken && user?.id && (
        <DesktopPurchaseFeedbackModal
          show
          copy={pricingCopy}
          userId={user.id}
          userEmail={user.email || ""}
          verificationToken={purchaseFeedbackToken}
          onOpenExternal={openExternal}
          onClose={() => setPurchaseFeedbackToken(null)}
          onSubmitted={() => dispatch(setRefetchUser())}
        />
      )}
    </div>
    </DesktopBackNavigation.Provider>
  );
}
