import { ArrowRight, Check, ChevronDown, LockKeyhole, Search, X } from "lucide-react";
import { createPortal } from "react-dom";
import { useEffect, useRef, useState, type MutableRefObject } from "react";
import type { Socket } from "socket.io-client";
import { useTranslation } from "../../hooks/useTranslation";
import { requestDesktopPricing } from "../../lib/desktopPricingNavigation";
import { useAppDispatch, useAppSelector } from "../../redux/hooks";
import { setBothModeUplinkTranslatedLanguage, setEnabled, setExtraTranslatedLanguages, setSpokenLanguage, setSpokenLanguageModel, setTranslatedLanguage, setTranslationMode, type TranslationMode } from "../../redux/slices/recorderSlice";
import type { SpeechToSpeechMode } from "../../utils/audioRoutingAutoConfig";
import {
    fastLanguagesMatch,
    pickDifferentFastLanguageValue,
    resolveFastLanguageKey,
} from "../../utils/realtimeTranslation/bothModeUplinkLanguage";
import { getLocalizedLanguageLabel, languages, microsoftSpokenLanguages, spokenLanguages } from "../../utils/languages";
import { WEB_URL } from "../../utils/WebApp";

type SpokenLanguagePlan = "basic" | "premium";
type TranslationModalTab = TranslationMode;

const DEFAULT_SPOKEN_LANGUAGE_PLAN: SpokenLanguagePlan = "basic";
const TRANSLATION_MODE_STORAGE_KEY = 'MeetingCopilot_translation_mode';
const SPOKEN_LANGUAGE_USAGE_KEY = 'MeetingCopilot_spoken_language_usage';

type SpokenLanguageUsageEntry = number | {
    count: number;
    lastSelectedAt: number;
};

// Per-plan local selection history; plans have different language sets.
const readSpokenLanguageUsage = (): Record<string, Record<string, SpokenLanguageUsageEntry>> => {
    try {
        return JSON.parse(localStorage.getItem(SPOKEN_LANGUAGE_USAGE_KEY) ?? '{}');
    } catch {
        return {};
    }
};

const recordSpokenLanguageUsage = (plan: SpokenLanguagePlan, languageKey: string) => {
    const usage = readSpokenLanguageUsage();
    const planUsage = usage[plan] ?? {};
    const previous = planUsage[languageKey];
    const previousCount = typeof previous === 'number' ? previous : previous?.count ?? 0;
    planUsage[languageKey] = {
        count: previousCount + 1,
        lastSelectedAt: Date.now(),
    };
    usage[plan] = planUsage;
    localStorage.setItem(SPOKEN_LANGUAGE_USAGE_KEY, JSON.stringify(usage));
};
type SpokenLanguageOption = typeof spokenLanguages[number];
const FAST_TRANSLATION_LANGUAGE_KEYS = [
    'zh-CN',
    'en',
    'fr',
    'de',
    'hi',
    'id',
    'it',
    'ja',
    'ko',
    'pt',
    'ru',
    'es',
    'vi',
] as const;

