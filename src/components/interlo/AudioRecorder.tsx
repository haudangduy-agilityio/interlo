import { splitTranscriptSentences } from '../../../shared/transcriptSentences';
import { userFacingError } from '../../utils/userFacingError'
import { productionLogger } from '../../lib/productionLogger'
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { io, Socket } from "socket.io-client";
import { useTranslation } from "../../hooks/useTranslation";
import { baseURL, wsURL } from "../../lib/baseURL";
import { useAppDispatch, useAppSelector } from "../../redux/hooks";
import { setBulletPointEnabled, setEnabled, setIsRecording as setReduxIsRecording } from "../../redux/slices/recorderSlice";
import { setClients, setJoined } from "../../redux/slices/sharingSlice";
import { languages, microsoftSpokenLanguages, spokenLanguages } from "../../utils/languages";
import { WEB_URL } from "../../utils/WebApp";
import Bullet from './Bullet';
import Controllers from "./Controllers";
import Loading from "./Loading";
import { resolvePostMeetingDestination, type PostMeetingDetailTab, type PostMeetingDestination } from "../../utils/postMeetingDestination";
import MeetingDetails from "./MeetingDetails";
import MicDisconnectModal from "./MicDisconnectModal";
import NotionEditor from "./NotionEditor";
import Tabs from "./Tabs";
import TranslationLatencyMonitor from "./TranslationLatencyMonitor";
import Transcript from "./Transcript";
import RealtimeTranslatedSpeechPlayer from "./RealtimeTranslatedSpeechPlayer";
import UpgradeNow from './UpgradeNow';
import {
  applyMicrophoneConfig,
  autoConfigureForMode,
  getStoredOtherAudioCaptureMode,
  isInPersonSpeechContext,
  isPhysicalCaptureDevice,
  pickPhysicalMicrophone,
  resolvePhysicalMicrophoneDeviceId,
  type OtherAudioCaptureMode,
} from "../../utils/audioRoutingAutoConfig";
import {
  applySessionEnvironment,
  ENVIRONMENT_CHANGED_EVENT,
  getLastOnlineSpeechMode,
  getRuntimeSessionEnvironment,
  getStoredSessionEnvironment,
  isInPersonSessionEnvironment,
  isOnlineSpeechMode,
  parseEnvironmentChangedEvent,
  rememberLastOnlineSpeechMode,
  speechModeToEnvironment,
  type SessionEnvironment,
} from "../../utils/sessionEnvironmentSync";
import {
  resolveRealtimeSpeakerName,
} from "../../utils/realtimeTranslation/bothModeSpeakerResolver";
import {
  resolveDisplayLanguageToOpenAi,
  resolveIncomingSpeechLanguageLabel,
} from "../../utils/realtimeTranslation/bothModeUplinkLanguage";
import { StreamingPcmDownsampler } from "../../utils/streamingPcmDownsampler";
import {
  mergeFastRealtimeIntoTranscript as mergeFastRealtimeSections,
  mergeFastServerTranscriptData,
} from "../../utils/fastRealtimeTranscriptMerge";
import { filterS2sOffBleedSections } from "../../utils/fastTranscriptBleedFilter";
import { readVoicePlaybackWait, subscribeVoicePlaybackWait } from '../../utils/voicePlaybackQuality';
import {
  isDualStreamCaptureActive,
  prepareDualPcmForTtsCapture,
  shouldSuppressMergedChunkForTts,
} from "../../utils/dualStreamTtsCaptureSuppression";
import {
  DELAY_ANALYSIS_CONTROLS_VISIBILITY_CHANGED_EVENT,
  getStoredDelayAnalysisControlsVisible,
  isDelayAnalysisAdminEmail,
} from "../../utils/delayAnalysisControls";
import {
  readCalendarNoteDraft,
  saveCalendarNoteDraft,
  type CalendarNoteContext,
  type LiveSessionPanelRequest,
} from "../../components/desktop-home/calendarNoteContext";
import {
  constrainTranslationQualityForMode,
  readStoredTranslationQuality,
  TRANSCRIPTION_MINIMUM_SECONDS,
  writeStoredTranslationQuality,
} from "../../utils/translationQualityStorage";
import { updateOperationalContextFromMeeting } from "../../lib/operationalContextApi";
import { getLiveTranslationInvitation, reserveLiveTranslationInvitation } from "../../lib/liveTranslationInvitationsApi";
import {
  consumeChatShareLaunch,
  getCalendarLiveTranslationInvitationCacheKey,
  purgeUnboundLiveTranslationStorage,
  readCachedLiveTranslationInvitation,
  writeLanguageChangeLiveSessionReload,
  writeRestoreRecordingViewOnReload,
  writeCachedLiveTranslationInvitation,
} from "../../lib/liveTranslationInvitationStorage";
import { getLiveTranslationChatCacheKey } from "./ShareMeeting";
import type { SharingLanguagePolicyPayload } from "../../lib/sharingLanguageLimits";
import { ATTENDEE_EMAIL_CHANGED_EVENT, readRequireAttendeeEmail, readPostMeetingSharing } from "../../lib/attendeeEmailPreference";
import {
  shouldSuppressSameLanguageOutput,
  useSameLanguageOutputPreference,
} from "../../hooks/useSameLanguageOutputPreference";
import {
  getDesktopPermissionStatus,
  hasMissingLiveAudioPermission,
} from "../../components/desktop-home/desktopPermissionStatus";
import { useLiveTranslationOnlyPreference } from "../../hooks/useLiveTranslationOnlyPreference";
import { useTranslationSplitTokenGuardPreference } from "../../hooks/useTranslationSplitTokenGuardPreference";
import {
  SAVED_RECORDING_FALLBACK_MIME_TYPE,
  SAVED_RECORDING_MIME_TYPE,
  createSavedRecordingGraph,
  getSavedRecordingOptions,
  stopMediaStreamTracks,
  stopMediaRecorderAndWait,
} from "../../lib/savedRecordingAudio";

const ALL_SPOKEN_LANGUAGE_OPTIONS = [...spokenLanguages, ...microsoftSpokenLanguages];

// Platform detection utility
const INACTIVITY_WARNING_TIME = 240 * 1000; // 4 minutes
const LEGACY_DELAY_ANALYSIS_STORAGE_KEY = 'MeetingCopilot_delay_analysis_enabled';
const TRANSLATION_DELAY_ANALYSIS_STORAGE_KEY = 'MeetingCopilot_translation_delay_analysis_enabled';
const STT_DELAY_ANALYSIS_STORAGE_KEY = 'MeetingCopilot_stt_delay_analysis_enabled';
const TRANSLATION_QUALITY_AUTO_ADJUST_STORAGE_KEY = 'MeetingCopilot_translation_quality_auto_adjust_enabled';
const TRANSLATION_MODE_STORAGE_KEY = 'MeetingCopilot_translation_mode';
const REAL_TIME_MEMORY_STORAGE_KEY = 'MeetingCopilot_real_time_memory_enabled';
const POST_MEETING_CONTENT_SHARING_STORAGE_KEY = 'createPostMeetingChat';
const POST_MEETING_CONTENT_SHARING_CHANGED_EVENT = 'createPostMeetingChatChanged';
const TEXT_TO_SPEECH_STORAGE_KEY = 'MeetingCopilot_translation_text_to_speech_enabled';
const TEXT_TO_SPEECH_SESSION_MODE_STORAGE_KEY = 'MeetingCopilot_translation_text_to_speech_session_mode';
const SPEECH_TO_SPEECH_MODE_STORAGE_KEY = 'MeetingCopilot_speech_to_speech_mode';
const TRANSLATED_SPEECH_PLAYBACK_EVENT = 'translatedSpeechPlaybackStateChanged';
const MEETING_AUDIO_CAPTURE_STORAGE_KEY = 'selectedMeetingAudioCaptureDevice';
const MEET_UPLINK_TTS_OWN_VOICE_KEY = 'MeetingCopilot_meet_uplink_tts_own_voice';
const MEET_UPLINK_TTS_OWN_VOICE_EVENT = 'MeetingCopilotMeetUplinkTtsOwnVoiceChanged';
const LOCAL_AUDIO_SOURCE_SAMPLE_INTERVAL_MS = 100;
const LOCAL_AUDIO_SOURCE_HISTORY_MS = 30 * 60 * 1000;
const LOCAL_AUDIO_SOURCE_LOOKAHEAD_MS = 350;
const LOCAL_AUDIO_SOURCE_LOOKBEHIND_MS = 350;
const LOCAL_AUDIO_SOURCE_RECENT_FALLBACK_MS = 1500;
const LOCAL_AUDIO_SOURCE_MIN_LEVEL = 0.004;
const LOCAL_AUDIO_SOURCE_MIC_MIN_LEVEL = 0.012;
const LOCAL_AUDIO_SOURCE_MIC_DOMINANCE_RATIO = 3.0;
const LOCAL_AUDIO_SOURCE_SYSTEM_PRIORITY_RATIO = 0.45;
const LOCAL_AUDIO_SOURCE_SWITCH_CONFIRM_MS = 250;
/** When meeting audio uses a dedicated virtual device, require stronger mic dominance before labeling as self-speech. */
const LOCAL_AUDIO_SOURCE_VIRTUAL_MIC_DOMINANCE_RATIO = 5.5;
const LOCAL_AUDIO_SOURCE_VIRTUAL_MIC_MIN_LEVEL = 0.018;
/**
 * Dedicated virtual meeting capture: treat a chunk as meeting (system) unless mic RMS peak
 * clearly dominates system peak (reduces false "You" when room/mic picks up meeting audio).
 */
const VIRTUAL_CHUNK_MIC_DOMINANCE_OVER_SYSTEM = 2.0;
const PCM_BYTES_PER_MS = 32;
const SHARED_ASR_PRE_ROLL_MS = 1_500;

/** Shared Socket.IO options: bounded retries + jitter to avoid reconnect storms (server cost). */
const DESKTOP_SOCKET_IO_OPTIONS = {
  reconnection: true,
  reconnectionAttempts: 25,
  reconnectionDelay: 1000,
  reconnectionDelayMax: 30000,
  randomizationFactor: 0.5,
} as const;

function getDesktopSocketAuth(): { token: string } {
  if (typeof window === "undefined") return { token: "" };
  return { token: localStorage.getItem("accessToken") || "" };
}

type SpokenLanguagePlan = 'basic' | 'premium';
type SpeechToSpeechMode = 'off' | 'online-incoming' | 'online-outgoing' | 'online-both' | 'in-person';

const getStoredSpokenLanguagePlan = (): SpokenLanguagePlan => {
  if (typeof window === 'undefined') return 'basic';
  try {
    const config = localStorage.getItem('MeetingCopilot_config');
    if (!config) return 'basic';
    const parsed = JSON.parse(config);
    return parsed?.spokenLanguagePlan === 'premium' ? 'premium' : 'basic';
  } catch {
    return 'basic';
  }
};

const getStoredTranslationMode = () => {
  if (typeof window === 'undefined') return 'single';
  const storedMode = localStorage.getItem(TRANSLATION_MODE_STORAGE_KEY);
  return storedMode === 'fast' || storedMode === 'multi' || storedMode === 'single' ? storedMode : 'single';
};

const isRealTimeMemoryEligible = (plan: string, mode: string) =>
  plan === 'premium' && mode !== 'fast';

const getTranslationQualityBounds = (plan: SpokenLanguagePlan, model?: string) => {
  if (model === 'microsoft' || plan === 'premium') {
    return { min: 0.1, max: 5.0, defaultValue: 2.0 };
  }
  return { min: 0.1, max: 5.0, defaultValue: 2.0 };
};

const clampTranslationQuality = (value: number, plan: SpokenLanguagePlan, model?: string) => {
  const bounds = getTranslationQualityBounds(plan, model);
  const fallback = Number.isFinite(value) ? value : bounds.defaultValue;
  return Math.min(bounds.max, Math.max(bounds.min, fallback));
};

/** Minimal shape of the Electron preload bridge exposed on `window.ipc` (absent in the browser). */
type ElectronIpcBridge = {
  getAppInfo: () => { version?: string } | undefined;
  send: (channel: string, payload?: unknown) => void;
};

/** Single typed accessor for the optional Electron bridge. */
const getElectronIpc = (): ElectronIpcBridge | undefined =>
  (window as Window & { ipc?: ElectronIpcBridge }).ipc;

/** Opaque slice of the app store: only the fields this component selects are declared. */
type AppRootState = {
  user: unknown;
  recorder: { isRecording: boolean };
  sharing: unknown;
};

type AudioSourceLabel = 'mic' | 'system' | 'unknown';
type AudioSourceSegmentMeta = { source?: string; [key: string]: unknown };
/** Per-chunk capture-source metadata sent alongside each audio chunk. */
type AudioSourceMeta = {
  source: AudioSourceLabel;
  segments?: AudioSourceSegmentMeta[];
  [key: string]: unknown;
};

type TimestampLike = Date | number | string | null | undefined;
type CaptionLike = { start?: TimestampLike; text?: string; [key: string]: unknown };
/** Loose transcript section as received from the server before presentation overrides. */
type TranscriptSectionLike = {
  sectionId?: number | string;
  start?: TimestampLike;
  end?: TimestampLike;
  speakerName?: string;
  captions?: CaptionLike[];
  translatedText?: string;
  desktopAudioSource?: string;
  [key: string]: unknown;
};

/** Transcript section held in component state (already normalized for display). */
type TranscriptSection = {
  start: Date;
  end: Date;
  sectionId: number;
  speakerName: string;
  captions: Array<{ start: Date; text: string }>;
  translatedText?: string;
  detectedSourceLanguage?: string;
  detectedSourceLanguageReliable?: boolean;
};

/** Server ack for `get_current_meeting_id`. */
type MeetingIdAck = {
  meetingId?: string;
  shareCode?: string;
  isRecording?: boolean;
  usage?: {
    translationUsage?: number;
    translationLimit?: number;
    transcriptionUsage?: number;
    transcriptionLimit?: number;
  };
};

/** Inline style that also carries CSS custom properties. */
type CssVarStyle = CSSProperties & { [cssVar: `--${string}`]: string };

/** Generic socket event body: only the fields a handler reads are narrowed. */
type SocketEventData = { [key: string]: unknown };
type TranscriptionResultPayload = LatencyEventPayload & {
  pipeline?: string;
  isPartial?: boolean;
  [key: string]: unknown;
};

type SharedAsrTranscriptPayload = TranscriptionResultPayload & {
  section?: TranscriptSectionLike;
};

type LanguageOption = { key: string; value: string; model?: string };
type LiveTranslationInvitation = { meetingId: string; shareCode: string };
type LatencySplitType = 'merge' | 'p-split' | 's-split';
/** Latency fields attached to `transcription_result` style socket events. */
type LatencyEventPayload = {
  translationLatencyMs?: number;
  translationInputChars?: number;
  translationSplitType?: LatencySplitType;
  sttLatencyMs?: number;
  sttInputChars?: number;
  sttSplitType?: LatencySplitType;
  text?: string;
};

/** Socket.IO client plus the ad-hoc readiness/routing flags this component stores on it. */
type AppSocket = Socket & {
  isDeepgramReady?: boolean;
  reverseSpokenLanguage?: string;
  reverseTranslatedLanguage?: string;
  reverseSpokenLanguageModel?: string;
  isSharedDeepgramReady?: boolean;
  sharedSpokenLanguage?: string;
  sharedSpokenLanguageModel?: string;
  sharedDefaultTargetLanguage?: string;
  sharedInitChunkSent?: boolean;
  sharedInitChunkSendInProgress?: boolean;
  sharedLastAudioSentSequence?: number;
};

export interface Message {
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
  audioBase64?: string;
  isSummary?: boolean;
  isExplanation?: boolean;
  summaryId?: string; // For summaries: unique ID, for explanations: links to related summary
  speaker?: string;
  /** Generate Speech payload attached to assistant messages by the Ask panel. */
  speechData?: { speechText: string; pronunciation: string; language: string };
  isMemoConfirmation?: boolean;
  actionType?: string;
}

