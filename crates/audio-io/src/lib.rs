//! audio-io owns audio devices, the device list and setup report, capture,
//! playback output, resampling and ring buffers.
//!
//! Must never allocate, lock or log in the real-time callback, and must
//! never block capture when a consumer is slow.

pub mod capture;
pub mod device_list;
pub mod devices;
pub mod error;
pub mod mic;
pub mod mic_callback;
pub mod output;
pub mod realtime;
pub mod resample;
pub mod ring;
pub mod setup_report;

pub use capture::{capture_plan, CaptureError, CaptureMethod, CaptureSource};
pub use device_list::{read_devices, DeviceListError, DeviceSnapshot};
pub use devices::{DeviceInfo, DeviceKind, Transport};
pub use error::AudioIoError;
pub use mic::{select_input, MicBackend, MicCapture, MicConfig};
pub use mic_callback::{
    InputLevel, MicCallback, MicCounters, MicSetupError, MicStats, MAX_CHUNK_FRAMES,
};
pub use output::{check_listening_device, AudioOutput, OutputError};
pub use resample::{RateConverter, Resampler, UnsupportedRate, TARGET_SAMPLE_RATE};
pub use ring::{ring_buffer, RingConsumer, RingProducer};
pub use setup_report::{
    build_report, mic_permission_from_error, microphone, InputOpenError, MicPermission, ModelState,
    SetupFacts, SetupFix, SetupItem, SetupItemId, SetupReport, SetupState, SetupWarning,
};
