//! playback owns the single playback queue and the mixer.
//!
//! Must never cut, skip or reorder a segment, and must never send the
//! partner's original audio into the meeting.
//!
//! Real-time rule (guardrail 8): everything in this crate runs on the control
//! side. The queue and the mixer rules allocate or are not thread-safe, so no
//! real-time audio callback may call them; the callback reads buffers that the
//! control thread filled earlier.

pub mod error;
pub mod mixer;
pub mod queue;

pub use error::PlaybackError;
pub use mixer::{partner_voice_gain, route_original, Destination, MixerState, DUCKED_GAIN};
pub use queue::{
    speed_factor, Direction, NullPlayer, PlaybackQueue, Segment, SegmentPlayer, SegmentState,
    HOLD_TIMEOUT_MS, MAX_QUEUED_SEGMENTS, MAX_SPEED_FACTOR, PLAYED_ID_MEMORY,
};
