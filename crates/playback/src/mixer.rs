//! Mixer rules: partner-voice ducking and where original audio may go.

/// Gain applied to the partner's translated voice while the owner speaks.
/// Starting value; expected to be tuned.
pub const DUCKED_GAIN: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// The owner's headphones.
    Headphones,
    /// The audio that goes into the meeting.
    Meeting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MixerState {
    /// The partner voice is currently ducked.
    pub ducking: bool,
    /// Original audio plays to the owner because translation is unavailable.
    pub original_fallback: bool,
}

impl MixerState {
    /// Updates `ducking` and returns the gain to apply now.
    pub fn apply_ducking(&mut self, owner_speaking: bool, ducking_enabled: bool) -> f32 {
        let gain = partner_voice_gain(owner_speaking, ducking_enabled);
        self.ducking = gain < 1.0;
        gain
    }

    /// Original audio is routed only in fallback mode, and never to the meeting.
    pub fn should_route_original(&self, dest: Destination) -> bool {
        self.original_fallback && route_original(dest)
    }
}

/// Gain for the partner's voice: ducked while the owner speaks, if enabled.
pub fn partner_voice_gain(owner_speaking: bool, ducking_enabled: bool) -> f32 {
    if owner_speaking && ducking_enabled {
        DUCKED_GAIN
    } else {
        1.0
    }
}

/// Original audio may reach every destination except the meeting.
pub fn route_original(dest: Destination) -> bool {
    dest != Destination::Meeting
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ducks_only_when_owner_speaks_and_enabled() {
        assert_eq!(partner_voice_gain(true, true), 0.3);
        assert_eq!(partner_voice_gain(true, false), 1.0);
        assert_eq!(partner_voice_gain(false, true), 1.0);
        assert_eq!(partner_voice_gain(false, false), 1.0);
    }

    #[test]
    fn original_never_goes_to_the_meeting() {
        assert!(!route_original(Destination::Meeting));
        assert!(route_original(Destination::Headphones));
    }

    #[test]
    fn state_tracks_ducking() {
        let mut s = MixerState::default();
        assert_eq!(s.apply_ducking(true, true), DUCKED_GAIN);
        assert!(s.ducking);
        assert_eq!(s.apply_ducking(false, true), 1.0);
        assert!(!s.ducking);
    }

    #[test]
    fn fallback_original_still_never_reaches_the_meeting() {
        let s = MixerState {
            ducking: false,
            original_fallback: true,
        };
        assert!(s.should_route_original(Destination::Headphones));
        assert!(!s.should_route_original(Destination::Meeting));
        let off = MixerState::default();
        assert!(!off.should_route_original(Destination::Headphones));
    }
}
