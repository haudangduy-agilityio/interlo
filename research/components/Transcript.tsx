import { formatTranscriptParagraphs, splitTranscriptSentences } from '../../../shared/transcriptSentences';
import { createTranslatedSpeechGain, getSpeechVolumeDirection } from '../../utils/translatedSpeechVolume';
import { productionLogger } from '../../lib/productionLogger'
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import axios from "axios";
import { useTranslation } from "../../hooks/useTranslation";
import { useAppSelector } from "../../redux/hooks";
import { generateShareURL } from "../../constants/sharing";
import UpgradeNow from "./UpgradeNow";
import { baseURL } from "../../lib/baseURL";
import {
    isUplinkPlaybackSink,
    resolveTranscriptPlaybackSource,
} from "../../utils/realtimeTranslation/realtimeAudioRouter";
import { groupTranscriptForDisplay } from "../../utils/fastTranscriptGrouping";
import {
    mergeContinuousGroupSourceText,
    mergeContinuousGroupTranslationText,
} from "../../utils/fastTranscriptDisplay";
import { useLiveSpeakerNamesPreference } from "../../hooks/useLiveSpeakerNamesPreference";
import { useLiveTranslationOnlyPreference } from "../../hooks/useLiveTranslationOnlyPreference";
import {
    shouldSuppressSameLanguageOutput,
    useSameLanguageOutputPreference,
} from "../../hooks/useSameLanguageOutputPreference";
import { useAutoscrollToBottom } from "../../hooks/useAutoscrollToBottom";

interface TranscriptProps {
    enabled: boolean;
    transcriptData: any;
    transcriptionLimitExceeded: boolean;
    translationLimitExceeded: boolean;
    transcriptionUsage: any;
    translationUsage: any;
    isRecording: boolean;
    isTranscriptionReady?: boolean;
    bulletPointEnabled: boolean;
    joined: boolean;
    bulletOverlayOpen?: boolean;
    bulletMinimizedHeight?: number;
    isMaximized?: boolean;
    reverseTranslationActive?: boolean;
    reverseTranscriptData?: any;
    spokenLanguage?: string;
    translatedLanguage?: string;
    shareCode?: string;
    canShowQrCode?: boolean;
    textToSpeechEnabled?: boolean;
    textToSpeechSessionMode?: 'online' | 'in-person';
    speechToSpeechMode?: 'off' | 'online-incoming' | 'online-outgoing' | 'online-both' | 'in-person';
    extraTranslatedLanguages?: string[];
    extraTranslationsBySection?: Record<number, Record<string, string>>;
    meetingFlow?: string;
    onSpeakerRename?: (currentSpeaker: string, nextSpeaker: string) => void;
}

const TRANSLATED_SPEECH_PLAYBACK_EVENT = 'translatedSpeechPlaybackStateChanged';
const AI_AUDIO_OUTPUT_STORAGE_KEY = 'selectedAIAudioOutput';
/** When set, system/remote rows play TTS here; mic/You rows use `selectedAIAudioOutput` (Meet uplink). */
const AI_AUDIO_OUTPUT_INCOMING_STORAGE_KEY = 'MeetingCopilot_ai_voice_output_incoming';
const MEET_UPLINK_TTS_OWN_VOICE_KEY = 'MeetingCopilot_meet_uplink_tts_own_voice';
const MEET_UPLINK_TTS_OWN_VOICE_EVENT = 'MeetingCopilotMeetUplinkTtsOwnVoiceChanged';

/** Limits parallel POST /chat/text-to-speech calls. Many at once hit browser per-host connection caps and stall or fail. */
const createTtsHttpSemaphore = (maxConcurrent: number) => {
    let inUse = 0;
    const waiters: Array<() => void> = [];

    const acquire = (): Promise<() => void> =>
        new Promise((resolveWithRelease) => {
            const tryGiveSlot = () => {
                if (inUse < maxConcurrent) {
                    inUse += 1;
                    resolveWithRelease(() => {
                        inUse -= 1;
                        const next = waiters.shift();
                        if (next) next();
                    });
                } else {
                    waiters.push(tryGiveSlot);
                }
            };
            tryGiveSlot();
        });

    return { acquire };
};

/** TTS API latency varies per clip; too many parallel calls can 429 and leave XHRs pending for a long time. */
const ttsHttpSemaphore = createTtsHttpSemaphore(5);
const TTS_CONTEXT_MAX_CHARS = 200;



/** Avoid scheduling a fetch for every eligible row on each render; playback effect still requests as needed. */
const TTS_PREFETCH_MAX_ROWS = 8;

