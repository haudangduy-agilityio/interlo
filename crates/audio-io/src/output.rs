//! Playback output and the listening-device rule.

use crate::devices::DeviceKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputError {
    /// Built-in speakers would leak the partner's voice back into the mic.
    #[error("built-in speakers need confirmed headphones")]
    SpeakersNeedHeadphones,
    /// The 16-channel virtual device is the virtual microphone: the meeting
    /// hears it, so listening on it would loop the partner's voice back.
    #[error("the virtual microphone cannot be the listening device")]
    VirtualMicrophoneLoop,
    #[error("audio output is not implemented yet")]
    NotImplemented,
    /// The sink took none of the frames of a non-empty write. A sink returns
    /// this instead of `Ok(0)`, so a caller that writes the rest again cannot
    /// loop forever.
    #[error("the audio output accepted no frames")]
    Stalled,
}

/// A sink for mono f32 frames at the output sample rate: the one interface
/// between the engine and the audio backends (virtual microphone, headphones,
/// later others). One frame is one mono sample.
///
/// Contract:
/// - `write_frames` returns how many frames it took: fewer than given is
///   allowed, more is not. The caller writes the rest again.
/// - A non-empty write that takes no frame returns `Err(OutputError::Stalled)`,
///   never `Ok(0)`.
/// - `Stalled` means the sink has no room now. Whether to retry, and how long
///   to wait, is the caller's policy; M1-08 and M1-12 settle it for a real sink.
/// - `write_frames` runs on a control-side worker thread, never inside an
///   audio callback, and returns without waiting for the device.
/// - `flush` ends a stream.
pub trait AudioOutput: Send {
    /// Returns the number of frames accepted.
    fn write_frames(&mut self, frames: &[f32]) -> Result<usize, OutputError>;
    fn flush(&mut self) -> Result<(), OutputError>;
}

/// Placeholder output until a real backend exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedOutput;

impl AudioOutput for UnimplementedOutput {
    fn write_frames(&mut self, _frames: &[f32]) -> Result<usize, OutputError> {
        Err(OutputError::NotImplemented)
    }

    fn flush(&mut self) -> Result<(), OutputError> {
        Err(OutputError::NotImplemented)
    }
}

/// Built-in speakers are refused unless the owner confirmed headphones.
/// The 16-channel virtual device (the virtual microphone) is always refused.
pub fn check_listening_device(
    kind: DeviceKind,
    headphones_confirmed: bool,
) -> Result<(), OutputError> {
    match kind {
        DeviceKind::BuiltInSpeakers if !headphones_confirmed => {
            Err(OutputError::SpeakersNeedHeadphones)
        }
        DeviceKind::VirtualOutput16ch => Err(OutputError::VirtualMicrophoneLoop),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakers_refused_without_headphones() {
        assert_eq!(
            check_listening_device(DeviceKind::BuiltInSpeakers, false),
            Err(OutputError::SpeakersNeedHeadphones)
        );
    }

    #[test]
    fn speakers_allowed_when_headphones_confirmed() {
        assert_eq!(
            check_listening_device(DeviceKind::BuiltInSpeakers, true),
            Ok(())
        );
    }

    #[test]
    fn virtual_microphone_refused_as_listening_device() {
        for confirmed in [false, true] {
            assert_eq!(
                check_listening_device(DeviceKind::VirtualOutput16ch, confirmed),
                Err(OutputError::VirtualMicrophoneLoop)
            );
        }
    }

    #[test]
    fn other_kinds_are_not_refused_by_the_loop_rule() {
        for kind in [
            DeviceKind::Microphone,
            DeviceKind::VirtualInput2ch,
            DeviceKind::Other,
        ] {
            assert_eq!(check_listening_device(kind, false), Ok(()));
        }
    }

    #[test]
    fn headphones_always_allowed() {
        assert_eq!(
            check_listening_device(DeviceKind::Headphones, false),
            Ok(())
        );
    }

    #[test]
    fn placeholder_output_is_not_implemented() {
        let mut out = UnimplementedOutput;
        assert_eq!(out.write_frames(&[0.0]), Err(OutputError::NotImplemented));
        assert_eq!(out.flush(), Err(OutputError::NotImplemented));
    }
}
