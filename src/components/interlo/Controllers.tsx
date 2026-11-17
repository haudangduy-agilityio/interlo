import { Pause, Play, Square } from "lucide-react";
import type { MutableRefObject, ReactNode } from "react";
import type { Socket } from "socket.io-client";
import { useTranslation } from "../../hooks/useTranslation";
import { useAppSelector } from "../../redux/hooks";
import type { SpeechToSpeechMode } from "../../utils/audioRoutingAutoConfig";
import LanguageSettingsPill from "./LanguageSettingsPill";

const RecordingControlTooltip = ({
    id,
    label,
    children,
}: {
    id: string;
    label: string;
    children: ReactNode;
}) => (
    <div className="group relative inline-flex">
        {children}
        <span
            id={id}
            role="tooltip"
            className="live-session-tooltip pointer-events-none invisible absolute left-0 top-full z-[100] mt-2 translate-y-1 whitespace-nowrap rounded-lg border px-2.5 py-1.5 text-[11px] font-semibold opacity-0 shadow-[0_8px_24px_rgba(0,0,0,0.35)] transition-[opacity,transform,visibility] duration-150 group-hover:visible group-hover:translate-y-0 group-hover:opacity-100 group-hover:delay-300 group-focus-within:visible group-focus-within:translate-y-0 group-focus-within:opacity-100"
        >
            {label}
        </span>
    </div>
);

const Controllers = ({ saveStateAndReload, pauseRecording, stopRecording, isRecording,
    onStartRecording, paused, transcriptionLimitExceeded, translationLimitExceeded,
    socketRef, bulletPointEnabled, stopButtonDisabled, authReady,
    speechToSpeechMode = 'off',
    languageOpenRequestId,
}
    :
    {
        saveStateAndReload: (newSpokenLanguage: string, model: string) => void,
        pauseRecording: () => void, stopRecording: () => void,
        isRecording: boolean, onStartRecording: () => void,
        paused: boolean,
        transcriptionLimitExceeded: boolean,
        translationLimitExceeded: boolean,
        socketRef: MutableRefObject<Socket | null>,
        meetingId: string,
        bulletPointEnabled: boolean,
        stopButtonDisabled: boolean,
        authReady: boolean,
        speechToSpeechMode?: SpeechToSpeechMode,
        languageOpenRequestId?: number,
    }) => {
    const { enabled } = useAppSelector((state) => state.recorder);
    const { joined } = useAppSelector(state => state.sharing);
    const { t } = useTranslation();

    // Calculate if buttons should be disabled based on limit conditions
    const isButtonsDisabled =
        // 0. Auth not ready yet (prevents crashes from early button clicks)
        !authReady ||
        // A. Translation limit exceeded when translation OR bullet points is ON (ignore if using shared translation)
        (!joined && translationLimitExceeded && (enabled || bulletPointEnabled)) ||
        // B. Transcription limit exceeded when translation OFF AND bullet points OFF (ignore if using shared translation)
        (!joined && transcriptionLimitExceeded && !enabled && !bulletPointEnabled) ||
        // C. Both limits exceeded (ignore if using shared translation)
        (!joined && transcriptionLimitExceeded && translationLimitExceeded);

    // Get tooltip message for disabled state
    const getTooltipMessage = () => {
        // Only show tooltips when NOT using shared translation
        if (!joined && transcriptionLimitExceeded && translationLimitExceeded) {
            return t('controllers.tooltips.both');
        } else if (!joined && translationLimitExceeded) {
            return t('controllers.tooltips.translation');
        } else if (!joined && transcriptionLimitExceeded) {
            return t('controllers.tooltips.transcription');
        }
        return "";
    };

    const startTooltip = (isButtonsDisabled && getTooltipMessage()) || t("controllers.startRecording");
    const stopTooltip = stopButtonDisabled ? t("audioUpload.waitBeforeStop") : t("controllers.stopRecording");

    return (
        <div className="mb-0 flex items-center justify-between gap-2 pb-1">
            <div className="flex justify-start items-center gap-1.5 w-[30%]">
                {!isRecording ? (
                    <RecordingControlTooltip id="start-recording-tooltip" label={startTooltip}>
                        <button
                            className={`text-white h-10 w-10 rounded-full flex flex-col justify-center items-center border-2 shadow-lg transition-all duration-200 ${isButtonsDisabled
                                ? 'bg-gray-400 border-gray-500 cursor-not-allowed'
                                : 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] border-[#484dff] hover:from-[#282bff] hover:to-[#383bff] hover:shadow-xl hover:-translate-y-0.5 hover:scale-[1.04] active:translate-y-0 active:scale-95'
                                }`}
                            onClick={() => !isButtonsDisabled && onStartRecording()}
                            disabled={isButtonsDisabled}
                            aria-label={t("controllers.startRecording")}
                            aria-describedby="start-recording-tooltip"
                        >
                            <Play className="w-5 h-5" />
                        </button>
                    </RecordingControlTooltip>
                ) : (
                    <button
                        className="bg-gradient-to-br from-[#16a316] via-[#1cba1c] to-[#22d122] text-white h-10 w-10 rounded-full flex justify-center items-center border-2 border-[#1cba1c] shadow-lg hover:from-[#14a014] hover:to-[#1fb71f] hover:shadow-xl transition-all duration-200"
                        onClick={pauseRecording}
                        disabled={!isRecording}
                    >
                        <Pause className="w-5 h-5" />
                    </button>
                )}

                <RecordingControlTooltip id="stop-recording-tooltip" label={stopTooltip}>
                    <button
                        className={`recording-stop-button h-10 w-10 rounded-full flex justify-center items-center border shadow-lg transition-all duration-200 ${(isRecording || paused) && !stopButtonDisabled
                            ? 'hover:-translate-y-0.5 hover:scale-[1.03] active:translate-y-0 active:scale-95'
                            : 'recording-stop-button--disabled cursor-not-allowed'
                            }`}
                        onClick={(isRecording || paused) && !stopButtonDisabled ? stopRecording : undefined}
                        disabled={(!isRecording && !paused) || stopButtonDisabled}
                        aria-label={t("controllers.stopRecording")}
                        aria-describedby="stop-recording-tooltip"
                    >
                        <Square className="h-3.5 w-3.5" fill="currentColor" strokeWidth={2} />
                    </button>
                </RecordingControlTooltip>
            </div>

            <LanguageSettingsPill
                saveStateAndReload={saveStateAndReload}
                socketRef={socketRef}
                isRecording={isRecording}
                paused={paused}
                speechToSpeechMode={speechToSpeechMode}
                openRequestId={languageOpenRequestId}
            />
        </div>
    );
};

export default Controllers;
