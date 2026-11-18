//! audio-io owns audio devices, capture, playback output, resampling and
//! ring buffers.
//!
//! Must never allocate, lock or log in the real-time callback, and must
//! never block capture when a consumer is slow.

pub mod capture;
pub mod devices;
pub mod error;
pub mod output;
pub mod resample;
pub mod ring;

pub use capture::{capture_plan, CaptureError, CaptureMethod, CaptureSource};
pub use devices::{DeviceInfo, DeviceKind};
pub use error::AudioIoError;
pub use output::{check_listening_device, AudioOutput, OutputError};
pub use resample::{Resampler, TARGET_SAMPLE_RATE};
pub use ring::{ring_buffer, RingConsumer, RingProducer};