// The combined Spoken Language / Translation pill plus the Language settings
// modal. Shared between the live session (Controllers) and the home top bar.
const LanguageSettingsPill = ({
    saveStateAndReload,
    socketRef,
    isRecording = false,
    paused = false,
    speechToSpeechMode = 'off',
    openRequestId,
}: {
    /** Recorder reload for spoken-language changes (live session only). */
    saveStateAndReload?: (newSpokenLanguage: string, model: string) => void,
    socketRef?: MutableRefObject<Socket | null>,
    isRecording?: boolean,
    paused?: boolean,
    speechToSpeechMode?: SpeechToSpeechMode,
    /** Opens the full language dialog once for a calendar launch request. */
    openRequestId?: number,
}) => {
    const {
        spokenLanguage,
        translatedLanguage,
        bothModeUplinkTranslatedLanguage,
        extraTranslatedLanguages,
        enabled,
        spokenLanguageModel,
        bulletPointEnabled: reduxBulletPointEnabled,
        translationMode,
    } = useAppSelector((state) => state.recorder);
    const { clients, joined, hostSpokenLanguagePlan, hostTranslationMode } = useAppSelector(state => state.sharing)
    const { user } = useAppSelector(state => state.user);
    const isPremiumOrAbove = !!user?.currentTier && user.currentTier !== 'Freemium' && user.currentTier !== 'Pro';
    const dispatch = useAppDispatch();
    const localConfig = localStorage.getItem('MeetingCopilot_config')
    const [showDisabled, setShowDisabled] = useState(false);
    const [showSpokenLanguageModal, setShowSpokenLanguageModal] = useState(false);
    const [showTranslationLanguageModal, setShowTranslationLanguageModal] = useState(false);
    const [languageSettingsFocus, setLanguageSettingsFocus] = useState<'spoken' | 'translation'>('spoken');
    const [translationModalTab, setTranslationModalTab] = useState<TranslationModalTab>(() => {
        const storedMode = localStorage.getItem(TRANSLATION_MODE_STORAGE_KEY);
        return storedMode === 'fast' || storedMode === 'multi' || storedMode === 'single' ? storedMode : 'single';
    });
    const [multilingualSearch, setMultilingualSearch] = useState("");
    const [fastTranslationSearch, setFastTranslationSearch] = useState("");
    const [fastUplinkTranslationSearch, setFastUplinkTranslationSearch] = useState("");
    const [_fastLanguagePicker, setFastLanguagePicker] = useState<'incoming' | 'uplink'>('incoming');
    const [spokenLanguageSearch, setSpokenLanguageSearch] = useState("");
    const [translatedLanguageSearch, setTranslatedLanguageSearch] = useState("");
    const [selectedSpokenLanguagePlan, setSelectedSpokenLanguagePlan] = useState<SpokenLanguagePlan>(DEFAULT_SPOKEN_LANGUAGE_PLAN);
    const [activeSpokenLanguagePlan, setActiveSpokenLanguagePlan] = useState<SpokenLanguagePlan>(DEFAULT_SPOKEN_LANGUAGE_PLAN);
    const spokenLanguageSearchInputRef = useRef<HTMLInputElement>(null);
    const translatedLanguageSearchInputRef = useRef<HTMLInputElement>(null);
    // Spoken-language picks reload the recorder; defer that until the modal
    // closes so the user can also pick a translation output first.
    const pendingSpokenReloadRef = useRef<{ key: string; model: string } | null>(null);
    const handledOpenRequestRef = useRef<number | null>(null);

    const isSharedGuestPlanLocked = joined && hostSpokenLanguagePlan != null;
    const isSharedGuestFastLocked = joined && hostTranslationMode === 'fast';
    const { t, currentLanguage } = useTranslation();
    const isSessionActive = paused || isRecording;

    useEffect(() => {
        const storedMode = localStorage.getItem(TRANSLATION_MODE_STORAGE_KEY);
        const mode = storedMode === 'fast' || storedMode === 'multi' || storedMode === 'single' ? storedMode : 'single';
        dispatch(setTranslationMode(mode as TranslationMode));
        setTranslationModalTab(mode as TranslationModalTab);
    }, [dispatch]);

    useEffect(() => {
        setTranslationModalTab(translationMode as TranslationModalTab);
    }, [translationMode]);

    // Shared code-join guest must mirror host translation mode.
    useEffect(() => {
        if (!joined || hostTranslationMode == null) return;
        const hasActiveLocalMeeting = isRecording || paused;
        if (hasActiveLocalMeeting) return;
        if (hostTranslationMode === 'fast') {
            if (translationModalTab !== 'fast') {
                updateTranslationModalTab('fast');
            } else if (!enabled) {
                dispatch(setEnabled(true));
            }
            return;
        }
        if (translationModalTab === 'fast') {
            updateTranslationModalTab('single');
            clearExtraTranslatedLanguages();
        }
    }, [joined, hostTranslationMode, translationModalTab, isRecording, paused, enabled]);

    // Fast mode stored from a prior session still needs translation enabled for the pipeline.
    useEffect(() => {
        if (translationMode !== 'fast' || enabled) return;
        dispatch(setEnabled(true));
        const cfgRaw = localStorage.getItem('MeetingCopilot_config');
        const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
        localStorage.setItem('MeetingCopilot_config', JSON.stringify({ ...cfg, translationEnabled: true }));
    }, [translationMode, enabled, dispatch]);

    useEffect(() => {
        if (localConfig) {
            const config = JSON.parse(localConfig)
            const translationEnabled = config.translationEnabled ?? true
            dispatch(setEnabled(translationEnabled))
            dispatch(setTranslatedLanguage(config.translatedLanguage ?? 'English'))
            dispatch(setBothModeUplinkTranslatedLanguage(config.bothModeUplinkTranslatedLanguage ?? 'English'))
            dispatch(setSpokenLanguage(config.spokenLanguage ?? (translationEnabled ? 'ja' : 'en')))
            dispatch(setSpokenLanguageModel(config.spokenLanguageModel ?? 'nova-3'))
            setSelectedSpokenLanguagePlan(config.spokenLanguagePlan ?? DEFAULT_SPOKEN_LANGUAGE_PLAN)
            setActiveSpokenLanguagePlan(config.spokenLanguagePlan ?? DEFAULT_SPOKEN_LANGUAGE_PLAN)
        }
    }, [localConfig])

    useEffect(() => {
        if (!showSpokenLanguageModal || languageSettingsFocus !== 'spoken') return;

        const frame = window.requestAnimationFrame(() => {
            spokenLanguageSearchInputRef.current?.focus();
            spokenLanguageSearchInputRef.current?.select();
        });

        return () => window.cancelAnimationFrame(frame);
    }, [showSpokenLanguageModal, activeSpokenLanguagePlan, languageSettingsFocus]);

    useEffect(() => {
        if (!showTranslationLanguageModal || languageSettingsFocus !== 'translation') return;

        const frame = window.requestAnimationFrame(() => {
            if (translationModalTab === 'single') {
                translatedLanguageSearchInputRef.current?.focus();
                translatedLanguageSearchInputRef.current?.select();
            }
        });

        return () => window.cancelAnimationFrame(frame);
    }, [showTranslationLanguageModal, translationModalTab, languageSettingsFocus]);

    const allSpokenLanguages = [...spokenLanguages, ...microsoftSpokenLanguages];
    const currentSpokenLanguageOption = allSpokenLanguages.find((language) => language.key === spokenLanguage);
    const currentTranslatedLanguageOption = languages.find((language) => language.value === translatedLanguage);
    const fastTranslationLanguages = FAST_TRANSLATION_LANGUAGE_KEYS
        .map((key) => languages.find((language) => language.key === key))
        .filter((language): language is typeof languages[number] => Boolean(language));
    const fastTranslatedLanguageKey = resolveFastLanguageKey(translatedLanguage, languages);
    const fastUplinkTranslatedLanguageKey = resolveFastLanguageKey(bothModeUplinkTranslatedLanguage, languages);
    const currentFastTranslatedLanguageOption = fastTranslationLanguages.find((language) => language.key === fastTranslatedLanguageKey);
    const currentFastUplinkLanguageOption = fastTranslationLanguages.find((language) => language.key === fastUplinkTranslatedLanguageKey);
    const showBothModeUplinkLanguage = translationMode === 'fast' && speechToSpeechMode === 'online-both';
    const bothModeLanguagesMatch = showBothModeUplinkLanguage
        && fastLanguagesMatch(translatedLanguage, bothModeUplinkTranslatedLanguage, languages);

    const persistMeetingCopilotConfig = (patch: Record<string, unknown>) => {
        const cfgRaw = localStorage.getItem('MeetingCopilot_config');
        const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
        localStorage.setItem('MeetingCopilot_config', JSON.stringify({
            ...cfg,
            translationEnabled: enabled,
            spokenLanguage,
            spokenLanguageModel,
            translatedLanguage,
            bothModeUplinkTranslatedLanguage,
            bulletPointEnabled: reduxBulletPointEnabled,
            spokenLanguagePlan: selectedSpokenLanguagePlan,
            ...patch,
        }));
    };

    useEffect(() => {
        if (!showBothModeUplinkLanguage) return;
        if (
            !bothModeUplinkTranslatedLanguage
            || fastLanguagesMatch(translatedLanguage, bothModeUplinkTranslatedLanguage, languages)
        ) {
            const next = pickDifferentFastLanguageValue(translatedLanguage, fastTranslationLanguages);
            if (next !== bothModeUplinkTranslatedLanguage) {
                dispatch(setBothModeUplinkTranslatedLanguage(next));
                persistMeetingCopilotConfig({ bothModeUplinkTranslatedLanguage: next });
            }
        }
    }, [showBothModeUplinkLanguage, translatedLanguage]);
    const modalSpokenPlan: SpokenLanguagePlan =
        isSharedGuestPlanLocked && hostSpokenLanguagePlan ? hostSpokenLanguagePlan : activeSpokenLanguagePlan;
    const availableSpokenLanguages = (modalSpokenPlan === "basic"
        ? spokenLanguages
        : microsoftSpokenLanguages
    ).slice().sort((a, b) => {
        if (a.key === "multi") return -1;
        if (b.key === "multi") return 1;
        return a.value.localeCompare(b.value);
    });

    const getSpokenLanguageDisplayLabel = (language: typeof allSpokenLanguages[number] | undefined, plan: SpokenLanguagePlan) => {
        const translatedValue = getLocalizedLanguageLabel(language || { key: spokenLanguage, value: spokenLanguage }, currentLanguage);
        const shortValue = translatedValue
            .replace("United Arab Emirates", "UAE")
            .replace("Saudi Arabia", "Saudi")
            .replace("Latin America", "LatAm")
            .replace("New Zealand", "NZ")
            .replace("South Korea", "Korea")
            .replace("Chinese (Mandarin, Simplified – Mainland China)", "Chinese (Simplified, Mainland)")
            .replace("Chinese (Mandarin, Simplified – Simplified Script)", "Chinese (Simplified Script)")
            .replace("Chinese (Mandarin, Traditional – Taiwan)", "Chinese (Traditional, Taiwan)")
            .replace("Chinese (Mandarin, Traditional – Traditional Script)", "Chinese (Traditional Script)")
            .replace("Chinese (Mandarin, Simplified)", "Chinese (Mandarin)")
            .replace("Cantonese (Traditional)", "Cantonese (HK)");
        const prefix = plan === "basic" ? "(B)" : "(P)";
        return `${prefix} ${shortValue}`;
    };

    const getTranslatedLanguageLabel = (language: typeof languages[number] | undefined) => {
        return getLocalizedLanguageLabel(language || { key: translatedLanguage, value: translatedLanguage }, currentLanguage);
    };

    const languageSearchPlaceholder = t("languageSettings.searchPlaceholder");
    const noLanguageFoundText = t("languageSettings.noResults");

    // Three most recently selected languages for the active plan.
    const spokenPlanUsage = readSpokenLanguageUsage()[modalSpokenPlan] ?? {};
    const recentlyUsedSpokenLanguages = Object.entries(spokenPlanUsage)
        .sort(([, left], [, right]) => {
            const leftCount = typeof left === 'number' ? left : left.count;
            const rightCount = typeof right === 'number' ? right : right.count;
            const leftLastSelectedAt = typeof left === 'number' ? 0 : left.lastSelectedAt;
            const rightLastSelectedAt = typeof right === 'number' ? 0 : right.lastSelectedAt;
            return rightLastSelectedAt - leftLastSelectedAt || rightCount - leftCount;
        })
        .map(([key]) => availableSpokenLanguages.find((language) => language.key === key))
        .filter((language): language is SpokenLanguageOption => Boolean(language))
        .slice(0, 3);

    const normalizedSpokenLanguageSearch = spokenLanguageSearch.trim().toLowerCase();
    const filteredSpokenLanguages = availableSpokenLanguages.filter((language) => {
        if (!normalizedSpokenLanguageSearch) return true;

        const localizedValue = getLocalizedLanguageLabel(language, currentLanguage);

        return [
            localizedValue,
            language.value,
            language.key,
        ].some((candidate) => candidate.toLowerCase().includes(normalizedSpokenLanguageSearch));
    });
    const normalizedTranslatedLanguageSearch = translatedLanguageSearch.trim().toLowerCase();
    const filteredTranslatedLanguages = languages.filter((language) => {
        if (!normalizedTranslatedLanguageSearch) return true;

        const localizedValue = getTranslatedLanguageLabel(language);

        return [
            localizedValue,
            language.value,
            language.key,
        ].some((candidate) => candidate.toLowerCase().includes(normalizedTranslatedLanguageSearch));
    });
    const normalizedFastTranslationSearch = fastTranslationSearch.trim().toLowerCase();
    const filteredFastTranslationLanguages = fastTranslationLanguages.filter((language) => {
        if (!normalizedFastTranslationSearch) return true;

        const localizedValue = getTranslatedLanguageLabel(language);

        return [
            localizedValue,
            language.value,
            language.key,
        ].some((candidate) => candidate.toLowerCase().includes(normalizedFastTranslationSearch));
    });
    const normalizedFastUplinkTranslationSearch = fastUplinkTranslationSearch.trim().toLowerCase();
    const filteredFastUplinkTranslationLanguages = fastTranslationLanguages.filter((language) => {
        if (!normalizedFastUplinkTranslationSearch) return true;
        const localizedValue = getTranslatedLanguageLabel(language);
        return [
            localizedValue,
            language.value,
            language.key,
        ].some((candidate) => candidate.toLowerCase().includes(normalizedFastUplinkTranslationSearch));
    });

    const clearExtraTranslatedLanguages = () => {
        if (extraTranslatedLanguages.length > 0) {
            dispatch(setExtraTranslatedLanguages([]));
        }
        const cfgRaw = localStorage.getItem('MeetingCopilot_config');
        const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
        localStorage.setItem('MeetingCopilot_config', JSON.stringify({ ...cfg, extraTranslatedLanguages: [] }));
    };

    const openLanguageSettings = (focus: 'spoken' | 'translation') => {
        setLanguageSettingsFocus(focus);
        setShowSpokenLanguageModal(true);
        setShowTranslationLanguageModal(true);
    };

    const closeLanguageSettings = () => {
        setShowSpokenLanguageModal(false);
        setShowTranslationLanguageModal(false);
        const pending = pendingSpokenReloadRef.current;
        if (pending) {
            pendingSpokenReloadRef.current = null;
            saveStateAndReload?.(pending.key, pending.model);
        }
    };

    const openMultilingualPricing = () => {
        closeLanguageSettings();
        if (requestDesktopPricing("multilingual-translation")) return;
        window.ipc?.send("open-link", `${WEB_URL}/pricing?referrer=multilingual-translation`);
    };

    useEffect(() => {
        if (openRequestId == null || handledOpenRequestRef.current === openRequestId) return;
        handledOpenRequestRef.current = openRequestId;
        setLanguageSettingsFocus('spoken');
        setSpokenLanguageSearch("");
        setTranslatedLanguageSearch("");
        setShowSpokenLanguageModal(true);
        setShowTranslationLanguageModal(true);
    }, [openRequestId]);

    const updateTranslationModalTab = (mode: TranslationModalTab) => {
        setTranslationModalTab(mode);
        dispatch(setTranslationMode(mode));
        localStorage.setItem(TRANSLATION_MODE_STORAGE_KEY, mode);
        window.dispatchEvent(new CustomEvent('translationModeChanged', { detail: mode }));

        const nextTranslationEnabled = mode === 'fast' ? true : enabled;

        // Fast mode requires translation ON for the realtime pipeline.
        if (mode === 'fast' && !enabled) {
            dispatch(setEnabled(true));
            const cfgRaw = localStorage.getItem('MeetingCopilot_config');
            const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
            localStorage.setItem('MeetingCopilot_config', JSON.stringify({ ...cfg, translationEnabled: true }));
        }

        if (socketRef?.current?.connected) {
            socketRef.current.emit('update_config', {
                translatedLanguage,
                translationEnabled: nextTranslationEnabled,
                bulletPointEnabled: reduxBulletPointEnabled,
                translationMode: mode,
            });
        }
    };

    const handleTranslationToggle = () => {
        if (clients.length !== 0) return;

        const newTranslationState = !enabled;
        // Fast (Audio) requires translation ON, so turning it off exits fast
        // mode and returns to the regular Basic/Premium model.
        if (!newTranslationState && translationMode === 'fast') {
            updateTranslationModalTab('single');
            clearExtraTranslatedLanguages();
        }

        localStorage.setItem('MeetingCopilot_config', JSON.stringify({
            translationEnabled: newTranslationState,
            spokenLanguage,
            spokenLanguageModel,
            translatedLanguage,
            bulletPointEnabled: reduxBulletPointEnabled,
            spokenLanguagePlan: selectedSpokenLanguagePlan,
        }));
        dispatch(setEnabled(newTranslationState));
    };

    // Spoken-language plan now also hosts the Fast (Audio) option. Picking Basic/Premium
    // leaves fast mode (back to single-language translation); picking Fast (Audio) enters it.
    const isFastPlanActive = translationMode === 'fast';
    // Keep source and target language visible as one configuration even while
    // translation is off; the switch communicates runtime state on its own.
    const showTranslationColumn = true;
    const isFastAudioPlanLocked = isSessionActive || (joined && !isSharedGuestFastLocked);

    const handleSelectSpokenPlan = (plan: SpokenLanguagePlan) => {
        if (translationMode === 'fast') {
            updateTranslationModalTab('single');
            clearExtraTranslatedLanguages();
        }
        setActiveSpokenLanguagePlan(plan);
    };

    const handleSelectFastAudioPlan = () => {
        if (isFastAudioPlanLocked) return;
        updateTranslationModalTab('fast');
        clearExtraTranslatedLanguages();
    };

    const handleFastTranslatedLanguageSelection = (language: typeof languages[number]) => {
        let nextUplinkLanguage = bothModeUplinkTranslatedLanguage;
        persistMeetingCopilotConfig({
            translatedLanguage: language.value,
            extraTranslatedLanguages: [],
        });
        if (extraTranslatedLanguages.length > 0) {
            dispatch(setExtraTranslatedLanguages([]));
        }
        dispatch(setTranslatedLanguage(language.value));
        if (
            showBothModeUplinkLanguage
            && fastLanguagesMatch(language.value, bothModeUplinkTranslatedLanguage, languages)
        ) {
            const nextUplink = pickDifferentFastLanguageValue(language.value, fastTranslationLanguages);
            nextUplinkLanguage = nextUplink;
            dispatch(setBothModeUplinkTranslatedLanguage(nextUplink));
            persistMeetingCopilotConfig({ bothModeUplinkTranslatedLanguage: nextUplink });
        }
        setFastTranslationSearch("");

        socketRef?.current?.emit('update_config', {
            translatedLanguage: language.value,
            bothModeUplinkTranslatedLanguage: nextUplinkLanguage,
            translationEnabled: true,
            bulletPointEnabled: reduxBulletPointEnabled,
            translationMode: 'fast',
        });
    };

    const handleBothModeUplinkLanguageSelection = (language: typeof languages[number]) => {
        persistMeetingCopilotConfig({ bothModeUplinkTranslatedLanguage: language.value });
        dispatch(setBothModeUplinkTranslatedLanguage(language.value));
        setFastUplinkTranslationSearch("");

        socketRef?.current?.emit('update_config', {
            translatedLanguage,
            bothModeUplinkTranslatedLanguage: language.value,
            translationEnabled: true,
            bulletPointEnabled: reduxBulletPointEnabled,
            translationMode: 'fast',
        });
    };

    const handleSpokenLanguageSelection = (language: SpokenLanguageOption, plan: SpokenLanguagePlan) => {
        localStorage.setItem('MeetingCopilot_config', JSON.stringify({
            translationEnabled: enabled,
            spokenLanguage: language.key,
            spokenLanguageModel: language.model,
            translatedLanguage: translatedLanguage,
            bulletPointEnabled: reduxBulletPointEnabled,
            spokenLanguagePlan: plan
        }))
        setSelectedSpokenLanguagePlan(plan);
        setActiveSpokenLanguagePlan(plan);
        window.dispatchEvent(new CustomEvent('spokenLanguagePlanChanged', { detail: plan }));
        setSpokenLanguageSearch("");
        recordSpokenLanguageUsage(plan, language.key);
        // Update the selection immediately for UI feedback; the actual recorder
        // reload waits until the user closes the modal.
        dispatch(setSpokenLanguage(language.key));
        dispatch(setSpokenLanguageModel(language.model));
        pendingSpokenReloadRef.current = { key: language.key, model: language.model };
    };

    const handleTranslatedLanguageSelection = (language: typeof languages[number]) => {
        // Picking from the "Normal Translation" tab enforces single-language mode:
        // clear any previously-selected extras so the user gets exactly what the tab name implies.
        localStorage.setItem('MeetingCopilot_config', JSON.stringify({
            translationEnabled: enabled,
            spokenLanguage,
            spokenLanguageModel,
            translatedLanguage: language.value,
            extraTranslatedLanguages: [],
            bulletPointEnabled: reduxBulletPointEnabled,
            spokenLanguagePlan: selectedSpokenLanguagePlan
        }))
        if (extraTranslatedLanguages.length > 0) {
            dispatch(setExtraTranslatedLanguages([]));
        }
        dispatch(setTranslatedLanguage(language.value))
        setTranslatedLanguageSearch("");
    };

    const renderSpokenLanguageOption = (language: SpokenLanguageOption, keyPrefix = "") => {
        const isSelected =
            language.key === spokenLanguage && selectedSpokenLanguagePlan === modalSpokenPlan;

        return (
            <button
                type="button"
                key={`${keyPrefix}${language.key}`}
                onClick={() => handleSpokenLanguageSelection(language, modalSpokenPlan)}
                className={`mb-1.5 flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left transition-all duration-200 ${isSelected
                    ? 'bg-[#484dff] text-white'
                    : 'bg-[#1A1A1A] text-white hover:bg-[#232323]'
                    }`}
            >
                <span className="min-w-0 flex-1 break-words text-[13px] leading-snug">
                    {getLocalizedLanguageLabel(language, currentLanguage)}
                </span>
                {isSelected && <Check className="h-4 w-4 shrink-0 text-white" />}
            </button>
        );
    };

    const fullToolbarSpokenLanguageLabel = translationModalTab === 'fast'
        ? t("controllers.translationModes.fastSpokenLanguageLabel")
        : getSpokenLanguageDisplayLabel(currentSpokenLanguageOption, selectedSpokenLanguagePlan);
    const toolbarSpokenLanguageLabel = fullToolbarSpokenLanguageLabel;
    const toolbarTranslatedLanguageLabel = translationModalTab === 'fast'
        ? getTranslatedLanguageLabel(currentFastTranslatedLanguageOption)
        : extraTranslatedLanguages.length > 0
        ? `(${1 + extraTranslatedLanguages.length}) ${t("controllers.multilingualButton")}`
        : getTranslatedLanguageLabel(currentTranslatedLanguageOption);
    const toolbarSpokenHeaderLabel = enabled
        ? t("controllers.spokenText")
        : t("controllers.transcribingText");
    const toolbarTranslationHeaderLabel = t("controllers.translation");

    return (
        <>
            <div
                className={`live-language-pill ${showBothModeUplinkLanguage ? 'h-[66px]' : 'h-[52px]'} relative ml-auto inline-flex w-fit max-w-full min-w-0 items-stretch overflow-visible rounded-[14px] border border-[#34343B] bg-[#17171A] p-1 shadow-[0_4px_14px_rgba(0,0,0,0.2)] transition-[background-color,border-color,box-shadow,width] duration-150 hover:border-[#484850] hover:bg-[#202024] hover:shadow-[0_6px_18px_rgba(0,0,0,0.26)]`}
            >
                <div onMouseEnter={() => {
                    if (paused || isRecording) {
                        setShowDisabled(true)
                    }
                }} onMouseLeave={() => {
                    if (paused || isRecording) {
                        setShowDisabled(false)
                    }
                }} className="relative flex w-fit min-w-0 max-w-[240px] items-stretch">
                    <button
                        type="button"
                        disabled={paused || isRecording}
                        onClick={() => {
                            if (!paused && !isRecording) {
                                if (isSharedGuestPlanLocked && hostSpokenLanguagePlan) {
                                    setActiveSpokenLanguagePlan(hostSpokenLanguagePlan);
                                } else {
                                    setActiveSpokenLanguagePlan(selectedSpokenLanguagePlan);
                                }
                                setSpokenLanguageSearch("");
                                openLanguageSettings('spoken');
                            }
                        }}
                        className="live-language-pill__segment flex w-full min-w-0 max-w-full items-center overflow-hidden rounded-[10px] px-1 py-1 text-right focus:outline-none focus-visible:ring-2 focus-visible:ring-[#7377FF] disabled:cursor-not-allowed"
                    >
                        <span className="grid w-full min-w-0 max-w-full grid-rows-[16px_18px] content-center">
                            {/* Both rows truncate: in a narrow window the header
                                label would otherwise spill over the translation
                                segment instead of clipping. */}
                            <span className="live-language-pill__label block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-right text-[10px] font-semibold uppercase leading-4 tracking-normal text-[#8D8D98]">
                                {toolbarSpokenHeaderLabel}
                            </span>
                            <span
                                title={fullToolbarSpokenLanguageLabel}
                                className="live-language-pill__value flex h-[18px] min-w-0 items-center justify-end text-[12px] font-semibold leading-[18px] text-[#F5F5F7]"
                            >
                                <span className="block w-full min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-right">
                                    {toolbarSpokenLanguageLabel}
                                </span>
                            </span>
                        </span>
                    </button>
                    {
                        showDisabled && (
                            <div className="absolute rounded-lg px-2 py-1 bottom-[-83px] right-0 w-[200px] opacity-100 h-[80px] bg-[#232323] flex justify-center items-center z-[3000]">
                                <p className="text-white text-[12px] leading-[16px]">{t("controllers.disabled")}</p>
                            </div>
                        )
                    }
                </div>
                {enabled && (
                    <div className="relative z-10 flex w-[18px] shrink-0 items-center justify-center">
                        <span className="live-language-pill__direction flex items-center justify-center text-[#8F92FF]" aria-hidden="true">
                            <ArrowRight className="h-4 w-4" strokeWidth={2} />
                        </span>
                    </div>
                )}
                <div className={`live-language-pill__target grid min-w-0 content-center overflow-hidden rounded-[10px] px-1 py-1 w-max max-w-[220px] ${showBothModeUplinkLanguage ? 'grid-rows-[16px_18px_16px]' : 'grid-rows-[16px_18px]'}`}>
                    <div className="flex min-w-0 items-center justify-between gap-1">
                        <span className="live-language-pill__label block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-[10px] font-semibold uppercase leading-4 tracking-[0.035em] text-[#8D8D98]">
                            {toolbarTranslationHeaderLabel}
                        </span>
                        <button
                            type="button"
                            role="switch"
                            aria-checked={enabled}
                            aria-label={`${t("controllers.translation")} ${enabled ? t('controllers.toggle.on') : t('controllers.toggle.off')}`}
                            disabled={clients.length > 0}
                            className={`live-language-pill__switch relative h-[14px] w-[22px] shrink-0 rounded-full border transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-[#7377FF] disabled:cursor-not-allowed disabled:opacity-50 ${enabled ? "border-[#5B60FF] bg-[#5156FF]" : "border-[#52525B] bg-[#35353B]"}`}
                            onClick={handleTranslationToggle}
                        >
                            <span className={`absolute left-0 top-[2px] h-2 w-2 rounded-full bg-white shadow-sm transition-transform duration-200 ${enabled ? 'translate-x-[11px]' : 'translate-x-[2px]'}`} />
                        </button>
                    </div>
                    <button
                        type="button"
                        onClick={(event) => {
                            event.stopPropagation();
                            updateTranslationModalTab(extraTranslatedLanguages.length > 0 ? 'multi' : translationModalTab === 'fast' ? 'fast' : 'single');
                            setFastLanguagePicker('incoming');
                            setMultilingualSearch("");
                            setFastTranslationSearch("");
                            setTranslatedLanguageSearch("");
                            openLanguageSettings('translation');
                        }}
                        title={enabled
                            ? translationModalTab === 'fast'
                                ? getTranslatedLanguageLabel(currentFastTranslatedLanguageOption)
                                : getTranslatedLanguageLabel(currentTranslatedLanguageOption)
                            : undefined}
                        aria-label={enabled ? undefined : t("languageSettings.open")}
                        className={`live-language-pill__value relative flex h-[18px] w-full min-w-0 items-center whitespace-nowrap pr-4 text-left text-[12px] font-semibold leading-[18px] focus:outline-none focus-visible:underline ${enabled ? 'text-[#F5F5F7]' : 'live-language-pill__value--disabled text-[#777781]'}`}
                    >
                        {enabled ? (translationModalTab === 'fast' && showBothModeUplinkLanguage ? (
                            <span className="flex min-w-0 items-center gap-1 overflow-hidden text-[11px] leading-tight">
                                <span className="shrink-0">{t("controllers.translationModes.bothModeIncomingToolbar")}:</span>
                                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-bold">
                                    {getTranslatedLanguageLabel(currentFastTranslatedLanguageOption)}
                                </span>
                            </span>
                        ) : (
                            <span className="block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-bold">
                                {toolbarTranslatedLanguageLabel}
                            </span>
                        )) : t('controllers.toggle.off')}
                    </button>
                    {showBothModeUplinkLanguage && (
                        <div className="relative block min-w-0 w-full">
                            <button
                                type="button"
                                onClick={(event) => {
                                    event.stopPropagation();
                                    setFastLanguagePicker('uplink');
                                    setFastUplinkTranslationSearch("");
                                    setFastTranslationSearch("");
                                    openLanguageSettings('translation');
                                }}
                                className="live-language-pill__value relative block w-full min-w-0 overflow-hidden whitespace-nowrap text-left text-[11px] text-[#F5F5F7] focus:outline-none focus-visible:underline"
                            >
                                <span className="flex min-w-0 items-center gap-1 overflow-hidden leading-tight">
                                    <span className="shrink-0">{t("controllers.translationModes.bothModeUplinkToolbar")}:</span>
                                    <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-bold">
                                        {getTranslatedLanguageLabel(currentFastUplinkLanguageOption)}
                                    </span>
                                </span>
                            </button>
                        </div>
                    )}
                </div>
                <button
                    type="button"
                    aria-label={t("languageSettings.open")}
                    onClick={() => {
                        updateTranslationModalTab(extraTranslatedLanguages.length > 0 ? 'multi' : translationModalTab === 'fast' ? 'fast' : 'single');
                        setFastLanguagePicker('incoming');
                        setMultilingualSearch("");
                        setFastTranslationSearch("");
                        setTranslatedLanguageSearch("");
                        openLanguageSettings('translation');
                    }}
                    className="live-language-pill__chevron absolute bottom-1 right-1 flex h-5 w-5 items-center justify-center rounded-lg text-[#83858F] transition-colors hover:text-[#B8B9C2] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#7377FF]"
                >
                    <ChevronDown className="h-3.5 w-3.5" strokeWidth={2} />
                </button>
            </div>
            {showSpokenLanguageModal && typeof document !== "undefined" && createPortal(
                <div
                    className="language-settings-backdrop fixed inset-0 z-[4000] flex items-center justify-center bg-black/70 p-4"
                    style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
                    onClick={closeLanguageSettings}
                >
                    <div
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="language-settings-title"
                        className={`language-settings-dialog flex w-full max-w-[720px] flex-col overflow-hidden rounded-[20px] border border-[#404040] bg-[#2A2A2A] shadow-2xl ${isFastPlanActive
                            ? 'h-auto max-h-[calc(100dvh-2rem)]'
                            : 'h-[calc(100dvh-2rem)] max-h-[720px]'}`}
                        onClick={(e) => e.stopPropagation()}
                    >
                    <div className="shrink-0 px-4 pb-1 pt-3.5">
                        <div className="flex items-start justify-between gap-4">
                            <div>
                                <h2 id="language-settings-title" className="text-lg font-semibold text-white">{t("languageSettings.title")}</h2>
                                <p className="mt-2 text-[10px] font-medium uppercase tracking-[0.28em] text-gray-300">
                                    {t("languageSettings.selectModel")}
                                </p>
                            </div>
                            <button
                                type="button"
                                onClick={closeLanguageSettings}
                                className="relative z-10 -mr-2 -mt-2 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-gray-300 transition-colors duration-200 hover:bg-[#1A1A1A] hover:text-white"
                                style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
                                aria-label={t("languageSettings.close")}
                            >
                                <X className="h-5 w-5" />
                            </button>
                        </div>
                        {!isSharedGuestPlanLocked ? (
                            <div className={`mt-2 grid gap-2 ${showTranslationColumn ? 'grid-cols-3' : 'grid-cols-2'}`}>
                                <button
                                    type="button"
                                    onClick={() => handleSelectSpokenPlan("basic")}
                                    className={`rounded-[14px] px-2 py-2 text-center text-[12px] font-semibold transition-all duration-200 ${!isFastPlanActive && activeSpokenLanguagePlan === "basic"
                                        ? 'bg-gradient-to-r from-[#3B3EEB] to-[#5F68FF] text-white shadow-lg'
                                        : 'bg-[#1A1A1A] text-gray-300 hover:bg-[#232323]'
                                        }`}
                                >
                                    {t("controllers.plan.basic")}
                                </button>
                                <button
                                    type="button"
                                    onClick={() => handleSelectSpokenPlan("premium")}
                                    className={`rounded-[14px] px-2 py-2 text-center text-[12px] font-semibold transition-all duration-200 ${!isFastPlanActive && activeSpokenLanguagePlan === "premium"
                                        ? 'bg-gradient-to-r from-[#3B3EEB] to-[#5F68FF] text-white shadow-lg'
                                        : 'bg-[#1A1A1A] text-gray-300 hover:bg-[#232323]'
                                        }`}
                                >
                                    {t("controllers.plan.premium")}
                                </button>
                                {showTranslationColumn && (
                                    <button
                                        type="button"
                                        disabled={isFastAudioPlanLocked}
                                        title={isFastAudioPlanLocked ? t("controllers.translationModes.fastSessionLocked") : ""}
                                        onClick={handleSelectFastAudioPlan}
                                        className={`rounded-[14px] px-2 py-2 text-center text-[12px] font-semibold transition-all duration-200 ${isFastPlanActive
                                            ? 'bg-gradient-to-r from-[#3B3EEB] to-[#5F68FF] text-white shadow-lg'
                                            : 'bg-[#1A1A1A] text-gray-300 hover:bg-[#232323]'
                                            } ${isFastAudioPlanLocked ? 'cursor-not-allowed opacity-60' : ''}`}
                                    >
                                        {t("controllers.plan.fastAudio")}
                                    </button>
                                )}
                            </div>
                        ) : (
                            <p className="mt-2 rounded-[12px] border border-[#45495f] bg-[#1A1A1A] px-3 py-2 text-[11px] leading-snug text-[#c8c8c8]">
                                {t("controllers.sharedSessionModelLocked")}
                            </p>
                        )}
                        {!isFastPlanActive && activeSpokenLanguagePlan === "premium" && (
                            <div className="mt-2 rounded-[16px] border border-[#45495f] bg-[#1A1A1A] px-3 py-2.5 leading-snug text-[#E6E9FF]">
                                <ul className="list-disc space-y-1 pl-4 text-[12px] text-[#D6DCFF]">
                                    <li>{t("controllers.plan.premiumBenefit")}</li>
                                    <li>{t("controllers.plan.premiumNote")}</li>
                                </ul>
                            </div>
                        )}
                        {isFastPlanActive && (
                            <div className="mt-2 rounded-[16px] border border-[#45495f] bg-[#1A1A1A] px-3 py-2.5 leading-snug text-[#E6E9FF]">
                                <ul className="list-disc space-y-1 pl-4 text-[12px] text-[#D6DCFF]">
                                    <li>{t("controllers.translationModes.fastDescriptionTranscription")}</li>
                                    <li>{t("controllers.translationModes.fastDescriptionLanguages")}</li>
                                    <li>{t("controllers.translationModes.fastDescriptionSpeech")}</li>
                                </ul>
                            </div>
                        )}
                    </div>
                    <div className="flex min-h-0 flex-1">
                    {!isFastPlanActive && (
                    <div className={`flex min-h-0 min-w-0 flex-col p-4 ${showTranslationColumn
                        ? 'flex-[340]'
                        : 'w-full flex-1'}`}>
                        <div className="mb-2 h-10">
                            <p className="text-sm font-semibold text-white">{t("languageSettings.spokenLanguage")}</p>
                            <p className="mt-1 text-[11px] text-gray-400">{t("languageSettings.spokenLanguageHint")}</p>
                        </div>
                        <>
                        <div className="language-search-field mb-2.5 h-[38px] rounded-[16px] border border-[#45495f] bg-[linear-gradient(135deg,#17181d_0%,#20222c_100%)] p-[1px] shadow-[0_12px_28px_rgba(0,0,0,0.22)]">
                            <div className="flex h-full items-center gap-2 rounded-[15px] bg-[linear-gradient(135deg,#111318_0%,#1A1D26_100%)] px-3">
                                <Search className="h-4 w-4 shrink-0 text-[#9EA6FF]" />
                                <input
                                    ref={spokenLanguageSearchInputRef}
                                    type="text"
                                    value={spokenLanguageSearch}
                                    onChange={(e) => setSpokenLanguageSearch(e.target.value)}
                                    placeholder={languageSearchPlaceholder}
                                    className="w-full bg-transparent text-[13px] text-white placeholder:text-gray-500 focus:outline-none"
                                />
                            </div>
                        </div>
                        <div className="min-h-0 flex-1 overflow-y-auto">
                            {filteredSpokenLanguages.length === 0 && (
                                <div className="rounded-xl border border-dashed border-[#404040] bg-[#1A1A1A] px-3 py-6 text-center text-sm text-gray-400">
                                    {noLanguageFoundText}
                                </div>
                            )}
                            {!normalizedSpokenLanguageSearch && recentlyUsedSpokenLanguages.length > 0 && (
                                <>
                                    <p className="mb-1.5 text-[10px] font-medium uppercase tracking-[0.28em] text-gray-300">
                                        {t("languageSettings.lastUsed")}
                                    </p>
                                    {recentlyUsedSpokenLanguages.map((language) => renderSpokenLanguageOption(language, 'recent-'))}
                                    <p className="mb-1.5 mt-3 text-[10px] font-medium uppercase tracking-[0.28em] text-gray-300">
                                        {t("languageSettings.allLanguages")}
                                    </p>
                                </>
                            )}
                            {filteredSpokenLanguages.map((language) => renderSpokenLanguageOption(language))}
                        </div>
                        </>
                    </div>
                    )}
                    {showTranslationLanguageModal && showTranslationColumn && (
                    <div className={`flex min-h-0 min-w-0 flex-col overflow-hidden p-4 ${isFastPlanActive
                        ? 'w-full flex-1'
                        : 'flex-[380] border-l border-[#404040]'}`}>
                        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                        <div className="mb-2 h-10">
                            <p className="whitespace-nowrap text-sm font-semibold text-white">{t("languageSettings.translationOutput")}</p>
                            <p className="mt-1 text-[11px] text-gray-400">
                                {t(isFastPlanActive
                                    ? "languageSettings.outputHintFast"
                                    : "languageSettings.outputHint")}
                            </p>
                        </div>
                        {translationModalTab === 'single' ? (
                            <>
                                <div className="language-search-field mb-2.5 h-[38px] rounded-[16px] border border-[#45495f] bg-[linear-gradient(135deg,#17181d_0%,#20222c_100%)] p-[1px] shadow-[0_12px_28px_rgba(0,0,0,0.22)]">
                                    <div className="flex h-full items-center gap-2 rounded-[15px] bg-[linear-gradient(135deg,#111318_0%,#1A1D26_100%)] px-3">
                                        <Search className="h-4 w-4 shrink-0 text-[#9EA6FF]" />
                                        <input
                                            ref={translatedLanguageSearchInputRef}
                                            type="text"
                                            value={translatedLanguageSearch}
                                            onChange={(e) => setTranslatedLanguageSearch(e.target.value)}
                                            placeholder={languageSearchPlaceholder}
                                            className="w-full bg-transparent text-[13px] text-white placeholder:text-gray-500 focus:outline-none"
                                        />
                                    </div>
                                </div>
                                <div className="min-h-0 flex-1 overflow-y-auto">
                                    {filteredTranslatedLanguages.length === 0 && (
                                        <div className="rounded-xl border border-dashed border-[#404040] bg-[#1A1A1A] px-4 py-7 text-center text-sm text-gray-400">
                                            {noLanguageFoundText}
                                        </div>
                                    )}
                                    {filteredTranslatedLanguages.map((language) => {
                                        const isSelected = language.value === translatedLanguage;
                                        return (
                                            <button
                                                type="button"
                                                key={language.key}
                                                onClick={() => handleTranslatedLanguageSelection(language)}
                                                className={`mb-1.5 flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left transition-all duration-200 ${isSelected
                                                    ? 'bg-[#484dff] text-white'
                                                    : 'bg-[#1A1A1A] text-white hover:bg-[#232323]'
                                                    }`}
                                            >
                                                <span className="min-w-0 flex-1 break-words text-[13px] leading-snug">
                                                    {getTranslatedLanguageLabel(language)}
                                                </span>
                                                {isSelected && <Check className="h-4 w-4 shrink-0 text-white" />}
                                            </button>
                                        );
                                    })}
                                </div>
                            </>
                        ) : translationModalTab === 'fast' ? (
                            <>
                                {bothModeLanguagesMatch && (
                                    <p className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[11px] leading-snug text-amber-100">
                                        {t("controllers.translationModes.bothModeSameLanguageWarning")}
                                    </p>
                                )}
                                {showBothModeUplinkLanguage && (
                                    <p className="mb-3 text-[10px] font-medium uppercase tracking-[0.28em] text-gray-300">
                                        {t("controllers.translationModes.bothModeIncomingLabel")}
                                    </p>
                                )}
                                <div className="language-search-field mb-2.5 rounded-[16px] border border-[#45495f] bg-[linear-gradient(135deg,#17181d_0%,#20222c_100%)] p-[1px] shadow-[0_12px_28px_rgba(0,0,0,0.22)]">
                                    <div className="flex items-center gap-2 rounded-[15px] bg-[linear-gradient(135deg,#111318_0%,#1A1D26_100%)] px-3 py-2">
                                        <Search className="h-4 w-4 shrink-0 text-[#9EA6FF]" />
                                        <input
                                            type="text"
                                            value={fastTranslationSearch}
                                            onChange={(e) => setFastTranslationSearch(e.target.value)}
                                            placeholder={languageSearchPlaceholder}
                                            className="w-full bg-transparent text-[13px] text-white placeholder:text-gray-500 focus:outline-none"
                                        />
                                    </div>
                                </div>
                                <div className={`overflow-y-auto ${showBothModeUplinkLanguage ? 'max-h-[176px]' : 'max-h-[360px]'}`}>
                                    {filteredFastTranslationLanguages.length === 0 && (
                                        <div className="rounded-xl border border-dashed border-[#404040] bg-[#1A1A1A] px-4 py-7 text-center text-sm text-gray-400">
                                            {noLanguageFoundText}
                                        </div>
                                    )}
                                    {filteredFastTranslationLanguages.map((language) => {
                                        const isSelected = language.key === fastTranslatedLanguageKey;
                                        return (
                                            <button
                                                type="button"
                                                key={`fast-translation-${language.key}`}
                                                onClick={() => handleFastTranslatedLanguageSelection(language)}
                                                className={`mb-1.5 flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left transition-all duration-200 ${isSelected
                                                    ? 'bg-[#484dff] text-white'
                                                    : 'bg-[#1A1A1A] text-white hover:bg-[#232323]'
                                                    }`}
                                            >
                                                <span className="min-w-0 flex-1 break-words text-[13px] leading-snug">
                                                    {getTranslatedLanguageLabel(language)}
                                                </span>
                                                {isSelected && <Check className="h-4 w-4 shrink-0 text-white" />}
                                            </button>
                                        );
                                    })}
                                </div>
                                {showBothModeUplinkLanguage && (
                                    <>
                                        <p className="mb-3 mt-8 text-[10px] font-medium uppercase tracking-[0.28em] text-gray-300">
                                            {t("controllers.translationModes.bothModeUplinkLabel")}
                                        </p>
                                        <div className="language-search-field mb-2.5 rounded-[16px] border border-[#45495f] bg-[linear-gradient(135deg,#17181d_0%,#20222c_100%)] p-[1px] shadow-[0_12px_28px_rgba(0,0,0,0.22)]">
                                            <div className="flex items-center gap-2 rounded-[15px] bg-[linear-gradient(135deg,#111318_0%,#1A1D26_100%)] px-3 py-2">
                                                <Search className="h-4 w-4 shrink-0 text-[#9EA6FF]" />
                                                <input
                                                    type="text"
                                                    value={fastUplinkTranslationSearch}
                                                    onChange={(e) => setFastUplinkTranslationSearch(e.target.value)}
                                                    placeholder={languageSearchPlaceholder}
                                                    className="w-full bg-transparent text-[13px] text-white placeholder:text-gray-500 focus:outline-none"
                                                />
                                            </div>
                                        </div>
                                        <div className="max-h-[176px] overflow-y-auto">
                                            {filteredFastUplinkTranslationLanguages.map((language) => {
                                                const isSelected = language.key === fastUplinkTranslatedLanguageKey;
                                                return (
                                                    <button
                                                        type="button"
                                                        key={`fast-uplink-${language.key}`}
                                                        onClick={() => handleBothModeUplinkLanguageSelection(language)}
                                                        className={`mb-1.5 flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left transition-all duration-200 ${isSelected
                                                            ? 'bg-[#484dff] text-white'
                                                            : 'bg-[#1A1A1A] text-white hover:bg-[#232323]'
                                                            }`}
                                                    >
                                                        <span className="min-w-0 flex-1 break-words text-[13px] leading-snug">
                                                            {getTranslatedLanguageLabel(language)}
                                                        </span>
                                                        {isSelected && <Check className="h-4 w-4 shrink-0 text-white" />}
                                                    </button>
                                                );
                                            })}
                                        </div>
                                    </>
                                )}
                            </>
                        ) : (
                            <>
                                <div className="mb-3 rounded-[16px] border border-[#45495f] bg-[#1A1A1A] px-3 py-2.5 text-[12px] leading-snug text-[#E6E9FF]">
                                    <p className="text-[11px] text-[#D6DCFF]">
                                        {t("controllers.multilingualSubtitle")}
                                    </p>
                                </div>
                                <div className="language-search-field mb-2.5 rounded-[16px] border border-[#45495f] bg-[linear-gradient(135deg,#17181d_0%,#20222c_100%)] p-[1px]">
                                    <div className="flex items-center gap-2 rounded-[15px] bg-[linear-gradient(135deg,#111318_0%,#1A1D26_100%)] px-3 py-2">
                                        <Search className="h-4 w-4 shrink-0 text-[#9EA6FF]" />
                                        <input
                                            type="text"
                                            value={multilingualSearch}
                                            onChange={(e) => setMultilingualSearch(e.target.value)}
                                            placeholder={languageSearchPlaceholder}
                                            className="w-full bg-transparent text-[13px] text-white placeholder:text-gray-500 focus:outline-none"
                                        />
                                    </div>
                                </div>
                                <div className="mb-3 flex flex-wrap gap-1.5">
                                    {[translatedLanguage, ...extraTranslatedLanguages].filter(Boolean).map((value, _index, combined) => {
                                        const language = languages.find((candidate) => candidate.value === value);
                                        return (
                                            <span
                                                key={`selected-translation-${value}`}
                                                className={`inline-flex max-w-full items-center gap-1.5 rounded-full border border-[#6268ff] bg-[#3438a8]/50 py-1 pl-2.5 text-[11px] font-medium text-white ${combined.length > 1 ? 'pr-1' : 'pr-2.5'}`}
                                            >
                                                <span className="max-w-[120px] truncate">
                                                    {getTranslatedLanguageLabel(language)}
                                                </span>
                                                {combined.length > 1 && (
                                                    <button
                                                        type="button"
                                                        aria-label={`Remove ${getTranslatedLanguageLabel(language)}`}
                                                        onClick={() => {
                                                            const nextCombined = combined.filter((selected) => selected !== value);
                                                            const [primary, ...extras] = nextCombined;
                                                            const cfgRaw = localStorage.getItem('MeetingCopilot_config');
                                                            const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
                                                            localStorage.setItem('MeetingCopilot_config', JSON.stringify({
                                                                ...cfg,
                                                                translatedLanguage: primary,
                                                                extraTranslatedLanguages: extras,
                                                            }));
                                                            if (primary !== translatedLanguage) {
                                                                dispatch(setTranslatedLanguage(primary));
                                                            }
                                                            dispatch(setExtraTranslatedLanguages(extras));
                                                        }}
                                                        className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[#D6D8FF] transition-colors hover:bg-white/15 hover:text-white"
                                                    >
                                                        <X className="h-3 w-3" />
                                                    </button>
                                                )}
                                            </span>
                                        );
                                    })}
                                </div>
                                <div className="min-h-0 flex-1 overflow-y-auto">
                                    {(() => {
                                        const combined = [translatedLanguage, ...extraTranslatedLanguages].filter(Boolean);
                                        return languages
                                            .filter((language) => {
                                                const s = multilingualSearch.trim().toLowerCase();
                                                if (!s) return true;
                                                return [getTranslatedLanguageLabel(language), language.value, language.key]
                                                    .some((c) => c.toLowerCase().includes(s));
                                            })
                                            .map((language) => {
                                                const isSelected = combined.includes(language.value);
                                                const atLimit = !isSelected && combined.length >= 3;
                                                return (
                                                    <button
                                                        type="button"
                                                        key={language.key}
                                                        disabled={atLimit}
                                                        onClick={() => {
                                                            let nextCombined: string[];
                                                            if (isSelected) {
                                                                if (combined.length <= 1) return;
                                                                nextCombined = combined.filter((v) => v !== language.value);
                                                            } else if (combined.length < 3) {
                                                                nextCombined = [...combined, language.value];
                                                            } else {
                                                                return;
                                                            }
                                                            const [primary, ...extras] = nextCombined;
                                                            const cfgRaw = localStorage.getItem('MeetingCopilot_config');
                                                            const cfg = cfgRaw ? JSON.parse(cfgRaw) : {};
                                                            localStorage.setItem('MeetingCopilot_config', JSON.stringify({
                                                                ...cfg,
                                                                translatedLanguage: primary,
                                                                extraTranslatedLanguages: extras,
                                                            }));
                                                            if (primary !== translatedLanguage) {
                                                                dispatch(setTranslatedLanguage(primary));
                                                            }
                                                            dispatch(setExtraTranslatedLanguages(extras));
                                                        }}
                                                        className={`mb-1.5 flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left transition-all duration-200 ${isSelected
                                                            ? 'bg-[#484dff] text-white'
                                                            : atLimit
                                                                ? 'bg-[#1A1A1A] text-gray-500 cursor-not-allowed opacity-60'
                                                                : 'bg-[#1A1A1A] text-white hover:bg-[#232323]'
                                                            }`}
                                                    >
                                                        <span className="min-w-0 flex-1 break-words text-[13px] leading-snug">
                                                            {getTranslatedLanguageLabel(language)}
                                                        </span>
                                                        {isSelected && (
                                                            <span className="flex items-center gap-2 shrink-0">
                                                                <span className="text-[10px] font-semibold uppercase tracking-wider text-[#C7CBFF]">
                                                                    {language.value === translatedLanguage ? '1' : `${extraTranslatedLanguages.indexOf(language.value) + 2}`}
                                                                </span>
                                                                <Check className="h-4 w-4 text-white" />
                                                            </span>
                                                        )}
                                                    </button>
                                                );
                                            });
                                    })()}
                                </div>
                            </>
                        )}
                        {!isFastPlanActive && (
                            <div className="group relative mt-2.5 shrink-0">
                                {!isPremiumOrAbove && (
                                    <div className="pointer-events-none absolute bottom-[calc(100%+8px)] left-0 right-0 z-[4100] invisible flex items-center justify-center gap-1.5 rounded-[12px] border border-[#55555E] bg-[#0B0B0B] px-3 py-2 text-center text-[11px] font-semibold leading-snug text-white opacity-0 shadow-[0_12px_32px_rgba(0,0,0,0.65)] transition-opacity group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100">
                                        <LockKeyhole className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                                        <span>{t("controllers.translationModes.upgradeToPremium")}</span>
                                    </div>
                                )}
                                <button
                                    type="button"
                                    role={isPremiumOrAbove ? "switch" : undefined}
                                    aria-checked={isPremiumOrAbove ? translationModalTab === 'multi' : undefined}
                                    aria-label={!isPremiumOrAbove
                                        ? t("controllers.translationModes.upgradeToPremium")
                                        : t("controllers.translationModes.multilingual")}
                                    onClick={() => {
                                        if (!isPremiumOrAbove) {
                                            openMultilingualPricing();
                                            return;
                                        }
                                        if (translationModalTab === 'multi') {
                                            updateTranslationModalTab('single');
                                            clearExtraTranslatedLanguages();
                                        } else {
                                            updateTranslationModalTab('multi');
                                        }
                                    }}
                                    className="flex h-9 w-full items-center justify-between rounded-[12px] bg-[#1A1A1A] px-3 text-[12px] font-semibold text-gray-300 transition-all duration-200 hover:bg-[#232323]"
                                >
                                    <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-bold">
                                        {t("controllers.translationModes.multilingual")}
                                    </span>
                                    <span
                                        className={`relative flex h-5 w-11 shrink-0 items-center rounded-full border-2 p-1 transition-all duration-300 ${translationModalTab === 'multi'
                                            ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] border-[#484dff]'
                                            : 'bg-gradient-to-br from-[#666] via-[#777] to-[#888] border-[#777]'}`}
                                    >
                                        <span className="absolute inset-y-0 left-[5px] flex items-center">
                                            {translationModalTab === 'multi' && (
                                                <span className="text-[8px] text-white">{t('controllers.toggle.on')}</span>
                                            )}
                                        </span>
                                        <span className="absolute inset-y-0 right-[6px] flex items-center">
                                            {translationModalTab !== 'multi' && (
                                                <span className="text-[8px] text-[#FFFFFF]">{t('controllers.toggle.off')}</span>
                                            )}
                                        </span>
                                        <span
                                            className={`h-4 w-4 transform rounded-full bg-white shadow-md transition-all duration-300 ${translationModalTab === 'multi' ? 'translate-x-5' : '-translate-x-1'}`}
                                        />
                                    </span>
                                </button>
                            </div>
                        )}
                        </div>
                    </div>
                    )}
                    </div>
                    </div>
                </div>,
                document.body,
            )}
        </>
    );
};

export default LanguageSettingsPill;
