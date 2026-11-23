//! audio-io owns audio devices, capture, playback output, resampling and
//! ring buffers.
//!
//! Must never allocate, lock or log in the real-time callback, and must
//! never block capture when a consumer is slow.

pub mod capture;
pub mod device_list;
pub mod devices;
pub mod error;
pub mod output;
pub mod resample;
pub mod ring;
pub mod setup_report;

pub use capture::{capture_plan, CaptureError, CaptureMethod, CaptureSource};
pub use device_list::{
    classify, parse_system_profiler, read_system_devices, DeviceListError, DeviceSnapshot,
    ProcessRunner, ToolRunner, SYSTEM_PROFILER_TIMEOUT_MS,
};
pub use devices::{DeviceInfo, DeviceKind, Transport};
pub use error::AudioIoError;
pub use output::{check_listening_device, AudioOutput, OutputError};
pub use resample::{Resampler, TARGET_SAMPLE_RATE};
pub use ring::{ring_buffer, RingConsumer, RingProducer};
pub use setup_report::{
    build_report, mic_permission_from_error, microphone, InputOpenError, MicPermission, ModelState,
    RowId, RowState, SetupFacts, SetupFix, SetupReport, SetupRow, SetupWarning,
};
