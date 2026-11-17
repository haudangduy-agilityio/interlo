import type { Socket } from 'socket.io-client';
import { useAppSelector } from '../../redux/hooks';
import { useRealtimeTranslatedSpeech } from '../../hooks/useRealtimeTranslatedSpeech';
import type { SpeechToSpeechMode } from '../../utils/realtimeTranslation/realtimeAudioRouter';

type RealtimeTranslatedSpeechPlayerProps = {
  socketRef: React.MutableRefObject<Socket | null>;
  textToSpeechSessionMode: 'online' | 'in-person';
  speechToSpeechMode: SpeechToSpeechMode;
  textToSpeechEnabled?: boolean;
  socketReady?: boolean;
};

const RealtimeTranslatedSpeechPlayer = ({
  socketRef,
  textToSpeechSessionMode,
  speechToSpeechMode,
  textToSpeechEnabled = false,
  socketReady = false,
}: RealtimeTranslatedSpeechPlayerProps) => {
  const { translationMode, enabled } = useAppSelector((state) => state.recorder);
  const isFastMode = translationMode === 'fast';

  useRealtimeTranslatedSpeech({
    socketRef,
    enabled: isFastMode && enabled,
    speechToSpeechMode,
    textToSpeechSessionMode,
    textToSpeechEnabled,
    socketReady,
  });

  return null;
};

export default RealtimeTranslatedSpeechPlayer;