const AudioRecorder = ({
  onRecordingComplete,
  onRecordingDiscarded,
  onLiveMeetingChange,
  onTranscriptMaximizedChange,
  onShareInvitationChange,
  onPermissionSetupRequired,
  autoStart = false,
  initialTab = 'Notes',
  panelRequest = null,
  calendarContext,
  initialShareInvitation = null,
}: {
  onRecordingComplete?: (meetingId: string, title?: string, initialTab?: PostMeetingDetailTab) => void;
  onRecordingDiscarded?: () => void;
  /** Fired when the live session starts/stops so the shell can keep this tree mounted. */
  onLiveMeetingChange?: (isLive: boolean, meetingId: string | null) => void;
  /** Lets the shell simplify native/custom window chrome for transcript presentation mode. */
  onTranscriptMaximizedChange?: (maximized: boolean) => void;
  /** Keeps the prepared session mounted while navigating around the desktop app. */
  onShareInvitationChange?: (invitation: { meetingId: string; shareCode: string } | null) => void;
  /** Opens desktop Setup when recording cannot begin without audio permissions. */
  onPermissionSetupRequired?: () => void;
  /** Start as soon as auth is ready, without requiring the recording UI button. */
  autoStart?: boolean;
  /** Notes for transcription-only sessions; transcript for live translation. */
  initialTab?: 'Transcript' | 'Notes';
  /** One-shot request to reveal a specific live-session control. */
  panelRequest?: LiveSessionPanelRequest | null;
  /** Calendar event selected by the shell for this live session. */
  calendarContext?: CalendarNoteContext | null;
  /** Restores the same invitation during a spoken-language page reload. */
  initialShareInvitation?: {
    meetingId: string;
    shareCode: string;
    reservationKey?: string;
  } | null;
} = {}) => {
  const { t } = useTranslation();
  const [showLiveTranslationOnly] = useLiveTranslationOnlyPreference();
  const { user } = useAppSelector((state: AppRootState) => state.user);
  const isPremiumOrAbove = user?.currentTier !== 'Freemium' && user?.currentTier !== 'Pro';
  const hasPaidMeetingStorageAccess = Boolean(
    user?.currentTier && user.currentTier !== 'Freemium'
  );
  const paidMeetingStorageAccessRef = useRef(hasPaidMeetingStorageAccess);
  useEffect(() => {
    paidMeetingStorageAccessRef.current = hasPaidMeetingStorageAccess;
  }, [hasPaidMeetingStorageAccess]);
  const reduxIsRecording = useAppSelector((state: AppRootState) => state.recorder.isRecording);
  const normalizedUserEmail = (user?.email || '').trim().toLowerCase();
  const liveShareStorageScope = user?.id || normalizedUserEmail || 'anonymous';
  const isDelayAnalysisAdmin = isDelayAnalysisAdminEmail(normalizedUserEmail);

  // Log desktop app version
  const appInfo = getElectronIpc()?.getAppInfo();
  // productionLogger.debug("Desktop App Version:", appInfo?.version || null);


  const [isRecording, setIsRecording] = useState(false);

  // Tell the main process when recording is live so it can show/hide the
  // minimized recording widget (MeetingCopilot logo + live wave) while the app is in
  // the background. Safe no-op outside Electron. Cleanup always clears so the
  // pill cannot stick after unmount (e.g. navigate-to-detail on stop).
  useEffect(() => {
    try {
      getElectronIpc()?.send('recording-state', { recording: isRecording });
    } catch { /* not running in Electron */ }
    return () => {
      try {
        getElectronIpc()?.send('recording-state', { recording: false });
      } catch { /* not running in Electron */ }
    };
  }, [isRecording]);

  // True once the server's transcription/translation pipeline (Deepgram) is live and ready
  // to receive audio. Drives the "Getting ready…" status in the transcript.
  const [isTranscriptionReady, setIsTranscriptionReady] = useState(false);
  const audioChunksRef = useRef<Blob[]>([]);
  const [messages, setMessages] = useState<Message[]>([]);
  const [chatInput, setChatInput] = useState<string>(''); // Add chat input state
  const [isUploadingAudio, setIsUploadingAudio] = useState(false);
  const [audioUploadError, setAudioUploadError] = useState<string | null>(null);
  const [lastAudioBlob, setLastAudioBlob] = useState<Blob | null>(null);
  const [lastMeetingId, setLastMeetingId] = useState<string | null>(null);

  const [transcriptData, setTranscriptData] = useState<TranscriptSection[]>([]);
  const [sameLanguageOutputEnabled] = useSameLanguageOutputPreference();
  const [translationSplitTokenGuardEnabled] = useTranslationSplitTokenGuardPreference();
  const speakerAliasesRef = useRef<Record<string, string>>({});

  const resolveSpeakerAlias = (speakerName: string) => {
    let resolved = speakerName;
    const visited = new Set<string>();
    while (!visited.has(resolved)) {
      const alias = speakerAliasesRef.current[resolved];
      if (!alias) break;
      visited.add(resolved);
      resolved = alias;
    }
    return resolved;
  };

  const applySpeakerAliases = <T extends { speakerName?: string }>(sections: T[]) => sections.map((section) => ({
    ...section,
    speakerName: resolveSpeakerAlias(section?.speakerName || 'Speaker'),
  }));

  const renameTranscriptSpeaker = (currentSpeaker: string, nextSpeakerValue: string) => {
    const nextSpeaker = nextSpeakerValue.trim();
    if (!nextSpeaker || nextSpeaker === currentSpeaker) return;

    const nextAliases = { ...speakerAliasesRef.current };
    Object.keys(nextAliases).forEach((sourceSpeaker) => {
      if (resolveSpeakerAlias(sourceSpeaker) === currentSpeaker) {
        nextAliases[sourceSpeaker] = nextSpeaker;
      }
    });
    nextAliases[currentSpeaker] = nextSpeaker;
    speakerAliasesRef.current = nextAliases;
    socketRef.current?.emit('speaker_aliases_updated', nextAliases);

    setTranscriptData((sections) => sections.map((section) =>
      section.speakerName === currentSpeaker
        ? { ...section, speakerName: nextSpeaker }
        : section
    ));
  };
  // Map<sectionId, { [targetLanguage]: translatedText }> for extra (multilingual) translations
  const [extraTranslationsBySection, setExtraTranslationsBySection] = useState<Record<number, Record<string, string>>>({});
  const extraTranslationInFlightRef = useRef<Set<string>>(new Set());
  const extraTranslationSourceRef = useRef<Record<string, string>>({});

  // Feed the most recent transcript lines (speaker + original + translation) to the
  // main process so the minimized recording widget can show them in its hover view.
  // No-op outside Electron.
  useEffect(() => {
    if (!isRecording) return;
    const lines = transcriptData.slice(-8).map((s) => ({
      speaker: s.speakerName || '',
      original: (s.captions || []).map((c) => c.text).join(' ').replace(/\s+/g, ' ').trim(),
      translation: s.translatedText || '',
    }));
    try {
      getElectronIpc()?.send('caption-update', { lines });
    } catch { /* not running in Electron */ }
  }, [transcriptData, isRecording]);
  // Real-time meeting context memory (Premium ASR only). Updated by the server
  // every ~15s and pushed via the `meeting_flow_update` socket event. The
  // Settings → Real-time memory Show/No-show toggle controls visibility.
  const [meetingFlow, setMeetingFlow] = useState<string>("");
  const [showMeetingFlow, setShowMeetingFlow] = useState<boolean>(() => {
    const plan = getStoredSpokenLanguagePlan();
    const mode = getStoredTranslationMode();
    if (!isRealTimeMemoryEligible(plan, mode)) return false;
    const v = localStorage.getItem(REAL_TIME_MEMORY_STORAGE_KEY);
    // Default: ON for premium users (matches Settings default), OFF otherwise.
    if (v === 'true') return true;
    if (v === 'false') return false;
    return true;
  });
  const [transcriptionLimitExceeded, setTranscriptionLimitExceeded] = useState(() => {
    return localStorage.getItem('MeetingCopilot_transcription_limit_exceeded') === 'true';
  });
  const [translationLimitExceeded, setTranslationLimitExceeded] = useState(() => {
    return localStorage.getItem('MeetingCopilot_translation_limit_exceeded') === 'true';
  });
  // Track if translation limit was reached during this session (separate from UI flag)
  const [translationLimitReachedInSession, setTranslationLimitReachedInSession] = useState(false);
  const [transcriptionUsage, setTranscriptionUsage] = useState(() => {
    const stored = localStorage.getItem('MeetingCopilot_transcription_usage');
    return stored ? JSON.parse(stored) : { used: 0, limit: 50 };
  });
  const [translationUsage, setTranslationUsage] = useState(() => {
    const stored = localStorage.getItem('MeetingCopilot_translation_usage');
    return stored ? JSON.parse(stored) : { used: 0, limit: 60 };
  });

  // Wrapper functions to update both state and localStorage
  const updateTranscriptionLimitExceeded = (exceeded: boolean) => {
    setTranscriptionLimitExceeded(exceeded);
    localStorage.setItem('MeetingCopilot_transcription_limit_exceeded', exceeded.toString());
  };

  const updateTranslationLimitExceeded = (exceeded: boolean) => {
    setTranslationLimitExceeded(exceeded);
    localStorage.setItem('MeetingCopilot_translation_limit_exceeded', exceeded.toString());
  };

  const updateTranscriptionUsage = (usage: { used: number; limit: number }) => {
    setTranscriptionUsage(usage);
    localStorage.setItem('MeetingCopilot_transcription_usage', JSON.stringify(usage));
    // Update exceeded flag based on usage
    updateTranscriptionLimitExceeded(usage.used >= usage.limit);
  };

  const updateTranslationUsage = (usage: { used: number; limit: number }) => {
    setTranslationUsage(usage);
    localStorage.setItem('MeetingCopilot_translation_usage', JSON.stringify(usage));
    // Update exceeded flag based on usage
    updateTranslationLimitExceeded(usage.used >= usage.limit);
  };

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const microphoneStreamRef = useRef<MediaStream | null>(null);
  const liveAudioCaptureGenerationRef = useRef(0);
  const systemAudioGainRef = useRef<GainNode | null>(null);
  const micAudioGainRef = useRef<GainNode | null>(null);
  const streamingSourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const streamingProcessorRef = useRef<ScriptProcessorNode | null>(null);
  const streamingMicSourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const streamingSystemSourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const streamingMicProcessorRef = useRef<ScriptProcessorNode | null>(null);
  const streamingSystemProcessorRef = useRef<ScriptProcessorNode | null>(null);
  const pcmChunkQueueRef = useRef<Int16Array[]>([]);
  const micPcmChunkQueueRef = useRef<Int16Array[]>([]);
  const systemPcmChunkQueueRef = useRef<Int16Array[]>([]);
  const useDualPcmCaptureRef = useRef(false);
  const useSingleSourcePcmCaptureRef = useRef(false);
  const fixedPcmSourceRef = useRef<'mic' | 'system' | null>(null);
  const translatedSpeechPlaybackActiveRef = useRef(false);
  const translatedPlaybackActiveBySourceRef = useRef({ mic: false, system: false });
  const translatedPlaybackIsolatedBySourceRef = useRef({ mic: false, system: false });
  const captureSuppressLogAtRef = useRef<Record<string, number>>({});
  const meetUplinkTtsOwnVoiceRef = useRef(
    typeof window !== 'undefined' && localStorage.getItem(MEET_UPLINK_TTS_OWN_VOICE_KEY) === 'true'
  );
  type TabType = "Transcript" | "Notes";
  // Memos is the default tab so the window opens ready to type a memo.
  const [activeTab, setActiveTab] = useState<TabType>(initialTab);

  useEffect(() => {
    setActiveTab(initialTab);
  }, [initialTab]);
  const calendarNoteContext = calendarContext ?? null;
  const calendarEventKey = calendarNoteContext?.calendarEventId || calendarNoteContext?.id;
  const isCalendarBoundShareSession = Boolean(calendarEventKey);
  const [liveShareReservationKey] = useState(() => {
    if (calendarEventKey) return `calendar-${calendarEventKey}`;
    if (initialShareInvitation?.reservationKey) return initialShareInvitation.reservationKey;
    return typeof crypto?.randomUUID === 'function'
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  });
  const liveShareInvitationCacheKey = getCalendarLiveTranslationInvitationCacheKey(
    liveShareStorageScope,
    liveShareReservationKey,
  );
  const [initialLiveShareInvitation] = useState(() => {
    if (initialShareInvitation?.meetingId && initialShareInvitation?.shareCode) {
      return {
        meetingId: initialShareInvitation.meetingId,
        shareCode: initialShareInvitation.shareCode,
      };
    }
    if (isCalendarBoundShareSession) {
      const cached = readCachedLiveTranslationInvitation(liveShareInvitationCacheKey);
      if (cached) {
        return { meetingId: cached.meetingId, shareCode: cached.shareCode };
      }
    }
    const chatLaunch = consumeChatShareLaunch();
    if (!isCalendarBoundShareSession && chatLaunch?.shareCode) {
      return {
        meetingId: chatLaunch.meetingId || '',
        shareCode: chatLaunch.shareCode,
      };
    }
    return null;
  });
  const [notepad, setNotepad] = useState(() => readCalendarNoteDraft(calendarNoteContext));
  const calendarStartRecordingPayload = useMemo(() => {
    if (!calendarNoteContext) return {};
    return {
      calendarEventId: calendarNoteContext.calendarEventId || calendarNoteContext.id,
      calendarEventTitle: calendarNoteContext.title,
      calendarAttendees: (calendarNoteContext.participants ?? []).map((participant: { name?: string; email?: string; self?: boolean }) => ({
        name: participant.name,
        email: participant.email,
        self: participant.self === true,
      })),
    };
  }, [calendarNoteContext]);
  const [error, setError] = useState<{ type: string, message?: string } | null>(null);

  // Calendar notes are backed up locally as the user types. The normal meeting
  // save still persists them to the server once a recording is completed.
  useEffect(() => {
    saveCalendarNoteDraft(calendarNoteContext, notepad);
  }, [calendarNoteContext, notepad]);
  const [recordingTime, setRecordingTime] = useState(0);
  const [translationTime, setTranslationTime] = useState(0);
  const recordingTimerRef = useRef<NodeJS.Timeout | null>(null);
  const translationTimerRef = useRef<NodeJS.Timeout | null>(null);
  const isRecordingRef = useRef<boolean>(false);
  const pausedRef = useRef<boolean>(false);
  const [countdown, setCountdown] = useState(60);
  const reloadTimerRef = useRef<NodeJS.Timeout | null>(null);
  const [isDeepgramError, setIsDeepgramError] = useState(false)
  const [deepgramErrorData, setDeepgramErrorData] = useState<{ message?: string, showReloadButton?: boolean, meetingId?: string } | null>(null)
  const [deepgramErrorCountdown, setDeepgramErrorCountdown] = useState(30);
  const deepgramErrorTimerRef = useRef<NodeJS.Timeout | null>(null);
  const [dialogCountdown, setDialogCountdown] = useState(60);
  const dialogCountdownTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Transcription limit dialog state
  const [showTranscriptionDialog, setShowTranscriptionDialog] = useState(false);
  const [transcriptionDialogCountdown, setTranscriptionDialogCountdown] = useState(60);
  const transcriptionDialogTimerRef = useRef<NodeJS.Timeout | null>(null);
  const transcriptionDialogTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const translationDialogNoButtonRef = useRef<HTMLButtonElement | null>(null);
  const transcriptionDialogOkayButtonRef = useRef<HTMLButtonElement | null>(null);
  const [transcriptionDialogData, setTranscriptionDialogData] = useState<{
    message: string;
  } | null>(null);
  const {
    spokenLanguage,
    translatedLanguage,
    bothModeUplinkTranslatedLanguage,
    extraTranslatedLanguages,
    spokenLanguageModel,
    enabled,
    bulletPointEnabled,
    translationMode,
  } = useAppSelector((state: AppRootState) => state.recorder);


  // Debug: Log bulletPointEnabled changes
  useEffect(() => {
  }, [bulletPointEnabled]);
  const { joined } = useAppSelector((state: AppRootState) => state.sharing);
  const dispatch = useAppDispatch();
  const socketRef = useRef<AppSocket | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingTimeoutRef, setLoadingTimeoutRef] = useState<NodeJS.Timeout | null>(null);
  const [paused, setPaused] = useState(false);
  const [showMeetingDetails, setShowMeetingDetails] = useState(false);
  const [meetingId, setMeetingId] = useState<string | null>(
    initialLiveShareInvitation?.meetingId || null,
  );
  const meetingIdRef = useRef<string | null>(null);
  meetingIdRef.current = meetingId;

  useEffect(() => {
    onLiveMeetingChange?.(isRecording, meetingId);
  }, [isRecording, meetingId, onLiveMeetingChange]);
  const [showSuccessLoading, setShowSuccessLoading] = useState(false);
  const [successLoadingMode, setSuccessLoadingMode] = useState<'meeting' | 'delay-analysis' | 'discarded'>('meeting');
  const [reloadCountdown, setReloadCountdown] = useState(5);
  const [countdownTimer, setCountdownTimer] = useState<NodeJS.Timeout | null>(null);
  const [authReady, setAuthReady] = useState(false);
  const [shareCode, setShareCode] = useState(initialLiveShareInvitation?.shareCode ?? '')
  const [reservedMeetingId, setReservedMeetingId] = useState<string | null>(
    initialLiveShareInvitation?.meetingId || null,
  );
  const shareInvitationRequestRef = useRef<Promise<string> | null>(null);
  const reservedMeetingIdRef = useRef<string | null>(null);
  reservedMeetingIdRef.current = reservedMeetingId;
  const shareCodeRef = useRef(shareCode);
  shareCodeRef.current = shareCode;
  useEffect(() => {
    purgeUnboundLiveTranslationStorage({
      keepCalendarCacheKey: isCalendarBoundShareSession ? liveShareInvitationCacheKey : null,
    });
  }, [isCalendarBoundShareSession, liveShareInvitationCacheKey]);
  useEffect(() => {
    let cancelled = false;
    const applyInvitation = (invitation: { meetingId: string; shareCode: string }) => {
      reservedMeetingIdRef.current = invitation.meetingId;
      meetingIdRef.current = invitation.meetingId;
      shareCodeRef.current = invitation.shareCode;
      setReservedMeetingId(invitation.meetingId);
      setMeetingId(invitation.meetingId);
      setShareCode(invitation.shareCode);
      onShareInvitationChange?.({
        meetingId: invitation.meetingId,
        shareCode: invitation.shareCode,
      });
    };

    if (!isCalendarBoundShareSession) {
      return () => {
        cancelled = true;
      };
    }

    const cached = readCachedLiveTranslationInvitation(liveShareInvitationCacheKey);
    if (cached) applyInvitation(cached);

    const calendarEventId = calendarNoteContext?.calendarEventId
      || calendarNoteContext?.id
      || liveShareReservationKey.slice('calendar-'.length);
    void getLiveTranslationInvitation({
      reservationKey: liveShareReservationKey,
      calendarEventId,
    }).then((invitation: LiveTranslationInvitation | null | undefined) => {
      if (cancelled || !invitation) return;
      writeCachedLiveTranslationInvitation(liveShareInvitationCacheKey, {
        reservationKey: liveShareReservationKey,
        meetingId: invitation.meetingId,
        shareCode: invitation.shareCode,
      });
      applyInvitation(invitation);
    }).catch(() => {
      // A missing reservation is expected before the first explicit share action.
    });

    return () => {
      cancelled = true;
    };
  }, [calendarNoteContext, isCalendarBoundShareSession, liveShareInvitationCacheKey, liveShareReservationKey]);
  const persistLiveShareInvitation = (invitationMeetingId: string, invitationShareCode: string) => {
    if (isCalendarBoundShareSession) {
      writeCachedLiveTranslationInvitation(liveShareInvitationCacheKey, {
        reservationKey: liveShareReservationKey,
        meetingId: invitationMeetingId,
        shareCode: invitationShareCode,
      });
    }
    onShareInvitationChange?.({
      meetingId: invitationMeetingId,
      shareCode: invitationShareCode,
    });
  };
  const clearLiveShareInvitationState = (endedShareCode?: string | null) => {
    // Drop LIVE participant UI immediately — do not wait for sharing_disabled
    // (AudioRecorder often unmounts before that socket event arrives).
    dispatch(setClients([]));
    dispatch(setJoined(false));

    const codeToClear = endedShareCode || shareCodeRef.current;
    purgeUnboundLiveTranslationStorage({
      keepCalendarCacheKey: isCalendarBoundShareSession ? liveShareInvitationCacheKey : null,
    });
    if (codeToClear) {
      window.localStorage.removeItem(getLiveTranslationChatCacheKey(codeToClear));
    }
    if (isCalendarBoundShareSession) {
      return;
    }
    setShareCode('');
    setReservedMeetingId(null);
    onShareInvitationChange?.(null);
  };
  const activateShareInvitationIfHostIsLive = async ({
    invitationShareCode,
    invitationMeetingId,
    sharingLanguagePolicy,
  }: {
    invitationShareCode: string;
    invitationMeetingId?: string | null;
    sharingLanguagePolicy?: SharingLanguagePolicyPayload;
  }): Promise<string | null> => {
    const activeSocket = socketRef.current;
    if (!activeSocket?.connected) {
      if (isRecordingRef.current) {
        throw new Error(t('desktopUi.errors.shareSessionReconnect'));
      }
      return null;
    }

    const serverState = await new Promise<{
      meetingId?: string;
      isRecording?: boolean;
    }>((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        reject(new Error(t('desktopUi.errors.shareSessionCheckTimeout')));
      }, 5000);
      activeSocket.emit('get_current_meeting_id', {}, (response?: {
        meetingId?: string;
        isRecording?: boolean;
      }) => {
        window.clearTimeout(timeout);
        resolve(response || {});
      });
    });

    if (!serverState.isRecording || !serverState.meetingId) return null;

    return new Promise<string>((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        reject(new Error(t('desktopUi.errors.shareSessionStartTimeout')));
      }, 7000);
      activeSocket.emit('activate_live_translation_sharing', {
        shareCode: invitationShareCode,
        meetingId: serverState.meetingId,
        invitationMeetingId: invitationMeetingId || undefined,
        ...(sharingLanguagePolicy ? { sharingLanguagePolicy } : {}),
      }, (response?: { ok?: boolean; meetingId?: string; error?: string }) => {
        window.clearTimeout(timeout);
        if (response?.ok) {
          resolve(response.meetingId || serverState.meetingId!);
          return;
        }
        reject(new Error(response?.error || t('desktopUi.errors.activateLiveLink')));
      });
    });
  };

  const reserveShareInvitation = async (
    sharingLanguagePolicy?: SharingLanguagePolicyPayload,
  ): Promise<string> => {
    if (shareCode) {
      const activeMeetingId = await activateShareInvitationIfHostIsLive({
        invitationShareCode: shareCode,
        invitationMeetingId: reservedMeetingIdRef.current,
        sharingLanguagePolicy,
      });
      if (activeMeetingId) {
        meetingIdRef.current = activeMeetingId;
        reservedMeetingIdRef.current = activeMeetingId;
        persistLiveShareInvitation(activeMeetingId, shareCode);
        setMeetingId(activeMeetingId);
        setReservedMeetingId(activeMeetingId);
      }
      return shareCode;
    }
    if (shareInvitationRequestRef.current) return shareInvitationRequestRef.current;
    const request = reserveLiveTranslationInvitation({
      reservationKey: liveShareReservationKey,
      title: calendarNoteContext?.title || 'Live translation',
      calendarEventId: calendarNoteContext?.calendarEventId || calendarNoteContext?.id,
      startsAt: calendarNoteContext?.startsAt,
      calendarAttendees: calendarNoteContext?.participants,
      sharingLanguagePolicy,
    }).then(async (invitation: LiveTranslationInvitation) => {
      const attachedMeetingId = await activateShareInvitationIfHostIsLive({
        invitationShareCode: invitation.shareCode,
        invitationMeetingId: invitation.meetingId,
        sharingLanguagePolicy,
      });
      const resolvedActiveMeetingId = attachedMeetingId || null;
      if (attachedMeetingId) meetingIdRef.current = attachedMeetingId;

      // The share menu can start recording immediately after this promise
      // resolves, before React has committed the state updates below. Keep the
      // refs synchronous so start_recording activates this exact reservation.
      const resolvedMeetingId = resolvedActiveMeetingId || invitation.meetingId;
      reservedMeetingIdRef.current = resolvedMeetingId;
      meetingIdRef.current = resolvedMeetingId;
      shareCodeRef.current = invitation.shareCode;
      persistLiveShareInvitation(resolvedMeetingId, invitation.shareCode);
      setReservedMeetingId(resolvedMeetingId);
      setMeetingId(resolvedMeetingId);
      setShareCode(invitation.shareCode);
      return invitation.shareCode;
    }).finally(() => {
      shareInvitationRequestRef.current = null;
    });
    shareInvitationRequestRef.current = request;
    return request;
  };
  const [audioDevices, setAudioDevices] = useState<MediaDeviceInfo[]>([]);
  const [selectedAudioDevice, setSelectedAudioDevice] = useState<string>(() => localStorage.getItem('selectedMicrophone') || "");
  const [environment, setEnvironment] = useState<SessionEnvironment>(() => getStoredSessionEnvironment());
  const environmentRef = useRef(environment);
  const translationModeRef = useRef(translationMode);
  const handleSpeechToSpeechModeChangeRef = useRef<(
    mode: SpeechToSpeechMode,
    preferredEnvironment?: SessionEnvironment,
  ) => void>(() => {});
  useEffect(() => { environmentRef.current = environment; }, [environment]);
  useEffect(() => {
    try {
      socketRef.current?.emit(
        'update_environment',
        getRuntimeSessionEnvironment(environment),
      );
    } catch (e) {
      productionLogger.warn('[AudioRecorder] failed to emit update_environment', e);
    }
  }, [environment]);
  const [selectedMeetingCaptureDevice, setSelectedMeetingCaptureDevice] = useState<string>(() => localStorage.getItem(MEETING_AUDIO_CAPTURE_STORAGE_KEY) || 'loopback');
  const [, setMeetingCopilotConfig] = useState('')
  const countdownIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const [stopButtonDisabled, setStopButtonDisabled] = useState(false);

  // Add state to manage modal visibility
  const [isModalOpen, setIsModalOpen] = useState(false);

  // State for translation limit dialog
  const [showTranslationDialog, setShowTranslationDialog] = useState(false);
  const [dialogData, setDialogData] = useState<{
    transcriptionUsage: number;
    transcriptionLimit: number;
    usedMinutes: number;
    limit: number;
  } | null>(null);

  const displayStreamRef = useRef<MediaStream | null>(null);
  const meetingAudioStreamRef = useRef<MediaStream | null>(null);
  const isRequestingDisplayStream = useRef(false);
  const isRequestingMeetingAudioStream = useRef(false);
  const dialogTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const localAudioSourceActivityRef = useRef<Array<{ timestamp: number; micLevel: number; systemLevel: number }>>([]);
  const localAudioSourceSegmentsRef = useRef<Array<{ source: 'mic' | 'system'; start: number; end?: number }>>([]);
  const localAudioSourceCurrentRef = useRef<'mic' | 'system' | null>(null);
  const localAudioSourceCandidateRef = useRef<{ source: 'mic' | 'system'; since: number } | null>(null);
  const captionSourceSplitStateRef = useRef<Map<string, {
    lastText: string;
    lastSource: 'mic' | 'system' | 'unknown';
    splits: Array<{ index: number; source: 'mic' | 'system' }>;
  }>>(new Map());
  const localAudioSourceSamplerRef = useRef<NodeJS.Timeout | null>(null);
  const localAudioSourceCleanupRef = useRef<(() => void) | null>(null);

  // Contextual translation-quality auto-adjust tracking
  const lastQualityAdjustmentRef = useRef<number>(0);
  const consecutiveSingleSentenceSectionsRef = useRef<number>(0);
  const activeSectionTranslationRef = useRef<string>('');
  const activeSectionIdRef = useRef<number | null>(null);
  const [translationQuality, setTranslationQuality] = useState(() => {
    const plan = getStoredSpokenLanguagePlan();
    return clampTranslationQuality(readStoredTranslationQuality(enabled), plan, spokenLanguageModel);
  });
  useEffect(() => {
    const plan = getStoredSpokenLanguagePlan();
    const nextQuality = clampTranslationQuality(
      readStoredTranslationQuality(enabled),
      plan,
      spokenLanguageModel
    );
    setTranslationQuality(nextQuality);
    writeStoredTranslationQuality(enabled, nextQuality);
  }, [enabled, spokenLanguageModel]);
  const [autoTranslationQualityAdjustEnabled, setAutoTranslationQualityAdjustEnabled] = useState(() => {
    const savedPreference = localStorage.getItem(TRANSLATION_QUALITY_AUTO_ADJUST_STORAGE_KEY);
    return savedPreference !== 'false';
  });
  const [delayAnalysisControlsVisible, setDelayAnalysisControlsVisible] = useState(() => getStoredDelayAnalysisControlsVisible());
  const [translationDelayAnalysisEnabled, setTranslationDelayAnalysisEnabled] = useState(false);
  const [sttDelayAnalysisEnabled, setSttDelayAnalysisEnabled] = useState(false);
  const [voicePlaybackWaitMs, setVoicePlaybackWaitMs] = useState(readVoicePlaybackWait);
  useEffect(() => subscribeVoicePlaybackWait(setVoicePlaybackWaitMs), []);
  const [textToSpeechEnabled, setTextToSpeechEnabled] = useState(() => {
    return localStorage.getItem(TEXT_TO_SPEECH_STORAGE_KEY) === 'true';
  });
  const [textToSpeechSessionMode, setTextToSpeechSessionMode] = useState<'online' | 'in-person'>(() => {
    return localStorage.getItem(TEXT_TO_SPEECH_SESSION_MODE_STORAGE_KEY) === 'in-person' ? 'in-person' : 'online';
  });
  const [speechToSpeechMode, setSpeechToSpeechMode] = useState<SpeechToSpeechMode>(() => {
    const stored = localStorage.getItem(SPEECH_TO_SPEECH_MODE_STORAGE_KEY) as SpeechToSpeechMode | null;
    if (stored === 'online-incoming' || stored === 'online-outgoing' || stored === 'online-both' || stored === 'in-person') {
      return stored;
    }
    if (localStorage.getItem(TEXT_TO_SPEECH_STORAGE_KEY) === 'true') {
      return localStorage.getItem(TEXT_TO_SPEECH_SESSION_MODE_STORAGE_KEY) === 'in-person' ? 'in-person' : 'online-incoming';
    }
    return 'off';
  });
  const speechToSpeechModeRef = useRef<SpeechToSpeechMode>(speechToSpeechMode);
  const textToSpeechSessionModeRef = useRef<'online' | 'in-person'>(textToSpeechSessionMode);
  const isVirtualMeetingCaptureEnabled = selectedMeetingCaptureDevice !== 'loopback';

  const shouldApplyTtsEchoSuppression =
    textToSpeechEnabled &&
    (textToSpeechSessionMode === 'online' || textToSpeechSessionMode === 'in-person');

  const shouldApplyFastPlaybackEchoSuppression = translationMode === 'fast';

  /** While TTS plays, duck/mute capture so translated audio is not fed back into STT. */
  const syncTranslatedSpeechPlaybackCaptureGates = (
    playbackActive: boolean,
    isIsolatedOutput = false,
    source?: 'mic' | 'system'
  ) => {
    if (source === 'mic' || source === 'system') {
      translatedPlaybackActiveBySourceRef.current[source] = playbackActive;
      translatedPlaybackIsolatedBySourceRef.current[source] = playbackActive && isIsolatedOutput;
    } else if (!playbackActive) {
      translatedPlaybackActiveBySourceRef.current.mic = false;
      translatedPlaybackActiveBySourceRef.current.system = false;
      translatedPlaybackIsolatedBySourceRef.current.mic = false;
      translatedPlaybackIsolatedBySourceRef.current.system = false;
    }

    const micPlayback = translatedPlaybackActiveBySourceRef.current.mic;
    const systemPlayback = translatedPlaybackActiveBySourceRef.current.system;
    translatedSpeechPlaybackActiveRef.current = micPlayback || systemPlayback;

    const applyEcho = shouldApplyFastPlaybackEchoSuppression || shouldApplyTtsEchoSuppression;
    if (!applyEcho) return;

    if (micAudioGainRef.current) {
      // Fast realtime must keep mic capture open; otherwise user speech is sent as silence
      // while translated playback is active.
      const duckMic = !shouldApplyFastPlaybackEchoSuppression && micPlayback;
      micAudioGainRef.current.gain.value = duckMic ? 0 : 1;
    }

    if (systemAudioGainRef.current) {
      // In fast realtime mode the echo-suppression is handled by normalizeAudioSourceMetaForPlayback
      // which reroutes system-sourced segments to 'mic' during playback. Muting the system gain
      // here would cut off all system audio capture while translation is speaking, creating a
      // dropout loop: translate → mute → silence → no input → gap. Only mute for classic TTS.
      const muteSystem = !shouldApplyFastPlaybackEchoSuppression
        && (systemPlayback || (micPlayback && !isVirtualMeetingCaptureEnabled));
      systemAudioGainRef.current.gain.value = muteSystem ? 0 : 1;
    }
  };

  useEffect(() => {
    const syncMeetUplinkFlag = () => {
      meetUplinkTtsOwnVoiceRef.current =
        typeof window !== 'undefined' && localStorage.getItem(MEET_UPLINK_TTS_OWN_VOICE_KEY) === 'true';
    };
    syncMeetUplinkFlag();
    const onStorage = (e: StorageEvent) => {
      if (e.key === MEET_UPLINK_TTS_OWN_VOICE_KEY) syncMeetUplinkFlag();
    };
    window.addEventListener('storage', onStorage);
    window.addEventListener(MEET_UPLINK_TTS_OWN_VOICE_EVENT, syncMeetUplinkFlag as EventListener);
    return () => {
      window.removeEventListener('storage', onStorage);
      window.removeEventListener(MEET_UPLINK_TTS_OWN_VOICE_EVENT, syncMeetUplinkFlag as EventListener);
    };
  }, []);

  // Re-apply routing auto-config when recording stops to ensure deferred capture device changes take effect
  useEffect(() => {
    if (!isRecording) {
      autoConfigureForMode(speechToSpeechMode, false).catch(console.error);
    }
  }, [isRecording, speechToSpeechMode]);

  const shouldRerouteFastSystemCaptureForPlayback = () => {
    if (speechToSpeechMode === 'online-incoming' || speechToSpeechMode === 'online-both') return false;
    if (isInPersonDualStreamCapture()) return false;
    if (isInPersonOtherAudioContext() && getStoredOtherAudioCaptureMode() === 'system') return false;
    const micPlayback = translatedPlaybackActiveBySourceRef.current.mic;
    const systemPlayback = translatedPlaybackActiveBySourceRef.current.system;
    if (translationMode !== 'fast') return false;
    return systemPlayback || (micPlayback && !isVirtualMeetingCaptureEnabled);
  };

  const normalizeAudioSourceMetaForPlayback = (audioSourceMeta: AudioSourceMeta): AudioSourceMeta => {
    if (!shouldRerouteFastSystemCaptureForPlayback()) return audioSourceMeta;

    const segments = Array.isArray(audioSourceMeta?.segments) ? audioSourceMeta.segments : [];
    if (segments.length > 0) {
      const normalizedSegments = segments.map((segment) => (
        segment?.source === 'system'
          ? { ...segment, source: 'mic' }
          : segment
      ));
      const hasMicSegment = normalizedSegments.some((segment) => segment?.source === 'mic' || segment?.source === 'unknown');
      const hasSystemSegment = normalizedSegments.some((segment) => segment?.source === 'system');

      return {
        ...audioSourceMeta,
        source: hasMicSegment ? 'mic' : (hasSystemSegment ? 'system' : audioSourceMeta?.source),
        segments: normalizedSegments,
        systemCaptureReroutedForTranslatedPlayback: true,
      };
    }

    if (audioSourceMeta?.source === 'system') {
      return {
        ...audioSourceMeta,
        source: 'mic',
        systemCaptureReroutedForTranslatedPlayback: true,
      };
    }

    return audioSourceMeta;
  };

  const shouldSuppressAudioChunkForTtsPlayback = (audioSourceMeta: AudioSourceMeta) => {
    const micPlayback = translatedPlaybackActiveBySourceRef.current.mic;
    const systemPlayback = translatedPlaybackActiveBySourceRef.current.system;
    if (!micPlayback && !systemPlayback) return false;

    const fastRealtimePlayback = translationMode === 'fast';
    if (!fastRealtimePlayback && !shouldApplyTtsEchoSuppression) {
      return false;
    }

    const segments = Array.isArray(audioSourceMeta?.segments) ? audioSourceMeta.segments : [];
    const hasMicSegment = segments.some((segment) => segment?.source === 'mic' || segment?.source === 'unknown');
    const source = audioSourceMeta?.source;

    if (fastRealtimePlayback) {
      const dualCaptureActive = isDualStreamCaptureActive({
        speechToSpeechMode,
        inPersonDualCapture: isInPersonDualStreamCapture(),
      });
      if (dualCaptureActive) {
        const suppressed = shouldSuppressMergedChunkForTts(
          audioSourceMeta,
          { mic: micPlayback, system: systemPlayback },
          dualCaptureActive,
          {
            speechToSpeechMode: speechToSpeechModeRef.current,
            isolatedPlayback: translatedPlaybackIsolatedBySourceRef.current,
            virtualMeetingCapture: isVirtualMeetingCaptureEnabled,
          }
        );
        if (suppressed) {
          logCaptureSuppressed('merged', 'tts-playback');
        }
        return suppressed;
      }
      return false;
    }

    if (textToSpeechSessionMode === 'in-person') {
      return true;
    }

    if (isVirtualMeetingCaptureEnabled) {
      return false;
    }

    if (segments.length > 0) {
      return !hasMicSegment;
    }

    return source === 'system' || source === 'unknown';
  };

  const enableTextToSpeech = (
    sessionMode: 'online' | 'in-person',
    nextSpeechMode: SpeechToSpeechMode,
    preferredEnvironment?: SessionEnvironment,
  ) => {
    setTextToSpeechEnabled(true);
    setTextToSpeechSessionMode(sessionMode);
    setSpeechToSpeechMode(nextSpeechMode);
    speechToSpeechModeRef.current = nextSpeechMode;
    localStorage.setItem(TEXT_TO_SPEECH_STORAGE_KEY, 'true');
    localStorage.setItem(TEXT_TO_SPEECH_SESSION_MODE_STORAGE_KEY, sessionMode);
    localStorage.setItem(SPEECH_TO_SPEECH_MODE_STORAGE_KEY, nextSpeechMode);
    syncHostTtsConfigToServerRef.current(true, sessionMode, nextSpeechMode);

    if (isOnlineSpeechMode(nextSpeechMode)) {
      rememberLastOnlineSpeechMode(nextSpeechMode);
    }
    const env = preferredEnvironment ?? speechModeToEnvironment(nextSpeechMode);
    if (env) {
      environmentRef.current = env;
      setEnvironment(env);
      applySessionEnvironment(env, 's2s');
      try {
        socketRef.current?.emit(
          'update_environment',
          getRuntimeSessionEnvironment(env),
        );
      } catch { /* socket may not be connected yet */ }
    }

    // Fire and forget auto config so standard toggles (hotkeys, init, etc) match routing
    autoConfigureForMode(nextSpeechMode, isRecordingRef.current).catch(console.error);
  };

  const disableTextToSpeech = () => {
    setTextToSpeechEnabled(false);
    setSpeechToSpeechMode('off');
    speechToSpeechModeRef.current = 'off';
    localStorage.setItem(TEXT_TO_SPEECH_STORAGE_KEY, 'false');
    localStorage.setItem(SPEECH_TO_SPEECH_MODE_STORAGE_KEY, 'off');
    syncHostTtsConfigToServerRef.current(false, textToSpeechSessionMode, 'off');

    environmentRef.current = 'online';
    setEnvironment('online');
    applySessionEnvironment('online', 's2s');

    // Fire and forget auto config restoration
    autoConfigureForMode('off', isRecordingRef.current).catch(console.error);

    syncTranslatedSpeechPlaybackCaptureGates(false);
  };

  const handleSpeechToSpeechModeChange = (
    mode: SpeechToSpeechMode,
    preferredEnvironment?: SessionEnvironment,
  ) => {
    if (translationMode !== 'fast') return;
    if (mode === 'off') {
      disableTextToSpeech();
      return;
    }

    enableTextToSpeech(
      mode === 'in-person' ? 'in-person' : 'online',
      mode,
      preferredEnvironment,
    );
  };

  handleSpeechToSpeechModeChangeRef.current = handleSpeechToSpeechModeChange;

  useEffect(() => {
    speechToSpeechModeRef.current = speechToSpeechMode;
  }, [speechToSpeechMode]);

  useEffect(() => {
    textToSpeechSessionModeRef.current = textToSpeechSessionMode;
  }, [textToSpeechSessionMode]);

  useEffect(() => {
    translationModeRef.current = translationMode;
  }, [translationMode]);

  useEffect(() => {
    const handler = (e: Event) => {
      const parsed = parseEnvironmentChangedEvent(e);
      if (!parsed) return;
      setEnvironment(parsed.env);

      if (parsed.source === 's2s') return;
      if (translationModeRef.current !== 'fast') return;

      const currentMode = speechToSpeechModeRef.current;
      if (isInPersonSessionEnvironment(parsed.env) && currentMode !== 'in-person') {
        handleSpeechToSpeechModeChangeRef.current('in-person', parsed.env);
      } else if (parsed.env === 'online' && currentMode === 'in-person') {
        handleSpeechToSpeechModeChangeRef.current(getLastOnlineSpeechMode());
      }
    };

    window.addEventListener(ENVIRONMENT_CHANGED_EVENT, handler as EventListener);
    return () => window.removeEventListener(ENVIRONMENT_CHANGED_EVENT, handler as EventListener);
  }, []);

  useEffect(() => {
    const handleTranslatedSpeechPlaybackStateChanged = (event: Event) => {
      const detail = (event as CustomEvent<{
        active?: boolean;
        isIsolatedOutput?: boolean;
        source?: 'mic' | 'system';
        intendedSinkId?: string;
        resolvedSinkId?: string;
        routingFailed?: boolean;
      }>).detail;
      if (detail?.routingFailed && speechToSpeechModeRef.current === 'online-both') {
        setError({
          type: 'generic',
          message: t('desktopUi.errors.twoWayAudioRouting'),
        });
      }
      syncTranslatedSpeechPlaybackCaptureGates(
        !!detail?.active,
        !!detail?.isIsolatedOutput,
        detail?.source
      );
    };

    window.addEventListener(TRANSLATED_SPEECH_PLAYBACK_EVENT, handleTranslatedSpeechPlaybackStateChanged as EventListener);
    return () => {
      window.removeEventListener(TRANSLATED_SPEECH_PLAYBACK_EVENT, handleTranslatedSpeechPlaybackStateChanged as EventListener);
      syncTranslatedSpeechPlaybackCaptureGates(false);
    };
  }, [isVirtualMeetingCaptureEnabled, speechToSpeechMode, t, textToSpeechEnabled, textToSpeechSessionMode, translationMode]);

  useEffect(() => {
    if (translationMode === 'fast') return;
    if (!textToSpeechEnabled) {
      syncTranslatedSpeechPlaybackCaptureGates(false);
    } else if (textToSpeechSessionMode === 'online' && isVirtualMeetingCaptureEnabled) {
      syncTranslatedSpeechPlaybackCaptureGates(false);
    }
  }, [isVirtualMeetingCaptureEnabled, textToSpeechEnabled, textToSpeechSessionMode, translationMode]);

  const [translationLatencies, setTranslationLatencies] = useState<Array<{ id: number; latencyMs: number; inputChars: number; splitType: 'merge' | 'p-split' | 's-split'; timestamp: number }>>([]);
  const translationCountRef = useRef(0);
  const [sttLatencies, setSttLatencies] = useState<Array<{ id: number; latencyMs: number; inputChars: number; splitType: 'merge' | 'p-split' | 's-split'; timestamp: number }>>([]);
  const sttCountRef = useRef(0);
  const [showMicDisconnectModal, setShowMicDisconnectModal] = useState(false);

  const [showPendingMeetingModal, setShowPendingMeetingModal] = useState(false);
  const [pendingErrorKey, setPendingErrorKey] = useState<string | null>(null);

  const [bulletOverlayOpen, setBulletOverlayOpen] = useState(false);
  const [bulletMinimizedHeight, setBulletMinimizedHeight] = useState(10);
  const [isTranscriptMaximized, setIsTranscriptMaximized] = useState(false);

  useEffect(() => {
    onTranscriptMaximizedChange?.(isTranscriptMaximized);
  }, [isTranscriptMaximized, onTranscriptMaximizedChange]);

  useEffect(() => () => {
    onTranscriptMaximizedChange?.(false);
  }, [onTranscriptMaximizedChange]);

  const [reverseTranslationActive, setReverseTranslationActive] = useState(false);
  const reverseTranslationActiveRef = useRef(false);
  const [reverseTranscriptData, setReverseTranscriptData] = useState<TranscriptSection[]>([]);
  const reverseSocketRef = useRef<AppSocket | null>(null);
  // MediaRecorder WebM streams typically include an initialization/header segment in the first chunk only.
  // If we start a new Deepgram stream mid-recording (reverse socket toggled on later), sending only
  // mid-stream WebM clusters can result in "no response" (Deepgram can't decode without the init segment).
  // We keep the very first WebM chunk from the recording and replay it once to the reverse socket when it starts.
  const webmInitChunkRef = useRef<Blob | null>(null);
  const reverseInitChunkSentRef = useRef(false);
  const reverseInitChunkSendInProgressRef = useRef(false);
  const reversePendingBuffersRef = useRef<ArrayBuffer[]>([]);
  const reverseAudioBufferRef = useRef<Blob[]>([]);  // Component-level reverse audio buffer
  const showDelayAnalysisControls = isDelayAnalysisAdmin && delayAnalysisControlsVisible;
  const delayAnalysisConfigEnabled = showDelayAnalysisControls && (translationDelayAnalysisEnabled || sttDelayAnalysisEnabled);
  const delayAnalysisConfigEnabledRef = useRef(delayAnalysisConfigEnabled);

  const recordTranslationLatency = (data: LatencyEventPayload | undefined) => {
    if (data?.translationLatencyMs === undefined) return;
    translationCountRef.current += 1;
    const id = translationCountRef.current;
    setTranslationLatencies(prev => [
      ...prev.slice(-49),
      {
        id,
        latencyMs: data.translationLatencyMs as number,
        inputChars: data.translationInputChars || 0,
        splitType: data.translationSplitType || 'merge',
        timestamp: Date.now(),
      }
    ]);
  };
  const recordSttLatency = (data: LatencyEventPayload | undefined) => {
    if (data?.sttLatencyMs === undefined || data?.translationLatencyMs !== undefined) return;
    sttCountRef.current += 1;
    const id = sttCountRef.current;
    setSttLatencies(prev => [
      ...prev.slice(-49),
      {
        id,
        latencyMs: data.sttLatencyMs as number,
        inputChars: data.sttInputChars || data?.text?.length || 0,
        splitType: data.sttSplitType || data.translationSplitType || 'merge',
        timestamp: Date.now(),
      }
    ]);
  };
  const withDelayAnalysisConfig = (payload: Record<string, unknown>) => {
    const resolvedTranslatedLanguage = payload.translatedLanguage ?? translatedLanguage;
    const resolvedUplinkLanguage =
      payload.bothModeUplinkTranslatedLanguage ?? bothModeUplinkTranslatedLanguage;
    const resolvedSpokenLanguage = payload.spokenLanguage ?? spokenLanguage;
    const resolvedSpeechMode = payload.speechToSpeechMode ?? speechToSpeechMode;
    const incomingSpeechLanguage = resolveIncomingSpeechLanguageLabel({
      speechToSpeechMode: resolvedSpeechMode,
      spokenLanguage: resolvedSpokenLanguage,
      translatedLanguage: resolvedTranslatedLanguage,
      bothModeUplinkTranslatedLanguage: resolvedUplinkLanguage,
    });

    return {
      ...payload,
      voicePlaybackWaitMs: readVoicePlaybackWait(),
      translatedLanguage: resolvedTranslatedLanguage,
      bothModeUplinkTranslatedLanguage: resolvedUplinkLanguage,
      incomingSpeechLanguage: incomingSpeechLanguage ?? '',
      delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
      translationMode,
      realTimeMemoryEnabled: isRealTimeMemoryEligible(getSpokenLanguagePlan(), translationMode),
      extraTranslatedLanguageCount: payload?.translationEnabled
        ? (Array.isArray(extraTranslatedLanguages) ? extraTranslatedLanguages.length : 0)
        : 0,
      extraTranslatedLanguages: Array.isArray(extraTranslatedLanguages)
        ? extraTranslatedLanguages
        : [],
      sameLanguageOutputEnabled,
      translationSplitTokenGuardEnabled,
      requireAttendeeEmail: readRequireAttendeeEmail(),
      createPostMeetingChat: readPostMeetingSharing(),
      translationOnlyViewEnabled:
        payload.translationOnlyViewEnabled ?? showLiveTranslationOnly,
    };
  };

  useEffect(() => {
    const syncEmailRequirement = () => {
      socketRef.current?.emit('update_require_attendee_email', readRequireAttendeeEmail());
    };
    window.addEventListener(ATTENDEE_EMAIL_CHANGED_EVENT, syncEmailRequirement);
    return () => window.removeEventListener(ATTENDEE_EMAIL_CHANGED_EVENT, syncEmailRequirement);
  }, []);

  const emitFullConfigToServer = () => {
    if (!socketRef.current?.connected) return;
    const safeQuality = getSafeTranslationQuality(translationQuality);
    socketRef.current.emit('update_config', withDelayAnalysisConfig({
      translatedLanguage,
      translationEnabled: enabled,
      bulletPointEnabled,
      translationQuality: safeQuality.toString(),
      textToSpeechEnabled,
      textToSpeechSessionMode,
      speechToSpeechMode,
      otherAudioCaptureMode: getStoredOtherAudioCaptureMode(),
    }));
  };

  useEffect(() => {
    emitFullConfigToServer();
    // The preference event updates this state immediately; the full config keeps
    // the host socket and participant fan-out behavior in sync mid-session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sameLanguageOutputEnabled, translationSplitTokenGuardEnabled]);

  useEffect(() => {
    const syncPostMeetingContentSharing = (event?: Event) => {
      if (
        event instanceof StorageEvent
        && event.key !== POST_MEETING_CONTENT_SHARING_STORAGE_KEY
      ) {
        return;
      }
      const detail = (event as CustomEvent<boolean> | undefined)?.detail;
      const enabled = readRequireAttendeeEmail() && (typeof detail === 'boolean'
        ? detail
        : readPostMeetingSharing());
      socketRef.current?.emit('update_create_post_meeting_chat', enabled);
    };

    window.addEventListener(
      POST_MEETING_CONTENT_SHARING_CHANGED_EVENT,
      syncPostMeetingContentSharing as EventListener,
    );
    window.addEventListener('storage', syncPostMeetingContentSharing);
    return () => {
      window.removeEventListener(
        POST_MEETING_CONTENT_SHARING_CHANGED_EVENT,
        syncPostMeetingContentSharing as EventListener,
      );
      window.removeEventListener('storage', syncPostMeetingContentSharing);
    };
  }, []);

  const syncHostTtsConfigToServer = (
    nextEnabled: boolean,
    sessionMode: 'online' | 'in-person',
    nextSpeechMode: SpeechToSpeechMode
  ) => {
    if (!socketRef.current?.connected) return;
    socketRef.current.emit('update_config', withDelayAnalysisConfig({
      translatedLanguage,
      translationEnabled: enabled,
      bulletPointEnabled,
      translationQuality: getSafeTranslationQuality(translationQuality).toString(),
      textToSpeechEnabled: nextEnabled,
      textToSpeechSessionMode: sessionMode,
      speechToSpeechMode: nextSpeechMode,
      otherAudioCaptureMode: getStoredOtherAudioCaptureMode(),
    }));
  };

  const syncOtherAudioCaptureModeToServer = (mode: OtherAudioCaptureMode) => {
    if (!socketRef.current?.connected) return;
    socketRef.current.emit('update_config', withDelayAnalysisConfig({
      translatedLanguage,
      translationEnabled: enabled,
      bulletPointEnabled,
      translationQuality: getSafeTranslationQuality(translationQuality).toString(),
      textToSpeechEnabled,
      textToSpeechSessionMode,
      speechToSpeechMode,
      otherAudioCaptureMode: mode,
    }));
  };

  const syncHostTtsConfigToServerRef = useRef(syncHostTtsConfigToServer);
  syncHostTtsConfigToServerRef.current = syncHostTtsConfigToServer;

  useEffect(() => {
    if (!shareCode || !textToSpeechEnabled) return;
    syncHostTtsConfigToServerRef.current(true, textToSpeechSessionMode, speechToSpeechMode);
  }, [shareCode, textToSpeechEnabled, textToSpeechSessionMode, speechToSpeechMode]);

  useEffect(() => {
    if (!socketRef.current?.connected) return;
    socketRef.current.emit('update_config', withDelayAnalysisConfig({
      translatedLanguage,
      translationEnabled: enabled,
      bulletPointEnabled,
      translationOnlyViewEnabled: showLiveTranslationOnly,
    }));
    // Other live settings use the same socket/config channel; the preference
    // hook is the only dependency that should trigger this focused sync.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showLiveTranslationOnly]);

  const isFinalizingRef = useRef<boolean>(false);
  const navigatedOnStopRef = useRef(false);
  const onRecordingCompleteRef = useRef(onRecordingComplete);
  onRecordingCompleteRef.current = onRecordingComplete;
  const onRecordingDiscardedRef = useRef(onRecordingDiscarded);
  onRecordingDiscardedRef.current = onRecordingDiscarded;
  const postMeetingDestinationRef = useRef<PostMeetingDestination | null>(null);
  const sharedAsrSocketPoolRef = useRef<Map<string, AppSocket>>(new Map());
  const sharedAsrTargetMapRef = useRef<Map<string, Set<string>>>(new Map());
  const sharedAsrPreRollRef = useRef<Array<{
    buffer: ArrayBuffer;
    audioSourceMeta: AudioSourceMeta;
    capturedAt: number;
    sequence: number;
  }>>([]);
  const sharedAsrPreRollSequenceRef = useRef(0);

  const buildAsrKey = (spoken: string, model: string) => `${spoken || ''}::${model || ''}`;

  /** Aligns with server `normalizeSpokenLang` intent: same base tag => same host ASR for shared/web (e.g. en vs en-US). */
  const baseSpokenTagForSharedAsr = (raw: string): string => {
    const t = (raw || "").trim().replace(/_/g, "-");
    if (!t) return "";
    const lower = t.toLowerCase();
    const byKey = ALL_SPOKEN_LANGUAGE_OPTIONS.find((l) => l.key.toLowerCase() === lower);
    if (byKey?.key) return byKey.key.split("-")[0]?.toLowerCase() || lower;
    const byValue = ALL_SPOKEN_LANGUAGE_OPTIONS.find((l) => l.value.toLowerCase() === lower);
    if (byValue?.key) return byValue.key.split("-")[0]?.toLowerCase() || lower;
    return lower.split("-")[0] || lower;
  };

  const cleanupSharedAsrSockets = () => {
    const pooled = Array.from(sharedAsrSocketPoolRef.current.values());
    pooled.forEach((pooledSocket) => {
      try { pooledSocket.emit?.('stop_recording'); } catch { /* socket already closed */ }
      try { pooledSocket.disconnect?.(); } catch { /* socket already closed */ }
    });
    sharedAsrSocketPoolRef.current.clear();
    sharedAsrTargetMapRef.current.clear();
    sharedAsrPreRollRef.current = [];
    sharedAsrPreRollSequenceRef.current = 0;
  };

  const getReverseLanguageConfig = () => {
    const translatedLanguageMatch = languages.find((lang: LanguageOption) => lang.value === translatedLanguage);
    const translatedLanguageKey = translatedLanguageMatch?.key || translatedLanguage;

    const reverseSpokenLanguageMatch =
      ALL_SPOKEN_LANGUAGE_OPTIONS.find((lang) => lang.key === translatedLanguageKey) ||
      ALL_SPOKEN_LANGUAGE_OPTIONS.find((lang) => lang.value === translatedLanguage);

    const reverseSpokenLanguage = reverseSpokenLanguageMatch?.key || spokenLanguage;
    const reverseSpokenLanguageModel = reverseSpokenLanguageMatch?.model || spokenLanguageModel;
    const reverseTranslatedLanguage =
      ALL_SPOKEN_LANGUAGE_OPTIONS.find((lang) => lang.key === spokenLanguage)?.value || spokenLanguage;

    return {
      reverseSpokenLanguage,
      reverseSpokenLanguageModel,
      reverseTranslatedLanguage,
    };
  };

  const downsampleTo16kHz = (input: Float32Array, inputSampleRate: number) => {
    if (inputSampleRate === 16000) {
      return input;
    }

    const sampleRateRatio = inputSampleRate / 16000;
    const newLength = Math.round(input.length / sampleRateRatio);
    const result = new Float32Array(newLength);
    let offsetResult = 0;
    let offsetBuffer = 0;

    while (offsetResult < result.length) {
      const nextOffsetBuffer = Math.round((offsetResult + 1) * sampleRateRatio);
      let accum = 0;
      let count = 0;
      for (let i = offsetBuffer; i < nextOffsetBuffer && i < input.length; i += 1) {
        accum += input[i]!;
        count += 1;
      }
      result[offsetResult] = count > 0 ? accum / count : 0;
      offsetResult += 1;
      offsetBuffer = nextOffsetBuffer;
    }

    return result;
  };

  const convertFloat32ToInt16 = (input: Float32Array) => {
    const output = new Int16Array(input.length);
    for (let i = 0; i < input.length; i += 1) {
      const sample = Math.max(-1, Math.min(1, input[i]!));
      output[i] = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
    }
    return output;
  };

  const buildPcmArrayBuffer = (chunks: Int16Array[]) => {
    const totalLength = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
    const merged = new Int16Array(totalLength);
    let offset = 0;

    chunks.forEach((chunk) => {
      merged.set(chunk, offset);
      offset += chunk.length;
    });

    return merged.buffer;
  };

  const buildFixedAudioSourceMeta = (
    source: 'mic' | 'system',
    durationMs: number,
    virtualMeetingCapture: boolean
  ) => ({
    sentAtMs: Date.now(),
    durationMs,
    source,
    segments: [{ source, startOffsetMs: 0, endOffsetMs: durationMs }],
    desktopMeetingCaptureIsVirtual: virtualMeetingCapture,
    dualPcmCapture: true,
  });

  const disconnectStreamingPcmNodes = () => {
    const disconnectPair = (
      source: MediaStreamAudioSourceNode | null,
      processor: ScriptProcessorNode | null
    ) => {
      if (processor) {
        try {
          processor.disconnect();
        } catch { /* already disconnected */ }
        processor.onaudioprocess = null;
      }
      if (source) {
        try {
          source.disconnect();
        } catch { /* already disconnected */ }
      }
    };
    disconnectPair(streamingSourceRef.current, streamingProcessorRef.current);
    disconnectPair(streamingMicSourceRef.current, streamingMicProcessorRef.current);
    disconnectPair(streamingSystemSourceRef.current, streamingSystemProcessorRef.current);
    streamingSourceRef.current = null;
    streamingProcessorRef.current = null;
    streamingMicSourceRef.current = null;
    streamingSystemSourceRef.current = null;
    streamingMicProcessorRef.current = null;
    streamingSystemProcessorRef.current = null;
    useDualPcmCaptureRef.current = false;
    useSingleSourcePcmCaptureRef.current = false;
    fixedPcmSourceRef.current = null;
  };

  const stopLocalAudioSourceSampler = () => {
    if (localAudioSourceSamplerRef.current) {
      clearInterval(localAudioSourceSamplerRef.current);
      localAudioSourceSamplerRef.current = null;
    }

    if (localAudioSourceCleanupRef.current) {
      localAudioSourceCleanupRef.current();
      localAudioSourceCleanupRef.current = null;
    }

    localAudioSourceCurrentRef.current = null;
    localAudioSourceCandidateRef.current = null;
    captionSourceSplitStateRef.current.clear();
  };

  const getAnalyserRms = (analyser: AnalyserNode, buffer: Float32Array<ArrayBuffer>) => {
    analyser.getFloatTimeDomainData(buffer);
    let sum = 0;
    for (let index = 0; index < buffer.length; index += 1) {
      const sample = buffer[index]!;
      sum += sample * sample;
    }
    return Math.sqrt(sum / buffer.length);
  };

  const classifyAudioSourceSample = (micLevel: number, systemLevel: number, isVirtualMeetingCapture = false) => {
    const micDomRatio = isVirtualMeetingCapture
      ? LOCAL_AUDIO_SOURCE_VIRTUAL_MIC_DOMINANCE_RATIO
      : LOCAL_AUDIO_SOURCE_MIC_DOMINANCE_RATIO;
    const micMinLevel = isVirtualMeetingCapture
      ? LOCAL_AUDIO_SOURCE_VIRTUAL_MIC_MIN_LEVEL
      : LOCAL_AUDIO_SOURCE_MIC_MIN_LEVEL;

    if (systemLevel >= LOCAL_AUDIO_SOURCE_MIN_LEVEL) {
      if (
        micLevel >= micMinLevel &&
        micLevel >= systemLevel * micDomRatio
      ) {
        return 'mic';
      }

      if (systemLevel >= micLevel * LOCAL_AUDIO_SOURCE_SYSTEM_PRIORITY_RATIO) {
        return 'system';
      }
    }

    if (
      micLevel >= micMinLevel &&
      micLevel >= Math.max(systemLevel * micDomRatio, micMinLevel)
    ) {
      return 'mic';
    }

    if (systemLevel >= LOCAL_AUDIO_SOURCE_MIN_LEVEL) {
      return 'system';
    }

    return 'unknown';
  };

  const recordLocalAudioSourceState = (
    timestamp: number,
    micLevel: number,
    systemLevel: number,
  ) => {
    const source = classifyAudioSourceSample(micLevel, systemLevel, isVirtualMeetingCaptureEnabled);
    if (source === 'unknown') return;

    const currentSource = localAudioSourceCurrentRef.current;
    if (!currentSource) {
      localAudioSourceCurrentRef.current = source;
      localAudioSourceSegmentsRef.current = [{ source, start: timestamp }];
      localAudioSourceCandidateRef.current = null;
      return;
    }

    if (source === currentSource) {
      localAudioSourceCandidateRef.current = null;
      return;
    }

    const candidate = localAudioSourceCandidateRef.current;
    if (!candidate || candidate.source !== source) {
      localAudioSourceCandidateRef.current = { source, since: timestamp };
      return;
    }

    if (timestamp - candidate.since < LOCAL_AUDIO_SOURCE_SWITCH_CONFIRM_MS) {
      return;
    }

    const switchTimestamp = candidate.since;
    const segments = localAudioSourceSegmentsRef.current;
    const previousSegment = segments[segments.length - 1];
    if (previousSegment && previousSegment.end === undefined) {
      previousSegment.end = switchTimestamp;
    }
    segments.push({ source, start: switchTimestamp });
    localAudioSourceCurrentRef.current = source;
    localAudioSourceCandidateRef.current = null;
  };

  const startLocalAudioSourceSampler = (
    audioContext: AudioContext,
    micSource: MediaStreamAudioSourceNode | null,
    systemSource: MediaStreamAudioSourceNode | null,
  ) => {
    stopLocalAudioSourceSampler();
    localAudioSourceActivityRef.current = [];
    localAudioSourceSegmentsRef.current = [];
    localAudioSourceCurrentRef.current = null;
    localAudioSourceCandidateRef.current = null;

    if (!micSource && !systemSource) {
      return;
    }

    const micAnalyser = micSource ? audioContext.createAnalyser() : null;
    const systemAnalyser = systemSource ? audioContext.createAnalyser() : null;

    if (micSource && micAnalyser) {
      micAnalyser.fftSize = 1024;
      micSource.connect(micAnalyser);
    }

    if (systemSource && systemAnalyser) {
      systemAnalyser.fftSize = 1024;
      systemSource.connect(systemAnalyser);
    }

    const micBuffer = micAnalyser ? new Float32Array(micAnalyser.fftSize) : null;
    const systemBuffer = systemAnalyser ? new Float32Array(systemAnalyser.fftSize) : null;

    localAudioSourceCleanupRef.current = () => {
      try { micSource?.disconnect(micAnalyser as AnalyserNode); } catch { /* node already disconnected */ }
      try { systemSource?.disconnect(systemAnalyser as AnalyserNode); } catch { /* node already disconnected */ }
    };

    localAudioSourceSamplerRef.current = setInterval(() => {
      const timestamp = Date.now();
      const micLevel = micAnalyser && micBuffer ? getAnalyserRms(micAnalyser, micBuffer) : 0;
      const systemLevel = systemAnalyser && systemBuffer ? getAnalyserRms(systemAnalyser, systemBuffer) : 0;
      const cutoff = timestamp - LOCAL_AUDIO_SOURCE_HISTORY_MS;

      localAudioSourceActivityRef.current = [
        ...localAudioSourceActivityRef.current.filter((sample) => sample.timestamp >= cutoff),
        { timestamp, micLevel, systemLevel },
      ];
      localAudioSourceSegmentsRef.current = localAudioSourceSegmentsRef.current.filter((segment) => (
        (segment.end || timestamp) >= cutoff
      ));
      recordLocalAudioSourceState(timestamp, micLevel, systemLevel);
    }, LOCAL_AUDIO_SOURCE_SAMPLE_INTERVAL_MS);
  };

  const timestampToMs = (value: TimestampLike) => {
    if (!value) return null;
    if (value instanceof Date) {
      const ms = value.getTime();
      return Number.isFinite(ms) ? ms : null;
    }
    if (typeof value === 'number') {
      return value > 100000000000 ? value : Date.now() + value * 1000;
    }
    const parsed = new Date(value).getTime();
    return Number.isFinite(parsed) ? parsed : null;
  };

  const inferDesktopAudioSource = (startValue: TimestampLike, endValue?: TimestampLike): AudioSourceLabel => {
    const startMs = timestampToMs(startValue);
    const endMs = timestampToMs(endValue) || (startMs ? startMs + 1000 : null);
    if (!startMs || !endMs) return 'unknown';
    const midpointMs = startMs + ((endMs - startMs) / 2);
    const segmentAtMidpoint = localAudioSourceSegmentsRef.current.find((segment) => (
      midpointMs >= segment.start && midpointMs < (segment.end || Number.POSITIVE_INFINITY)
    ));
    if (segmentAtMidpoint?.source) {
      return segmentAtMidpoint.source;
    }

    const windowStart = Math.min(startMs, endMs) - LOCAL_AUDIO_SOURCE_LOOKBEHIND_MS;
    const windowEnd = Math.max(startMs, endMs) + LOCAL_AUDIO_SOURCE_LOOKAHEAD_MS;
    let samples = localAudioSourceActivityRef.current.filter((sample) => (
      sample.timestamp >= windowStart && sample.timestamp <= windowEnd
    ));

    if (samples.length === 0) {
      const fallbackStart = Date.now() - LOCAL_AUDIO_SOURCE_RECENT_FALLBACK_MS;
      samples = localAudioSourceActivityRef.current.filter((sample) => sample.timestamp >= fallbackStart);
    }

    if (samples.length === 0) return 'unknown';

    const totals = samples.reduce((acc, sample) => ({
      mic: acc.mic + sample.micLevel,
      system: acc.system + sample.systemLevel,
    }), { mic: 0, system: 0 });

    const avgMic = totals.mic / samples.length;
    const avgSystem = totals.system / samples.length;

    const virtualCapture = isVirtualMeetingCaptureEnabled;
    return classifyAudioSourceSample(avgMic, avgSystem, virtualCapture);
  };

  const findAppendOnlyCaptionSplitIndex = (text: string, lockedTextLength: number) => {
    const lockedIndex = Math.max(0, Math.min(text.length - 1, lockedTextLength));
    const searchEnd = Math.min(text.length - 1, lockedIndex + 80);

    for (let index = lockedIndex; index <= searchEnd; index += 1) {
      if (/\s/.test(text.charAt(index))) {
        return index;
      }
    }

    return lockedIndex;
  };

  const splitCaptionBySourceSlices = (
    caption: CaptionLike,
    intervalStart: TimestampLike,
    intervalEnd: TimestampLike,
    splitKey: string,
  ): Array<{ source: AudioSourceLabel; caption: CaptionLike }> => {
    const text = typeof caption?.text === 'string' ? caption.text : '';
    const inferredIntervalSource = inferDesktopAudioSource(intervalStart, intervalEnd);
    const currentSource: AudioSourceLabel = inferredIntervalSource === 'unknown'
      ? (localAudioSourceCurrentRef.current || 'unknown')
      : inferredIntervalSource;
    const existingState = captionSourceSplitStateRef.current.get(splitKey);
    const textShrankOrChanged = existingState && !text.startsWith(existingState.lastText);
    const initialSource: AudioSourceLabel = inferredIntervalSource === 'unknown' ? currentSource : inferredIntervalSource;
    const state = !existingState || textShrankOrChanged
      ? {
        lastText: '',
        lastSource: initialSource,
        splits: initialSource === 'unknown' ? [] : [{ index: 0, source: initialSource }],
      }
      : existingState;

    if (text.trim().length === 0) {
      captionSourceSplitStateRef.current.set(splitKey, {
        ...state,
        lastText: text,
      });
      return [{ source: state.lastSource, caption }];
    }

    const nextSource = currentSource === 'unknown' ? state.lastSource : currentSource;
    const sourceChanged = (
      nextSource !== 'unknown' &&
      state.lastSource !== 'unknown' &&
      nextSource !== state.lastSource
    );
    let splitAdded = false;

    if (sourceChanged && text.length > state.lastText.length) {
      const splitIndex = findAppendOnlyCaptionSplitIndex(text, state.lastText.length);
      if (splitIndex > 0 && splitIndex < text.length) {
        state.splits.push({ index: splitIndex, source: nextSource });
        splitAdded = true;
      }
    } else if (state.splits.length === 0 && nextSource !== 'unknown') {
      state.splits.push({ index: 0, source: nextSource });
    }

    state.lastText = text;
    if (!sourceChanged || splitAdded) {
      state.lastSource = nextSource;
    }
    state.splits = state.splits
      .filter((split, splitIndex, splits) => (
        split.index < text.length && splits.findIndex((candidate) => candidate.index === split.index) === splitIndex
      ))
      .sort((a, b) => a.index - b.index);
    captionSourceSplitStateRef.current.set(splitKey, state);

    if (state.splits.length <= 1) {
      return [{ source: state.splits[0]?.source || nextSource, caption }];
    }

    const parts: Array<{ source: AudioSourceLabel; caption: CaptionLike }> = [];
    state.splits.forEach((split, splitIndex) => {
      const nextSplit = state.splits[splitIndex + 1];
      const partText = text.slice(split.index, nextSplit?.index || text.length).trim();
      if (!partText) return;
      parts.push({
        source: split.source,
        caption: {
          ...caption,
          text: partText,
        },
      });
    });

    return parts.length > 0 ? parts : [{ source: inferDesktopAudioSource(intervalStart, intervalEnd), caption }];
  };

  const getAudioSourceMetadataForChunk = (durationMs: number): AudioSourceMeta => {
    const virtualMeetingCapture = isVirtualMeetingCaptureEnabled;
    const chunkEndMs = Date.now();
    const chunkStartMs = chunkEndMs - Math.max(1, durationMs);
    const samples = localAudioSourceActivityRef.current.filter((sample) => (
      sample.timestamp >= chunkStartMs - LOCAL_AUDIO_SOURCE_SAMPLE_INTERVAL_MS &&
      sample.timestamp <= chunkEndMs + LOCAL_AUDIO_SOURCE_SAMPLE_INTERVAL_MS
    ));

    if (virtualMeetingCapture && samples.length > 0) {
      let maxMic = 0;
      let maxSys = 0;
      samples.forEach((s) => {
        if (s.micLevel > maxMic) maxMic = s.micLevel;
        if (s.systemLevel > maxSys) maxSys = s.systemLevel;
      });
      if (maxSys >= LOCAL_AUDIO_SOURCE_MIN_LEVEL) {
        const micClearlyDominates =
          maxMic >= maxSys * VIRTUAL_CHUNK_MIC_DOMINANCE_OVER_SYSTEM &&
          maxMic >= LOCAL_AUDIO_SOURCE_VIRTUAL_MIC_MIN_LEVEL;
        if (!micClearlyDominates) {
          return {
            sentAtMs: chunkEndMs,
            durationMs,
            source: 'system',
            segments: [{ source: 'system', startOffsetMs: 0, endOffsetMs: durationMs }],
            desktopMeetingCaptureIsVirtual: true,
          };
        }
      }
    }

    if (samples.length === 0) {
      const source = localAudioSourceCurrentRef.current || 'unknown';
      return {
        sentAtMs: chunkEndMs,
        durationMs,
        source,
        segments: [{ source, startOffsetMs: 0, endOffsetMs: durationMs }],
        desktopMeetingCaptureIsVirtual: virtualMeetingCapture,
      };
    }

    const segments: Array<{ source: 'mic' | 'system' | 'unknown'; startOffsetMs: number; endOffsetMs: number }> = [];
    let currentSource: 'mic' | 'system' | 'unknown' = classifyAudioSourceSample(samples[0]!.micLevel, samples[0]!.systemLevel, virtualMeetingCapture);
    let currentStartMs = chunkStartMs;

    samples.forEach((sample, sampleIndex) => {
      const source = classifyAudioSourceSample(sample.micLevel, sample.systemLevel, virtualMeetingCapture);
      if (source === currentSource) return;

      const switchMs = Math.max(chunkStartMs, Math.min(chunkEndMs, sample.timestamp));
      segments.push({
        source: currentSource,
        startOffsetMs: Math.max(0, Math.round(currentStartMs - chunkStartMs)),
        endOffsetMs: Math.max(0, Math.round(switchMs - chunkStartMs)),
      });
      currentSource = source;
      currentStartMs = switchMs;

      if (sampleIndex === samples.length - 1) {
        currentStartMs = switchMs;
      }
    });

    segments.push({
      source: currentSource,
      startOffsetMs: Math.max(0, Math.round(currentStartMs - chunkStartMs)),
      endOffsetMs: Math.max(1, Math.round(durationMs)),
    });

    const totals = samples.reduce((acc, sample) => {
      const source = classifyAudioSourceSample(sample.micLevel, sample.systemLevel, virtualMeetingCapture);
      acc[source] = (acc[source] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);
    const source = (totals.mic || 0) > (totals.system || 0) ? 'mic' : ((totals.system || 0) > 0 ? 'system' : 'unknown');

    return {
      sentAtMs: chunkEndMs,
      durationMs,
      source,
      segments: segments.filter((segment) => segment.endOffsetMs > segment.startOffsetMs),
      desktopMeetingCaptureIsVirtual: virtualMeetingCapture,
    };
  };

  const isInPersonOtherAudioContext = (): boolean => (
    isInPersonSpeechContext(speechToSpeechModeRef.current)
  );

  const isInPersonDualStreamCapture = (): boolean => (
    isInPersonOtherAudioContext() && getStoredOtherAudioCaptureMode() === 'both'
  );

  const logCaptureSuppressed = (stream: 'mic' | 'system' | 'merged', reason: string) => {
    const key = `${stream}:${reason}`;
    const now = Date.now();
    if (now - (captureSuppressLogAtRef.current[key] ?? 0) < 1000) return;
    captureSuppressLogAtRef.current[key] = now;
    const playback = translatedPlaybackActiveBySourceRef.current;
    productionLogger.debug('[FAST-S2S] capture-suppressed', {
      stream,
      reason,
      micPlayback: playback.mic,
      systemPlayback: playback.system,
      mode: speechToSpeechModeRef.current,
    });
  };

  const getDualStreamCaptureActive = (): boolean => (
    isDualStreamCaptureActive({
      speechToSpeechMode: speechToSpeechModeRef.current,
      inPersonDualCapture: isInPersonDualStreamCapture(),
    })
  );

  const prepareDualPcmCapture = (pcm: Int16Array, streamId: 'mic' | 'system'): Int16Array => {
    const playback = translatedPlaybackActiveBySourceRef.current;
    const capturedPcm = prepareDualPcmForTtsCapture(
      pcm,
      streamId,
      playback,
      getDualStreamCaptureActive(),
      {
        speechToSpeechMode: speechToSpeechModeRef.current,
        isolatedPlayback: translatedPlaybackIsolatedBySourceRef.current,
        virtualMeetingCapture: isVirtualMeetingCaptureEnabled,
      }
    );
    if (capturedPcm !== pcm) {
      logCaptureSuppressed(streamId, 'tts-playback');
    }
    return capturedPcm;
  };

  const shouldSuppressS2sOffMicBleedCapture = (): boolean => {
    if (speechToSpeechModeRef.current !== 'off' || translationModeRef.current !== 'fast') {
      return false;
    }

    const fallbackStart = Date.now() - LOCAL_AUDIO_SOURCE_RECENT_FALLBACK_MS;
    const samples = localAudioSourceActivityRef.current.filter(
      (sample) => sample.timestamp >= fallbackStart
    );
    if (samples.length === 0) return false;

    const totals = samples.reduce((acc, sample) => ({
      mic: acc.mic + sample.micLevel,
      system: acc.system + sample.systemLevel,
    }), { mic: 0, system: 0 });
    const avgMic = totals.mic / samples.length;
    const avgSystem = totals.system / samples.length;

    return classifyAudioSourceSample(avgMic, avgSystem, false) === 'system';
  };

  const applyFastTranscriptPresentation = (sections: TranscriptSectionLike[]) => applySpeakerAliases<TranscriptSection>(
    filterS2sOffBleedSections(
      applyDesktopSpeakerLabelOverrides(sections),
      {
        speechToSpeechMode: speechToSpeechModeRef.current,
        translationMode: translationModeRef.current,
      }
    )
  );

  const getRealtimeSpeakerNameForSource = (source: 'mic' | 'system') => {
    const otherAudioCaptureMode = getStoredOtherAudioCaptureMode();

    return resolveRealtimeSpeakerName(source, {
      translationMode: translationModeRef.current,
      speechToSpeechMode: speechToSpeechModeRef.current,
      environment: getRuntimeSessionEnvironment(environmentRef.current),
      otherAudioCaptureMode,
    });
  };

  const applyDesktopSpeakerLabelOverrides = (data: TranscriptSectionLike[]): TranscriptSection[] => {
    if (!Array.isArray(data)) return [];

    const normalizedSections: TranscriptSectionLike[] = [];
    const inPerson = isInPersonSessionEnvironment(environmentRef.current);
    const activeTranslationMode = translationModeRef.current;
    const activeSpeechToSpeechMode = speechToSpeechModeRef.current;
    const isFastMode = activeTranslationMode === 'fast';
    data.forEach((section) => {
      const captions = Array.isArray(section?.captions) ? section.captions : [];
      const originalSpeakerName = section?.speakerName || 'Speaker';
      const micLabel = inPerson ? originalSpeakerName : 'You';
      const remoteLabel = getRealtimeSpeakerNameForSource('system');
      const serverAudioSource = section?.desktopAudioSource === 'mic' || section?.desktopAudioSource === 'system'
        ? section.desktopAudioSource
        : null;

      if (activeSpeechToSpeechMode === 'online-incoming' && isFastMode) {
        if (serverAudioSource === 'mic') return;
        const resolvedSource = serverAudioSource
          || inferDesktopAudioSource(section?.start || captions[0]?.start, section?.end);
        if (resolvedSource === 'mic') return;
        normalizedSections.push({
          ...section,
          speakerName: getRealtimeSpeakerNameForSource('system'),
          desktopAudioSource: 'system',
          captions: captions.length > 0 ? captions : section?.captions,
        });
        return;
      }

      if (activeSpeechToSpeechMode === 'online-outgoing' && isFastMode) {
        if (serverAudioSource === 'system') return;
        const resolvedSource = serverAudioSource
          || inferDesktopAudioSource(section?.start || captions[0]?.start, section?.end);
        if (resolvedSource === 'system') return;
        normalizedSections.push({
          ...section,
          speakerName: getRealtimeSpeakerNameForSource('mic'),
          desktopAudioSource: 'mic',
          captions: captions.length > 0 ? captions : section?.captions,
        });
        return;
      }

      if (activeSpeechToSpeechMode === 'online-both' && isFastMode) {
        const resolvedSource = serverAudioSource
          || inferDesktopAudioSource(section?.start || captions[0]?.start, section?.end);
        if (resolvedSource !== 'mic' && resolvedSource !== 'system') return;
        normalizedSections.push({
          ...section,
          speakerName: getRealtimeSpeakerNameForSource(resolvedSource),
          desktopAudioSource: resolvedSource,
          captions: captions.length > 0 ? captions : section?.captions,
        });
        return;
      }

      if (activeSpeechToSpeechMode === 'off' && isFastMode) {
        const resolvedSource = serverAudioSource
          || inferDesktopAudioSource(section?.start || captions[0]?.start, section?.end);
        if (resolvedSource === 'mic' || resolvedSource === 'system') {
          normalizedSections.push({
            ...section,
            speakerName: getRealtimeSpeakerNameForSource(resolvedSource),
            desktopAudioSource: resolvedSource,
            captions: captions.length > 0 ? captions : section?.captions,
          });
          return;
        }
      }

      if (serverAudioSource) {
        const resolvedSource = serverAudioSource;
        normalizedSections.push({
          ...section,
          speakerName: resolvedSource === 'mic'
            ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel)
            : (isFastMode ? remoteLabel : (section?.speakerName === 'You' ? originalSpeakerName : originalSpeakerName)),
          desktopAudioSource: resolvedSource,
        });
        return;
      }

      if (captions.length === 0) {
        const source = inferDesktopAudioSource(section?.start, section?.end);
        normalizedSections.push({
          ...section,
          speakerName: source === 'mic' ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel) : (isFastMode ? remoteLabel : originalSpeakerName),
          desktopAudioSource: source,
        });
        return;
      }

      const sectionSource = inferDesktopAudioSource(section?.start || captions[0]?.start, section?.end);
      const groups: Array<{ source: 'mic' | 'system' | 'unknown'; captions: CaptionLike[]; startIndex: number }> = [];
      const expandedCaptions = captions.flatMap((caption, captionIndex) => {
        const nextCaption = captions[captionIndex + 1];
        const intervalStart = caption?.start || section?.start;
        const intervalEnd = nextCaption?.start || section?.end || caption?.start;
        return splitCaptionBySourceSlices(
          caption,
          intervalStart,
          intervalEnd,
          `${section?.sectionId || 'section'}:${captionIndex}`,
        );
      });

      expandedCaptions.forEach(({ caption, source: captionSource }, captionIndex) => {
        const source = captionSource === 'unknown'
          ? (sectionSource === 'unknown' ? (groups[groups.length - 1]?.source || 'unknown') : sectionSource)
          : captionSource;
        const speakerName = source === 'mic' ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel) : (isFastMode ? remoteLabel : originalSpeakerName);
        const currentGroup = groups[groups.length - 1];
        const currentSpeakerName = currentGroup?.source === 'mic' ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel) : (isFastMode ? remoteLabel : originalSpeakerName);

        if (!currentGroup || currentSpeakerName !== speakerName) {
          groups.push({ source, captions: [caption], startIndex: captionIndex });
          return;
        }

        currentGroup.captions.push(caption);
      });

      if (groups.length <= 1) {
        const source = groups[0]?.source || sectionSource;
        normalizedSections.push({
          ...section,
          speakerName: source === 'mic' ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel) : (isFastMode ? remoteLabel : originalSpeakerName),
          desktopAudioSource: source,
          captions: groups[0]?.captions || captions,
        });
        return;
      }

      groups.forEach((group, groupIndex) => {
        const firstCaption = group.captions[0];
        const nextGroup = groups[groupIndex + 1];
        normalizedSections.push({
          ...section,
          start: firstCaption?.start || section?.start,
          end: nextGroup?.captions?.[0]?.start || section?.end,
          sectionId: Number.isFinite(section?.sectionId)
            ? ((section.sectionId as number) * 1000) + groupIndex
            : `${section?.sectionId || 'section'}-${groupIndex}`,
          speakerName: group.source === 'mic' ? (isFastMode ? getRealtimeSpeakerNameForSource('mic') : micLabel) : (isFastMode ? remoteLabel : originalSpeakerName),
          captions: group.captions,
          translatedText: groupIndex === groups.length - 1 ? section?.translatedText : undefined,
          desktopAudioSource: group.source,
        });
      });
    });

    return normalizedSections as TranscriptSection[];
  };

  const getFastRealtimeMergeContext = () => ({
    getSpeakerName: getRealtimeSpeakerNameForSource,
    defaultIncomingLanguage: resolveDisplayLanguageToOpenAi(translatedLanguage),
    defaultUplinkLanguage: resolveDisplayLanguageToOpenAi(bothModeUplinkTranslatedLanguage),
  });

  const mergeFastRealtimeIntoTranscript = (
    prev: TranscriptSection[],
    payload: Parameters<typeof mergeFastRealtimeSections>[1]
  ) => mergeFastRealtimeSections(prev, payload, getFastRealtimeMergeContext());

  // Component-level function to flush reverse audio buffer
  const flushReverseAudioBuffer = () => {
    if (reverseAudioBufferRef.current.length > 0) {
      const combinedBlob = new Blob(reverseAudioBufferRef.current, { type: "audio/webm" });
      combinedBlob.arrayBuffer().then((buffer) => {
        const reverseSocket = reverseSocketRef.current;
        if (reverseSocket?.connected && reverseSocket.isDeepgramReady) {
          const estimatedDurationMs = Math.max(250, reverseAudioBufferRef.current.length * 250);
          const audioSourceMeta = getAudioSourceMetadataForChunk(estimatedDurationMs);
          reverseSocket.emit("audio_chunk", {
            spokenLanguage: reverseSocket.reverseSpokenLanguage || spokenLanguage,
            spokenLanguageModel: reverseSocket.reverseSpokenLanguageModel || spokenLanguageModel,
            translatedLanguage: reverseSocket.reverseTranslatedLanguage || translatedLanguage,
            translationEnabled: true,
            buffer: buffer,
            audioSourceMeta,
          });
        }
      });
      reverseAudioBufferRef.current = [];
    }
  };

  const emitReversePendingBuffer = (reverseSocket: AppSocket, queuedBuffer: ArrayBuffer) => {
    const durationMs = Math.max(1, Math.round(queuedBuffer.byteLength / PCM_BYTES_PER_MS));
    const audioSourceMeta = getAudioSourceMetadataForChunk(durationMs);
    reverseSocket.emit("audio_chunk", {
      spokenLanguage: reverseSocket.reverseSpokenLanguage || spokenLanguage,
      spokenLanguageModel: reverseSocket.reverseSpokenLanguageModel || spokenLanguageModel,
      translatedLanguage: reverseSocket.reverseTranslatedLanguage || translatedLanguage,
      translationEnabled: true,
      buffer: queuedBuffer,
      audioSourceMeta,
    });
  };

  const flushReversePendingBuffers = (reverseSocket: AppSocket | null) => {
    if (!reverseSocket?.connected || !reverseSocket?.isDeepgramReady) return;
    const queued = reversePendingBuffersRef.current;
    if (!queued.length) return;
    reversePendingBuffersRef.current = [];

    if (!reverseInitChunkSentRef.current && webmInitChunkRef.current) {
      webmInitChunkRef.current.arrayBuffer().then((initBuffer) => {
        const initDurationMs = Math.max(250, Math.round(initBuffer.byteLength / PCM_BYTES_PER_MS));
        const initAudioSourceMeta = getAudioSourceMetadataForChunk(initDurationMs);
        reverseSocket.emit("audio_chunk", {
          spokenLanguage: reverseSocket.reverseSpokenLanguage || spokenLanguage,
          spokenLanguageModel: reverseSocket.reverseSpokenLanguageModel || spokenLanguageModel,
          translatedLanguage: reverseSocket.reverseTranslatedLanguage || translatedLanguage,
          translationEnabled: true,
          buffer: initBuffer,
          audioSourceMeta: initAudioSourceMeta,
        });
        reverseInitChunkSentRef.current = true;
        queued.forEach((queuedBuffer) => {
          emitReversePendingBuffer(reverseSocket, queuedBuffer);
        });
      }).catch((e) => {
        productionLogger.error('Failed to flush reverse init chunk:', e);
        reversePendingBuffersRef.current = [...queued, ...reversePendingBuffersRef.current].slice(-40);
      });
      return;
    }

    queued.forEach((queuedBuffer) => {
      emitReversePendingBuffer(reverseSocket, queuedBuffer);
    });
  };

  // Set up component-level event listener for reverse socket ready
  useEffect(() => {
    const handleReverseSocketReady = () => {
      flushReverseAudioBuffer();
    };

    window.addEventListener('reverseSocketReady', handleReverseSocketReady);
    return () => window.removeEventListener('reverseSocketReady', handleReverseSocketReady);
  }, [spokenLanguage, spokenLanguageModel, translatedLanguage]);

  useEffect(() => {
    return () => {
      cleanupSharedAsrSockets();
    };
  }, []);

  // Stabilize bullet height and ensure chatBox is ready when switching tabs
  useEffect(() => {
    // Force a consistent height state when switching between tabs
    // This prevents size fluctuations when navigating between Transcript and Notes
    if (activeTab === "Transcript" || activeTab === "Notes") {
      // Don't force bulletOverlayOpen here - let Bullet component manage its own state
      if (activeTab === "Transcript") {
        // On Transcript the Ask MeetingCopilot input takes focus; on Memos the memo
        // editor keeps it (NotionEditor focuses itself when active).
        setTimeout(() => {
          // Trigger focus by dispatching a custom event that the Bullet component can listen to
          window.dispatchEvent(new CustomEvent('focusAskMeetingCopilot'));
        }, 100);
      }
    } else {
      // Close Ask MeetingCopilot when not on an Ask MeetingCopilot tab to avoid interference
      if (bulletOverlayOpen) {
        setBulletOverlayOpen(false);
        setBulletMinimizedHeight(10);
      }
    }
  }, [activeTab]);

  // Initialize Ask MeetingCopilot panel state based on Real-time Summary toggle on app reload
  useEffect(() => {
    // Debug: Log current state
    productionLogger.debug('🔄 Current Ask MeetingCopilot state:', {
      bulletPointEnabled,
      bulletOverlayOpen,
      bulletMinimizedHeight,
      activeTab
    });
  }, [bulletPointEnabled, bulletOverlayOpen]);

  // AI-generated prompts state - DISABLED
  const [aiPrompt3] = useState<string | undefined>(undefined);
  const [aiPrompt4] = useState<string | undefined>(undefined);

  // No transcription monitoring state
  const [showNoTranscriptionPopup, setShowNoTranscriptionPopup] = useState(false);
  const [noTranscriptionTimer, setNoTranscriptionTimer] = useState<NodeJS.Timeout | null>(null);
  const [lastTranscriptionTime, setLastTranscriptionTime] = useState<Date | null>(new Date());
  const MeetingCopilotConfigLocal = localStorage.getItem("MeetingCopilot_config");

  const getSpokenLanguagePlan = () => {
    try {
      const config = localStorage.getItem('MeetingCopilot_config');
      if (config) {
        const parsed = JSON.parse(config);
        return parsed.spokenLanguagePlan || 'basic';
      }
    } catch (e) {
      productionLogger.error('Error parsing MeetingCopilot_config', e);
    }
    return 'basic';
  };

  const getSafeTranslationQuality = (value: number) => {
    const plan = getSpokenLanguagePlan();
    return constrainTranslationQualityForMode(
      enabled,
      clampTranslationQuality(value, plan, spokenLanguageModel)
    );
  };

  const bumpTranscriptionActivityTimer = () => {
    if (!isRecordingRef.current) return;
    setLastTranscriptionTime(new Date());
    setCountdown(60);
    // Close inactivity modal and stop its countdown as soon as we get transcript again
    if (countdownIntervalRef.current) {
      clearInterval(countdownIntervalRef.current);
      countdownIntervalRef.current = null;
    }
    setIsModalOpen(false);
  };

  const applyFastRealtimeTranscriptUpdate = (payload: Parameters<typeof mergeFastRealtimeIntoTranscript>[1]) => {
    bumpTranscriptionActivityTimer();
    if (noTranscriptionTimer) {
      clearTimeout(noTranscriptionTimer);
      setNoTranscriptionTimer(null);
      setShowNoTranscriptionPopup(false);
    }
    setTranscriptData((prev) => applyFastTranscriptPresentation(
      mergeFastRealtimeIntoTranscript(prev, payload)
    ));
  };

  useEffect(() => {
    if (MeetingCopilotConfigLocal) {
      setMeetingCopilotConfig(MeetingCopilotConfigLocal)
    }
  }, [])

  // Note: Chat messages are NOT persisted to localStorage
  // They should clear on app reload and only persist during tab navigation within the same session


  const renderBulletComponent = () => {
    // Always render Bullet component to maintain state; show it on the Transcript
    // and Memos tabs when not maximized
    const shouldShow = (activeTab === "Transcript" || activeTab === "Notes") && !isTranscriptMaximized;
    return <Bullet
      show={shouldShow}
      socketRef={socketRef}
      isToggleOn={bulletPointEnabled}
      setIsToggleOn={(value) => {
        dispatch(setBulletPointEnabled(value));
        // Save to localStorage for persistence
        try {
          const config = localStorage.getItem('MeetingCopilot_config');
          const parsedConfig = config ? JSON.parse(config) : {};
          localStorage.setItem('MeetingCopilot_config', JSON.stringify({
            ...parsedConfig,
            bulletPointEnabled: value
          }));
        } catch (error) {
          productionLogger.warn('Failed to save bullet point state to localStorage:', error);
        }
      }}
      onOpenChange={setBulletOverlayOpen}
      onHeightChange={setBulletMinimizedHeight}
      messages={messages}
      setMessages={setMessages}
      userId={user?.id}
      transcriptData={transcriptData}
      input={chatInput}
      setInput={setChatInput}
      notepad={notepad}
      setNotepad={setNotepad}
      prompt3={aiPrompt3}
      prompt4={aiPrompt4}
      spokenLanguage={spokenLanguage}
      translatedLanguage={translatedLanguage}
      onNavigateToMemos={() => setActiveTab("Notes")}
      autoFocusInput={activeTab === "Transcript"}
    />;
  };

  // Function to save state to localStorage and reload the page
  const saveStateAndReload = (newSpokenLanguage: string, newSpokenLanguageModel: string) => {
    // Set the language change flag

    // Save the current state
    const currentState = {
      spokenLanguage: newSpokenLanguage,
      spokenLanguageModel: newSpokenLanguageModel,
      translatedLanguage,
      translationEnabled: enabled,
      isRecording,
      paused,
      recordingTime,
      translationTime,
      notepad,
      meetingId,
      isLanguageChange: true,
      shareCode
    };
    let didReload = false;
    const reload = () => {
      if (didReload) return;
      didReload = true;
      writeRestoreRecordingViewOnReload();
      if (currentState.meetingId && currentState.shareCode) {
        persistLiveShareInvitation(currentState.meetingId, currentState.shareCode);
        writeLanguageChangeLiveSessionReload({
          meetingId: currentState.meetingId,
          shareCode: currentState.shareCode,
          reservationKey: liveShareReservationKey,
          wasRecording: currentState.isRecording,
        });
      }
      window.location.reload();
    };

    // Make sure we have the latest meetingID from the server before saving
    if (socketRef.current && socketRef.current.connected && isRecording) {
      const reloadFallback = setTimeout(reload, 1500);
      socketRef.current.emit("get_current_meeting_id", {}, (response?: MeetingIdAck) => {
        clearTimeout(reloadFallback);
        if (response && response.meetingId) {
          //productionLogger.debug(`Got latest meetingID from server: ${response.meetingId}`);
          // productionLogger.debug("response", response)

          currentState.meetingId = response.meetingId;
          currentState.shareCode = response?.shareCode || currentState.shareCode

        }


        // Also save the current transcript data separately
        //productionLogger.debug(`Saved existing transcript data with ${transcriptData.length} entries`);

        // Reload the page to create a new socket connection
        reload();
      });
    } else {

      // Also save the current transcript data separately
      //productionLogger.debug(`Saved existing transcript data with ${transcriptData.length} entries`);

      // Reload the page to create a new socket connection
      reload();
    }
  };

  const stopNoTranscriptionMonitoring = () => {
    if (noTranscriptionTimer) {
      clearTimeout(noTranscriptionTimer);
      setNoTranscriptionTimer(null);
    }
    setShowNoTranscriptionPopup(false);
    setLastTranscriptionTime(null);
  };

  // Background audio upload function - runs asynchronously with captured data
  const uploadAudioInBackgroundWithData = async (audioBlob: Blob, targetMeetingId: string, capturedMessages: Message[]) => {
    if (!audioBlob || audioBlob.size === 0) return;

    setIsUploadingAudio(true);
    setAudioUploadError(null);

    // Simplify messages for saving (especially Generate Speech)
    const simplifiedMessages = capturedMessages.map(msg => {
      if (msg.speechData) {
        // For Generate Speech, save both speech text and pronunciation
        const speechContent = `${t('bullet.generateSpeech')}: ${msg.speechData.speechText}`;
        const pronunciationContent = msg.speechData.pronunciation ? `\n${t('bullet.pronunciation')}: ${msg.speechData.pronunciation}` : '';
        return {
          role: msg.role,
          content: msg.role === 'user' ? msg.content : speechContent + pronunciationContent,
          timestamp: msg.timestamp,
          actionType: msg.actionType
        };
      }
      return {
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp,
        actionType: msg.actionType
      };
    });

    const formData = new FormData();
    formData.append('audio', audioBlob, 'recording.webm');
    formData.append('meetingId', targetMeetingId);
    formData.append('meetingChat', JSON.stringify(simplifiedMessages));

    try {
      const response = await fetch(`${baseURL}/audio/upload-audio`, {
        method: 'POST',
        body: formData,
        headers: {
          'Authorization': String(localStorage.getItem('accessToken')),
        },
      });

      if (!response.ok) {
        throw new Error(`Upload failed: ${response.status}`);
      }

      await response.json();

      // Clear stored audio blob on success
      setLastAudioBlob(null);
      setLastMeetingId(null);

    } catch {
      setAudioUploadError(t('desktopUi.errors.audioUpload'));
    } finally {
      setIsUploadingAudio(false);
    }
  };

  // Legacy background audio upload function - uses current messages state
  const uploadAudioInBackground = async (audioBlob: Blob, targetMeetingId: string) => {
    return uploadAudioInBackgroundWithData(audioBlob, targetMeetingId, messages);
  };

  // Retry audio upload function
  const retryAudioUpload = () => {
    if (lastAudioBlob && lastMeetingId) {
      uploadAudioInBackground(lastAudioBlob, lastMeetingId);
    }
  };

  // Finalize meeting immediately - no waiting for audio
  const finalizeMeetingImmediately = async () => {
    // Guard against double-finalize (e.g. stopRecording + leftover inactivity timeout)
    if (isFinalizingRef.current) return;
    isFinalizingRef.current = true;
    navigatedOnStopRef.current = false;

    // Clear share UI before navigate/unmount so stale clients cannot survive remount.
    clearLiveShareInvitationState(shareCodeRef.current);

    // Store current values before state changes
    const currentMeetingId = meetingId;
    const currentMessages = [...messages];
    const currentNotepad = notepad;
    const currentTranscriptData = transcriptData;
    const isDelayAnalysisSession = delayAnalysisConfigEnabled;
    const meetingStoragePreference = localStorage.getItem('MeetingCopilot_meeting_storage_preference');
    const postMeetingDestination = resolvePostMeetingDestination(meetingStoragePreference, paidMeetingStorageAccessRef.current);
    postMeetingDestinationRef.current = postMeetingDestination;
    const discardMeeting = paidMeetingStorageAccessRef.current
      && meetingStoragePreference === 'nothing';
    const generateMeetingNotes = !paidMeetingStorageAccessRef.current
      || (meetingStoragePreference !== 'transcript' && meetingStoragePreference !== 'nothing');
    const shouldNavigateToDetail =
      !!onRecordingCompleteRef.current
      && !isDelayAnalysisSession
      && !discardMeeting
      && !!currentMeetingId;
    const shouldReturnHome = discardMeeting && !!onRecordingDiscardedRef.current;

    // Show MeetingDetails only when staying on the recorder fallback path
    setShowMeetingDetails(!isDelayAnalysisSession && !discardMeeting && !shouldNavigateToDetail);

    // Stop recording immediately
    setPaused(false);
    dispatch(setEnabled(false));
    setIsRecording(false);
    try {
      getElectronIpc()?.send('recording-state', { recording: false });
    } catch { /* not running in Electron */ }

    if (!shouldNavigateToDetail && !shouldReturnHome) {
      setLoading(true);

      // Set 2-minute timeout to clear loading state if server doesn't respond
      const timeout = setTimeout(() => {
        setLoading(false);
        setLoadingTimeoutRef(null);
        productionLogger.debug("⏰ Loading timeout: Server didn't respond with meeting_finalized within 2 minutes");
        // Reload page as fallback
        window.location.reload();
      }, 120000); // 2 minutes

      setLoadingTimeoutRef(timeout);
    }

    // Clear recording timer
    if (recordingTimerRef.current) {
      clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = null;
    }

    // Simplify messages for saving (especially Generate Speech)
    const simplifiedMessages = currentMessages.map(msg => {
      if (msg.speechData) {
        // For Generate Speech, save both speech text and pronunciation
        const speechContent = `${t('bullet.generateSpeech')}: ${msg.speechData.speechText}`;
        const pronunciationContent = msg.speechData.pronunciation ? `\n${t('bullet.pronunciation')}: ${msg.speechData.pronunciation}` : '';
        return {
          role: msg.role,
          content: msg.role === 'user' ? msg.content : speechContent + pronunciationContent,
          timestamp: msg.timestamp,
          actionType: msg.actionType
        };
      }
      return {
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp,
        actionType: msg.actionType
      };
    });

    // Stop sending audio to Deepgram (meeting finalizes on server)
    socketRef.current?.emit("stop_recording", {
      notepad: currentNotepad,
      recording: null,
      askMeetingCopilotMessages: simplifiedMessages,
      delayAnalysisEnabled: isDelayAnalysisSession,
      discardMeeting,
      generateMeetingNotes,
      speakerAliases: speakerAliasesRef.current,
    });

    // Clean up any shared ASR sockets immediately
    cleanupSharedAsrSockets();

    if (shouldReturnHome) {
      // No audio upload is needed for a discarded meeting. Leaving the recorder
      // runs its existing capture cleanup and restores the main window bounds.
      navigatedOnStopRef.current = true;
      onRecordingDiscardedRef.current?.();
      return;
    }

    // Wait for MediaRecorder's final dataavailable event before snapshotting the saved audio.
    // The live STT/S2S pipeline has already been stopped independently above.
    const savedMediaRecorder = mediaRecorderRef.current;
    let savedRecordingComplete = true;
    if (savedMediaRecorder) {
      try {
        await stopMediaRecorderAndWait(savedMediaRecorder);
      } catch (error) {
        savedRecordingComplete = false;
        setAudioUploadError(t('desktopUi.errors.audioUpload'));
        productionLogger.warn('Saved recording did not finalize cleanly; skipping partial audio upload', {
          meetingId: currentMeetingId,
          recorderState: savedMediaRecorder.state,
          error: error instanceof Error ? error.message : 'unknown recorder stop error',
        });
      } finally {
        // `onstop` owns the normal cleanup. This fallback also covers encoder
        // errors/timeouts where the stop event never arrives.
        releaseLiveAudioCaptureResources(savedMediaRecorder.stream);
        if (mediaRecorderRef.current === savedMediaRecorder) {
          mediaRecorderRef.current = null;
        }
      }
    } else {
      // Capture acquisition can still be in flight when Stop is clicked.
      releaseLiveAudioCaptureResources();
    }

    const currentAudioChunks = [...audioChunksRef.current];
    const savedRecordingMimeType = savedMediaRecorder?.mimeType || SAVED_RECORDING_MIME_TYPE;

    // Start background audio upload if we have audio AND transcript data
    // If no transcript, server will delete the meeting, so don't waste time uploading audio
    const hasTranscript = currentTranscriptData && currentTranscriptData.length > 0;

    if (savedRecordingComplete && !isDelayAnalysisSession && !discardMeeting && currentAudioChunks.length > 0 && currentMeetingId && hasTranscript) {
      const audioBlob = new Blob(currentAudioChunks, { type: savedRecordingMimeType });

      // Store for retry capability
      setLastAudioBlob(audioBlob);
      setLastMeetingId(currentMeetingId);

      uploadAudioInBackgroundWithData(audioBlob, currentMeetingId, currentMessages);
    } else if (!hasTranscript) {
      // No transcript data available, skipping audio upload (meeting will be deleted by server)
    }

    // Reset audio chunks to free memory
    audioChunksRef.current = [];

    // Leave the recorder immediately; detail page shows skeletons while save finishes
    if (shouldNavigateToDetail) {
      navigatedOnStopRef.current = true;
      onRecordingCompleteRef.current?.(
        currentMeetingId,
        calendarNoteContext?.title,
        postMeetingDestination === 'transcript' ? 'transcript' : 'enhanced',
      );
    }
  };

  // Legacy wrapper - now uses immediate finalization (no reload)
  const saveMeetingAndReload = async () => {
    await finalizeMeetingImmediately();
  };

  const stopRecordingDueToInactivity = async () => {
    await saveMeetingAndReload();
  };

  // Countdown-only function for transcription dialog
  const startTranscriptionDialogCountdown = () => {
    // productionLogger.debug('🔴 TRANSCRIPTION DIALOG DEBUG: Starting countdown', {
    //   timestamp: new Date().toISOString()
    // });

    setTranscriptionDialogCountdown(60);
    transcriptionDialogTimerRef.current = setInterval(() => {
      setTranscriptionDialogCountdown(prev => {
        if (prev <= 1) {
          clearInterval(transcriptionDialogTimerRef.current ?? undefined);
          transcriptionDialogTimerRef.current = null;

          // When countdown expires, programmatically click the "Okay" button
          // This ensures identical execution path to manual click
          // productionLogger.debug('🔴 TRANSCRIPTION DIALOG DEBUG: Countdown expired, clicking Okay button programmatically', {
          //   timestamp: new Date().toISOString()
          // });

          // Programmatically click the "Okay" button to ensure identical logic
          if (transcriptionDialogOkayButtonRef.current) {
            transcriptionDialogOkayButtonRef.current.click();
          } else {
            productionLogger.error('Transcription dialog: Okay button ref not found, falling back to handler');
            handleTranscriptionDialogResponse();
          }
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
  };

  // Handler for transcription dialog response - only handles dialog state
  const handleTranscriptionDialogResponse = async () => {
    // Clear timers
    if (transcriptionDialogTimeoutRef.current) {
      clearTimeout(transcriptionDialogTimeoutRef.current);
      transcriptionDialogTimeoutRef.current = null;
    }
    if (transcriptionDialogTimerRef.current) {
      clearInterval(transcriptionDialogTimerRef.current ?? undefined);
      transcriptionDialogTimerRef.current = null;
    }

    // Hide dialog
    setShowTranscriptionDialog(false);
    setTranscriptionDialogData(null);

    // Use universal save meeting function
    await saveMeetingAndReload();
  };


  // Countdown-only function for translation dialog
  const startTranslationDialogCountdown = () => {
    // productionLogger.debug('🔴 TRANSLATION DIALOG DEBUG: Starting countdown', {
    //   timestamp: new Date().toISOString()
    // });

    setDialogCountdown(30); // 30 seconds countdown
    dialogCountdownTimerRef.current = setInterval(() => {
      setDialogCountdown(prev => {
        if (prev <= 1) {
          clearInterval(dialogCountdownTimerRef.current ?? undefined);
          dialogCountdownTimerRef.current = null;

          // When countdown expires, programmatically click the "No" button to ensure identical logic
          // productionLogger.debug('🔴 TRANSLATION DIALOG DEBUG: Countdown expired, clicking No button programmatically', {
          //   timestamp: new Date().toISOString()
          // });

          // Programmatically click the "No" button to ensure identical logic
          if (translationDialogNoButtonRef.current) {
            translationDialogNoButtonRef.current.click();
          } else {
            productionLogger.error('Translation dialog: No button ref not found, falling back to handler');
            handleTranslationDialogResponse(false, 'auto-close-fallback');
          }
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
  };

  // Handler for translation dialog responses - only handles dialog state and logic
  const handleTranslationDialogResponse = async (continueWithTranscription: boolean, _source = 'unknown') => {
    // productionLogger.debug('🔴 TRANSLATION DIALOG DEBUG: handleTranslationDialogResponse called', {
    //   continueWithTranscription,
    //   source,
    //   audioChunksLength: audioChunks.length,
    //   transcriptDataLength: transcriptData?.length || 0,
    //   mediaRecorderState: mediaRecorderRef.current?.state,
    //   meetingId: meetingId,
    //   timestamp: new Date().toISOString()
    // });

    // Clear timeout
    if (dialogTimeoutRef.current) {
      clearTimeout(dialogTimeoutRef.current);
      dialogTimeoutRef.current = null;
    }

    // Clear countdown timer
    if (dialogCountdownTimerRef.current) {
      clearInterval(dialogCountdownTimerRef.current ?? undefined);
      dialogCountdownTimerRef.current = null;
    }

    // Hide dialog
    setShowTranslationDialog(false);
    setDialogData(null);

    if (continueWithTranscription) {
      // User clicked Yes - turn off translation toggle and real-time summary toggle, then continue with transcription
      // productionLogger.debug('🔴 TRANSLATION DIALOG DEBUG: User chose to continue with transcription');
      dispatch(setEnabled(false));
      dispatch(setBulletPointEnabled(false));

      // Update socket config to disable translation
      socketRef.current?.emit("update_config", withDelayAnalysisConfig({
        spokenLanguage,
        spokenLanguageModel,
        translatedLanguage,
        translationEnabled: false
      }));

      // Also disable bullet points since they consume translation minutes
      socketRef.current?.emit('toggle_bullet_points', false);

      // Stop translation timer
      if (translationTimerRef.current) {
        clearInterval(translationTimerRef.current);
        translationTimerRef.current = null;
      }

      // Reset translation time
      setTranslationTime(0);

      // Reset the translation limit exceeded flag since we're continuing with transcription
      updateTranslationLimitExceeded(false);
      return;
    } else {
      // User clicked No or timed out - show paywall and stop recording
      // productionLogger.debug('🔴 TRANSLATION DIALOG DEBUG: User chose to stop recording, about to call saveMeetingAndReload', {
      //   source,
      //   audioChunksLength: audioChunks.length,
      //   transcriptDataLength: transcriptData?.length || 0,
      //   meetingId: meetingId
      // });
      updateTranslationLimitExceeded(true);

      // Use universal save meeting function
      await saveMeetingAndReload();
    }
  };


  // Function to get the current meeting ID from the server
  const getCurrentMeetingId = () => {
    if (socketRef.current && socketRef.current.connected) {
      //productionLogger.debug(`Getting current meeting ID with language: ${spokenLanguage}, model: ${spokenLanguageModel}`);
      socketRef.current.emit("get_current_meeting_id", {}, (response?: MeetingIdAck) => {
        if (response && response.meetingId) {
          // productionLogger.debug("response", response)
          //productionLogger.debug(`Received meetingID from server: ${response.meetingId}`);
          meetingIdRef.current = response.meetingId;
          setMeetingId(response.meetingId);
          if (response.shareCode) {
            reservedMeetingIdRef.current = response.meetingId;
            shareCodeRef.current = response.shareCode;
            persistLiveShareInvitation(response.meetingId, response.shareCode);
            setReservedMeetingId(response.meetingId);
            setShareCode(response.shareCode);
          }
        } else {
          //productionLogger.debug("No meetingID received from server");
        }
      });
    } else {
      //productionLogger.debug("Socket not connected, cannot get meetingID");
    }
  };

  // Recover the server-owned meeting/share identity after a reconnect or a
  // renderer refresh. This also repairs sessions where the initial
  // `sharing_code` event arrived while React was switching into recording UI.
  useEffect(() => {
    if (authReady && isRecording) {
      getCurrentMeetingId();
    }
  }, [authReady, isRecording]);

  // A link can be reserved after recording starts, and the desktop socket can
  // reconnect while the meeting keeps running. Re-attach the durable link to
  // the live host whenever those identities become available. The server
  // treats this operation idempotently.
  useEffect(() => {
    const activeSocket = socketRef.current;
    const activeMeetingId = meetingId || meetingIdRef.current;
    if (!authReady || !isRecording || !shareCode || !activeMeetingId || !activeSocket?.connected) {
      return;
    }
    activeSocket.emit('activate_live_translation_sharing', {
      shareCode,
      meetingId: activeMeetingId,
    }, (response?: { ok?: boolean; meetingId?: string; error?: string }) => {
      if (response?.ok && response.meetingId && response.meetingId !== meetingIdRef.current) {
        meetingIdRef.current = response.meetingId;
        reservedMeetingIdRef.current = response.meetingId;
        setMeetingId(response.meetingId);
        setReservedMeetingId(response.meetingId);
        persistLiveShareInvitation(response.meetingId, shareCode);
      }
      if (response?.ok === false) {
        productionLogger.warn('[AudioRecorder] Could not restore live translation sharing:', response.error);
      }
    });
  }, [authReady, isRecording, meetingId, shareCode]);

  // Function to get available audio devices
  const getAudioDevices = async () => {
    try {
      //productionLogger.debug("🔍 Getting audio devices...");

      // Get all media devices without requesting microphone access
      //productionLogger.debug("📱 Enumerating all media devices...");
      const devices = await navigator.mediaDevices.enumerateDevices();
      //productionLogger.debug(`📊 Found ${devices.length} total devices`);

      // Filter for audio input devices
      const audioInputDevices = devices.filter(device => device.kind === 'audioinput');
      //productionLogger.debug(`🎤 Found ${audioInputDevices.length} audio input devices:`);

      audioInputDevices.forEach((_device, _index) => {
        //productionLogger.debug(`  ${index + 1}. ${device.label || 'Unnamed device'} (ID: ${device.deviceId})`);
      });

      setAudioDevices(audioInputDevices);

      const preferPhysicalMic = isInPersonSpeechContext(speechToSpeechModeRef.current);
      const resolveDefaultMicId = () => {
        if (preferPhysicalMic) {
          const physicalMic = pickPhysicalMicrophone(audioInputDevices);
          if (physicalMic?.deviceId) return physicalMic.deviceId;
        }
        return audioInputDevices[0]?.deviceId;
      };

      // If we have devices and no selected device, select the first one
      if (audioInputDevices.length > 0 && !selectedAudioDevice) {
        const deviceId = resolveDefaultMicId();
        if (deviceId) {
          setSelectedAudioDevice(deviceId);
          localStorage.setItem('selectedMicrophone', deviceId);
        }
      } else if (selectedAudioDevice) {
        // Check if the selected device still exists in the list
        const selectedDevice = audioInputDevices.find((d) => d.deviceId === selectedAudioDevice);
        const selectedIsVirtual = preferPhysicalMic
          && selectedDevice
          && !isPhysicalCaptureDevice(selectedDevice);
        if ((!selectedDevice || selectedIsVirtual) && audioInputDevices.length > 0) {
          const deviceId = resolveDefaultMicId();
          if (deviceId) {
            setSelectedAudioDevice(deviceId);
            localStorage.setItem('selectedMicrophone', deviceId);
          }
        }
      }

      if (selectedMeetingCaptureDevice !== 'loopback') {
        const meetingCaptureExists = audioInputDevices.some(d => d.deviceId === selectedMeetingCaptureDevice);
        if (!meetingCaptureExists) {
          setSelectedMeetingCaptureDevice('loopback');
          localStorage.setItem(MEETING_AUDIO_CAPTURE_STORAGE_KEY, 'loopback');
        }
      }
    } catch {
      //productionLogger.debug("❌ Error getting audio devices:", error);
      setError({ type: 'microphone' });
    }
  };

  // Effect to get audio devices when component mounts
  useEffect(() => {
    getAudioDevices();

    // Listen for device changes
    navigator.mediaDevices.addEventListener('devicechange', getAudioDevices);

    return () => {
      navigator.mediaDevices.removeEventListener('devicechange', getAudioDevices);
    };
  }, [selectedAudioDevice, selectedMeetingCaptureDevice]);

  useEffect(() => {
    const handleMicrophoneChanged = (event: Event) => {
      const deviceId = (event as CustomEvent<string>).detail;
      if (typeof deviceId === 'string') {
        setSelectedAudioDevice(deviceId);
      }
    };

    const handleMeetingAudioCaptureChanged = (event: Event) => {
      const deviceId = (event as CustomEvent<string>).detail;
      if (typeof deviceId === 'string') {
        setSelectedMeetingCaptureDevice(deviceId);
      }
    };

    window.addEventListener('microphoneChanged', handleMicrophoneChanged as EventListener);
    window.addEventListener('meetingAudioCaptureChanged', handleMeetingAudioCaptureChanged as EventListener);
    return () => {
      window.removeEventListener('microphoneChanged', handleMicrophoneChanged as EventListener);
      window.removeEventListener('meetingAudioCaptureChanged', handleMeetingAudioCaptureChanged as EventListener);
    };
  }, []);

  const stopMeetingAudioStreams = () => {
    if (meetingAudioStreamRef.current) {
      stopMediaStreamTracks(meetingAudioStreamRef.current);
      meetingAudioStreamRef.current = null;
    }
    if (displayStreamRef.current) {
      stopMediaStreamTracks(displayStreamRef.current);
      displayStreamRef.current = null;
    }
  };

  const stopMicrophoneAudioStream = () => {
    const microphoneStream = microphoneStreamRef.current;
    microphoneStreamRef.current = null;
    stopMediaStreamTracks(microphoneStream);
  };

  const closeRecordingAudioContext = () => {
    const audioContext = audioContextRef.current;
    audioContextRef.current = null;
    if (audioContext && audioContext.state !== 'closed') {
      void audioContext.close().catch((error) => {
        productionLogger.warn('Failed to close recording audio context:', error);
      });
    }
  };

  /** Terminal cleanup only. Pause/resume deliberately keeps these resources. */
  const releaseLiveAudioCaptureResources = (recorderStream?: MediaStream | null) => {
    liveAudioCaptureGenerationRef.current += 1;
    stopLocalAudioSourceSampler();
    disconnectStreamingPcmNodes();
    systemAudioGainRef.current = null;
    micAudioGainRef.current = null;
    stopMediaStreamTracks(recorderStream);
    stopMicrophoneAudioStream();
    stopMeetingAudioStreams();
    closeRecordingAudioContext();
  };

  useEffect(() => {
    stopMeetingAudioStreams();
  }, [selectedMeetingCaptureDevice]);

  const getLoopbackSystemAudioStream = async () => {
    if (isRequestingDisplayStream.current) {
      return displayStreamRef.current;
    }
    if (displayStreamRef.current) {
      // Check if cached stream is still active
      const tracks = displayStreamRef.current.getAudioTracks();
      if (tracks.length > 0 && tracks[0]!.readyState === 'live') {
        return displayStreamRef.current;
      } else {
        // Clear dead stream reference
        displayStreamRef.current = null;
      }
    }
    isRequestingDisplayStream.current = true;
    try {
      // Now we can collect the actual system audio since permission is already granted
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: false,
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          sampleRate: 44100
        }
      });

      // Add error handling for when permission is denied
      stream.getVideoTracks().forEach(track => {
        track.onended = () => {
          setError({ type: 'systemAudio' });
        };
      });

      displayStreamRef.current = stream;
      return stream;
    } catch (err) {
      productionLogger.error("Failed to get system audio:", err);
      setError({ type: 'screenRecording' });
      return null;
    } finally {
      isRequestingDisplayStream.current = false;
    }
  };

  const getMeetingAudioStream = async () => {
    const useLoopbackCapture = selectedMeetingCaptureDevice === 'loopback'
      || isInPersonOtherAudioContext();
    if (useLoopbackCapture) {
      return await getLoopbackSystemAudioStream();
    }

    if (isRequestingMeetingAudioStream.current) {
      return meetingAudioStreamRef.current;
    }

    if (meetingAudioStreamRef.current) {
      const tracks = meetingAudioStreamRef.current.getAudioTracks();
      if (tracks.length > 0 && tracks[0]!.readyState === 'live') {
        return meetingAudioStreamRef.current;
      }
      meetingAudioStreamRef.current = null;
    }

    isRequestingMeetingAudioStream.current = true;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          deviceId: { exact: selectedMeetingCaptureDevice },
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
        },
        video: false,
      });

      stream.getAudioTracks().forEach(track => {
        track.onended = () => {
          setError({ type: 'systemAudio' });
        };
      });

      meetingAudioStreamRef.current = stream;
      return stream;
    } catch (err) {
      productionLogger.error("Failed to get meeting audio capture device:", err);
      setError({ type: 'systemAudio' });
      return null;
    } finally {
      isRequestingMeetingAudioStream.current = false;
    }
  };

  useEffect(() => {
    if (!navigator.mediaDevices || !window.MediaRecorder) {
      alert(t('error.recording.unsupportedBrowser'));
      return;
    }

    // Only establish socket connection if we have a user
    if (!user?.id) {
      //productionLogger.debug("Waiting for user to be loaded...");
      return;
    }



    // // // Restore state from localStorage
    // restoreStateFromLocalStorage();

    // if(!isInitialRestoration){
    //   return
    // }

    // Initialize socket connection
    setAuthReady(false); // Reset auth state on new connection
    socketRef.current = io(wsURL, {
      ...DESKTOP_SOCKET_IO_OPTIONS,
      auth: getDesktopSocketAuth(),
      query: {
        userId: user.id,
        device: "desktop",
        appVersion: appInfo?.version || "unknown",
        spokenLanguagePlan: getSpokenLanguagePlan(),
        spokenLanguage,
        spokenLanguageModel,
        translationEnabled: enabled ? "true" : "false",
        translatedLanguage,
        translationMode,
        bulletPointEnabled: bulletPointEnabled ? "true" : "false"
      },
    });

    // Main socket readiness is still tracked for diagnostics/reconnects,
    // but the primary recorder no longer waits on it before sending audio.
    socketRef.current.isDeepgramReady = false;

    socketRef.current.on('deepgram_ready', (data: unknown) => {
      productionLogger.debug('✅ Main socket Deepgram ready:', data);
      socketRef.current!.isDeepgramReady = true;
      setIsTranscriptionReady(true);
    });

    // Real-time meeting memory update from the server. Ignore stale updates if
    // the session is not Premium spoken language in non-Fast mode.
    socketRef.current.on('meeting_flow_update', (data: { flow?: unknown } | undefined) => {
      if (!isRealTimeMemoryEligible(getSpokenLanguagePlan(), translationModeRef.current)) {
        setMeetingFlow("");
        return;
      }
      if (typeof data?.flow === 'string') {
        setMeetingFlow(data.flow);
      }
    });

    // If the transport drops mid-recording, the server cleans up the Deepgram stream.
    // Without resetting readiness + re-emitting start_recording on reconnect, the session can appear "stopped".
    socketRef.current.on('disconnect', (reason: unknown) => {
      socketRef.current!.isDeepgramReady = false;
      setIsTranscriptionReady(false);
      productionLogger.debug('❌ Main socket disconnected:', reason);
    });

    socketRef.current.on('connect', () => {
      // productionLogger.debug(`Socket connected successfully with language: ${spokenLanguage}, model: ${spokenLanguageModel}`);
      setError(null);

      // Sync current environment so the server can label web speakers correctly.
      socketRef.current?.emit(
        'update_environment',
        getRuntimeSessionEnvironment(environmentRef.current),
      );

      // Trigger server to check and emit limit status by sending current config
      socketRef.current?.emit("update_config", withDelayAnalysisConfig({
        spokenLanguagePlan: getSpokenLanguagePlan(),
        spokenLanguage,
        spokenLanguageModel,
        translatedLanguage,
        translationEnabled: enabled,
        bulletPointEnabled,
        translationMode,
        textToSpeechEnabled,
        textToSpeechSessionMode,
        speechToSpeechMode,
        otherAudioCaptureMode: getStoredOtherAudioCaptureMode(),
      }));

      // If we reconnect while a recording is ongoing, restart the backend session.
      // This will create a new backend meeting if the previous socket died unexpectedly, but it's better than a silent "stopped" stream.
      if (isRecordingRef.current) {
        socketRef.current!.isDeepgramReady = false;
        emitFullConfigToServer();
        socketRef.current?.emit("start_recording", {
          existingMeetingId: meetingIdRef.current || reservedMeetingIdRef.current || undefined,
          shareCode: shareCodeRef.current || undefined,
          spokenLanguagePlan: getSpokenLanguagePlan(),
          translationEnabled: enabled,
          translationMode,
          textToSpeechEnabled,
          textToSpeechSessionMode,
          speechToSpeechMode,
          otherAudioCaptureMode: getStoredOtherAudioCaptureMode(),
          delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
          discardMeeting: paidMeetingStorageAccessRef.current
            && localStorage.getItem('MeetingCopilot_meeting_storage_preference') === 'nothing',
          ...calendarStartRecordingPayload,
        });
      }

      // Request current usage data from server
      socketRef.current?.emit("get_current_meeting_id", {}, (response?: MeetingIdAck) => {
        // This callback might contain usage data - check if response has usage info
        if (response && response.usage) {
          if (response.usage.translationUsage !== undefined && response.usage.translationLimit !== undefined) {
            updateTranslationUsage({
              used: response.usage.translationUsage,
              limit: response.usage.translationLimit
            });
          }
          if (response.usage.transcriptionUsage !== undefined && response.usage.transcriptionLimit !== undefined) {
            updateTranscriptionUsage({
              used: response.usage.transcriptionUsage,
              limit: response.usage.transcriptionLimit
            });
          }
        }
      });

      if (isRecording) {
        // Normal case for when we were recording before the page reload
        socketRef.current?.emit("update_config", withDelayAnalysisConfig({
          translatedLanguage,
          translationEnabled: enabled,
        }));

        // Request current transcript data after initial connection
        socketRef.current?.emit("get_current_meeting_id", {}, (response?: MeetingIdAck) => {
          if (response && response.meetingId) {
            //productionLogger.debug(`Got meetingID after initial connection: ${response.meetingId}`);
            // productionLogger.debug("response", response)

            setMeetingId(response.meetingId);
            // setShareCode(response?.shareCode)
          }
        });
      }
    });

    socketRef.current.on('shared_asr_requested', (data: {
      asrConfig?: { spokenLanguage?: string; spokenLanguageModel?: string; translatedLanguage?: string };
      shareCode?: string;
    } | undefined) => {
      productionLogger.debug('🔔 [SHARED_ASR_REQUESTED] Event received on desktop! Raw data:', JSON.stringify(data));
      const requestedConfig = data?.asrConfig;
      if (!requestedConfig) {
        productionLogger.debug('❌ [SHARED_ASR_REQUESTED] No asrConfig in data, returning early');
        return;
      }

      const nextSpokenLanguage = requestedConfig.spokenLanguage;
      const nextSpokenLanguageModel = requestedConfig.spokenLanguageModel;
      const nextTranslatedLanguage = requestedConfig.translatedLanguage;
      const sessionShareCode = data?.shareCode || shareCode;
      if (!nextSpokenLanguage || !nextSpokenLanguageModel || !nextTranslatedLanguage || !sessionShareCode) {
        productionLogger.debug('❌ [SHARED_ASR_REQUESTED] Missing field, returning early:', {
          nextSpokenLanguage, nextSpokenLanguageModel, nextTranslatedLanguage, sessionShareCode
        });
        return;
      }

      // Fast mode: host OpenAI Realtime handles transcription for web — no shared Deepgram/Microsoft socket.
      if (translationMode === 'fast' || nextSpokenLanguageModel === 'fast-mode') {
        productionLogger.debug('ℹ️ [SHARED_ASR_REQUESTED] Fast-mode session — no classic shared-ASR socket needed');
        return;
      }

      productionLogger.debug('🌐 [SHARED_ASR_REQUESTED] Host received request:', {
        shareCode: sessionShareCode,
        spokenLanguage: nextSpokenLanguage,
        spokenLanguageModel: nextSpokenLanguageModel,
        translatedLanguage: nextTranslatedLanguage,
      });
      ensureSharedAsrPipeline(
        nextSpokenLanguage,
        nextSpokenLanguageModel,
        nextTranslatedLanguage,
        sessionShareCode
      );
    });

    socketRef.current.on('shared_asr_routes_sync', (data: {
      shareCode?: string;
      routes?: Array<{ spokenLanguage?: string; spokenLanguageModel?: string; translatedLanguages?: string[] }>;
    } | undefined) => {
      if (translationMode === 'fast') return;
      const sessionShareCode = data?.shareCode || shareCode;
      const routes = Array.isArray(data?.routes) ? data.routes : [];
      reconcileSharedAsrPipelines(routes, sessionShareCode);
    });

    // Add handler for meeting_not_found event
    socketRef.current.on('meeting_not_found', (data) => {
      setMeetingId(data.newMeetingId);
      setReservedMeetingId(data.newMeetingId);
    });

    // Handle meeting finalization — fallback when we did not navigate on stop
    socketRef.current.on('meeting_finalized', (data) => {
      clearLiveShareInvitationState(shareCodeRef.current);
      // Clear loading timeout since server responded
      if (loadingTimeoutRef) {
        clearTimeout(loadingTimeoutRef);
        setLoadingTimeoutRef(null);
      }

      const finalizedMeetingId = data?.meetingId || meetingIdRef.current;
      const isDelayAnalysis = data?.reason === 'delay_analysis_mode';
      const wasDiscarded = data?.reason === 'user_storage_preference';

      if (
        finalizedMeetingId
        && !data?.meetingDeleted
        && !isDelayAnalysis
        && !wasDiscarded
      ) {
        void updateOperationalContextFromMeeting(finalizedMeetingId).catch((error: unknown) => {
          productionLogger.error('Could not update operational context after desktop meeting:', error);
        });
      }

      // Already opened desktop meeting notes from stop; the context update above
      // must still run after the server confirms notes were saved.
      if (navigatedOnStopRef.current) {
        return;
      }

      if (wasDiscarded && onRecordingDiscardedRef.current) {
        setLoading(false);
        setShowSuccessLoading(false);
        onRecordingDiscardedRef.current();
        return;
      }

      const shouldOpenDesktopNotes =
        !!onRecordingCompleteRef.current
        && !isDelayAnalysis
        && !data?.meetingDeleted
        && !!finalizedMeetingId;

      if (shouldOpenDesktopNotes) {
        setLoading(false);
        setShowMeetingDetails(false);
        setShowSuccessLoading(false);
        const destination = postMeetingDestinationRef.current || resolvePostMeetingDestination(
          localStorage.getItem('MeetingCopilot_meeting_storage_preference'), paidMeetingStorageAccessRef.current,
        );
        onRecordingCompleteRef.current?.(finalizedMeetingId, undefined, destination === 'transcript' ? 'transcript' : 'enhanced');
        return;
      }

      // Always show success message with countdown for all platforms
      // This ensures consistent behavior across Mac and Windows
      setLoading(false);
      setShowMeetingDetails(false);
      setSuccessLoadingMode(isDelayAnalysis ? 'delay-analysis' : wasDiscarded ? 'discarded' : 'meeting');
      setShowSuccessLoading(true);
      setReloadCountdown(5);

      // Start countdown timer
      const timer = setInterval(() => {
        setReloadCountdown(prev => {
          if (prev <= 1) {
            clearInterval(timer);
            window.location.reload();
            return 0;
          }
          return prev - 1;
        });
      }, 1000);

      setCountdownTimer(timer);
    });

    // Listen for usage limits sent by server on connect
    socketRef.current.on('usage_limits', (data) => {
      // productionLogger.debug("📊 USAGE LIMITS - Received from server:", data);

      if (data.transcriptionUsage !== undefined && data.transcriptionLimit !== undefined) {
        updateTranscriptionUsage({
          used: data.transcriptionUsage,
          limit: data.transcriptionLimit
        });
      }

      if (data.translationUsage !== undefined && data.translationLimit !== undefined) {
        updateTranslationUsage({
          used: data.translationUsage,
          limit: data.translationLimit
        });
      }

      // Mark auth as ready when usage limits are received
      setAuthReady(true);
    });

    socketRef.current.on('transcription_limit_exceeded', (data) => {
      updateTranscriptionUsage({
        used: data.usedMinutes,
        limit: data.limit
      });

      // Check if this is a "both limits exceeded" scenario
      const isBothLimitsExceeded = translationLimitReachedInSession || translationLimitExceeded;

      // Server has already disconnected Deepgram when this event is emitted
      if (isRecordingRef.current) {
        if (isBothLimitsExceeded) {
          // Show "both limits exceeded" dialog
          setTranscriptionDialogData({
            message: "bothLimitsMessage"
          });
        } else if (!enabled && !bulletPointEnabled) {
          // Original logic: only transcription was being used
          setTranscriptionDialogData({
            message: "message"
          });
        } else if (enabled || bulletPointEnabled) {
          // If translation or bullet points are currently ON, just set the limit flag but continue recording
          return;
        }

        setShowTranscriptionDialog(true);

        // Start countdown timer using dedicated function
        startTranscriptionDialogCountdown();

        // Set up 60-second timeout for auto-stop (fallback)
        transcriptionDialogTimeoutRef.current = setTimeout(() => {
          handleTranscriptionDialogResponse();
        }, 60000);
      }
    });

    socketRef.current.on('translation_limit_exceeded', (data) => {
      // productionLogger.debug("🔍 DIALOG DEBUG - translation_limit_exceeded received:", data);
      // productionLogger.debug("🔍 DIALOG DEBUG - data.canSwitchToTranscription:", data.canSwitchToTranscription);
      // productionLogger.debug("🔍 DIALOG DEBUG - isRecording:", isRecordingRef.current);

      updateTranslationUsage({
        used: Math.min(data.usedMinutes, data.limit),
        limit: data.limit
      });

      // Mark that translation limit was reached in this session (never reset this)
      setTranslationLimitReachedInSession(true);

      // If we can continue with transcription AND meeting recording is live
      if (data.canSwitchToTranscription && isRecordingRef.current) {
        // productionLogger.debug("✅ DIALOG DEBUG - Conditions met, showing dialog");
        // Show dialog first - let user decide
        setDialogData({
          transcriptionUsage: data.transcriptionUsage,
          transcriptionLimit: data.transcriptionLimit,
          usedMinutes: data.usedMinutes,
          limit: data.limit
        });
        setShowTranslationDialog(true);
        // productionLogger.debug("✅ DIALOG DEBUG - setShowTranslationDialog(true) called");

        // Start countdown timer using dedicated function
        startTranslationDialogCountdown();

        // Set up 60-second timeout for auto-No (fallback)
        dialogTimeoutRef.current = setTimeout(() => {
          // Auto-No after 60 seconds - stop recording
          handleTranslationDialogResponse(false, 'timeout-fallback');
        }, 60000); // 60 seconds

        return; // Exit here, response will be handled by dialog buttons
      } else {
        // productionLogger.debug("❌ DIALOG DEBUG - Conditions NOT met, showing paywall instead");
        // productionLogger.debug("❌ DIALOG DEBUG - canSwitchToTranscription:", data.canSwitchToTranscription);
        // productionLogger.debug("❌ DIALOG DEBUG - isRecording:", isRecording);
        // No transcription minutes available - show paywall immediately
        updateTranslationLimitExceeded(true);
        // Don't set transcriptionLimitExceeded here - only set it when we receive that event
        // The paywall message about translation limit will show
      }


    });

    // Handler for when both limits are exceeded
    socketRef.current.on('both_limits_exceeded', (data) => {
      // productionLogger.debug("🚨 CLIENT: Received both_limits_exceeded event:", data);
      // productionLogger.debug("🚨 CLIENT: isRecordingRef.current:", isRecordingRef.current);

      // Set both limit flags
      updateTranscriptionUsage({
        used: data.transcriptionUsage,
        limit: data.transcriptionLimit
      });
      updateTranslationUsage({
        used: data.translationUsage,
        limit: data.translationLimit
      });

      // Show dialog only if recording is active (live recording scenario)
      if (isRecordingRef.current) {
        // productionLogger.debug("✅ CLIENT: Showing both limits exceeded dialog - recording will be stopped");
        setTranscriptionDialogData({
          message: "bothLimitsMessage"
        });
        setShowTranscriptionDialog(true);

        // Start countdown timer using dedicated function
        startTranscriptionDialogCountdown();

        // Set up 60-second timeout for auto-stop (fallback)
        transcriptionDialogTimeoutRef.current = setTimeout(() => {
          // Only execute if the countdown timer hasn't already handled it
          if (transcriptionDialogTimerRef.current) {
            clearInterval(transcriptionDialogTimerRef.current ?? undefined);
            transcriptionDialogTimerRef.current = null;
          }
          handleTranscriptionDialogResponse();
        }, 60000);

        // productionLogger.debug("✅ CLIENT: Recording will be stopped due to both limits exceeded");
      } else {
        // productionLogger.debug("📱 CLIENT: Both limits exceeded at connection time - no recording active, no dialog needed");
        // No dialog for connection-time scenario since there's no recording to stop
        // The paywall UI will show the limits are exceeded through the limit flags
      }
    });

    // Track sustained connection failures — show error modal after 30s
    let connectionErrorTimer: ReturnType<typeof setTimeout> | null = null;

    socketRef.current.on("connect_error", (error: Error) => {
      const msg = error?.message || String(error);
      if (/unauthorized|forbidden/i.test(msg)) {
        try {
          socketRef.current?.io.reconnection(false);
        } catch { /* noop */ }
        setError({ type: 'connection', message: t('desktopUi.errors.sessionExpired') });
        return;
      }
      productionLogger.error("Socket connection error:", error);
      // Start a 30-second timer on first error; if we don't reconnect by then, show the modal
      if (!connectionErrorTimer) {
        connectionErrorTimer = setTimeout(() => {
          productionLogger.error("Socket still disconnected after 30s — showing error modal");
          setError({ type: 'connection' });
          connectionErrorTimer = null;
        }, 30000);
      }
    });

    // Clear the failure timer when connection is restored
    socketRef.current.on('connect', () => {
      if (connectionErrorTimer) {
        clearTimeout(connectionErrorTimer);
        connectionErrorTimer = null;
      }
    });

    socketRef.current.on("error", (data) => {
      productionLogger.error("Server error:", data.message);
      setError({ type: 'connection', message: data.message });
      setIsRecording(false);
    });

    socketRef.current.on("meeting_already_exists", (data) => {
      //productionLogger.debug("Meeting already exists:", data.message);
      alert(data.message);
      setIsRecording(false);
      setPaused(false);

      // Clear any existing timers
      if (recordingTimerRef.current) {
        clearInterval(recordingTimerRef.current);
        recordingTimerRef.current = null;
      }
      if (translationTimerRef.current) {
        clearInterval(translationTimerRef.current);
        translationTimerRef.current = null;
      }

      // Reset timers
      setRecordingTime(0);
      setTranslationTime(0);
    });

    socketRef.current.on("recording_blocked", (data) => {
      productionLogger.debug("Recording blocked:", data.message);

      // Store the translation key for the pending meeting modal (separate from error state)
      let translationKey = 'pending.chrome_extension'; // fallback
      if (data.type === 'chrome_extension') {
        translationKey = 'pending.chrome_extension';
      } else if (data.type === 'multiple_desktop_sessions') {
        translationKey = 'pending.multiple_desktop';
      } else if (data.type === 'server_error') {
        translationKey = 'pending.server_error';
      }

      // Store the translation key for the pending modal (avoid conflict with error modal)
      setPendingErrorKey(translationKey);

      // Show modal
      setShowPendingMeetingModal(true);

      // Stop any ongoing recording attempts
      setIsRecording(false);
      setPaused(false);

      // Clear any existing timers
      if (recordingTimerRef.current) {
        clearInterval(recordingTimerRef.current);
        recordingTimerRef.current = null;
      }
      if (translationTimerRef.current) {
        clearInterval(translationTimerRef.current);
        translationTimerRef.current = null;
      }

      // Reset timers
      setRecordingTime(0);
      setTranslationTime(0);

      // Reload page after 20 seconds
      setTimeout(() => {
        window.location.reload();
      }, 20000);
    });

    socketRef.current.on("transcript_data", (data) => {
      //productionLogger.debug("Received transcript data:", data);
      if (data && Array.isArray(data) && data.length > 0) {
        // Reset inactivity timer when main transcript receives data.
        bumpTranscriptionActivityTimer();

        // Reset no transcription monitoring since we got new transcription
        if (noTranscriptionTimer) {
          clearTimeout(noTranscriptionTimer);
          setNoTranscriptionTimer(null);
          setShowNoTranscriptionPopup(false);
        }

        // Normal transcript data update, with desktop-side mic/system speaker override.
        if (translationModeRef.current === 'fast') {
          setTranscriptData((prev) => applyFastTranscriptPresentation(
            mergeFastServerTranscriptData(prev, data)
          ));
          return;
        }
        setTranscriptData(applySpeakerAliases(applyDesktopSpeakerLabelOverrides(data)));
      }
    });

    // Track translation latency from transcription_result events
    socketRef.current.on('transcription_result', (data: TranscriptionResultPayload) => {
      recordTranslationLatency(data);
      recordSttLatency(data);
      if (data?.pipeline === 'realtime-fast') {
        applyFastRealtimeTranscriptUpdate({
          sourceText: data.text,
          translatedText: data.translation,
          desktopAudioSource: data.desktopAudioSource,
          streamId: data.desktopAudioSource,
          sectionId: data.sectionId,
          outputLanguage: data.outputLanguage,
          bothModeIncomingLanguage: data.bothModeIncomingLanguage,
          bothModeUplinkLanguage: data.bothModeUplinkLanguage,
          sourceStatus: data.sourceStatus,
          sourceProvider: data.sourceProvider,
          detectedSourceLanguage: data.detectedSourceLanguage,
          detectedSourceLanguageReliable: data.detectedSourceLanguageReliable,
          sameLanguageOutputSuppressed: data.sameLanguageOutputSuppressed === true,
          timestamp: data.timestamp,
          isFinal: !data.isPartial,
        });
      }
    });

    socketRef.current.on('realtime_translation_caption', (data: SocketEventData | undefined) => {
      if (!data) return;
      applyFastRealtimeTranscriptUpdate({
        sourceText: data.sourceText,
        translatedText: data.translatedText,
        sourceDelta: data.sourceDelta,
        translatedDelta: data.translatedDelta,
        streamId: data.streamId || data.source,
        sectionId: data.sectionId,
        outputLanguage: data.outputLanguage,
        bothModeIncomingLanguage: data.bothModeIncomingLanguage,
        bothModeUplinkLanguage: data.bothModeUplinkLanguage,
        sourceStatus: data.sourceStatus,
        sourceProvider: data.sourceProvider,
        detectedSourceLanguage: data.detectedSourceLanguage,
        detectedSourceLanguageReliable: data.detectedSourceLanguageReliable,
        sameLanguageOutputSuppressed: data.sameLanguageOutputSuppressed === true,
        isFinal: !!data.isFinal,
      });
    });

    socketRef.current.on('pipeline_mode', (payload: { mode?: string; reason?: string; message?: string }) => {
      if (translationMode !== 'fast') return;
      if (payload?.mode === 'realtime-fast') {
        productionLogger.debug('[FAST-S2S] pipeline active');
        return;
      }
      const detail = payload?.message || payload?.reason || 'classic';
      productionLogger.warn('[FAST-S2S] expected realtime-fast pipeline but got:', payload?.mode, detail);
      setError({
        type: 'connection',
        message: t('desktopUi.errors.fastTranslationUnavailable'),
      });
    });

    socketRef.current.on('realtime_translation_error', (payload: { message?: string; recoverable?: boolean }) => {
      if (translationMode !== 'fast') return;
      productionLogger.error('[FAST-S2S] realtime translation error:', payload?.message);
      if (payload?.message) {
        setError({
          type: 'connection',
          message: payload.message.includes('OPENAI_API_KEY')
            ? t('desktopUi.errors.fastTranslationNotConfigured')
            : payload.message,
        });
      }
    });

    // Chat message handler for summaries and insights (NEW: Synchronized with Ask MeetingCopilot chat)
    socketRef.current.on('chat_message_received', (chatMessage) => {
      productionLogger.debug('📨 Chat message received:', chatMessage);

      // Suppress explanation-only echo messages; UI renders explanations under summaries
      if (chatMessage && chatMessage.isExplanation) {
        productionLogger.debug('🧹 Suppressing explanation-only chat echo in UI');
        return;
      }

      // Add message to chat history with chronological ordering
      setMessages(prev => {
        const newMessages = [...prev, {
          role: chatMessage.role,
          content: chatMessage.content,
          timestamp: new Date(chatMessage.timestamp),
          isSummary: chatMessage.isSummary || false,
          speaker: chatMessage.speaker
        }];

        // Sort messages chronologically to maintain proper ordering
        return newMessages.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());
      });
    });

    // Legacy Real-time Summary event listeners (DEPRECATED: kept for backward compatibility)
    socketRef.current.on('real_time_summary_generated', (data) => {
      productionLogger.debug('⚠️ DEPRECATED: real_time_summary_generated event received (use chat_message_received instead)');
      // Add to messages as a special summary message
      const summaryMessage = {
        role: 'assistant' as const,
        content: data.text,
        timestamp: new Date(data.timestamp),
        isSummary: true,
        summaryId: `summary_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        speaker: data.speaker
      };
      setMessages(prev => [...prev, summaryMessage]);
    });

    socketRef.current.on('real_time_summaries_updated', (data) => {
      productionLogger.debug('⚠️ DEPRECATED: real_time_summaries_updated event received (use chat_message_received instead)');
      // Update all summaries in messages
      const summaryMessages = data.summaries.map((summary: { text: string; timestamp: string | number | Date; summaryId?: string; speaker?: string }) => ({
        role: 'assistant' as const,
        content: summary.text,
        timestamp: new Date(summary.timestamp),
        isSummary: true,
        summaryId: summary.summaryId || `summary_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        speaker: summary.speaker
      }));

      // Replace existing summary messages with updated ones
      setMessages(prev => {
        const nonSummaryMessages = prev.filter(msg => !msg.isSummary);
        return [...nonSummaryMessages, ...summaryMessages];
      });
    });

    // Auto-reconnect when the app regains visibility (debounced to avoid duplicate connects)
    let visibilityReconnectTimer: ReturnType<typeof setTimeout> | null = null;
    const handleVisibilityChange = () => {
      if (document.visibilityState !== 'visible' || !socketRef.current || socketRef.current.connected) {
        return;
      }
      if (visibilityReconnectTimer) clearTimeout(visibilityReconnectTimer);
      visibilityReconnectTimer = setTimeout(() => {
        visibilityReconnectTimer = null;
        if (socketRef.current && !socketRef.current.connected) {
          productionLogger.debug('🔄 App visible again, attempting socket reconnection...');
          socketRef.current.connect();
        }
      }, 400);
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      // Clear connection error timer
      if (connectionErrorTimer) {
        clearTimeout(connectionErrorTimer);
        connectionErrorTimer = null;
      }
      if (visibilityReconnectTimer) {
        clearTimeout(visibilityReconnectTimer);
        visibilityReconnectTimer = null;
      }
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      if (socketRef.current) {
        // socketRef.current.emit("leaving_meeting", { notepad, recording: null });
        socketRef.current.disconnect();
        socketRef.current.removeAllListeners();
      }
    };
  }, [spokenLanguage, spokenLanguageModel, user?.id]); // REMOVED: enabled, translatedLanguage, bulletPointEnabled - these should not trigger socket reconnection

  // Automatically adjust translation quality based on translated paragraph shape.
  // - 5 consecutive one-sentence paragraphs => increase quality by 0.5s
  // - 1 active paragraph with >= 12 sentences => decrease quality by 0.5s
  // A shared cooldown prevents oscillation from rapid adjustments.
  useEffect(() => {
    if (!enabled || !autoTranslationQualityAdjustEnabled || translationMode === 'fast') {
      activeSectionIdRef.current = null;
      activeSectionTranslationRef.current = '';
      consecutiveSingleSentenceSectionsRef.current = 0;
      lastQualityAdjustmentRef.current = 0;
      return;
    }

    const bounds = getTranslationQualityBounds(getSpokenLanguagePlan(), spokenLanguageModel);
    const min = enabled ? bounds.min : Math.max(bounds.min, TRANSCRIPTION_MINIMUM_SECONDS);
    const { max } = bounds;
    const COOLDOWN_MS = 10000;

    const applyQualityChange = (targetQuality: number, reason: string) => {
      if (targetQuality === translationQuality) {
        return;
      }

      lastQualityAdjustmentRef.current = Date.now();
      setTranslationQuality(targetQuality);
      writeStoredTranslationQuality(enabled, targetQuality);

      if (socketRef.current?.connected) {
        socketRef.current.emit('update_config', withDelayAnalysisConfig({
          translatedLanguage,
          translationEnabled: enabled,
          bulletPointEnabled,
          translationQuality: targetQuality.toString(),
        }));
      }

      window.dispatchEvent(new CustomEvent('translationQualityChanged', { detail: targetQuality }));
      productionLogger.debug(`⚡ Auto-adjusting translation quality: ${translationQuality}s → ${targetQuality}s (${reason})`);
    };

    // Skip if no sections
    if (transcriptData.length === 0) {
      activeSectionIdRef.current = null;
      activeSectionTranslationRef.current = '';
      consecutiveSingleSentenceSectionsRef.current = 0;
      return;
    }

    // Get active section (last section in array - currently being updated)
    const activeSection = transcriptData[transcriptData.length - 1];
    const currentActiveSectionId = activeSection?.sectionId ?? null;
    const currentActiveTranslation = activeSection?.translatedText || '';

    // Check if this is a new active section or if translation is being updated
    const isNewSection = currentActiveSectionId !== activeSectionIdRef.current;
    const isTranslationUpdating = currentActiveTranslation !== activeSectionTranslationRef.current && currentActiveTranslation.length > 0;
    const isActivelyTranslating = isNewSection || isTranslationUpdating;

    if (isNewSection && activeSectionIdRef.current !== null) {
      const previousSentences = splitTranscriptSentences(activeSectionTranslationRef.current);
      if (previousSentences.length === 1) {
        consecutiveSingleSentenceSectionsRef.current += 1;
      } else if (previousSentences.length > 0) {
        consecutiveSingleSentenceSectionsRef.current = 0;
      }

      if (
        consecutiveSingleSentenceSectionsRef.current >= 5 &&
        translationQuality < max
      ) {
        const targetQuality = Math.min(max, translationQuality + 0.5);
        consecutiveSingleSentenceSectionsRef.current = 0;
        applyQualityChange(targetQuality, '5 consecutive one-sentence paragraphs');
        return;
      }
    }

    // Update tracking refs
    if (isNewSection) {
      activeSectionIdRef.current = currentActiveSectionId;
      activeSectionTranslationRef.current = currentActiveTranslation;
    } else if (isTranslationUpdating) {
      // Translation is actively being updated
      activeSectionTranslationRef.current = currentActiveTranslation;
    }

    // Only check the active section if it has translation and is actively being translated/updated
    if (!activeSection?.translatedText || !isActivelyTranslating) {
      return;
    }

    // Count sentences in the active box
    const sentences = splitTranscriptSentences(activeSection.translatedText);

    // If active box has >= 12 sentences, reduce quality by 0.5s
    if (sentences.length >= 12 && translationQuality > min) {
      const now = Date.now();
      const timeSinceLastAdjustment = now - lastQualityAdjustmentRef.current;

      // Check cooldown: only reduce if 10 seconds have passed since last reduction
      if (timeSinceLastAdjustment < COOLDOWN_MS) {
        productionLogger.debug(`⏳ Quality adjustment cooldown active (${Math.ceil((COOLDOWN_MS - timeSinceLastAdjustment) / 1000)}s remaining)`);
        return; // Skip this reduction, wait for cooldown
      }

      // Calculate target quality: reduce by 0.5s from current, but not below the provider-safe floor
      const targetQuality = Math.max(min, translationQuality - 0.5);

      // Only update if we need to reduce quality further
      if (targetQuality < translationQuality) {
        consecutiveSingleSentenceSectionsRef.current = 0;
        applyQualityChange(targetQuality, `active paragraph has ${sentences.length} sentences`);
      }
    }
  }, [transcriptData, translationQuality, translatedLanguage, enabled, bulletPointEnabled, spokenLanguageModel, autoTranslationQualityAdjustEnabled, textToSpeechEnabled, translationMode]);

  // When the multilingual selection changes during a live session, push the
  // updated extra-language count to the server so the translation minute
  // multiplier (1×/2×/3×) reflects the new audience size.
  useEffect(() => {
    if (!socketRef.current?.connected) return;
    socketRef.current.emit("update_config", withDelayAnalysisConfig({
      translatedLanguage,
      translationEnabled: enabled,
      bulletPointEnabled,
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [extraTranslatedLanguages.length]);

  // Multilingual translation fan-out: for each finalized section, translate the
  // original spoken text into each extra target language via the server REST endpoint.
  useEffect(() => {
    if (translationMode === 'fast') return;
    if (!enabled || !extraTranslatedLanguages || extraTranslatedLanguages.length === 0) {
      return;
    }
    if (!transcriptData || transcriptData.length === 0) return;

    // Only fan out when the primary translation has produced text — that signals
    // the section is mature enough to translate without burning calls on every keystroke.
    transcriptData.forEach((item) => {
      const suppressPrimary = shouldSuppressSameLanguageOutput({
        section: item,
        targetLanguage: translatedLanguage,
        sameLanguageOutputEnabled,
        translationMode,
        spokenLanguage,
      });
      if (!item.translatedText && !suppressPrimary) return;
      const sourceText = (item.captions || []).map((c) => c.text).join(' ').trim();
      if (!sourceText) return;

      extraTranslatedLanguages.forEach((targetLang: string, extraLanguageIndex: number) => {
        const suppressTarget = shouldSuppressSameLanguageOutput({
          section: item,
          targetLanguage: targetLang,
          sameLanguageOutputEnabled,
          translationMode,
          spokenLanguage,
        });
        if (suppressTarget) {
          setExtraTranslationsBySection((prev) => {
            if (!prev[item.sectionId]?.[targetLang]) return prev;
            const nextSection = { ...prev[item.sectionId] };
            delete nextSection[targetLang];
            return { ...prev, [item.sectionId]: nextSection };
          });
          return;
        }
        const key = `${item.sectionId}::${targetLang}`;
        const prevSource = extraTranslationSourceRef.current[key];
        if (prevSource === sourceText) return;
        if (extraTranslationInFlightRef.current.has(key)) return;
        extraTranslationInFlightRef.current.add(key);

        fetch(`${baseURL}/translate/extra`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': localStorage.getItem('accessToken') || '',
          },
          body: JSON.stringify({
            text: sourceText,
            targetLang,
            targetPosition: extraLanguageIndex + 2,
          }),
        })
          .then((r) => r.ok ? r.json() : null)
          .then((data) => {
            const translation = data?.data?.translation;
            if (typeof translation === 'string' && translation.trim().length > 0) {
              extraTranslationSourceRef.current[key] = sourceText;
              setExtraTranslationsBySection((prev) => ({
                ...prev,
                [item.sectionId]: { ...(prev[item.sectionId] || {}), [targetLang]: translation },
              }));
            }
          })
          .catch(() => { /* best-effort */ })
          .finally(() => {
            extraTranslationInFlightRef.current.delete(key);
          });
      });
    });
  }, [transcriptData, extraTranslatedLanguages, enabled, translationMode, translatedLanguage, spokenLanguage, sameLanguageOutputEnabled]);

  useEffect(() => {
    if (authReady && socketRef.current) {
      const activeSocket = socketRef.current;
      const handleSharingCode = (data: { shareCode?: string, userId?: string }) => {
        if (data?.shareCode) {
          const activeMeetingId = meetingIdRef.current || reservedMeetingIdRef.current;
          shareCodeRef.current = data.shareCode;
          if (activeMeetingId) {
            persistLiveShareInvitation(activeMeetingId, data.shareCode);
          }
          setShareCode(data.shareCode);
          try {
            const allowParticipantS2S = localStorage.getItem('allowParticipantS2S') === 'true';
            socketRef.current?.emit('update_allow_participant_s2s', allowParticipantS2S);
            socketRef.current?.emit('update_require_attendee_email', readRequireAttendeeEmail());
            const createPostMeetingChat = readPostMeetingSharing();
            socketRef.current?.emit('update_create_post_meeting_chat', createPostMeetingChat);
          } catch {
            // best-effort — ShareMeeting also syncs this toggle
          }
        }
      };

      activeSocket.on('sharing_code', handleSharingCode);

      // Cleanup function to remove the event listener
      return () => {
        activeSocket.off('sharing_code', handleSharingCode);
      };
    }
  }, [authReady]);


  useEffect(() => {
    if (authReady && socketRef.current) {
      const activeSocket = socketRef.current;
      const handleMeetingId = (data: { meetingId?: string }) => {
        if (data?.meetingId) {
          meetingIdRef.current = data.meetingId;
          setMeetingId(data.meetingId);
        }
      };

      activeSocket.on('meeting_id', handleMeetingId);

      // Cleanup function to remove the event listener
      return () => {
        activeSocket.off('meeting_id', handleMeetingId);
      };
    }
  }, [authReady]);







  useEffect(() => {
    if (enabled && isRecording) {
      if (!translationTimerRef.current) {
        translationTimerRef.current = setInterval(() => {
          setTranslationTime((prev) => prev + 1);
        }, 1000);
      }
    } else {
      if (translationTimerRef.current) {
        clearInterval(translationTimerRef.current);
        translationTimerRef.current = null;
      }
    }

    return () => {
      if (translationTimerRef.current) {
        clearInterval(translationTimerRef.current);
        translationTimerRef.current = null;
      }
    };
  }, [enabled, isRecording]);

  // Keep isRecordingRef in sync with isRecording state
  useEffect(() => {
    isRecordingRef.current = isRecording;
    // When a session starts, transcription isn't live until the server emits `deepgram_ready`.
    // Reset so the "Getting ready…" status reflects the new session. (If the socket already
    // reported ready, keep it true to avoid a flash.)
    if (isRecording) {
      setIsTranscriptionReady(Boolean(socketRef.current?.isDeepgramReady));
    } else {
      setIsTranscriptionReady(false);
    }
  }, [isRecording]);

  useEffect(() => {
    pausedRef.current = paused;
  }, [paused]);

  // Show paywall when translation or bullet points are toggled ON while translation limit is exceeded
  useEffect(() => {
    if (translationLimitExceeded && (enabled || bulletPointEnabled)) {
      // Only show paywall if not already in a shared session
      if (!joined) {
        updateTranslationLimitExceeded(true); // Ensure paywall shows in Transcript component
      }
    }
  }, [enabled, bulletPointEnabled, translationLimitExceeded, joined]);

  // NOTE: Dialog should only appear when translation_limit_exceeded EVENT is received during live recording
  // Removed useEffect that showed dialog for pre-existing limits

  // NOTE: Both limits exceeded scenario is now handled directly in the 'both_limits_exceeded' event handler

  /** Resume MediaRecorder + sockets after manual pause or inactivity modal (no new mic/display capture). */
  const resumePausedRecordingSession = () => {
    if (!mediaRecorderRef.current || mediaRecorderRef.current.state !== "paused") {
      return false;
    }
    pcmChunkQueueRef.current = [];
    micPcmChunkQueueRef.current = [];
    systemPcmChunkQueueRef.current = [];
    mediaRecorderRef.current.resume();
    setPaused(false);
    setIsRecording(true);
    setStopButtonDisabled(true);
    setTimeout(() => setStopButtonDisabled(false), 5000);
    if (!recordingTimerRef.current) {
      recordingTimerRef.current = setInterval(() => {
        setRecordingTime((prev) => prev + 1);
      }, 1000);
    }
    socketRef.current?.emit("resume_recording");
    const rev = reverseSocketRef.current;
    if (rev?.connected) {
      rev.emit("resume_recording");
    }
    Array.from(sharedAsrSocketPoolRef.current.values()).forEach((s) => {
      if (s?.connected) {
        s.emit("resume_recording");
      }
    });
    return true;
  };

  const startRecording = async () => {
    try {
      if (!user?.id) {
        productionLogger.error("Cannot start recording: No user ID available");
        return;
      }
      //productionLogger.debug("🔴 Starting recording...");

      // Pending meeting check now handled server-side before meeting creation

      setError(null);

      // Resume from pause without re-acquiring streams (manual pause or inactivity modal)
      if (mediaRecorderRef.current?.state === "paused") {
        resumePausedRecordingSession();
        return;
      }
      // Avoid re-running capture when useEffect fires after resume (MR already active)
      if (mediaRecorderRef.current?.state === "recording") {
        return;
      }
      const captureGeneration = liveAudioCaptureGenerationRef.current + 1;
      liveAudioCaptureGenerationRef.current = captureGeneration;
      //productionLogger.debug("🎙️ Starting recording process...");
      updateTranscriptionLimitExceeded(false);
      updateTranslationLimitExceeded(false);

      const inPersonCapture = isInPersonOtherAudioContext();
      const onlineIncomingOnly = speechToSpeechMode === 'online-incoming';
      const onlineOutgoingMicOnly =
        translationMode === 'fast' && speechToSpeechMode === 'online-outgoing';
      const onlineBothFast =
        translationMode === 'fast' && speechToSpeechMode === 'online-both';
      const s2sOffCapture = speechToSpeechMode === 'off' && translationMode === 'fast';

      if (onlineBothFast) {
        const routingResult = await autoConfigureForMode('online-both', false);
        if (!routingResult.success) {
          setError({
            type: 'generic',
            message: routingResult.reason === 'unsafe-two-way-routing'
              ? 'Two-way mode needs separate virtual cables. Use VB-CABLE for the uplink and VAC Line 1 or CABLE-A for meeting capture.'
              : 'Two-way mode could not find both required virtual audio paths.',
          });
          return;
        }
      }

      if (onlineIncomingOnly) {
        const routingResult = await autoConfigureForMode('online-incoming', false);
        if (routingResult.reason === 'no-virtual-device') {
          productionLogger.warn('[FAST-S2S] capture-path', {
            speechToSpeechMode,
            capturePath: 'loopback-fallback',
            reason: 'no-virtual-device',
            virtualDeviceLabel: routingResult.virtualDeviceLabel,
          });
        }
      }

      const usePhysicalCapture = inPersonCapture || s2sOffCapture;
      const otherAudioCaptureMode = inPersonCapture
        ? getStoredOtherAudioCaptureMode()
        : 'both';
      const shouldCaptureMic = (
        !onlineIncomingOnly
        && (!inPersonCapture || otherAudioCaptureMode !== 'system')
      );
      const shouldCaptureSystem = (
        !onlineOutgoingMicOnly
        && (!inPersonCapture || otherAudioCaptureMode !== 'microphone')
      );

      if (usePhysicalCapture) {
        stopMeetingAudioStreams();
        if (shouldCaptureSystem && selectedMeetingCaptureDevice !== 'loopback') {
          setSelectedMeetingCaptureDevice('loopback');
          localStorage.setItem(MEETING_AUDIO_CAPTURE_STORAGE_KEY, 'loopback');
        }
      }

      let micStream: MediaStream | null = null;
      const inPersonMicOnly = inPersonCapture && otherAudioCaptureMode === 'microphone';
      if (shouldCaptureMic) {
        let audioConstraints: boolean | { deviceId: { exact: string } } = true;
        if (usePhysicalCapture) {
          let allDevices = await navigator.mediaDevices.enumerateDevices();
          const inputsNeedLabelPrime = allDevices
            .filter((device) => device.kind === 'audioinput')
            .every((device) => !(device.label || '').trim());
          if (inputsNeedLabelPrime) {
            try {
              const primeStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
              primeStream.getTracks().forEach((track) => track.stop());
              allDevices = await navigator.mediaDevices.enumerateDevices();
            } catch (primeError) {
              productionLogger.warn('Failed to prime microphone labels before capture:', primeError);
            }
          }
          const micResolution = resolvePhysicalMicrophoneDeviceId(
            allDevices,
            selectedAudioDevice || audioDevices[0]?.deviceId
          );
          if (micResolution.deviceId !== 'default') {
            applyMicrophoneConfig(micResolution.deviceId, false);
            setSelectedAudioDevice(micResolution.deviceId);
            audioConstraints = { deviceId: { exact: micResolution.deviceId } };
          }
        } else {
          const micDeviceId = selectedAudioDevice || audioDevices[0]?.deviceId;
          audioConstraints = micDeviceId
            ? { deviceId: { exact: micDeviceId } }
            : true;
        }
        stopMicrophoneAudioStream();
        micStream = await navigator.mediaDevices.getUserMedia({
          audio: audioConstraints,
          video: false,
        });

        if (
          liveAudioCaptureGenerationRef.current !== captureGeneration
          || isFinalizingRef.current
        ) {
          stopMediaStreamTracks(micStream);
          return;
        }
        microphoneStreamRef.current = micStream;

        micStream.getAudioTracks().forEach(track => {
          track.onended = () => {
            setShowMicDisconnectModal(true);
          };
        });
      }

      const meetingAudioStream = shouldCaptureSystem ? await getMeetingAudioStream() : null;
      if (
        liveAudioCaptureGenerationRef.current !== captureGeneration
        || isFinalizingRef.current
      ) {
        releaseLiveAudioCaptureResources();
        return;
      }
      if (shouldCaptureSystem) {
        const capturePath = selectedMeetingCaptureDevice === 'loopback' ? 'loopback' : 'virtual';
        const meetingTrackLabel = meetingAudioStream?.getAudioTracks()[0]?.label || null;
        productionLogger.debug('[FAST-S2S] capture-path', {
          speechToSpeechMode,
          meetingCaptureDevice: selectedMeetingCaptureDevice,
          capturePath,
          meetingTrackLabel,
          shouldCaptureMic,
          shouldCaptureSystem,
        });
      }
      if (shouldCaptureSystem && !meetingAudioStream) {
        releaseLiveAudioCaptureResources();
        setError({ type: 'audioStream' });
        return;
      }

      if (!micStream && !meetingAudioStream) {
        setError({ type: 'audioStream' });
        return;
      }
      //productionLogger.debug("✅ Meeting audio access granted");

      // Log the tracks from the meeting audio stream
      const displayTracks = meetingAudioStream?.getAudioTracks() ?? [];
      displayTracks.forEach((_track, _index) => {
        //productionLogger.debug(`  ${index + 1}. Track ID: ${track.id}, Label: ${track.label}, Enabled: ${track.enabled}, Muted: ${track.muted}`);
      });

      // Merge the streams - system audio + microphone
      //productionLogger.debug("🔄 Creating audio context and merging streams...");
      const audioContext = new AudioContext();
      // Keep the existing destination untouched for live PCM/STT. Saved WebM audio
      // uses a separate mono branch with headroom and peak limiting.
      const destination = audioContext.createMediaStreamDestination();
      const savedRecordingGraph = createSavedRecordingGraph(audioContext);
      let systemAudioSource: MediaStreamAudioSourceNode | null = null;
      let micAudioSource: MediaStreamAudioSourceNode | null = null;

      // Store the audio context for later use
      audioContextRef.current = audioContext;

      // Connect system audio alongside the microphone for online and in-person modes.
      if (meetingAudioStream) {
        systemAudioSource = audioContext.createMediaStreamSource(meetingAudioStream);
        const systemAudioGain = audioContext.createGain();
        const muteSystemAtStart = !shouldApplyFastPlaybackEchoSuppression
          && (
            translatedPlaybackActiveBySourceRef.current.system
            || (
              translatedPlaybackActiveBySourceRef.current.mic
              && !isVirtualMeetingCaptureEnabled
            )
          );
        systemAudioGain.gain.value = muteSystemAtStart ? 0 : 1;
        systemAudioSource.connect(systemAudioGain);
        systemAudioGain.connect(destination);
        savedRecordingGraph.connectSource(systemAudioGain);
        systemAudioGainRef.current = systemAudioGain;
      }

      // Connect microphone audio if available (gain node so TTS playback can duck mic and stop AI bleed-through)
      if (micStream && shouldCaptureMic) {
        micAudioSource = audioContext.createMediaStreamSource(micStream);
        const micGain = audioContext.createGain();
        micGain.gain.value = 1;
        micAudioSource.connect(micGain);
        micGain.connect(destination);
        savedRecordingGraph.connectSource(micGain);
        micAudioGainRef.current = micGain;
      }

      startLocalAudioSourceSampler(audioContext, micAudioSource, systemAudioSource);

      syncTranslatedSpeechPlaybackCaptureGates(false, false);

      const stream = destination.stream;
      //productionLogger.debug("✅ Streams merged successfully");

      // Log the tracks from the merged stream
      const mergedTracks = stream.getAudioTracks();
      //productionLogger.debug(`🎵 Merged stream has ${mergedTracks.length} audio tracks:`);
      mergedTracks.forEach((_track, _index) => {
        //productionLogger.debug(`  ${index + 1}. Track ID: ${track.id}, Label: ${track.label}, Enabled: ${track.enabled}, Muted: ${track.muted}`);
      });

      // Only clear messages and input when starting a new recording
      setMessages([]);
      setChatInput(''); // Clear chat input text

      try {
        const savedRecordingOptions = getSavedRecordingOptions(
          MediaRecorder.isTypeSupported.bind(MediaRecorder),
        );
        if (savedRecordingOptions.mimeType === SAVED_RECORDING_FALLBACK_MIME_TYPE) {
          productionLogger.warn('Explicit WebM Opus recording is unavailable; using WebM fallback');
        }
        const mediaRecorder = new MediaRecorder(savedRecordingGraph.stream, savedRecordingOptions);
        mediaRecorderRef.current = mediaRecorder;

        setRecordingTime(0);
        setTranslationTime(0);
        setIsRecording(true);
        setPaused(false);
        isFinalizingRef.current = false;
        postMeetingDestinationRef.current = null;

        // Disable stop button for first 5 seconds to prevent race condition
        setStopButtonDisabled(true);
        setTimeout(() => {
          setStopButtonDisabled(false);
        }, 5000);

        // Reset session tracking for new recording
        setTranslationLimitReachedInSession(false);
        pcmChunkQueueRef.current = [];
        micPcmChunkQueueRef.current = [];
        systemPcmChunkQueueRef.current = [];

        // NOTE: Dialog only shows when translation_limit_exceeded event is received during live recording

        const audioBuffer: Blob[] = [];
        // Fast mode feeds audio to OpenAI every 100ms for continuous low-latency translation.
        // Classic mode keeps the original 250ms cadence which suits Deepgram's batching.
        const CHUNK_INTERVAL = onlineBothFast ? 50 : (translationMode === 'fast' ? 100 : 250);
        let initialDelayApplied = false;

        mediaRecorder.ondataavailable = (event) => {
          if (event.data.size > 0) {
            // Capture the first WebM chunk (contains init/header data) for reverse socket bootstrapping.
            if (!webmInitChunkRef.current) {
              webmInitChunkRef.current = event.data;
            }
            audioBuffer.push(event.data);
            audioChunksRef.current.push(event.data);
          }
        };

        const emitMainAudioChunk = (
          buffer: ArrayBuffer,
          audioSourceMeta: AudioSourceMeta,
          filteredAtCapture = false,
        ) => {
          if (!buffer.byteLength) return;
          // Dual PCM is filtered per capture frame. Checking playback again here
          // could discard genuine speech captured before translation started.
          if (!filteredAtCapture && shouldSuppressAudioChunkForTtsPlayback(audioSourceMeta)) {
            if (translationMode !== 'fast') return;
            // Continuous translation needs the silence too, so the next spoken
            // fragment isn't joined directly onto the previous one in model time.
            buffer = new ArrayBuffer(buffer.byteLength);
          }
          const mainSocket = socketRef.current;
          if (!mainSocket?.connected) return;
          mainSocket.emit("audio_chunk", {
            spokenLanguage,
            spokenLanguageModel,
            translatedLanguage,
            translationEnabled: enabled,
            buffer,
            audioSourceMeta,
          });
        };

        const sendAudioChunk = () => {
          if (!isRecordingRef.current || pausedRef.current) {
            return;
          }

          if (useDualPcmCaptureRef.current) {
            const micQueue = micPcmChunkQueueRef.current;
            const systemQueue = systemPcmChunkQueueRef.current;
            if (micQueue.length === 0 && systemQueue.length === 0) {
              return;
            }
            const virtualCapture = usePhysicalCapture ? false : isVirtualMeetingCaptureEnabled;
            if (micQueue.length > 0) {
              const micBuffer = buildPcmArrayBuffer(micQueue);
              micPcmChunkQueueRef.current = [];
              const micDurationMs = Math.max(1, Math.round(micBuffer.byteLength / PCM_BYTES_PER_MS));
              emitMainAudioChunk(
                micBuffer,
                buildFixedAudioSourceMeta('mic', micDurationMs, virtualCapture),
                true,
              );
            }
            if (systemQueue.length > 0) {
              const systemBuffer = buildPcmArrayBuffer(systemQueue);
              systemPcmChunkQueueRef.current = [];
              const systemDurationMs = Math.max(1, Math.round(systemBuffer.byteLength / PCM_BYTES_PER_MS));
              emitMainAudioChunk(
                systemBuffer,
                buildFixedAudioSourceMeta('system', systemDurationMs, virtualCapture),
                true,
              );
            }
            return;
          }

          const pcmQueue = pcmChunkQueueRef.current;
          if (pcmQueue.length === 0) {
            return;
          }

          const buffer = buildPcmArrayBuffer(pcmQueue);
          const durationMs = Math.max(1, Math.round(buffer.byteLength / PCM_BYTES_PER_MS));
          const audioSourceMeta = inPersonMicOnly
            ? buildFixedAudioSourceMeta('mic', durationMs, false)
            : useSingleSourcePcmCaptureRef.current && fixedPcmSourceRef.current
            ? buildFixedAudioSourceMeta(
              fixedPcmSourceRef.current,
              durationMs,
              fixedPcmSourceRef.current === 'system' && !inPersonCapture && isVirtualMeetingCaptureEnabled
            )
            : normalizeAudioSourceMetaForPlayback(
              getAudioSourceMetadataForChunk(durationMs)
            );
          pcmChunkQueueRef.current = [];
          emitMainAudioChunk(buffer, audioSourceMeta);

          const capturedAt = Date.now();
          const preRollSequence = sharedAsrPreRollSequenceRef.current + 1;
          sharedAsrPreRollSequenceRef.current = preRollSequence;
          sharedAsrPreRollRef.current.push({
            buffer: buffer.slice(0),
            audioSourceMeta,
            capturedAt,
            sequence: preRollSequence,
          });
          const preRollCutoff = capturedAt - SHARED_ASR_PRE_ROLL_MS;
          while (
            sharedAsrPreRollRef.current.length > 0
            && sharedAsrPreRollRef.current[0]!.capturedAt < preRollCutoff
          ) {
            sharedAsrPreRollRef.current.shift();
          }

          const reverseSocket = reverseSocketRef.current;
          const isReverseActive = reverseTranslationActiveRef.current;
          const isReverseConnected = reverseSocket?.connected;
          const isReverseDeepgramReady = reverseSocket?.isDeepgramReady;

          if (isReverseActive && isReverseConnected && isReverseDeepgramReady && translationMode !== 'fast') {
            reverseSocket.emit("audio_chunk", {
              spokenLanguage: reverseSocket.reverseSpokenLanguage || spokenLanguage,
              spokenLanguageModel: reverseSocket.reverseSpokenLanguageModel || spokenLanguageModel,
              translatedLanguage: reverseSocket.reverseTranslatedLanguage || translatedLanguage,
              translationEnabled: true,
              buffer,
              audioSourceMeta,
            });
          }

          const pooledSockets = Array.from(sharedAsrSocketPoolRef.current.values());
          pooledSockets.forEach((sharedSocket) => {
            const isSharedConnected = sharedSocket?.connected;
            const isSharedDeepgramReady = sharedSocket?.isDeepgramReady;
            const isSharedReadyEventReceived = sharedSocket?.isSharedDeepgramReady;

            if (isSharedConnected && isSharedDeepgramReady && isSharedReadyEventReceived) {
              sharedSocket.emit("audio_chunk", {
                spokenLanguage: sharedSocket.sharedSpokenLanguage || spokenLanguage,
                spokenLanguageModel: sharedSocket.sharedSpokenLanguageModel || spokenLanguageModel,
                translatedLanguage: sharedSocket.sharedDefaultTargetLanguage || translatedLanguage,
                translationEnabled: true,
                buffer,
                audioSourceMeta,
              });
              sharedSocket.sharedLastAudioSentSequence = preRollSequence;
            }
          });
        };

        // Fast mode: minimal startup delay — the OpenAI session has its own pendingAudio
        // buffer for pre-ready chunks, so we don't need to hold audio on the client side.
        // Classic mode keeps the original 2000ms warm-up for Deepgram connection stability.
        const initialAudioDelayMs = onlineBothFast ? 0 : (translationMode === 'fast' ? 100 : 2000);

        // Apply initial delay before sending the first audio chunk
        const applyInitialDelay = () => {
          if (initialAudioDelayMs === 0) {
            initialDelayApplied = true;
            sendAudioChunk();
            return;
          }
          setTimeout(() => {
            initialDelayApplied = true;
            if (audioBuffer.length > 0) {
              sendAudioChunk();
            }
          }, initialAudioDelayMs);
        };

        const chunkSender = setInterval(() => {
          if (!initialDelayApplied) {
            // Don't send chunks during the initial delay
            return;
          }
          sendAudioChunk();
        }, CHUNK_INTERVAL);

        // Apply the initial delay
        applyInitialDelay();

        const onlineIncomingSystemOnly =
          translationMode === 'fast'
          && onlineIncomingOnly
          && systemAudioSource != null;
        const onlineOutgoingMicOnlyCapture =
          onlineOutgoingMicOnly && micAudioSource != null;
        const useDualPcmCapture =
          translationMode === 'fast' &&
          micAudioSource != null &&
          systemAudioSource != null &&
          (
            s2sOffCapture ||
            speechToSpeechMode === 'online-both' ||
            (inPersonCapture && otherAudioCaptureMode === 'both')
          );
        const useSingleSourcePcmCapture =
          translationMode === 'fast' &&
          !useDualPcmCapture &&
          (
            onlineIncomingSystemOnly ||
            onlineOutgoingMicOnlyCapture ||
            ((micAudioSource != null && systemAudioSource == null) ||
              (systemAudioSource != null && micAudioSource == null))
          );
        useDualPcmCaptureRef.current = useDualPcmCapture;
        useSingleSourcePcmCaptureRef.current = useSingleSourcePcmCapture;
        fixedPcmSourceRef.current = useSingleSourcePcmCapture
          ? (onlineIncomingSystemOnly || !micAudioSource ? 'system' : 'mic')
          : null;

        const onlineBothDownsamplers = onlineBothFast
          ? {
            mic: new StreamingPcmDownsampler(audioContext.sampleRate),
            system: new StreamingPcmDownsampler(audioContext.sampleRate),
            mixed: new StreamingPcmDownsampler(audioContext.sampleRate),
          }
          : null;
        const pushPcmFromProcessEvent = (
          processEvent: AudioProcessingEvent,
          source: 'mic' | 'system' | 'mixed' = 'mixed'
        ) => {
          const inputData = processEvent.inputBuffer.getChannelData(0);
          const downsampled = onlineBothDownsamplers
            ? onlineBothDownsamplers[source].push(inputData)
            : downsampleTo16kHz(inputData, audioContext.sampleRate);
          return convertFloat32ToInt16(downsampled);
        };

        if (useDualPcmCapture) {
          const dualProcessorBufferSize = onlineBothFast ? 2048 : 4096;
          const micProcessor = audioContext.createScriptProcessor(dualProcessorBufferSize, 1, 1);
          streamingMicSourceRef.current = micAudioSource;
          streamingMicProcessorRef.current = micProcessor;
          micProcessor.onaudioprocess = (processEvent) => {
            if (!isRecordingRef.current) return;
            if (shouldSuppressS2sOffMicBleedCapture()) return;
            micPcmChunkQueueRef.current.push(prepareDualPcmCapture(
              pushPcmFromProcessEvent(processEvent, 'mic'), 'mic'
            ));
          };
          micAudioSource!.connect(micProcessor);
          micProcessor.connect(audioContext.destination);

          const systemProcessor = audioContext.createScriptProcessor(dualProcessorBufferSize, 1, 1);
          streamingSystemSourceRef.current = systemAudioSource;
          streamingSystemProcessorRef.current = systemProcessor;
          systemProcessor.onaudioprocess = (processEvent) => {
            if (!isRecordingRef.current) return;
            systemPcmChunkQueueRef.current.push(prepareDualPcmCapture(
              pushPcmFromProcessEvent(processEvent, 'system'), 'system'
            ));
          };
          systemAudioSource!.connect(systemProcessor);
          systemProcessor.connect(audioContext.destination);
        } else if (useSingleSourcePcmCapture) {
          const singleSource = micAudioSource || systemAudioSource;
          const singleProcessor = audioContext.createScriptProcessor(4096, 1, 1);
          if (micAudioSource) {
            streamingMicSourceRef.current = micAudioSource;
            streamingMicProcessorRef.current = singleProcessor;
          } else {
            streamingSystemSourceRef.current = systemAudioSource;
            streamingSystemProcessorRef.current = singleProcessor;
          }
          singleProcessor.onaudioprocess = (processEvent) => {
            if (!isRecordingRef.current) return;
            pcmChunkQueueRef.current.push(pushPcmFromProcessEvent(
              processEvent,
              fixedPcmSourceRef.current || 'mixed'
            ));
          };
          singleSource!.connect(singleProcessor);
          singleProcessor.connect(audioContext.destination);
        } else {
          const streamingSource = audioContext.createMediaStreamSource(stream);
          const streamingProcessor = audioContext.createScriptProcessor(4096, 1, 1);
          streamingSourceRef.current = streamingSource;
          streamingProcessorRef.current = streamingProcessor;

          streamingProcessor.onaudioprocess = (processEvent) => {
            if (!isRecordingRef.current) return;
            pcmChunkQueueRef.current.push(pushPcmFromProcessEvent(processEvent));
          };

          streamingSource.connect(streamingProcessor);
          streamingProcessor.connect(audioContext.destination);
        }

        mediaRecorder.onstop = () => {
          clearInterval(chunkSender);
          sendAudioChunk();
          pcmChunkQueueRef.current = [];
          micPcmChunkQueueRef.current = [];
          systemPcmChunkQueueRef.current = [];
          savedRecordingGraph.disconnect();
          stopMediaStreamTracks(savedRecordingGraph.stream);
          stopMediaStreamTracks(stream);
          releaseLiveAudioCaptureResources(mediaRecorder.stream);
          if (mediaRecorderRef.current === mediaRecorder) {
            mediaRecorderRef.current = null;
          }
        };

        // Sync config before starting so fast-mode pipeline is active on the server.
        socketRef.current?.emit(
          'update_environment',
          getRuntimeSessionEnvironment(environmentRef.current),
        );
        emitFullConfigToServer();

        // Start recording with new meeting
        socketRef.current?.emit("start_recording", {
          existingMeetingId: reservedMeetingIdRef.current || undefined,
          shareCode: shareCodeRef.current || undefined,
          spokenLanguagePlan: getSpokenLanguagePlan(),
          translationEnabled: enabled,
          translationMode,
          textToSpeechEnabled,
          textToSpeechSessionMode,
          speechToSpeechMode,
          otherAudioCaptureMode: getStoredOtherAudioCaptureMode(),
          delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
          discardMeeting: paidMeetingStorageAccessRef.current
            && localStorage.getItem('MeetingCopilot_meeting_storage_preference') === 'nothing',
          ...calendarStartRecordingPayload,
        });

        // Get the meeting ID after starting recording
        setTimeout(() => {
          getCurrentMeetingId();
        }, 1000);

        // Start recording timer
        if (!recordingTimerRef.current) {
          recordingTimerRef.current = setInterval(() => {
            setRecordingTime((prev) => prev + 1);
          }, 1000);
        }

        mediaRecorder.start(100);
      } catch {
        if (mediaRecorderRef.current?.state !== 'inactive') {
          try {
            mediaRecorderRef.current?.stop();
          } catch {
            // Continue with direct graph cleanup below.
          }
        }
        savedRecordingGraph.disconnect();
        stopMediaStreamTracks(savedRecordingGraph.stream);
        stopMediaStreamTracks(stream);
        releaseLiveAudioCaptureResources(mediaRecorderRef.current?.stream);
        mediaRecorderRef.current = null;
        //productionLogger.error("❌ Error starting recording:", err);
        //productionLogger.debug("Failed to start recording system audio.");
      }
    } catch (err) {
      releaseLiveAudioCaptureResources(mediaRecorderRef.current?.stream);
      mediaRecorderRef.current = null;
      productionLogger.error("Error starting recording:", err);
      //productionLogger.debug("Failed to start recording system audio.");
    }
  };


  // productionLogger.debug(meetingId, "meetingId")

  useEffect(() => {
    if (isRecording) startRecording();
  }, [isRecording]);

  // Watch Redux isRecording state and trigger local recording
  useEffect(() => {
    if (reduxIsRecording && !isRecording) {
      onStartRecording();
      // Reset Redux state after triggering
      dispatch(setReduxIsRecording(false));
    }
  }, [reduxIsRecording, isRecording, dispatch]);

  const pauseRecording = () => {
    if (
      mediaRecorderRef.current &&
      mediaRecorderRef.current.state === "recording"
    ) {
      pcmChunkQueueRef.current = [];
      micPcmChunkQueueRef.current = [];
      systemPcmChunkQueueRef.current = [];
      mediaRecorderRef.current.pause();
      setPaused(true);
      setIsRecording(false);

      // Clear only recording timer — translation is handled by useEffect
      if (recordingTimerRef.current) {
        clearInterval(recordingTimerRef.current);
        recordingTimerRef.current = null;
      }

      socketRef.current?.emit("pause_recording");
      const rev = reverseSocketRef.current;
      if (rev?.connected) {
        rev.emit("pause_recording");
      }
      Array.from(sharedAsrSocketPoolRef.current.values()).forEach((s) => {
        if (s?.connected) {
          s.emit("pause_recording");
        }
      });
    }
  };

  // Handler for start recording button - ensures stop button is disabled first
  const handleReverseTranslationToggle = (active: boolean) => {
    if (translationMode === 'fast') {
      productionLogger.debug('[FAST-S2S] Reverse translation is disabled in Fast mode');
      return;
    }
    productionLogger.debug('🎛️ handleReverseTranslationToggle called with:', active);
    setReverseTranslationActive(active);
    productionLogger.debug('🎛️ Current isRecording state:', isRecording);
    productionLogger.debug('🎛️ Current languages - spoken:', spokenLanguage, 'translated:', translatedLanguage);

    if (active) {
      if (isRecording) {
        // Recording is already active, create reverse socket immediately (true reverse direction)
        const reverseConfig = getReverseLanguageConfig();

        productionLogger.debug(`Starting reverse translation immediately: ${reverseConfig.reverseSpokenLanguage} -> ${reverseConfig.reverseTranslatedLanguage}`);
        createReverseSocketConnection(
          reverseConfig.reverseSpokenLanguage,
          reverseConfig.reverseTranslatedLanguage,
          reverseConfig.reverseSpokenLanguageModel
        );
      } else {
        // Recording not started yet, socket will be created when recording starts
        productionLogger.debug('Reverse translation enabled - will start when recording begins');
      }
    } else if (reverseSocketRef.current) {
      // Stop reverse translation
      productionLogger.debug('Stopping reverse translation');
      reverseSocketRef.current.disconnect();
      reverseSocketRef.current = null;
      setReverseTranscriptData([]);
      // Clear any buffered audio chunks
      reverseAudioBufferRef.current = [];
    }
  };

  // Keep reverseTranslationActiveRef in sync so inner closures (like sendAudioChunk)
  // always see the latest toggle state
  useEffect(() => {
    reverseTranslationActiveRef.current = reverseTranslationActive;
  }, [reverseTranslationActive]);

  useEffect(() => {
    if (translationMode !== 'fast' && textToSpeechEnabled) {
      disableTextToSpeech();
    }
  }, [translationMode]);

  // Create a second desktop socket that looks like the main one but with reversed languages,
  // so the server can associate it with the same userId/monthlyUsage and allow audio_chunk processing.
  const createReverseSocketConnection = (spokenLang: string, translatedLang: string, reverseSpokenLanguageModel: string) => {
    productionLogger.debug('🔄 CREATING REVERSE SOCKET CONNECTION');
    productionLogger.debug('Reverse spoken language:', spokenLang);
    productionLogger.debug('Reverse translated language:', translatedLang);

    if (!user?.id) {
      productionLogger.error('Cannot create reverse socket: No user ID available');
      return;
    }

    try {
      const socket: AppSocket = io(wsURL, {
        ...DESKTOP_SOCKET_IO_OPTIONS,
        auth: getDesktopSocketAuth(),
        query: {
          userId: user.id,
          device: "desktop",
          appVersion: appInfo?.version || "unknown",
          spokenLanguagePlan: getSpokenLanguagePlan(),
          spokenLanguage: spokenLang,
          spokenLanguageModel: reverseSpokenLanguageModel,
          translatedLanguage: translatedLang,
          translationEnabled: "true",
          bulletPointEnabled: "false",
          isReverseTranslation: 'true'
        },
      });

      productionLogger.debug('✅ Reverse socket created successfully');
      // Ensure we start in a "not ready" state until the server confirms Deepgram is connected
      socket.isDeepgramReady = false;
      socket.reverseSpokenLanguage = spokenLang;
      socket.reverseTranslatedLanguage = translatedLang;
      socket.reverseSpokenLanguageModel = reverseSpokenLanguageModel;

      // Add connection event handlers for debugging
      socket.on('connect', () => {
        productionLogger.debug('🔗 Reverse socket connected to server');

        // IMPORTANT: Only start the reverse recording AFTER the socket is connected.
        // If we emit before connect (common when toggling mid-session), Socket.IO may drop it,
        // and because we gate audio on isDeepgramReady, we can deadlock (no Deepgram -> no ready -> no audio).
        if (isRecording) {
          socket.emit('start_recording', {
            translatedLanguage: translatedLang, // reverse socket uses its own handshake spokenLanguage
            translationEnabled: true,
            delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current
          });
        }

        // Mirror current config to reverse socket on connect so Deepgram
        // endpointing/quality matches main socket for lower latency.
        try {
          socket.emit("update_config", withDelayAnalysisConfig({
            translatedLanguage: translatedLang,
            translationEnabled: true,
            bulletPointEnabled: false,
            translationQuality: translationQuality.toString(),
          }));
        } catch { /* config mirroring is best-effort; the socket may be closing */ }
      });

      socket.on('disconnect', () => {
        productionLogger.debug('❌ Reverse socket disconnected from server');
        socket.isDeepgramReady = false;
        reverseInitChunkSentRef.current = false;
        reverseInitChunkSendInProgressRef.current = false;
        reversePendingBuffersRef.current = [];
      });

      socket.on('error', (error: unknown) => {
        productionLogger.error('💥 Reverse socket error:', error);
      });

      // Set up reverse transcript data handling (delta-first).
      socket.on('reverse_transcription_result', (data: SharedAsrTranscriptPayload | undefined) => {
        recordTranslationLatency(data);
        recordSttLatency(data);
        const section = data?.section;
        if (!section || typeof section.sectionId !== 'number') return;
        const normalized = applyDesktopSpeakerLabelOverrides([section]);
        const normalizedSection = normalized[0] || (section as TranscriptSection);
        setReverseTranscriptData((prev) => {
          const index = prev.findIndex((item) => item.sectionId === normalizedSection.sectionId);
          if (index === -1) {
            return [...prev, normalizedSection];
          }
          const next = [...prev];
          next[index] = normalizedSection;
          return next;
        });
      });

      // Backward-compatible full snapshot handler (throttled on server).
      socket.on('transcript_data', (data: TranscriptSectionLike[] | null | undefined) => {
        setReverseTranscriptData(applyDesktopSpeakerLabelOverrides(data || []));
      });

      // Handle BOTH deepgram_ready events (generic and reverse-specific)
      socket.on('deepgram_ready', () => {
        socket.isDeepgramReady = true;
        // Flush any buffered audio chunks now that Deepgram is ready
        flushReverseAudioBuffer();
        flushReversePendingBuffers(socket);
      });

      socket.on('reverse_deepgram_ready', () => {
        socket.isDeepgramReady = true;
        flushReverseAudioBuffer();
        flushReversePendingBuffers(socket);
      });

      // Handle reverse Deepgram errors — mark not ready so audio stops flowing
      socket.on('reverse_deepgram_error', (data: { message?: string } | undefined) => {
        productionLogger.warn('🔄❌ Reverse Deepgram error:', data?.message);
        socket.isDeepgramReady = false;
      });

      // Store the reverse socket reference
      reverseSocketRef.current = socket;
      reverseInitChunkSentRef.current = false;

    } catch (error) {
      productionLogger.error('Failed to create reverse socket connection:', error);
    }
  };

  const createSharedAsrSocketConnection = (spokenLang: string, spokenLangModel: string, translatedLang: string, sessionShareCode: string) => {
    if (!user?.id || !sessionShareCode) return null;

    const socket: AppSocket = io(wsURL, {
      ...DESKTOP_SOCKET_IO_OPTIONS,
      auth: getDesktopSocketAuth(),
      query: {
        userId: user.id,
        device: "desktop",
        appVersion: appInfo?.version || "unknown",
        spokenLanguagePlan: getSpokenLanguagePlan(),
        spokenLanguage: spokenLang,
        spokenLanguageModel: spokenLangModel,
        translatedLanguage: translatedLang,
        translationEnabled: "true",
        bulletPointEnabled: "false",
        shareCode: sessionShareCode,
        isReverseTranslation: 'true',
        isSharedAsr: 'true',
      },
    });

    socket.isDeepgramReady = false;
    socket.isSharedDeepgramReady = false;
    socket.sharedSpokenLanguage = spokenLang;
    socket.sharedSpokenLanguageModel = spokenLangModel;
    socket.sharedDefaultTargetLanguage = translatedLang;
    socket.sharedInitChunkSent = false;
    socket.sharedInitChunkSendInProgress = false;
    socket.sharedLastAudioSentSequence = 0;

    socket.on('connect', () => {
      try {
        socket.emit("update_config", withDelayAnalysisConfig({
          translatedLanguage: socket.sharedDefaultTargetLanguage || translatedLang,
          translationEnabled: true,
          bulletPointEnabled: false,
          translationQuality: translationQuality.toString(),
        }));
      } catch { /* config sync is best-effort; the socket may be closing */ }

      // Configure the pooled route before starting it so the server has the
      // multilingual/same-language policy before the first transcript arrives.
      // Use isRecordingRef (not isRecording) to avoid a stale async closure.
      if (isRecordingRef.current) {
        socket.emit('start_recording', {
          translatedLanguage: socket.sharedDefaultTargetLanguage || translatedLang,
          translationEnabled: true,
          delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
          translationMode,
        });
      }
    });

    socket.on('disconnect', () => {
      socket.isDeepgramReady = false;
      socket.isSharedDeepgramReady = false;
      socket.sharedInitChunkSent = false;
      socket.sharedInitChunkSendInProgress = false;
    });

    socket.on('deepgram_ready', () => {
      socket.isDeepgramReady = true;
      productionLogger.debug('✅🌐 Shared ASR deepgram_ready — accepting audio immediately');
    });

    socket.on('reverse_deepgram_ready', () => {
      socket.isDeepgramReady = true;
      productionLogger.debug('✅🌐 Shared ASR reverse_deepgram_ready — accepting audio immediately');
    });

    socket.on('shared_deepgram_ready', () => {
      socket.isSharedDeepgramReady = true;
      socket.isDeepgramReady = true;
      const lastSentSequence = Number(socket.sharedLastAudioSentSequence || 0);
      for (const entry of sharedAsrPreRollRef.current) {
        if (entry.sequence <= lastSentSequence) continue;
        socket.emit("audio_chunk", {
          spokenLanguage: socket.sharedSpokenLanguage || spokenLanguage,
          spokenLanguageModel: socket.sharedSpokenLanguageModel || spokenLanguageModel,
          translatedLanguage: socket.sharedDefaultTargetLanguage || translatedLanguage,
          translationEnabled: true,
          buffer: entry.buffer,
          audioSourceMeta: entry.audioSourceMeta,
        });
        socket.sharedLastAudioSentSequence = entry.sequence;
      }
      productionLogger.debug('✅🌐 Shared ASR shared_deepgram_ready — sending audio chunks now');
    });

    socket.on('reverse_deepgram_error', () => {
      socket.isDeepgramReady = false;
      socket.isSharedDeepgramReady = false;
    });

    const onSharedAsrTranscript = (data: SharedAsrTranscriptPayload | undefined, shouldRecordTranslationLatency = false, shouldRecordSttLatency = true) => {
      if (shouldRecordTranslationLatency) {
        recordTranslationLatency(data);
      }
      if (shouldRecordSttLatency) {
        recordSttLatency(data);
      }
      const transcriptText = typeof data?.text === 'string' ? data.text.trim() : '';
      const sectionText = typeof data?.section?.captions?.[0]?.text === 'string'
        ? data.section.captions[0].text.trim()
        : '';
      if (transcriptText || sectionText) {
        bumpTranscriptionActivityTimer();
      }
    };

    socket.on('transcription_result', (data: SharedAsrTranscriptPayload) => onSharedAsrTranscript(data));
    socket.on('reverse_transcription_result', (data: SharedAsrTranscriptPayload) => onSharedAsrTranscript(data, true, true));
    socket.on('transcript_data', (data: unknown) => {
      if (Array.isArray(data) && data.length > 0) {
        bumpTranscriptionActivityTimer();
      }
    });

    return socket;
  };

  const ensureSharedAsrPipeline = (spokenLang: string, spokenLangModel: string, translatedLang: string, sessionShareCode: string) => {
    productionLogger.debug('🔧 [ensureSharedAsrPipeline] Called with:', { spokenLang, spokenLangModel, translatedLang, sessionShareCode });
    if (!spokenLang || !spokenLangModel || !translatedLang || !sessionShareCode) {
      productionLogger.debug('❌ [ensureSharedAsrPipeline] Missing param, returning early');
      return;
    }

    const requestedAsrKey = buildAsrKey(spokenLang, spokenLangModel);
    const mainAsrKey = buildAsrKey(spokenLanguage, spokenLanguageModel);
    const sameBaseSpoken =
      baseSpokenTagForSharedAsr(spokenLang) === baseSpokenTagForSharedAsr(spokenLanguage) &&
      Boolean(baseSpokenTagForSharedAsr(spokenLang));
    const matchesMainAsr = requestedAsrKey === mainAsrKey || sameBaseSpoken;
    productionLogger.debug('🔧 [ensureSharedAsrPipeline] requestedAsrKey:', requestedAsrKey, 'mainAsrKey:', mainAsrKey, 'match:', matchesMainAsr);

    const targetSet = sharedAsrTargetMapRef.current.get(requestedAsrKey) || new Set<string>();
    const alreadyHadTarget = targetSet.has(translatedLang);
    targetSet.add(translatedLang);
    sharedAsrTargetMapRef.current.set(requestedAsrKey, targetSet);

    // Main ASR already exists (exact key or same base spoken tag, e.g. en vs en-US); use host stream for web.
    if (matchesMainAsr) {
      productionLogger.debug('ℹ️ [ensureSharedAsrPipeline] Requested matches main ASR, no new socket needed');
      return;
    }

    let pooledSocket: AppSocket | null | undefined = sharedAsrSocketPoolRef.current.get(requestedAsrKey);
    if (!pooledSocket) {
      productionLogger.debug('🆕 [SHARED_ASR_SOCKET] Creating shared ASR socket:', {
        asrKey: requestedAsrKey,
        shareCode: sessionShareCode,
        targetLanguage: translatedLang,
      });
      pooledSocket = createSharedAsrSocketConnection(spokenLang, spokenLangModel, translatedLang, sessionShareCode);
      if (!pooledSocket) return;
      sharedAsrSocketPoolRef.current.set(requestedAsrKey, pooledSocket);
      return;
    }

    if (alreadyHadTarget && pooledSocket.connected) {
      return;
    }

    pooledSocket.sharedDefaultTargetLanguage = translatedLang;
    productionLogger.debug('♻️ [SHARED_ASR_SOCKET] Reusing shared ASR socket:', {
      asrKey: requestedAsrKey,
      shareCode: sessionShareCode,
      targetLanguage: translatedLang,
      alreadyHadTarget,
      connected: !!pooledSocket.connected,
    });
    if (pooledSocket.connected) {
      try {
        pooledSocket.emit("update_config", withDelayAnalysisConfig({
          translatedLanguage: translatedLang,
          translationEnabled: true,
          bulletPointEnabled: false,
          translationQuality: translationQuality.toString(),
        }));
      } catch { /* config sync is best-effort; the socket may be closing */ }
      if (isRecordingRef.current) {
        try {
          pooledSocket.emit('start_recording', {
            translatedLanguage: translatedLang,
            translationEnabled: true,
            delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
            translationMode,
          });
        } catch { /* start is best-effort; the socket may have disconnected */ }
      }
    }
  };

  const reconcileSharedAsrPipelines = (
    routes: Array<{
      spokenLanguage?: string;
      spokenLanguageModel?: string;
      translatedLanguages?: string[];
    }>,
    sessionShareCode: string
  ) => {
    if (!sessionShareCode) return;
    const desiredKeys = new Set<string>();

    for (const route of routes || []) {
      const routeSpoken = (route.spokenLanguage || '').trim();
      const routeModel = (route.spokenLanguageModel || '').trim();
      const targets = (route.translatedLanguages || []).map((target) => target.trim()).filter(Boolean);
      if (!routeSpoken || !routeModel || targets.length === 0) continue;

      const key = buildAsrKey(routeSpoken, routeModel);
      const sameBaseSpoken =
        baseSpokenTagForSharedAsr(routeSpoken) === baseSpokenTagForSharedAsr(spokenLanguage)
        && Boolean(baseSpokenTagForSharedAsr(routeSpoken));
      if (key !== buildAsrKey(spokenLanguage, spokenLanguageModel) && !sameBaseSpoken) {
        desiredKeys.add(key);
      }
      sharedAsrTargetMapRef.current.set(key, new Set(targets));
      for (const target of targets) {
        ensureSharedAsrPipeline(routeSpoken, routeModel, target, sessionShareCode);
      }
    }

    for (const [key, pooledSocket] of sharedAsrSocketPoolRef.current.entries()) {
      if (desiredKeys.has(key)) continue;
      try { pooledSocket.emit?.('stop_recording'); } catch { /* socket already closed */ }
      try { pooledSocket.disconnect?.(); } catch { /* socket already closed */ }
      sharedAsrSocketPoolRef.current.delete(key);
      sharedAsrTargetMapRef.current.delete(key);
    }
  };

  // Keep reverse socket language routing aligned with current settings.
  // If reverse ASR source language/model changes, reconnect reverse socket.
  // If only reverse translation target changes, update config in-place.
  useEffect(() => {
    if (!reverseTranslationActive || !isRecording) return;
    const reverseSocket = reverseSocketRef.current;
    if (!reverseSocket) return;

    const reverseConfig = getReverseLanguageConfig();
    const requiresReconnect =
      reverseSocket.reverseSpokenLanguage !== reverseConfig.reverseSpokenLanguage ||
      reverseSocket.reverseSpokenLanguageModel !== reverseConfig.reverseSpokenLanguageModel;

    if (requiresReconnect) {
      try {
        reverseSocket.disconnect();
      } catch { /* socket already disconnected */ }
      reverseSocketRef.current = null;
      setReverseTranscriptData([]);
      createReverseSocketConnection(
        reverseConfig.reverseSpokenLanguage,
        reverseConfig.reverseTranslatedLanguage,
        reverseConfig.reverseSpokenLanguageModel
      );
      return;
    }

    if (reverseSocket.connected && reverseSocket.reverseTranslatedLanguage !== reverseConfig.reverseTranslatedLanguage) {
      reverseSocket.reverseTranslatedLanguage = reverseConfig.reverseTranslatedLanguage;
      reverseSocket.emit("update_config", withDelayAnalysisConfig({
        translatedLanguage: reverseConfig.reverseTranslatedLanguage,
        translationEnabled: true,
        bulletPointEnabled: false,
        translationQuality: translationQuality.toString(),
      }));
    }
  }, [reverseTranslationActive, isRecording, spokenLanguage, spokenLanguageModel, translatedLanguage, translationQuality]);

  const onStartRecording = async () => {
    try {
      const permissionStatus = await getDesktopPermissionStatus();
      if (hasMissingLiveAudioPermission(permissionStatus)) {
        onPermissionSetupRequired?.();
        return;
      }
    } catch {
      onPermissionSetupRequired?.();
      return;
    }

    // Preserve the pre-created link if Start is clicked while its reservation
    // request is still completing.
    if (shareInvitationRequestRef.current) {
      await shareInvitationRequestRef.current.catch(() => undefined);
    }
    // Disable stop button BEFORE starting recording to prevent race condition
    setStopButtonDisabled(true);

    // Set timeout to re-enable stop button after 5 seconds
    setTimeout(() => {
      setStopButtonDisabled(false);
    }, 5000);

    // Now start recording
    setIsRecording(true);

    // If reverse translation is enabled but socket not created yet, create it now
    if (reverseTranslationActive && !reverseSocketRef.current) {
      // True reverse direction
      const reverseConfig = getReverseLanguageConfig();
      productionLogger.debug(`Creating reverse socket on recording start: ${reverseConfig.reverseSpokenLanguage} -> ${reverseConfig.reverseTranslatedLanguage}`);
      createReverseSocketConnection(
        reverseConfig.reverseSpokenLanguage,
        reverseConfig.reverseTranslatedLanguage,
        reverseConfig.reverseSpokenLanguageModel
      );
    }

    // If reverse translation socket already exists, start recording on it
    if (reverseTranslationActive && reverseSocketRef.current) {
      reverseSocketRef.current.emit('start_recording', {
        translatedLanguage: reverseSocketRef.current.reverseTranslatedLanguage || translatedLanguage,
        translationEnabled: true,
        delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
      });
    }

    // Start pooled shared ASR sockets as well (if any were requested before recording start)
    const pooledSockets = Array.from(sharedAsrSocketPoolRef.current.values());
    pooledSockets.forEach((sharedSocket) => {
      if (!sharedSocket?.connected) return;
      try {
        sharedSocket.emit('start_recording', {
          translatedLanguage: sharedSocket.sharedDefaultTargetLanguage || translatedLanguage,
          translationEnabled: true,
          delayAnalysisEnabled: delayAnalysisConfigEnabledRef.current,
          translationMode,
        });
      } catch { /* start is best-effort; the socket may have disconnected */ }
    });
  };

  const autoStartHandledRef = useRef(false);
  useEffect(() => {
    if (!autoStart || autoStartHandledRef.current || !authReady || isRecording) return;
    autoStartHandledRef.current = true;
    onStartRecording();
  }, [autoStart, authReady, isRecording]);

  const stopRecording = async () => {
    // Immediately stop recording state to prevent auto-resume on reconnection
    setIsRecording(false);
    try {
      getElectronIpc()?.send('recording-state', { recording: false });
    } catch { /* not running in Electron */ }

    // Reset stop button disabled state when stopping
    setStopButtonDisabled(false);

    // Stop reverse recording if active
    if (reverseTranslationActive && reverseSocketRef.current) {
      reverseSocketRef.current.emit('stop_recording');
    }

    cleanupSharedAsrSockets();

    // Use the universal save meeting function for all stop recording scenarios
    await saveMeetingAndReload();
  };

  // Function to handle modal response
  const handleModalResponse = (response: boolean) => {
    setIsModalOpen(false);

    // Always clear inactivity timers for both Yes and No paths so
    // stopRecordingDueToInactivity does not fire after the user already responded.
    if (countdownIntervalRef.current) {
      clearInterval(countdownIntervalRef.current);
      countdownIntervalRef.current = null;
    }

    if (response) {
      stopRecording();
    } else {
      resumePausedRecordingSession();
      setCountdown(60);
      setLastTranscriptionTime(new Date());
      socketRef.current?.emit('confirm_still_recording');
    }
  };

  // Update the inactivity check to show the modal only if no transcription is received for 20 seconds
  useEffect(() => {
    const checkInactivity = setInterval(() => {
      const timeSinceLast = Date.now() - lastTranscriptionTime!.getTime();
      //productionLogger.debug("Inactivity (sec):", timeSinceLast / 1000, "Recording:", isRecording);

      if (timeSinceLast > INACTIVITY_WARNING_TIME && isRecording && !paused) {
        if (!isModalOpen) {
          pauseRecording();
          setIsModalOpen(true);
          setCountdown(60);

          // Clear any existing countdown interval
          if (countdownIntervalRef.current) {
            clearInterval(countdownIntervalRef.current);
          }

          const countdownInterval = setInterval(() => {
            setCountdown((prev) => {
              if (prev <= 1) {
                clearInterval(countdownInterval);
                countdownIntervalRef.current = null;
                // Countdown matches the visible 60s timer; do not use a separate short timeout
                // (that used to finalize while the modal still showed ~40s left).
                stopRecordingDueToInactivity();
                return 0;
              }
              return prev - 1;
            });
          }, 1000);
          countdownIntervalRef.current = countdownInterval;
        }
      } else if (isModalOpen && timeSinceLast <= INACTIVITY_WARNING_TIME) {
        if (countdownIntervalRef.current) {
          clearInterval(countdownIntervalRef.current);
          countdownIntervalRef.current = null;
        }
        setIsModalOpen(false);
        setCountdown(60);
      }
    }, 5000);

    return () => clearInterval(checkInactivity);
  }, [lastTranscriptionTime, isModalOpen, isRecording, paused]);

  useEffect(() => {
    if (isRecording) {
      setCountdown(60);
      setLastTranscriptionTime(new Date());
    }
  }, [isRecording]);

  // Add useEffect to handle system audio stream error
  useEffect(() => {
    const errorValue: unknown = error;
    if (errorValue === "Failed to get system audio stream.") {
      // Clear any existing reload timer
      if (reloadTimerRef.current) {
        clearInterval(reloadTimerRef.current);
      }

      // Set up new reload timer
      reloadTimerRef.current = setInterval(() => {
        window.location.reload();
      }, 5000);

      // Cleanup function
      return () => {
        if (reloadTimerRef.current) {
          clearInterval(reloadTimerRef.current);
          reloadTimerRef.current = null;
        }
      };
    }
  }, [error]);

  // Handler for mic disconnect modal
  const handleMicDisconnectModal = (shouldStop: boolean) => {
    setShowMicDisconnectModal(false);
    if (shouldStop) {
      stopRecording();
    }
    // If shouldStop is false, do nothing (system audio keeps going)
  };

  // Handler for error modal Okay button
  const handleErrorModalOkay = async () => {
    const isAuthFailure = !!error?.message && /session expired|not authorized|unauthorized|forbidden/i.test(error.message);

    if (isAuthFailure) {
      // Expired/invalid JWT should take user to sign-in instead of reloading recorder state.
      localStorage.removeItem('accessToken');
      setError(null);
      window.location.href = "/home/";
      return;
    }

    setError(null);

    // Only save meeting if we have actual recording data worth saving
    const hasActiveRecording = isRecording && meetingId && (
      transcriptData?.length > 0 ||     // Any speech transcribed?
      audioChunksRef.current.length > 0 || // Any audio recorded?
      messages.length > 0               // Any AI chat happened?
    );

    if (hasActiveRecording) {
      productionLogger.debug("Had active recording - saving meeting");
      await stopRecording(); // Save the meeting
    } else {
      productionLogger.debug("No active recording - just reloading");
      try {
        getElectronIpc()?.send('recording-state', { recording: false });
      } catch { /* not running in Electron */ }
      window.location.reload(); // Just reload without saving
    }
  };

  // Handler for Deepgram error Okay button click (manual or automatic)
  const handleDeepgramErrorOkayClick = async () => {
    // productionLogger.debug('🔴 CLIENT: Deepgram error Okay clicked - calling saveMeetingAndReload');

    // Clear countdown timer
    if (deepgramErrorTimerRef.current) {
      clearInterval(deepgramErrorTimerRef.current ?? undefined);
      deepgramErrorTimerRef.current = null;
    }

    // Hide modal
    setIsDeepgramError(false);
    setDeepgramErrorData(null);

    // Save meeting and reload
    await saveMeetingAndReload();
  };

  // Removed automatic coupling between translation and bullet points
  // Bullet points can now be toggled independently

  // Ensure backend is notified only when bullet point state actually changes
  const lastBulletPointState = useRef<boolean | null>(null);
  const hasNotifiedOnConnection = useRef<boolean>(false);

  // Notify server of initial bullet point state when socket connects
  useEffect(() => {
    if (socketRef.current && socketRef.current.connected && !hasNotifiedOnConnection.current) {
      socketRef.current.emit('toggle_bullet_points', bulletPointEnabled);
      lastBulletPointState.current = bulletPointEnabled;
      hasNotifiedOnConnection.current = true;
    }
  }, [socketRef.current?.connected]);

  // Notify server only when bullet point state actually changes
  useEffect(() => {
    if (socketRef.current && socketRef.current.connected) {
      // Only emit if the state has actually changed
      if (lastBulletPointState.current !== bulletPointEnabled) {
        socketRef.current.emit('toggle_bullet_points', bulletPointEnabled);
        lastBulletPointState.current = bulletPointEnabled;
      }
    }
  }, [bulletPointEnabled]);

  // productionLogger.debug("🔴 Error:", error);

  useEffect(() => {
    const safeQuality = getSafeTranslationQuality(translationQuality);
    if (safeQuality !== translationQuality) {
      setTranslationQuality(safeQuality);
      writeStoredTranslationQuality(enabled, safeQuality);
    }
  }, [translationQuality, textToSpeechEnabled, enabled]);

  useEffect(() => {
    const syncAutoTranslationQualityPreference = () => {
      const savedPreference = localStorage.getItem(TRANSLATION_QUALITY_AUTO_ADJUST_STORAGE_KEY);
      setAutoTranslationQualityAdjustEnabled(savedPreference !== 'false');
    };

    window.addEventListener('storage', syncAutoTranslationQualityPreference);
    window.addEventListener('translationQualityAutoAdjustChanged', syncAutoTranslationQualityPreference as EventListener);
    return () => {
      window.removeEventListener('storage', syncAutoTranslationQualityPreference);
      window.removeEventListener('translationQualityAutoAdjustChanged', syncAutoTranslationQualityPreference as EventListener);
    };
  }, []);

  // Sync the Show / No-show preference from Settings → Real-time memory.
  useEffect(() => {
    const sync = (e?: Event) => {
      if (!isRealTimeMemoryEligible(getSpokenLanguagePlan(), translationModeRef.current)) {
        setShowMeetingFlow(false);
        setMeetingFlow("");
        return;
      }
      const detail = (e as CustomEvent<boolean> | undefined)?.detail;
      if (typeof detail === 'boolean') { setShowMeetingFlow(detail); return; }
      const v = localStorage.getItem(REAL_TIME_MEMORY_STORAGE_KEY);
      if (v === 'true' || v === 'false') setShowMeetingFlow(v === 'true');
    };
    window.addEventListener('realTimeMemoryChanged', sync as EventListener);
    window.addEventListener('translationModeChanged', sync as EventListener);
    window.addEventListener('spokenLanguagePlanChanged', sync as EventListener);
    window.addEventListener('storage', sync);
    return () => {
      window.removeEventListener('realTimeMemoryChanged', sync as EventListener);
      window.removeEventListener('translationModeChanged', sync as EventListener);
      window.removeEventListener('spokenLanguagePlanChanged', sync as EventListener);
      window.removeEventListener('storage', sync);
    };
  }, []);

  useEffect(() => {
    if (!isDelayAnalysisAdmin) {
      setDelayAnalysisControlsVisible(false);
      setTranslationDelayAnalysisEnabled(false);
      setSttDelayAnalysisEnabled(false);
      localStorage.removeItem(LEGACY_DELAY_ANALYSIS_STORAGE_KEY);
      localStorage.removeItem(TRANSLATION_DELAY_ANALYSIS_STORAGE_KEY);
      localStorage.removeItem(STT_DELAY_ANALYSIS_STORAGE_KEY);
      return;
    }

    const syncDelayAnalysisControlsVisibility = (event?: Event) => {
      const detail = (event as CustomEvent<boolean> | undefined)?.detail;
      if (typeof detail === 'boolean') {
        setDelayAnalysisControlsVisible(detail);
        return;
      }
      setDelayAnalysisControlsVisible(getStoredDelayAnalysisControlsVisible());
    };

    syncDelayAnalysisControlsVisibility();
    window.addEventListener('storage', syncDelayAnalysisControlsVisibility);
    window.addEventListener(DELAY_ANALYSIS_CONTROLS_VISIBILITY_CHANGED_EVENT, syncDelayAnalysisControlsVisibility as EventListener);

    const storedTranslationDelayAnalysis = localStorage.getItem(TRANSLATION_DELAY_ANALYSIS_STORAGE_KEY);
    const legacyDelayAnalysis = localStorage.getItem(LEGACY_DELAY_ANALYSIS_STORAGE_KEY);
    setTranslationDelayAnalysisEnabled(
      storedTranslationDelayAnalysis !== null
        ? storedTranslationDelayAnalysis === 'true'
        : legacyDelayAnalysis === 'true'
    );
    setSttDelayAnalysisEnabled(localStorage.getItem(STT_DELAY_ANALYSIS_STORAGE_KEY) === 'true');

    return () => {
      window.removeEventListener('storage', syncDelayAnalysisControlsVisibility);
      window.removeEventListener(DELAY_ANALYSIS_CONTROLS_VISIBILITY_CHANGED_EVENT, syncDelayAnalysisControlsVisibility as EventListener);
    };
  }, [isDelayAnalysisAdmin]);

  useEffect(() => {
    if (!isDelayAnalysisAdmin) return;
    localStorage.setItem(TRANSLATION_DELAY_ANALYSIS_STORAGE_KEY, translationDelayAnalysisEnabled ? 'true' : 'false');
  }, [translationDelayAnalysisEnabled, isDelayAnalysisAdmin]);

  useEffect(() => {
    if (!isDelayAnalysisAdmin) return;
    localStorage.setItem(STT_DELAY_ANALYSIS_STORAGE_KEY, sttDelayAnalysisEnabled ? 'true' : 'false');
  }, [sttDelayAnalysisEnabled, isDelayAnalysisAdmin]);

  useEffect(() => {
    delayAnalysisConfigEnabledRef.current = delayAnalysisConfigEnabled;
  }, [delayAnalysisConfigEnabled]);

  useEffect(() => {
    translationCountRef.current = 0;
    setTranslationLatencies([]);
  }, [translationDelayAnalysisEnabled]);

  useEffect(() => {
    sttCountRef.current = 0;
    setSttLatencies([]);
  }, [sttDelayAnalysisEnabled]);

  useEffect(() => {
    if (socketRef.current) {
      const safeQuality = getSafeTranslationQuality(translationQuality);
      // Small delay to avoid duplicate emissions during rapid state changes or initial connection
      const timeoutId = setTimeout(() => {
        socketRef.current?.emit("update_config", withDelayAnalysisConfig({
          translatedLanguage,
          bothModeUplinkTranslatedLanguage,
          translationEnabled: enabled,
          bulletPointEnabled, // Add bullet points to the config update
          translationQuality: safeQuality.toString(), // Use provider-safe value
          textToSpeechEnabled,
          textToSpeechSessionMode,
          speechToSpeechMode,
          translationMode,
        }));

        // Also mirror to reverse socket if present so reverse path
        // uses the same translationQuality and language.
        if (reverseSocketRef.current && reverseSocketRef.current.connected) {
          const reverseConfig = getReverseLanguageConfig();
          reverseSocketRef.current.emit("update_config", withDelayAnalysisConfig({
            translatedLanguage: reverseConfig.reverseTranslatedLanguage,
            translationEnabled: true,
            bulletPointEnabled: false,
            translationQuality: safeQuality.toString(),
          }));
        }

        // Mirror to shared ASR sockets so they stay in sync with host config.
        const pooled = Array.from(sharedAsrSocketPoolRef.current.values());
        pooled.forEach((sharedSock) => {
          if (sharedSock?.connected) {
            try {
              sharedSock.emit("update_config", withDelayAnalysisConfig({
                translatedLanguage: sharedSock.sharedDefaultTargetLanguage || translatedLanguage,
                translationEnabled: true,
                bulletPointEnabled: false,
                translationQuality: safeQuality.toString(),
              }));
            } catch { /* config mirroring is best-effort; the socket may be closing */ }
          }
        });
      }, 100);

      return () => clearTimeout(timeoutId);
    }
  }, [socketRef.current, enabled, spokenLanguage, translatedLanguage, bothModeUplinkTranslatedLanguage, bulletPointEnabled, translationQuality, voicePlaybackWaitMs, textToSpeechEnabled, textToSpeechSessionMode, speechToSpeechMode, translationMode, delayAnalysisConfigEnabled, translationSplitTokenGuardEnabled])

  // Listen for translation quality changes from Settings
  useEffect(() => {
    const handleTranslationQualityChange = (event: CustomEvent) => {
      const safeQuality = getSafeTranslationQuality(Number(event.detail));
      productionLogger.debug('Translation quality changed to:', safeQuality);
      // Send updated config to server
      if (socketRef.current && socketRef.current.connected) {
        socketRef.current.emit("update_config", withDelayAnalysisConfig({
          translatedLanguage,
          translationEnabled: enabled,
          bulletPointEnabled,
          translationQuality: safeQuality,
        }));
      }
      setTranslationQuality(safeQuality);
      writeStoredTranslationQuality(enabled, safeQuality);
    };

    window.addEventListener('translationQualityChanged', handleTranslationQualityChange as EventListener);

    return () => {
      window.removeEventListener('translationQualityChanged', handleTranslationQualityChange as EventListener);
    };
  }, [enabled, translatedLanguage, bulletPointEnabled]);

  useEffect(() => {
    const s = socketRef.current;
    if (!s) return;
    const handleDeepgramError = async (data: { error?: unknown } | undefined) => {
      if (data?.error) {
        // If we're currently recording, trigger normal stop recording to save audio
        if (isRecording && transcriptData?.length > 0) {
          // productionLogger.debug('Triggering normal stop recording due to Deepgram error...');
          await stopRecording();
          return; // stopRecording() will handle the cleanup and reload
        }

        setIsDeepgramError(true)
        // Auto-reload for backward compatibility (v0.1.1 behavior)
        setTimeout(() => {
          window.location.reload()
        }, 3000)
      }
    };

    const handleDeepgramConnectionFailed = async (data: { message?: string; showReloadButton?: boolean; meetingId?: string } | null) => {
      // productionLogger.debug('🔴 CLIENT: Deepgram connection failed event received:', data);
      // productionLogger.debug('🔴 CLIENT: Current state - isRecording:', isRecording, 'transcriptData length:', transcriptData?.length);

      // Store current meetingId in localStorage as fallback for audio upload
      if (meetingId) {
        localStorage.setItem('MeetingCopilot_fallback_meeting_id', meetingId);
        // productionLogger.debug('🔴 CLIENT: Stored fallback meetingId:', meetingId);
      }

      // Always show the disconnection modal for consistent user experience
      // productionLogger.debug('🔴 CLIENT: Showing Deepgram disconnection modal');
      setDeepgramErrorData(data); // Store server message and button info
      setIsDeepgramError(true);

      // Start 60-second countdown that will auto-click "Okay" button
      setDeepgramErrorCountdown(60);
      deepgramErrorTimerRef.current = setInterval(() => {
        setDeepgramErrorCountdown(prev => {
          if (prev <= 1) {
            clearInterval(deepgramErrorTimerRef.current ?? undefined);
            deepgramErrorTimerRef.current = null;

            // Auto-click the "Okay" button after countdown
            // productionLogger.debug('🔴 CLIENT: Auto-clicking Okay button after 60 seconds');
            handleDeepgramErrorOkayClick();
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    };


    s.on("deepgram_error", handleDeepgramError);
    s.on("deepgram_connection_failed", handleDeepgramConnectionFailed);

    return () => {
      s.off("deepgram_error", handleDeepgramError);
      s.off("deepgram_connection_failed", handleDeepgramConnectionFailed);
    };
  }, [socketRef?.current]);

  // Sync notepad content to server during recording for backup purposes
  useEffect(() => {
    if (socketRef.current && isRecording && notepad !== undefined) {
      // Debounce the sync to avoid too many server calls
      const timeoutId = setTimeout(() => {
        socketRef.current?.emit('sync_notepad', { notepad });
        // productionLogger.debug('📝 CLIENT: Synced notepad to server - length:', notepad.length);
      }, 1000); // Wait 1 second after last change

      return () => clearTimeout(timeoutId);
    }
  }, [notepad, isRecording, socketRef.current]);

  // AI prompt generation effect - DISABLED
  // useEffect(() => {
  //   if (!isRecording || !enabled || transcriptData.length === 0) {
  //     setAiPrompt3(undefined);
  //     setAiPrompt4(undefined);
  //     return;
  //   }

  //   const generatePrompts = async () => {
  //     const now = Date.now();
  //     // Only generate if 10 seconds have passed since last generation
  //     if (now - lastPromptGenerationTime < 10000) {
  //       return;
  //     }

  //     try {
  //       // Get the last 10 transcript entries (most recent conversation)
  //       const recentTranscript = transcriptData.slice(-10);

  //       // Format transcript for the API
  //       const transcriptText = recentTranscript.map(entry => {
  //         const text = entry.captions.map(c => c.text).join(' ');
  //         const translation = entry.translatedText ? ` (${entry.translatedText})` : '';
  //         return `${entry.speakerName}: ${text}${translation}`;
  //       }).join('\n');

  //       if (!transcriptText.trim()) {
  //         return;
  //       }

  //       // Call API to generate questions
  //       const response = await axios.post(`${baseURL}/chat/generate-prompts`, {
  //         transcript: transcriptText,
  //         userId: user?.id,
  //         translationLanguage: translatedLanguage
  //       });

  //       if (response.data?.data?.prompts) {
  //         const prompts = response.data.data.prompts;
  //         setAiPrompt3(prompts[0] || undefined);
  //         setAiPrompt4(prompts[1] || undefined);
  //         setLastPromptGenerationTime(now);
  //       }
  //     } catch (error) {
  //       productionLogger.error('Error generating AI prompts:', error);
  //     }
  //   };

  //   // Generate prompts immediately when recording starts with transcript data
  //   generatePrompts();

  //   // Set up interval to generate prompts every 5 seconds
  //   const interval = setInterval(generatePrompts, 5000);

  //   return () => clearInterval(interval);
  // }, [isRecording, enabled, transcriptData, lastPromptGenerationTime, user?.id]);


  // Cleanup timers on unmount
  useEffect(() => {
    return () => {
      stopLocalAudioSourceSampler();
      syncTranslatedSpeechPlaybackCaptureGates(false);
      systemAudioGainRef.current = null;
      micAudioGainRef.current = null;
      stopMeetingAudioStreams();
      disconnectStreamingPcmNodes();
      if (countdownIntervalRef.current) {
        clearInterval(countdownIntervalRef.current);
        countdownIntervalRef.current = null;
      }
      if (deepgramErrorTimerRef.current) {
        clearInterval(deepgramErrorTimerRef.current ?? undefined);
        deepgramErrorTimerRef.current = null;
      }
      if (dialogCountdownTimerRef.current) {
        clearInterval(dialogCountdownTimerRef.current ?? undefined);
        dialogCountdownTimerRef.current = null;
      }
      if (transcriptionDialogTimerRef.current) {
        clearInterval(transcriptionDialogTimerRef.current ?? undefined);
        transcriptionDialogTimerRef.current = null;
      }
      if (loadingTimeoutRef) {
        clearTimeout(loadingTimeoutRef);
        setLoadingTimeoutRef(null);
      }
      if (transcriptionDialogTimeoutRef.current) {
        clearTimeout(transcriptionDialogTimeoutRef.current);
        transcriptionDialogTimeoutRef.current = null;
      }
      if (countdownTimer) {
        clearInterval(countdownTimer);
        setCountdownTimer(null);
      }
    };
  }, [countdownTimer]);
  useEffect(() => {
    return () => {
      const recorder = mediaRecorderRef.current;
      if (recorder && recorder.state !== 'inactive') {
        try {
          recorder.stop();
        } catch {
          // Direct resource cleanup below still releases every input track.
        }
      }
      releaseLiveAudioCaptureResources(recorder?.stream);
      mediaRecorderRef.current = null;
      if (deepgramErrorTimerRef.current) {
        clearInterval(deepgramErrorTimerRef.current ?? undefined);
        deepgramErrorTimerRef.current = null;
      }
    };
  }, []);


  if (loading) {
    return <Loading label={t('loading.saving')} />;
  }

  if (showSuccessLoading) {
    const meetingUrl = successLoadingMode === 'meeting' && meetingId
      ? `${WEB_URL}/meeting/${meetingId}`
      : undefined;
    const countdownText = t('meetingDetails.countdown').replace('{seconds}', reloadCountdown.toString());
    return (
      <Loading
        label={successLoadingMode === 'delay-analysis'
          ? t("desktopUi.recording.delayAnalysisEnded")
          : successLoadingMode === 'discarded'
            ? t("desktopUi.recording.discarded")
            : t('meetingDetails.title')}
        isSuccess={true}
        meetingUrl={meetingUrl}
        countdownText={countdownText}
      />
    );
  }

  return (
    <div className="live-session-ui relative flex h-full flex-col px-4 pb-1">
      {/* Error Modal */}
      {error !== null && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-xl font-bold text-white mb-4">
              {error.type !== 'generic' && t(`error.${error.type}.title`) ? t(`error.${error.type}.title`) : t('error.title')}
            </h2>
            <p className="text-gray-300 text-base leading-relaxed mb-6">
              {userFacingError(error.message, error.type !== 'generic' && t(`error.${error.type}.message`) ? t(`error.${error.type}.message`) : t('error.message'))}
            </p>
            <button
              className="bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white px-6 py-3 rounded-full font-semibold hover:from-[#282bff] hover:to-[#383bff] hover:shadow-lg transition-all duration-200 shadow-md border border-[#484dff]/60"
              onClick={handleErrorModalOkay}
            >
              {error?.message && /session expired|not authorized|unauthorized|forbidden/i.test(error.message)
                ? t("auth.signIn")
                : t('error.button')}
            </button>
          </div>
        </div>
      )}

      {/* Add the pending meeting modal */}
      {showPendingMeetingModal && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-xl font-bold text-white mb-4">{t('pending.title')}</h2>
            <p className="text-gray-300 text-base leading-relaxed">
              {pendingErrorKey ? t(pendingErrorKey) : t("pending.multiple_desktop")}
            </p>
          </div>
        </div>
      )}

      {/* Audio Upload Progress UI */}
      {(isUploadingAudio || audioUploadError) && (
        <div className="fixed top-4 right-4 bg-black border border-gray-600 rounded-lg p-3 shadow-lg z-50 max-w-xs">
          {isUploadingAudio ? (
            <div className="flex items-center space-x-2">
              <div className="animate-spin rounded-full h-4 w-4 border-2 border-gray-300 border-t-white"></div>
              <span className="text-sm text-white">{t('audioUpload.saving')}</span>
            </div>
          ) : audioUploadError ? (
            <div className="space-y-2">
              <div className="flex items-center space-x-2">
                <div className="h-4 w-4 text-red-400">⚠</div>
                <span className="text-sm text-red-400">{t('audioUpload.failed')}</span>
              </div>
              <button
                onClick={retryAudioUpload}
                className="text-xs bg-blue-600 hover:bg-blue-700 text-white px-2 py-1 rounded"
              >
                {t('audioUpload.retry')}
              </button>
              <button
                onClick={() => setAudioUploadError(null)}
                className="text-xs bg-gray-600 hover:bg-gray-700 text-white px-2 py-1 rounded ml-2"
              >
                {t('audioUpload.dismiss')}
              </button>
            </div>
          ) : null}
        </div>
      )}
      {!isTranscriptMaximized && (
        <Controllers saveStateAndReload={saveStateAndReload} pauseRecording={pauseRecording}
          stopRecording={stopRecording} isRecording={isRecording} onStartRecording={onStartRecording}
          paused={paused} transcriptionLimitExceeded={transcriptionLimitExceeded}
          translationLimitExceeded={translationLimitExceeded} socketRef={socketRef} meetingId={meetingId as string}
          bulletPointEnabled={bulletPointEnabled} stopButtonDisabled={stopButtonDisabled} authReady={authReady}
          speechToSpeechMode={speechToSpeechMode}
          languageOpenRequestId={panelRequest?.panel === 'language' ? panelRequest.id : undefined} />
      )}
      <div
        className={`${activeTab === 'Transcript' || activeTab === 'Notes'
          ? 'relative flex min-h-0 flex-1 flex-col overflow-hidden'
          : 'relative h-[calc(100%-160px)]'
          }`}
        style={(activeTab === 'Transcript' || activeTab === 'Notes') && !isTranscriptMaximized
          ? {
            // Ask MeetingCopilot is fixed to the bottom of the window. Reserve its actual
            // rendered height once, here, so transcript children can simply
            // flex and scroll within the remaining visible area.
            marginBottom: `${bulletMinimizedHeight}vh`,
            // Dropdowns inside the tabs use the same measured inset so their
            // own scroll area ends above Ask MeetingCopilot rather than behind it.
            '--ask-MeetingCopilot-height': `${bulletMinimizedHeight}vh`,
          } as CssVarStyle
          : { '--ask-MeetingCopilot-height': '0vh' } as CssVarStyle}
      >
        <Tabs
          enabled={enabled}
          socket={socketRef}
          isRecording={isRecording}
          shareCode={shareCode}
          selected={activeTab}
          setSelected={setActiveTab}
          isMaximized={isTranscriptMaximized}
          setIsMaximized={setIsTranscriptMaximized}
          onReverseTranslationToggle={handleReverseTranslationToggle}
          showTranslationDelayAnalysisToggle={showDelayAnalysisControls}
          translationDelayAnalysisEnabled={translationDelayAnalysisEnabled}
          onTranslationDelayAnalysisToggle={() => setTranslationDelayAnalysisEnabled(prev => !prev)}
          showSttDelayAnalysisToggle={showDelayAnalysisControls}
          sttDelayAnalysisEnabled={sttDelayAnalysisEnabled}
          onSttDelayAnalysisToggle={() => setSttDelayAnalysisEnabled(prev => !prev)}
          textToSpeechEnabled={textToSpeechEnabled}
          textToSpeechSessionMode={textToSpeechSessionMode}
          speechToSpeechMode={speechToSpeechMode}
          onSpeechToSpeechModeChange={handleSpeechToSpeechModeChange}
          onOtherAudioCaptureModeChange={syncOtherAudioCaptureModeToServer}
          onStartRecording={onStartRecording}
          socketConnected={authReady}
          onSharedJoinModelSwitch={saveStateAndReload}
          preferNotes={Boolean(calendarNoteContext)}
          meetingId={reservedMeetingId || meetingId}
          meetingTitle={calendarNoteContext?.title || t("desktopTopBar.cta.startLiveTranslation")}
          calendarParticipants={calendarNoteContext?.participants}
          onReserveShareInvitation={reserveShareInvitation}
          qualityOpenRequestId={panelRequest?.panel === 'quality' ? panelRequest.id : undefined}
          speechOpenRequestId={panelRequest?.panel === 'voice' ? panelRequest.id : undefined}
          sharingOpenRequestId={panelRequest?.panel === 'sharing' ? panelRequest.id : undefined}
        />
        {activeTab === "Transcript" && showDelayAnalysisControls && (translationDelayAnalysisEnabled || sttDelayAnalysisEnabled) && (
          <div className="pointer-events-none absolute left-0 right-0 top-[49px] z-20 flex flex-col gap-2 px-2">
            {translationDelayAnalysisEnabled && (
              <div className="pointer-events-auto overflow-hidden rounded-lg border border-border shadow-2xl">
                <TranslationLatencyMonitor
                  title={t("desktopUi.debug.translationDelay")}
                  itemLabel={t("controllers.translation")}
                  entries={translationLatencies}
                />
              </div>
            )}
            {sttDelayAnalysisEnabled && (
              <div className="pointer-events-auto overflow-hidden rounded-lg border border-border shadow-2xl">
                <TranslationLatencyMonitor
                  title={t("desktopUi.debug.sttDelay")}
                  itemLabel={t("tabs.transcript")}
                  entries={sttLatencies}
                />
              </div>
            )}
          </div>
        )}
        {showMeetingDetails && meetingId ?
          <div className="h-[calc(100dvh-210px)] flex flex-col justify-start items-center pt-20">
            <MeetingDetails meetingId={meetingId} />
          </div>
          :
          <>
            <RealtimeTranslatedSpeechPlayer
              socketRef={socketRef}
              textToSpeechSessionMode={textToSpeechSessionMode}
              speechToSpeechMode={speechToSpeechMode}
              textToSpeechEnabled={textToSpeechEnabled}
              socketReady={authReady}
            />
            {activeTab === "Transcript" && (
              <Transcript enabled={enabled} transcriptData={transcriptData} transcriptionLimitExceeded={transcriptionLimitExceeded}
                translationLimitExceeded={translationLimitExceeded} transcriptionUsage={transcriptionUsage}
                translationUsage={translationUsage} isRecording={isRecording} isTranscriptionReady={isTranscriptionReady} bulletPointEnabled={bulletPointEnabled} joined={joined}
                bulletOverlayOpen={bulletOverlayOpen} bulletMinimizedHeight={bulletMinimizedHeight} isMaximized={isTranscriptMaximized}
                reverseTranslationActive={reverseTranslationActive} reverseTranscriptData={reverseTranscriptData}
                spokenLanguage={spokenLanguage} translatedLanguage={translatedLanguage} shareCode={shareCode} canShowQrCode={isPremiumOrAbove}
                textToSpeechEnabled={textToSpeechEnabled} textToSpeechSessionMode={textToSpeechSessionMode}
                speechToSpeechMode={speechToSpeechMode}
                extraTranslatedLanguages={extraTranslatedLanguages} extraTranslationsBySection={extraTranslationsBySection}
                meetingFlow={showMeetingFlow && isRealTimeMemoryEligible(getSpokenLanguagePlan(), translationMode) ? meetingFlow : ""}
                onSpeakerRename={renameTranscriptSpeaker} />
            )}

            {activeTab === "Notes" && (
              <div className="-mx-3 flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                {calendarNoteContext && (
                  <div className="mx-3 mb-2 flex shrink-0 items-center justify-between gap-4 rounded-xl border border-[#2D2D32] bg-[#151517] px-3 py-2">
                    <div className="min-w-0">
                      <p className="truncate text-xs font-semibold text-[#D7D7DB]">{calendarNoteContext.title}</p>
                      <p className="mt-0.5 truncate text-[11px] text-[#777780]">
                        {calendarNoteContext.dateLabel} · {calendarNoteContext.time}
                      </p>
                    </div>
                    <span className="shrink-0 text-[10px] font-medium text-[#9FCF73]">
                      {t("desktopUi.recording.draftSaved")}
                    </span>
                  </div>
                )}
                <div className="min-h-0 flex-1">
                  <NotionEditor content={notepad} setContent={setNotepad} isActive={activeTab === "Notes"} />
                </div>
              </div>
            )}

          </>
        }
      </div>
      {renderBulletComponent()}
      {isModalOpen && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-xl font-bold text-white mb-3">{t('inactivity.title')}</h2>
            <p className="text-gray-300 text-lg font-semibold mb-6">({countdown} {t('inactivity.countdown')})</p>

            <div className="flex gap-3 justify-center">
              <button
                className="bg-gradient-to-br from-[#16a316] via-[#1cba1c] to-[#22d122] text-white px-6 py-3 rounded-full font-semibold hover:from-[#14a014] hover:to-[#1fb71f] hover:shadow-lg transition-all duration-200 shadow-md border border-[#1cba1c]/60"
                onClick={() => handleModalResponse(false)}
              >
                {t('inactivity.buttons.yes')}
              </button>
              <button
                className="bg-gradient-to-br from-[#333] via-[#444] to-[#555] text-white px-6 py-3 rounded-full font-semibold hover:from-[#444] hover:to-[#666] hover:shadow-lg transition-all duration-200 shadow-md border border-[#555]/60"
                onClick={() => handleModalResponse(true)}
              >
                {t('inactivity.buttons.no')}
              </button>
            </div>
          </div>
        </div>
      )}

      {isDeepgramError && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-xl font-bold text-white mb-4">
              {t('deepgram.title')}
            </h2>
            <p className="text-gray-300 text-base leading-relaxed mb-6">
              {t('deepgram.message')}
            </p>
            {deepgramErrorData?.showReloadButton && (
              <div className="flex flex-col items-center">
                <p className="text-sm text-gray-400 mb-4">
                  {t('deepgram.restartCountdown', { seconds: deepgramErrorCountdown })}
                </p>
                <button
                  onClick={handleDeepgramErrorOkayClick}
                  className="bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white px-6 py-3 rounded-full font-semibold hover:from-[#282bff] hover:to-[#383bff] hover:shadow-lg transition-all duration-200 shadow-md border border-[#484dff]/60"
                >
                  {t('deepgram.button')}
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Translation limit dialog */}
      {/* productionLogger.debug("🔍 DIALOG RENDER - showTranslationDialog:", showTranslationDialog, "dialogData:", dialogData) */}
      {showTranslationDialog && dialogData && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-lg font-semibold mb-4">{t('limitDialogs.translation.title')}</h2>
            <p className="mb-4">
              {t('limitDialogs.translation.availableMessage').replace('{transcriptionUsage}', dialogData.transcriptionUsage.toString()).replace('{transcriptionLimit}', dialogData.transcriptionLimit.toString())}
            </p>
            <p className="mb-6">
              {t('limitDialogs.translation.continueQuestion')}
            </p>
            <p className="text-sm text-gray-400 mb-6">
              {t('limitDialogs.translation.autoCloseMessage').replace('{countdown}', dialogCountdown.toString())}
            </p>
            <div className="flex gap-4 justify-center mb-4">
              <button
                className="border-[2px] border-primary text-black bg-[#D9D9D9] px-5 py-2 rounded-full font-semibold"
                onClick={() => handleTranslationDialogResponse(true, 'manual-yes')}
              >
                {t('limitDialogs.translation.buttons.yes')}
              </button>
              <button
                ref={translationDialogNoButtonRef}
                className="border-[2px] border-gray-500 text-white bg-gray-700 px-5 py-2 rounded-full font-semibold"
                onClick={() => handleTranslationDialogResponse(false, 'manual-no')}
              >
                {t('limitDialogs.translation.buttons.no')}
              </button>
            </div>
            <div className="flex justify-center">
              <UpgradeNow
                referrer="desktop-translation"
                className="bg-gradient-to-r from-[#383bff] via-[#484dff] to-[#585dff] hover:from-[#282bff] hover:to-[#383bff] text-white font-semibold py-3 px-6 rounded-full transition-all duration-200 shadow-lg hover:shadow-xl border border-[#484dff] hover:border-[#383bff] text-sm"
                text={t('upgrade.title')}
              />
            </div>
          </div>
        </div>
      )}

      {/* Transcription limit dialog */}
      {showTranscriptionDialog && transcriptionDialogData && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-lg font-semibold mb-4">{t('limitDialogs.transcription.title')}</h2>
            <p className="mb-6">
              {transcriptionDialogData.message === "bothLimitsMessage"
                ? t('limitDialogs.transcription.bothLimitsMessage')
                : t('limitDialogs.transcription.message')}
            </p>
            <p className="text-sm text-gray-400 mb-6">
              {t('limitDialogs.transcription.autoCloseMessage').replace('{countdown}', transcriptionDialogCountdown.toString())}
            </p>
            <div className="flex gap-4 justify-center mb-4">
              <button
                ref={transcriptionDialogOkayButtonRef}
                className="border-[2px] border-primary text-black bg-[#D9D9D9] px-5 py-2 rounded-full font-semibold"
                onClick={() => handleTranscriptionDialogResponse()}
              >
                {t('limitDialogs.transcription.buttons.okay')}
              </button>
            </div>
            <div className="flex justify-center">
              <UpgradeNow
                referrer={transcriptionDialogData.message === "bothLimitsMessage" ? "desktop-translation-transcription" : "desktop-transcription"}
                className="bg-gradient-to-r from-[#383bff] via-[#484dff] to-[#585dff] hover:from-[#282bff] hover:to-[#383bff] text-white font-semibold py-3 px-6 rounded-full transition-all duration-200 shadow-lg hover:shadow-xl border border-[#484dff] hover:border-[#383bff] text-sm"
                text={t('upgrade.title')}
              />
            </div>
          </div>
        </div>
      )}

      {/* Microphone disconnect modal */}
      <MicDisconnectModal open={showMicDisconnectModal} onClose={handleMicDisconnectModal} />

      {/* No transcription popup after limits exceeded */}
      {showNoTranscriptionPopup && (
        <div className="live-session-modal-overlay">
          <div className="live-session-modal-surface">
            <h2 className="text-lg font-semibold mb-4">{t('limitDialogs.noTranscription.title')}</h2>
            <p className="mb-4">
              {t('limitDialogs.noTranscription.message')}
            </p>
            <p className="mb-6">
              {t('limitDialogs.noTranscription.upgradeMessage')}
            </p>
            <div className="flex gap-4 justify-center">
              <button
                onClick={() => {
                  setShowNoTranscriptionPopup(false);
                  stopNoTranscriptionMonitoring();
                }}
                className="bg-primary text-white text-[14px] h-[36px] px-6 rounded-full flex justify-center items-center hover:opacity-80 font-semibold"
              >
                {t('limitDialogs.noTranscription.buttons.ok')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default AudioRecorder;