const Transcript = ({ enabled, transcriptData, transcriptionLimitExceeded, translationLimitExceeded, transcriptionUsage, translationUsage, isRecording, isTranscriptionReady = false, bulletPointEnabled, joined, isMaximized, reverseTranslationActive, reverseTranscriptData, spokenLanguage, translatedLanguage, shareCode, canShowQrCode, textToSpeechEnabled = false, textToSpeechSessionMode = 'online', speechToSpeechMode = textToSpeechEnabled ? (textToSpeechSessionMode === 'in-person' ? 'in-person' : 'online-incoming') : 'off', extraTranslatedLanguages, extraTranslationsBySection, meetingFlow, onSpeakerRename }: TranscriptProps) => {
    const { t } = useTranslation();
    const [showLiveSpeakerNames] = useLiveSpeakerNamesPreference();
    const [showLiveTranslationOnly] = useLiveTranslationOnlyPreference();
    const [sameLanguageOutputEnabled] = useSameLanguageOutputPreference();
    const { translationMode } = useAppSelector((state) => state.recorder);
    const hideSourceCaptions = enabled && showLiveTranslationOnly;
    const useRealtimeSpeechPlayback = translationMode === 'fast' && enabled;
    const useFastOnlineTranscriptLayout = translationMode === 'fast' && (
        speechToSpeechMode === 'off'
        || speechToSpeechMode === 'online-incoming'
        || speechToSpeechMode === 'online-outgoing'
        || speechToSpeechMode === 'online-both'
    );
    const useConsolidatedFastGroupRender =
        useFastOnlineTranscriptLayout
        && (
            speechToSpeechMode === 'online-incoming'
            || speechToSpeechMode === 'online-outgoing'
            || speechToSpeechMode === 'online-both'
        );
    const renderSpeakerName = (speakerName: string) => (
        <span
            contentEditable={Boolean(onSpeakerRename)}
            suppressContentEditableWarning
            onBlur={(event) => {
                const nextSpeaker = event.currentTarget.innerText.trim();
                if (!nextSpeaker) {
                    event.currentTarget.innerText = speakerName;
                    return;
                }
                if (nextSpeaker !== speakerName) {
                    onSpeakerRename?.(speakerName, nextSpeaker);
                }
            }}
            onKeyDown={(event) => {
                if (event.key === 'Enter') {
                    event.preventDefault();
                    event.currentTarget.blur();
                }
                if (event.key === 'Escape') {
                    event.currentTarget.innerText = speakerName;
                    event.currentTarget.blur();
                }
            }}
            title={onSpeakerRename ? t("desktopUi.transcript.renameSpeaker") : undefined}
            className={`block rounded-md border border-[#303036] bg-[#19191C] px-2 py-0.5 text-[13px] font-semibold text-[#D7D7DB] outline-none ${onSpeakerRename ? 'cursor-text focus:border-[#5156FF]/70' : ''}`}
        >
            {speakerName}
        </span>
    );

    const hasExtras = !!extraTranslatedLanguages && extraTranslatedLanguages.length > 0;
    const transcriptDisplayGroups = useMemo(() => groupTranscriptForDisplay<any>(
        transcriptData,
        { fastMode: useFastOnlineTranscriptLayout, mode: speechToSpeechMode },
    ), [transcriptData, useFastOnlineTranscriptLayout, speechToSpeechMode]);
    const singleTranscriptScroll = useAutoscrollToBottom(
        null,
        reverseTranslationActive ? 'inactive' : 'single',
    );
    const primarySplitTranscriptScroll = useAutoscrollToBottom(
        null,
        reverseTranslationActive ? 'split' : 'inactive',
    );
    const reverseSplitTranscriptScroll = useAutoscrollToBottom(
        null,
        reverseTranslationActive ? 'split' : 'inactive',
    );

    // Mixed-line renderer: zip the primary translation with each extra-language translation
    // sentence-by-sentence so the user sees a small multilingual group, then the next group,
    // etc. (instead of one long block per language). Used when extras are selected.
    const renderMixedTranslations = (
        item: any,
        primaryText: string | undefined,
        sectionId: number,
        isLocked: boolean,
    ) => {
        if (!hasExtras) return null;
        const bySection = (extraTranslationsBySection && extraTranslationsBySection[sectionId]) || {};

        const allLangs = [translatedLanguage || '', ...(extraTranslatedLanguages || [])].filter(Boolean);
        const langs = allLangs
            .filter((targetLanguage, targetIndex) => {
                if (targetIndex === 0
                    && translationMode === 'fast'
                    && !sameLanguageOutputEnabled
                    && item?.sameLanguageOutputSuppressed === true) {
                    return false;
                }
                return !shouldSuppressSameLanguageOutput({
                    section: item,
                    targetLanguage,
                    sameLanguageOutputEnabled,
                    translationMode,
                    spokenLanguage,
                });
            });
        if (langs.length === 0) return null;
        const textsByLang: Record<string, string[]> = {};
        if (translatedLanguage) {
            textsByLang[translatedLanguage] = splitTranscriptSentences(primaryText);
        }
        for (const lang of extraTranslatedLanguages || []) {
            textsByLang[lang] = splitTranscriptSentences(bySection[lang]);
        }
        const maxLen = Math.max(...langs.map((l) => textsByLang[l].length), 0);
        if (maxLen === 0) return null;

        const sizePx = `${Math.max(14, Math.round(20 * (fontSize / 50)))}px`;
        const CHUNK = 2; // sentences per language per group — keeps each "mix line" readable
        const groups: JSX.Element[] = [];
        for (let start = 0; start < maxLen; start += CHUNK) {
            const groupIdx = start / CHUNK;
            groups.push(
                <div
                    key={`mix-${sectionId}-${groupIdx}`}
                    className={`${groupIdx > 0 ? 'mt-2 pt-2 border-t border-gray-700/60' : ''} flex flex-col gap-1 w-full`}
                >
                    {langs.map((lang) => {
                        const slice = textsByLang[lang].slice(start, start + CHUNK).join('').trim();
                        if (!slice) return null;
                        return (
                            <span
                                key={`mix-${sectionId}-${groupIdx}-${lang}`}
                                dir="auto"
                                className={`live-translation-text ${isLocked ? 'live-translation-text--settled text-gray-200' : 'live-translation-text--provisional text-gray-600 opacity-80'} block whitespace-pre-line break-words overflow-wrap-anywhere w-full transition-colors duration-300`}
                                style={{ fontSize: sizePx }}
                            >
                                {slice}
                            </span>
                        );
                    })}
                </div>
            );
        }
        return <>{groups}</>;
    };

    // Translation locking state
    const [lockedTranslations, setLockedTranslations] = useState<Set<number>>(new Set());
    // Counter to force TTS playback effect re-evaluation for stable-window fallback
    const [ttsReevalTick, setTtsReevalTick] = useState(0);
    const [translationTimestamps, setTranslationTimestamps] = useState<Map<number, Date>>(new Map());
    const translationFreezeSnapshotsRef = useRef<Map<number, string>>(new Map());
    const [showQrCode, setShowQrCode] = useState(() => localStorage.getItem('showQrCode') === 'true');
    const spokenTranslationKeysRef = useRef<Set<string>>(new Set());
    const ttsAudioCacheRef = useRef<Map<string, Promise<{ audioBase64: string; mimeType: string } | null>>>(new Map());
    const lastSpeechKeysBySectionRef = useRef<Map<string, string>>(new Map());
    const ttsQueueRef = useRef<Promise<void>>(Promise.resolve());
    const activeAudioRef = useRef<HTMLAudioElement | null>(null);
    const activeAudioContextRef = useRef<AudioContext | null>(null);
    const activePlaybackCleanupRef = useRef<(() => void) | null>(null);
    const textToSpeechEnabledRef = useRef(textToSpeechEnabled);
    const textToSpeechSessionModeRef = useRef(textToSpeechSessionMode);
    const speechToSpeechModeRef = useRef(speechToSpeechMode);
    const transcriptDataRef = useRef<any[]>(transcriptData || []);
    const lockedTranslationsRef = useRef<Set<number>>(new Set());
    const translatedLanguageRef = useRef(translatedLanguage);
    const meetUplinkTtsOwnVoiceRef = useRef(
        typeof window !== 'undefined' && localStorage.getItem(MEET_UPLINK_TTS_OWN_VOICE_KEY) === 'true'
    );

    useEffect(() => {
        const sync = () => {
            meetUplinkTtsOwnVoiceRef.current =
                typeof window !== 'undefined' && localStorage.getItem(MEET_UPLINK_TTS_OWN_VOICE_KEY) === 'true';
        };
        const onStorage = (e: StorageEvent) => {
            if (e.key === MEET_UPLINK_TTS_OWN_VOICE_KEY) sync();
        };
        window.addEventListener('storage', onStorage);
        window.addEventListener(MEET_UPLINK_TTS_OWN_VOICE_EVENT, sync as EventListener);
        return () => {
            window.removeEventListener('storage', onStorage);
            window.removeEventListener(MEET_UPLINK_TTS_OWN_VOICE_EVENT, sync as EventListener);
        };
    }, []);

    useEffect(() => {
        translatedLanguageRef.current = translatedLanguage;
    }, [translatedLanguage]);

    const getTranscriptRowIdentity = (item: any, index: number) => String(item?.sectionId ?? index);

    const getSectionSpeechKey = (item: any, index: number) => {
        const sectionId = item?.sectionId ?? index;
        const revision = Number(item?.ttsPlayback?.revision);

        if (Number.isFinite(revision) && revision > 0) {
            return `${sectionId}:${revision}`;
        }

        const translatedText = item?.translatedText?.trim() || '';
        return `${sectionId}:${translatedText}`;
    };

    const isSectionTranslationLocked = (item: any, index: number) => {
        if (item?.ttsPlayback?.ready === true) {
            return true;
        }

        return lockedTranslations.has(index);
    };

    const isTtsReadyImmediate = (item: any) => {
        const r = item?.ttsPlayback?.readyReason;
        return r === 'finalized' || r === 'section-closed';
    };

    const isSectionEligibleForTtsPlayback = (item: any, index: number) => {
        if (!sameLanguageOutputEnabled && item?.sameLanguageOutputSuppressed === true) return false;
        if (!item?.translatedText?.trim()) return false;
        if (item?.ttsPlayback?.ready !== true) return false;
        // Block while a translation API call is still in-flight — the text may still change.
        if (item?.ttsPlayback?.translationPending === true) return false;
        // Sections finalized by speaker change or utterance end are immediately eligible
        if (isTtsReadyImmediate(item)) return true;
        // Fallback for the last active section: accept stable-window once the translation
        // has been stable for stabilityMs + 500ms buffer. stabilityMs is derived from the
        // user's translation quality setting on the server, so this adapts automatically.
        if (item?.ttsPlayback?.readyReason === 'stable-window' && item?.ttsPlayback?.readyAt) {
            const dynamicFallbackMs = (item?.ttsPlayback?.stabilityMs ?? 2350) + 500;
            const readyAge = Date.now() - new Date(item.ttsPlayback.readyAt).getTime();
            return readyAge >= dynamicFallbackMs;
        }
        return false;
    };

    const emitTranslatedSpeechPlaybackState = (active: boolean, detail: Record<string, any> = {}) => {
        window.dispatchEvent(new CustomEvent(TRANSLATED_SPEECH_PLAYBACK_EVENT, {
            detail: {
                active,
                mode: textToSpeechSessionMode,
                ...detail,
            },
        }));
    };


    const getConfiguredAiAudioOutputId = () => {
        if (typeof window === 'undefined') return 'default';
        return localStorage.getItem(AI_AUDIO_OUTPUT_STORAGE_KEY) || 'default';
    };

    const isMeetUplinkTtsOwnVoiceEnabled = () =>
        typeof window !== 'undefined' && localStorage.getItem(MEET_UPLINK_TTS_OWN_VOICE_KEY) === 'true';

    const getAiPlaybackSinkIdForSection = (item: any) => {
        if (typeof window === 'undefined') return 'default';
        const uplink = localStorage.getItem(AI_AUDIO_OUTPUT_STORAGE_KEY) || 'default';
        const incoming = localStorage.getItem(AI_AUDIO_OUTPUT_INCOMING_STORAGE_KEY);
        if (!incoming) {
            return uplink;
        }
        const source = resolveTranscriptPlaybackSource(item?.desktopAudioSource, item?.speakerName);
        if (isUplinkPlaybackSink(source, speechToSpeechModeRef.current, isMeetUplinkTtsOwnVoiceEnabled())) {
            return uplink;
        }
        return incoming;
    };

    const routeAudioToConfiguredOutput = async (
        audio: HTMLAudioElement,
        sectionId: number | string,
        speechKey: string,
        outputDeviceId?: string
    ) => {
        const resolved = outputDeviceId ?? getConfiguredAiAudioOutputId();
        const sinkSetter = (audio as any).setSinkId as ((sinkId: string) => Promise<void>) | undefined;

        if (!sinkSetter || resolved === 'default') {
            return 'default';
        }

        try {
            await sinkSetter.call(audio, resolved);
            productionLogger.debug('[TTS]', {
                context: 'ttsRoutedToSink',
                sectionId,
                speechKey,
                outputDeviceId: resolved,
            });
            return resolved;
        } catch (error) {
            productionLogger.error('[TTS] Failed to route translated speech to selected output:', error, { sectionId, speechKey, outputDeviceId: resolved });
            return 'default';
        }
    };



    const shouldSkipTextToSpeechForSection = (item: any, sessionMode: 'online' | 'in-person' = textToSpeechSessionMode) => {
        if (!sameLanguageOutputEnabled && item?.sameLanguageOutputSuppressed === true) {
            return true;
        }
        if (sessionMode !== 'online') {
            return false;
        }

        const uplinkEnabled = isMeetUplinkTtsOwnVoiceEnabled();
        productionLogger.debug('[TTS-DEBUG] shouldSkip check:', {
            uplinkEnabled,
            refValue: meetUplinkTtsOwnVoiceRef.current,
            desktopAudioSource: item?.desktopAudioSource,
            speakerName: item?.speakerName,
            eligibleForOnline: item?.ttsPlayback?.eligibleForOnline,
        });

        const mode = speechToSpeechModeRef.current;
        const isOwnSpeech = item?.speakerName === 'You' || item?.desktopAudioSource === 'mic' || item?.desktopAudioSource === 'unknown';

        if (mode === 'online-both') return false;
        if (mode === 'online-outgoing') return !isOwnSpeech;
        if (mode === 'online-incoming') return isOwnSpeech;

        if (typeof item?.ttsPlayback?.eligibleForOnline === 'boolean') {
            return !item.ttsPlayback.eligibleForOnline;
        }

        return item?.speakerName === 'You' || item?.desktopAudioSource === 'mic' || item?.desktopAudioSource === 'unknown';
    };

    const getPreviousTranslatedContext = (rowIndex: number) => {
        const rows = transcriptDataRef.current || [];
        for (let i = rowIndex - 1; i >= 0; i -= 1) {
            const prev = rows[i]?.translatedText?.trim();
            if (!prev) continue;
            return prev.length > TTS_CONTEXT_MAX_CHARS ? prev.slice(-TTS_CONTEXT_MAX_CHARS) : prev;
        }
        return undefined;
    };

    const startTtsAudioRequest = useCallback((speechKey: string, text: string, targetLanguage?: string, rowIndex?: number) => {
        if (useRealtimeSpeechPlayback) {
            productionLogger.debug('[FAST-S2S] REST TTS disabled — using realtime stream');
            return Promise.resolve(null);
        }

        const existing = ttsAudioCacheRef.current.get(speechKey);
        if (existing) return existing;

        const previousText = rowIndex !== undefined ? getPreviousTranslatedContext(rowIndex) : undefined;

        const request = (async (): Promise<{ audioBase64: string; mimeType: string } | null> => {
            const release = await ttsHttpSemaphore.acquire();
            try {
                const postTts = async () => {
                    let lastErr: unknown;
                    for (let attempt = 0; attempt < 3; attempt += 1) {
                        try {
                            return await axios.post(
                                `${baseURL}/chat/text-to-speech`,
                                {
                                    text,
                                    targetLanguage,
                                    ...(previousText ? { previousText } : {}),
                                },
                                { timeout: 30_000 }
                            );
                        } catch (e) {
                            lastErr = e;
                            const status = axios.isAxiosError(e) ? e.response?.status : undefined;
                            if (status === 503 && attempt < 2) {
                                await new Promise((r) => setTimeout(r, 900 * (attempt + 1)));
                                continue;
                            }
                            throw e;
                        }
                    }
                    throw lastErr;
                };

                const response = await postTts();
                const audioBase64 = response.data?.data?.audioBase64;
                const mimeType = response.data?.data?.mimeType || 'audio/mpeg';
                if (!audioBase64) return null;
                return { audioBase64, mimeType };
            } catch (error) {
                productionLogger.error('Failed to generate translated speech:', error);
                ttsAudioCacheRef.current.delete(speechKey);
                return null;
            } finally {
                release();
            }
        })();

        ttsAudioCacheRef.current.set(speechKey, request);
        return request;
    }, [useRealtimeSpeechPlayback]);

    /** When a clip actually starts, aggressively start fetches for the following rows so a slow clip N is less likely to starve the pipeline. */
    const warmTtsFetchesAfterRow = (afterIndex: number) => {
        if (textToSpeechSessionModeRef.current !== 'online' || !textToSpeechEnabledRef.current) return;

        const rows = transcriptDataRef.current || [];
        let started = 0;
        const maxAhead = 6;

        for (let j = afterIndex + 1; j < rows.length && started < maxAhead; j += 1) {
            const row = rows[j];
            if (!row?.translatedText?.trim()) continue;
            if (shouldSkipTextToSpeechForSection(row, textToSpeechSessionModeRef.current)) continue;
            if (row?.ttsPlayback?.ready !== true) continue;

            const clientLocked =
                lockedTranslationsRef.current.has(j) || isTtsReadyImmediate(row);
            if (!clientLocked) continue;

            const speechKey = getSectionSpeechKey(row, j);
            if (spokenTranslationKeysRef.current.has(speechKey)) continue;
            if (ttsAudioCacheRef.current.has(speechKey)) continue;

            const lang = row?.ttsPlayback?.targetLanguage || translatedLanguageRef.current;
            void startTtsAudioRequest(speechKey, row.translatedText.trim(), lang, j);
            started += 1;
        }
    };

    // "Getting ready" status: shown while recording has started but the transcription/
    // translation pipeline isn't live yet (server hasn't emitted `deepgram_ready`). Clears
    // once the pipeline is ready OR the first transcript arrives — whichever happens first.
    // A safety timeout hides it after 15s so a missed/failed ready signal can't strand it.
    const hasTranscript = !!transcriptData && transcriptData.length > 0;
    const [preparingTimedOut, setPreparingTimedOut] = useState(false);
    const showPreparing = isRecording && !isTranscriptionReady && !hasTranscript && !preparingTimedOut;
    useEffect(() => {
        if (!isRecording || isTranscriptionReady || hasTranscript) {
            setPreparingTimedOut(false);
            return;
        }
        const timer = setTimeout(() => setPreparingTimedOut(true), 15000);
        return () => clearTimeout(timer);
    }, [isRecording, isTranscriptionReady, hasTranscript]);

    const [fontSize, setFontSize] = useState(() => {
        const savedFontSize = localStorage.getItem('textSize');
        return savedFontSize ? parseInt(savedFontSize) : 50; // Default 50%
    });

    useEffect(() => {
        textToSpeechEnabledRef.current = textToSpeechEnabled;
    }, [textToSpeechEnabled]);

    useEffect(() => {
        textToSpeechSessionModeRef.current = textToSpeechSessionMode;
    }, [textToSpeechSessionMode]);

    useEffect(() => {
        speechToSpeechModeRef.current = speechToSpeechMode;
    }, [speechToSpeechMode]);

    useEffect(() => {
        lockedTranslationsRef.current = lockedTranslations;
    }, [lockedTranslations]);

    useEffect(() => {
        transcriptDataRef.current = transcriptData || [];
    }, [transcriptData]);

    const getSectionSourceDisplayText = (item: any) => (
        Array.isArray(item?.captions)
            ? item.captions.map((caption: any) => caption?.text || '').join(' ')
            : ''
    ).trim();

    const getPrimaryTargetLanguage = (item: any) => translationMode === 'fast'
        ? item?.fastOutputLanguage || translatedLanguage
        : translatedLanguage;

    const suppressPrimaryTranslation = (item: any) => {
        if (!sameLanguageOutputEnabled
            && translationMode === 'fast'
            && item?.sameLanguageOutputSuppressed === true) return true;
        return shouldSuppressSameLanguageOutput({
            section: item,
            targetLanguage: getPrimaryTargetLanguage(item),
            sameLanguageOutputEnabled,
            translationMode,
            spokenLanguage,
        });
    };

    const shouldHideSectionSource = (item: any) => {
        if (!hideSourceCaptions) return false;
        const targets = [getPrimaryTargetLanguage(item) || '', ...(extraTranslatedLanguages || [])].filter(Boolean);
        return targets.some((targetLanguage, targetIndex) => {
            if (targetIndex === 0
                && translationMode === 'fast'
                && !sameLanguageOutputEnabled
                && item?.sameLanguageOutputSuppressed === true) {
                return false;
            }
            return !shouldSuppressSameLanguageOutput({
                section: item,
                targetLanguage,
                sameLanguageOutputEnabled,
                translationMode,
                spokenLanguage,
            });
        });
    };

    const getTranslationFreezeSignature = (item: any) => {
        const sourceText = getSectionSourceDisplayText(item);
        return `${sourceText}::${(item?.translatedText || '').trim()}`;
    };

    const renderSectionSourceText = (item: any) => {
        if (shouldHideSectionSource(item)) return null;
        const sourceText = getSectionSourceDisplayText(item);
        if (!sourceText) return null;

        return (
            <span
                className="text-[#B2B2B2] block break-words overflow-wrap-anywhere"
                style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px` }}
            >
                {sourceText}
            </span>
        );
    };

    const formatPrimaryTranslation = (text: string | undefined) => {
        if (!text) return '';
        if (useFastOnlineTranscriptLayout) return text.trim();
        return formatTranscriptParagraphs(text);
    };

    const renderTranscriptGroupEntries = (group: {
        entries: Array<{ item: any; index: number }>;
    }) => {
        if (useConsolidatedFastGroupRender && !hasExtras) {
            const mergedSource = mergeContinuousGroupSourceText(group.entries.filter(({ item }) => !shouldHideSectionSource(item)));
            const mergedTranslation = mergeContinuousGroupTranslationText(group.entries.filter(({ item }) => !suppressPrimaryTranslation(item)));
            const isTranslationLocked = group.entries.some(
                ({ item, index }) => isSectionTranslationLocked(item, index)
            );

            return (
                <div className="w-full">
                    {mergedSource && (
                        <span
                            className="text-[#B2B2B2] block break-words overflow-wrap-anywhere"
                            style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px` }}
                        >
                            {mergedSource}
                        </span>
                    )}
                    {mergedTranslation && (
                        <span
                            dir="auto"
                            className={`live-translation-text ${isTranslationLocked ? 'live-translation-text--settled text-gray-200' : 'live-translation-text--provisional text-gray-600 opacity-80'} block whitespace-pre-line transition-colors duration-300 break-words overflow-wrap-anywhere w-full`}
                            style={{ fontSize: `${Math.max(14, Math.round(20 * (fontSize / 50)))}px` }}
                        >
                            {formatPrimaryTranslation(mergedTranslation)}
                        </span>
                    )}
                </div>
            );
        }

        return (
            <div className="w-full">
                {group.entries.map(({ item, index }, entryIndex) => {
                    const isTranslationLocked = isSectionTranslationLocked(item, index);
                    return (
                        <div key={`entry-${item?.sectionId ?? index}`} className={entryIndex > 0 ? 'mt-3' : undefined}>
                            {renderSectionSourceText(item)}
                            {hasExtras ? (
                                renderMixedTranslations(item, item.translatedText, item.sectionId, isTranslationLocked)
                            ) : (
                                item.translatedText && !suppressPrimaryTranslation(item) && (
                                    <span
                                        dir="auto"
                                        className={`live-translation-text ${isTranslationLocked ? 'live-translation-text--settled text-gray-200' : 'live-translation-text--provisional text-gray-600 opacity-80'} block whitespace-pre-line transition-colors duration-300 break-words overflow-wrap-anywhere w-full`}
                                        style={{ fontSize: `${Math.max(14, Math.round(20 * (fontSize / 50)))}px` }}
                                    >
                                        {formatPrimaryTranslation(item.translatedText)}
                                    </span>
                                )
                            )}
                        </div>
                    );
                })}
            </div>
        );
    };

    useEffect(() => {
        const handleStorageChange = () => {
            const savedFontSize = localStorage.getItem('textSize');
            setFontSize(savedFontSize ? parseInt(savedFontSize) : 50);
        };

        window.addEventListener('storage', handleStorageChange);

        // Also listen for manual updates within the same tab
        const interval = setInterval(() => {
            const savedFontSize = localStorage.getItem('textSize');
            const currentSize = savedFontSize ? parseInt(savedFontSize) : 50;
            if (currentSize !== fontSize) {
                setFontSize(currentSize);
            }
        }, 500);

        return () => {
            window.removeEventListener('storage', handleStorageChange);
            clearInterval(interval);
        };
    }, [fontSize]);

    useEffect(() => {
        const handleShowQrCodeChanged = (event: Event) => {
            const nextValue = (event as CustomEvent<boolean>).detail;
            if (typeof nextValue === 'boolean') {
                setShowQrCode(nextValue);
                return;
            }
            setShowQrCode(localStorage.getItem('showQrCode') === 'true');
        };

        window.addEventListener('showQrCodeChanged', handleShowQrCodeChanged as EventListener);
        return () => {
            window.removeEventListener('showQrCodeChanged', handleShowQrCodeChanged as EventListener);
        };
    }, []);

    // Translation locking logic
    useEffect(() => {
        if (!transcriptData || transcriptData.length === 0) return;

        const currentTime = new Date();
        const savedQuality = localStorage.getItem('translationQuality');
        const translationQualitySeconds = savedQuality ? parseFloat(savedQuality) : 1.0;

        const newLockedTranslations = new Set(lockedTranslations);
        const newTimestamps = new Map(translationTimestamps);
        const activeIndexes = new Set<number>();

        transcriptData.forEach((item, index) => {
            activeIndexes.add(index);
            if (!item.translatedText) return;

            const signature = getTranslationFreezeSignature(item);
            const previousSignature = translationFreezeSnapshotsRef.current.get(index);
            if (previousSignature !== signature) {
                translationFreezeSnapshotsRef.current.set(index, signature);
                newTimestamps.set(index, currentTime);
                newLockedTranslations.delete(index);
                productionLogger.debug(`[Translation Lock] Reset tracking for index ${index} because text changed at:`, currentTime);
                return;
            }

            if (newLockedTranslations.has(index)) return;

            // Track translation timestamp for this paragraph
            if (!newTimestamps.has(index)) {
                newTimestamps.set(index, currentTime);
                productionLogger.debug(`[Translation Lock] Started tracking translation for index ${index} at:`, currentTime);
            }

            const translationStartTime = newTimestamps.get(index)!;
            const timeDiff = currentTime.getTime() - translationStartTime.getTime();

            // Check locking conditions
            const nextParagraph = index < transcriptData.length - 1 ? transcriptData[index + 1] : null;
            const isNewSpeaker = index > 0 && transcriptData[index - 1].speakerName !== item.speakerName;
            const newSpeakerTimestamp = isNewSpeaker ? item.start : null;

            const condition1 = timeDiff >= translationQualitySeconds * 1000;
            const condition2 = nextParagraph && nextParagraph.start <= currentTime;
            const condition3 = isNewSpeaker && newSpeakerTimestamp !== null && newSpeakerTimestamp <= currentTime;

            const shouldLock = condition1 || condition2 || condition3;

            if (shouldLock) {
                productionLogger.debug(`[Translation Lock] Locking translation for index ${index}:`, {
                    condition1: `Time passed: ${condition1} (${timeDiff}ms >= ${translationQualitySeconds * 1000}ms)`,
                    condition2: `Next paragraph: ${condition2}`,
                    condition3: `New speaker: ${condition3}`,
                    translationQualitySeconds,
                    timeDiff
                });
                newLockedTranslations.add(index);
            }
        });

        Array.from(translationFreezeSnapshotsRef.current.keys()).forEach(index => {
            if (!activeIndexes.has(index)) {
                translationFreezeSnapshotsRef.current.delete(index);
                newTimestamps.delete(index);
                newLockedTranslations.delete(index);
            }
        });

        setLockedTranslations(newLockedTranslations);
        setTranslationTimestamps(newTimestamps);
    }, [transcriptData]); // Remove dependencies that cause infinite loops

    // Add a timer to periodically check for locking
    useEffect(() => {
        const interval = setInterval(() => {
            if (!transcriptData || transcriptData.length === 0) return;

            const currentTime = new Date();
            const savedQuality = localStorage.getItem('translationQuality');
            const translationQualitySeconds = savedQuality ? parseFloat(savedQuality) : 1.0;

            setLockedTranslations(prev => {
                const newLocked = new Set(prev);
                let hasChanges = false;

                transcriptData.forEach((item, index) => {
                    if (!item.translatedText || newLocked.has(index)) return;
                    const signature = getTranslationFreezeSignature(item);
                    if (translationFreezeSnapshotsRef.current.get(index) !== signature) return;

                    const translationStart = translationTimestamps.get(index);
                    if (!translationStart) return;

                    const timeDiff = currentTime.getTime() - translationStart.getTime();
                    const shouldLock = timeDiff >= translationQualitySeconds * 1000;

                    if (shouldLock) {
                        productionLogger.debug(`[Translation Lock] Timer locking index ${index} after ${timeDiff}ms`);
                        newLocked.add(index);
                        hasChanges = true;
                    }
                });

                return hasChanges ? newLocked : prev;
            });
        }, 120); // Check often enough for sub-second translation quality values.

        return () => clearInterval(interval);
    }, [transcriptData, translationTimestamps]);

    // Prefetch TTS audio while waiting for client translation lock (online only) so playback starts sooner after lock.
    useEffect(() => {
        if (useRealtimeSpeechPlayback) return;
        if (!textToSpeechEnabled || textToSpeechSessionMode !== 'online' || !transcriptData?.length) return;

        let prefetchCount = 0;
        for (let index = 0; index < transcriptData.length; index += 1) {
            if (prefetchCount >= TTS_PREFETCH_MAX_ROWS) break;
            const item = transcriptData[index];
            if (!item?.translatedText?.trim()) continue;
            if (shouldSkipTextToSpeechForSection(item)) continue;
            if (!item?.ttsPlayback?.ready) continue;
            if (lockedTranslations.has(index) && !isTtsReadyImmediate(item)) continue;

            const speechKey = getSectionSpeechKey(item, index);
            if (spokenTranslationKeysRef.current.has(speechKey)) continue;
            if (ttsAudioCacheRef.current.has(speechKey)) continue;

            const targetLanguage = item?.ttsPlayback?.targetLanguage || translatedLanguage;
            void startTtsAudioRequest(speechKey, item.translatedText.trim(), targetLanguage, index);
            prefetchCount += 1;
        }
    }, [transcriptData, lockedTranslations, textToSpeechEnabled, textToSpeechSessionMode, speechToSpeechMode, translatedLanguage, startTtsAudioRequest, useRealtimeSpeechPlayback]);

    useEffect(() => {
        if (useRealtimeSpeechPlayback) return;
        const rows = transcriptData || [];
        const nextIds = new Set<string>();
        rows.forEach((item, index) => {
            const sectionKey = String(item?.sectionId ?? index);
            nextIds.add(sectionKey);
            const speechKey = getSectionSpeechKey(item, index);
            const prev = lastSpeechKeysBySectionRef.current.get(sectionKey);
            if (prev && prev !== speechKey) {
                ttsAudioCacheRef.current.delete(prev);
            }
            lastSpeechKeysBySectionRef.current.set(sectionKey, speechKey);
        });
        for (const key of Array.from(lastSpeechKeysBySectionRef.current.keys())) {
            if (!nextIds.has(key)) {
                lastSpeechKeysBySectionRef.current.delete(key);
            }
        }
    }, [transcriptData]);

    useEffect(() => {
        const readySections = (transcriptData || [])
            .map((item, index) => ({ item, index }))
            .filter(({ item, index }) => {
                const eligible = isSectionEligibleForTtsPlayback(item, index);
                // DEBUG: trace why mic sections don't reach TTS
                if (item?.desktopAudioSource === 'mic' || item?.speakerName === 'You') {
                    productionLogger.debug('[TTS-ELIGIBILITY]', {
                        index,
                        sectionId: item?.sectionId,
                        speakerName: item?.speakerName,
                        desktopAudioSource: item?.desktopAudioSource,
                        eligible,
                        hasTranslatedText: !!item?.translatedText?.trim(),
                        ttsReady: item?.ttsPlayback?.ready,
                        readyReason: item?.ttsPlayback?.readyReason,
                        isLocked: lockedTranslations.has(index),
                        sessionMode: textToSpeechSessionMode,
                    });
                }
                return eligible;
            });

        if (!textToSpeechEnabled || !transcriptData?.length) {
            readySections.forEach(({ item, index }) => {
                spokenTranslationKeysRef.current.add(getSectionSpeechKey(item, index));
            });
            return;
        }

        readySections.forEach(({ item, index }) => {
            const translatedText = item?.translatedText?.trim();
            if (!translatedText) return;
            const sectionId = item?.sectionId ?? index;
            const speechKey = getSectionSpeechKey(item, index);
            const targetLanguage = item?.ttsPlayback?.targetLanguage || translatedLanguage;

            if (shouldSkipTextToSpeechForSection(item)) {
                spokenTranslationKeysRef.current.add(speechKey);
                productionLogger.debug('[TTS]', {
                    context: 'ttsSkippedMic',
                    sectionId,
                    speechKey,
                    blockedReason: item?.ttsPlayback?.blockedReason || item?.desktopAudioSource || 'unknown',
                });
                return;
            }

            // Generate and play audio as soon as the translation freezes.
            const ttsAudioPromise = startTtsAudioRequest(speechKey, translatedText, targetLanguage, index);

            if (spokenTranslationKeysRef.current.has(speechKey)) return;

            spokenTranslationKeysRef.current.add(speechKey);
            productionLogger.debug('[TTS]', {
                context: 'ttsQueued',
                sectionId,
                speechKey,
            });
            ttsQueueRef.current = ttsQueueRef.current
                .catch(() => undefined)
                .then(async () => {
                    if (!textToSpeechEnabledRef.current) {
                        spokenTranslationKeysRef.current.delete(speechKey);
                        return;
                    }

                    const ttsAudio = await ttsAudioPromise;
                    if (!textToSpeechEnabledRef.current || !ttsAudio?.audioBase64) {
                        spokenTranslationKeysRef.current.delete(speechKey);
                        return;
                    }

                    // Match the same identity rule as sectionId / speech keys: server sectionId when present, else row index.
                    const rowIdentity = getTranscriptRowIdentity(item, index);
                    const latestSectionIndex = transcriptDataRef.current.findIndex(
                        (entry: any, i: number) => getTranscriptRowIdentity(entry, i) === rowIdentity
                    );
                    const latestSection = latestSectionIndex >= 0 ? transcriptDataRef.current[latestSectionIndex] : undefined;
                    const latestSpeechKey = latestSection
                        ? getSectionSpeechKey(latestSection, latestSectionIndex)
                        : null;
                    const latestIsReady = latestSection?.ttsPlayback?.ready === true;
                    const latestShouldSkip = latestSection ? shouldSkipTextToSpeechForSection(latestSection) : false;
                    const latestIndex = latestSectionIndex >= 0 ? latestSectionIndex : index;
                    const onlineMode = textToSpeechSessionModeRef.current === 'online';
                    const latestClientLocked = onlineMode
                        ? lockedTranslationsRef.current.has(latestIndex) || isTtsReadyImmediate(latestSection)
                        : true;

                    if (!latestSection || latestSpeechKey !== speechKey || !latestIsReady || latestShouldSkip || !latestClientLocked) {
                        productionLogger.debug('[TTS]', {
                            context: 'ttsDroppedAsStale',
                            sectionId,
                            speechKey,
                            latestSpeechKey,
                        });
                        spokenTranslationKeysRef.current.delete(speechKey);
                        return;
                    }

                    await new Promise<void>((resolve) => {
                        const audioBase64 = ttsAudio.audioBase64;
                        const mimeType = ttsAudio.mimeType || 'audio/mpeg';
                        let objectUrl: string | null = null;
                        try {
                            const binary = atob(audioBase64);
                            const len = binary.length;
                            const bytes = new Uint8Array(len);
                            for (let i = 0; i < len; i += 1) {
                                bytes[i] = binary.charCodeAt(i);
                            }
                            objectUrl = URL.createObjectURL(new Blob([bytes], { type: mimeType }));
                        } catch (e) {
                            productionLogger.error('[TTS] Failed to decode audio blob:', e);
                            spokenTranslationKeysRef.current.delete(speechKey);
                            resolve();
                            return;
                        }

                        const audio = new Audio(objectUrl);
                        audio.addEventListener('playing', () => warmTtsFetchesAfterRow(index), { once: true });
                        const revokeUrl = () => {
                            if (objectUrl) {
                                URL.revokeObjectURL(objectUrl);
                                objectUrl = null;
                            }
                        };

                        const startPlayback = async () => {
                            const sinkForRow = getAiPlaybackSinkIdForSection(latestSection);
                            // Route the processed stream, so boost also reaches headphones and virtual cables.
                            const outputAudio = new Audio();
                            const routedOutputDeviceId = await routeAudioToConfiguredOutput(
                                outputAudio, sectionId, speechKey, sinkForRow
                            );
                            if (!textToSpeechEnabledRef.current) {
                                revokeUrl();
                                resolve();
                                return;
                            }
                            const isIsolatedOutput = routedOutputDeviceId !== 'default';
                            let audioContext: AudioContext | null = null;
                            let volumeStage: ReturnType<typeof createTranslatedSpeechGain> | null = null;

                            activeAudioRef.current = audio;
                            audio.volume = 1;
                            try {
                                audioContext = new AudioContext();
                                activeAudioContextRef.current = audioContext;
                                const source = audioContext.createMediaElementSource(audio);
                                const direction = getSpeechVolumeDirection(
                                    resolveTranscriptPlaybackSource(latestSection?.desktopAudioSource, latestSection?.speakerName),
                                    speechToSpeechModeRef.current
                                );
                                // Retain the legacy default-output boost at the 100% setting.
                                volumeStage = createTranslatedSpeechGain(audioContext, direction, isIsolatedOutput ? 1 : 1.8);
                                source.connect(volumeStage.input);
                                if (isIsolatedOutput) {
                                    const destination = audioContext.createMediaStreamDestination();
                                    volumeStage.output.connect(destination);
                                    outputAudio.srcObject = destination.stream;
                                } else {
                                    volumeStage.output.connect(audioContext.destination);
                                }
                            } catch (error) {
                                productionLogger.error('Failed to configure translated speech volume:', error);
                                volumeStage?.dispose();
                                void audioContext?.close().catch(() => undefined);
                                if (activeAudioContextRef.current === audioContext) activeAudioContextRef.current = null;
                                if (activeAudioRef.current === audio) activeAudioRef.current = null;
                                revokeUrl();
                                spokenTranslationKeysRef.current.delete(speechKey);
                                resolve();
                                return;
                            }
                            emitTranslatedSpeechPlaybackState(true, { sectionId, speechKey, isIsolatedOutput });

                            let cleanedUp = false;
                            const cleanup = () => {
                                if (cleanedUp) return;
                                cleanedUp = true;
                                if (activePlaybackCleanupRef.current === cleanup) activePlaybackCleanupRef.current = null;
                                audio.pause();
                                if (activeAudioRef.current === audio) {
                                    activeAudioRef.current = null;
                                }
                                if (activeAudioContextRef.current === audioContext) {
                                    activeAudioContextRef.current = null;
                                }
                                emitTranslatedSpeechPlaybackState(false, { sectionId, speechKey, isIsolatedOutput });
                                volumeStage?.dispose();
                                outputAudio.pause();
                                outputAudio.srcObject = null;
                                audioContext?.close().catch(() => undefined);
                                revokeUrl();
                                resolve();
                            };
                            activePlaybackCleanupRef.current = cleanup;

                            audio.onended = () => {
                                cleanup();
                                resolve();
                            };
                            audio.onerror = () => {
                                cleanup();
                                resolve();
                            };
                            const play = async () => {
                                await audioContext?.resume();
                                if (cleanedUp) return;
                                if (isIsolatedOutput) await outputAudio.play();
                                if (cleanedUp) { outputAudio.pause(); return; }
                                await audio.play();
                            };
                            play().catch((error) => {
                                productionLogger.error('Failed to play translated speech:', error);
                                spokenTranslationKeysRef.current.delete(speechKey);
                                cleanup();
                                resolve();
                            });
                        };

                        void startPlayback();
                    });
                });
        });
    }, [lockedTranslations, startTtsAudioRequest, textToSpeechEnabled, textToSpeechSessionMode, speechToSpeechMode, transcriptData, translatedLanguage, ttsReevalTick, useRealtimeSpeechPlayback]);

    // Periodically check if any stable-window section has crossed the 5-second fallback threshold.
    // When one does, bump ttsReevalTick to force the playback effect to re-evaluate eligibility.
    useEffect(() => {
        if (!textToSpeechEnabled) return;

        const interval = setInterval(() => {
            const rows = transcriptDataRef.current || [];
            const now = Date.now();
            for (let i = 0; i < rows.length; i += 1) {
                const item = rows[i];
                if (item?.ttsPlayback?.ready !== true) continue;
                if (item?.ttsPlayback?.readyReason !== 'stable-window') continue;
                if (!item?.ttsPlayback?.readyAt) continue;

                const speechKey = getSectionSpeechKey(item, i);
                if (spokenTranslationKeysRef.current.has(speechKey)) continue;

                const dynamicFallbackMs = (item?.ttsPlayback?.stabilityMs ?? 2350) + 500;
                const readyAge = now - new Date(item.ttsPlayback.readyAt).getTime();
                if (readyAge >= dynamicFallbackMs - 500 && readyAge <= dynamicFallbackMs + 2000) {
                    // Section is crossing the threshold — force playback effect re-evaluation
                    setTtsReevalTick(c => c + 1);
                    break;
                }
            }
        }, 1000);

        return () => clearInterval(interval);
    }, [textToSpeechEnabled]);

    useEffect(() => {
        if (!textToSpeechEnabled) {
            activePlaybackCleanupRef.current?.();
            activeAudioRef.current?.pause();
            activeAudioRef.current = null;
            activeAudioContextRef.current?.close().catch(() => undefined);
            activeAudioContextRef.current = null;
            emitTranslatedSpeechPlaybackState(false);
            ttsAudioCacheRef.current.clear();
            ttsQueueRef.current = Promise.resolve();
        }
    }, [textToSpeechEnabled]);

    useEffect(() => {
        return () => {
            textToSpeechEnabledRef.current = false;
            activePlaybackCleanupRef.current?.();
            activeAudioRef.current?.pause();
            activeAudioRef.current = null;
            activeAudioContextRef.current?.close().catch(() => undefined);
            activeAudioContextRef.current = null;
            emitTranslatedSpeechPlaybackState(false);
            ttsAudioCacheRef.current.clear();
            ttsQueueRef.current = Promise.resolve();
        };
    }, []);

    // Reset translation timestamp when translation is updated
    useEffect(() => {
        if (!transcriptData || transcriptData.length === 0) return;

        const newTimestamps = new Map(translationTimestamps);
        let timestampsUpdated = false;

        transcriptData.forEach((item, index) => {
            if (item.translatedText && !lockedTranslations.has(index)) {
                // Reset timestamp if translation was updated (for quality time reset)
                const currentTime = new Date();
                const existingTimestamp = translationTimestamps.get(index);

                if (!existingTimestamp || item.translationUpdatedAt) {
                    newTimestamps.set(index, currentTime);
                    timestampsUpdated = true;
                }
            }
        });

        if (timestampsUpdated) {
            setTranslationTimestamps(newTimestamps);
        }
    }, [transcriptData?.map(item => item.translatedText).join('')]); // Remove dependencies that cause infinite loops

    const shareUrl = shareCode ? generateShareURL(shareCode) : '';
    const qrCodeUrl = shareUrl
        ? `https://api.qrserver.com/v1/create-qr-code/?size=140x140&data=${encodeURIComponent(shareUrl)}`
        : '';

    return (
        <div className="relative flex min-h-0 w-full flex-1 flex-col">
            {isMaximized && canShowQrCode && showQrCode && shareUrl && (
                <div className="absolute top-0 left-2 z-30 p-2 rounded-lg border border-[#555] bg-[#121212]/90">
                    <img src={qrCodeUrl} alt={t("transcript.shareQrAlt")} className="w-[110px] h-[110px] rounded-sm" />
                </div>
            )}
            {/* Real-time meeting memory chip — only shown when the server has populated
                meetingFlow AND the Settings → Real-time memory toggle is set to Show.
                The parent (AudioRecorder) passes "" when the toggle is No-show.
                Absolute-positioned overlay so it doesn't push the transcript down. */}
            {meetingFlow && meetingFlow.trim().length > 0 && (
                <div className="pointer-events-none absolute top-2 left-2 right-2 z-20">
                    <div className="pointer-events-auto rounded-lg border border-[#3B3EEB]/45 bg-[#15161E]/40 px-3 py-1.5 shadow-xl backdrop-blur-[1px]">
                        <div className="flex items-start gap-2">
                            <span className="text-[9px] font-semibold uppercase tracking-[0.18em] text-[#A3A8FF] mt-0.5 shrink-0">
                                {t("settings.realTimeMemory.title")}
                            </span>
                            <span dir="auto" className="text-[11px] leading-snug text-gray-200 break-words overflow-wrap-anywhere">
                                {meetingFlow}
                            </span>
                        </div>
                    </div>
                </div>
            )}
            {(
                // A. translation_limit_exceeded: Show paywall if translation is ON OR bullet points is ON (ignore if using shared translation)
                (!joined && translationLimitExceeded && (enabled || bulletPointEnabled)) ||
                // B. transcription_limit_exceeded: Show paywall if translation is OFF AND bullet points is OFF (ignore if using shared translation)
                (!joined && transcriptionLimitExceeded && !enabled && !bulletPointEnabled) ||
                // C. Both limits exceeded: Always show heavy user paywall message (ignore if using shared translation)
                (!joined && transcriptionLimitExceeded && translationLimitExceeded)
            ) && (
                    <div className="absolute top-0 left-0 right-0 z-10 flex flex-col items-center justify-start bg-[#242424] opacity-[.95] rounded-b-[10px] p-8">
                        <h2 className="text-[20px] font-semibold text-[#FBFBFB] mb-4">
                            {transcriptionLimitExceeded && translationLimitExceeded
                                ? t("transcript.limits.title.both")
                                : transcriptionLimitExceeded
                                    ? t("transcript.limits.title.transcription")
                                    : t("transcript.limits.title.translation")}
                        </h2>
                        <p className="text-[#FBFBFB] text-[16px] text-center mb-6">
                            {(transcriptionLimitExceeded && !translationLimitExceeded) && (
                                <span className="block mb-1">
                                    {t("transcript.limits.message.transcription.used")} {parseInt(transcriptionUsage.used.toString())} {t("transcript.limits.message.transcription.outOf")} {parseInt(transcriptionUsage.limit.toString())} {t("transcript.limits.message.transcription.minutes")} {t("transcript.limits.message.transcription.upgrade")}
                                </span>
                            )}
                            {(translationLimitExceeded && !transcriptionLimitExceeded) && (
                                <span className="block mb-1">
                                    {t("transcript.limits.message.translation.used")} {parseInt(translationUsage.used.toString())} {t("transcript.limits.message.translation.outOf")} {parseInt(translationUsage.limit.toString())} {t("transcript.limits.message.translation.minutes")} {t("transcript.limits.message.translation.upgrade")}
                                </span>
                            )}
                            {transcriptionLimitExceeded && translationLimitExceeded && (
                                <span className="block">
                                    {t("transcript.limits.message.both")}
                                </span>
                            )}
                        </p>
                        <div className="flex gap-4">
                            {
                                (transcriptionLimitExceeded && translationLimitExceeded) ? <UpgradeNow referrer={'desktop-translation-transcription'} /> : <>
                                    {
                                        transcriptionLimitExceeded ? <UpgradeNow referrer={'desktop-transcription'} /> : <UpgradeNow referrer={'desktop-translation'} />
                                    }
                                </>
                            }

                        </div>
                    </div>
                )}
            {reverseTranslationActive ? (
                // Split view mode
                <div className="-mx-3 flex min-h-0 flex-1 gap-2">
                    {/* Main translation side */}
                    <div className="flex min-h-0 flex-1 flex-col">
                        <div className="text-center py-2 bg-gradient-to-r from-[#5156FF] to-[#7C3AED] text-white rounded-md mb-2 text-sm font-medium">
                            {spokenLanguage} → {translatedLanguage}
                        </div>
                        <div
                            ref={primarySplitTranscriptScroll.containerRef}
                            className="custom-scrollbar min-h-0 flex-1 overflow-y-auto px-1 pb-1"
                        >
                            <div ref={primarySplitTranscriptScroll.contentRef}>
                                {transcriptDisplayGroups.map((group) => (
                                    <div
                                        key={group.key}
                                        className="desktop-transcript-segment mb-2 flex w-full flex-col items-start justify-start gap-1 rounded-lg p-2 last:mb-0"
                                    >
                                        {showLiveSpeakerNames && group.showSpeaker && (
                                            renderSpeakerName(group.speakerName)
                                        )}
                                        {renderTranscriptGroupEntries(group)}
                                    </div>
                                ))}
                            </div>
                        </div>
                    </div>

                    {/* Reverse translation side */}
                    <div className="flex min-h-0 flex-1 flex-col">
                        <div className="text-center py-2 bg-gradient-to-r from-[#7C3AED] to-[#5156FF] text-white rounded-md mb-2 text-sm font-medium">
                            {translatedLanguage} → {spokenLanguage}
                        </div>
                        <div
                            ref={reverseSplitTranscriptScroll.containerRef}
                            className="custom-scrollbar min-h-0 flex-1 overflow-y-auto px-1 pb-1"
                        >
                            <div ref={reverseSplitTranscriptScroll.contentRef}>
                                {reverseTranscriptData?.map((item, index) => {
                                    const previousSpeaker = index > 0 ? reverseTranscriptData[index - 1].speakerName : null;
                                    const speakerChanged = item.speakerName !== previousSpeaker;

                                    return (
                                        <div
                                            key={`reverse-${index}`}
                                            className="desktop-transcript-segment mb-2 flex w-full flex-col items-start justify-start gap-1 rounded-lg p-2 last:mb-0"
                                        >
                                            {showLiveSpeakerNames && speakerChanged && (
                                                <span className="block rounded-md border border-[#303036] bg-[#19191C] px-2 py-0.5 text-[13px] font-semibold text-[#D7D7DB]">
                                                    {item.speakerName}
                                                </span>
                                            )}
                                            <div className="w-full">
                                                {renderSectionSourceText(item)}
                                                {item.translatedText && (
                                                    <span
                                                        dir="auto"
                                                        className="text-gray-200 block whitespace-pre-line transition-colors duration-300 break-words overflow-wrap-anywhere w-full"
                                                        style={{ fontSize: `${Math.max(14, Math.round(20 * (fontSize / 50)))}px` }}
                                                    >
                                                        {formatTranscriptParagraphs(item.translatedText)}
                                                    </span>
                                                )}
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    </div>
                </div>
            ) : (
                // Single view mode (original)
                <div
                    ref={singleTranscriptScroll.containerRef}
                    className="custom-scrollbar -mx-3 min-h-0 flex-1 overflow-y-auto px-1 pb-1"
                >
                    <div ref={singleTranscriptScroll.contentRef}>
                        {showPreparing && (
                            <div className="sticky top-3 z-10 mt-3 mb-2 flex items-center justify-center gap-2.5 rounded-lg border border-[#3B3EEB]/40 bg-[#15161E]/95 px-3.5 py-2.5 shadow-lg backdrop-blur-sm animate-[fadeInDown_0.3s_ease-out]">
                                <span className="h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-[#6E73FF] border-t-transparent" />
                                <span className="text-[12px] font-medium text-[#D6DCFF] animate-pulse">
                                    {t("transcript.preparing")}
                                </span>
                            </div>
                        )}
                        {transcriptDisplayGroups.map((group) => (
                            <div
                                key={group.key}
                                className="desktop-transcript-segment mb-2 flex w-full flex-col items-start justify-start gap-1 rounded-lg p-2 last:mb-0"
                            >
                                {showLiveSpeakerNames && group.showSpeaker && (
                                    renderSpeakerName(group.speakerName)
                                )}
                                {renderTranscriptGroupEntries(group)}
                            </div>
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
};

export default Transcript;
