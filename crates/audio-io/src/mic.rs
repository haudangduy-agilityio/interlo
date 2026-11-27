//! The microphone source: opens an input device by name, converts what the sound
//! card delivers to mono f32 at `TARGET_SAMPLE_RATE` and pushes it to the ring
//! (M1-06, FR-01).
//!
//! CONTROL SIDE. This file starts and stops the stream, so it may allocate and
//! take locks. The audio callback itself is `MicCallback`, which is callback safe.
//! The sound card sits behind the `MicBackend` trait so tests use a fake; the real
//! backend over the audio I/O library is `SystemBackend`.

use crate::capture::{CaptureError, CaptureMethod, CaptureSource};
use crate::mic_callback::{InputLevel, MicCallback, MicCounters, MicStats};
use crate::ring::RingProducer;
use crate::setup_report::InputOpenError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One input device as a backend lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceEntry {
    pub name: String,
    /// Input channels; a device with none is not an input.
    pub input_channels: u16,
    pub is_default: bool,
}

/// Sample formats the callback converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    F32,
    I16,
    /// Any other format. `start` refuses it.
    Other,
}

/// The stream format of a device: what the callback receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample: SampleFormat,
}

/// One block of interleaved input from the sound card.
#[derive(Debug, Clone, Copy)]
pub enum InputBlock<'a> {
    F32(&'a [f32]),
    I16(&'a [i16]),
}

/// Called by the backend for every block, on the audio thread.
pub type BlockHandler = Box<dyn FnMut(InputBlock<'_>) + Send + 'static>;
/// Called by the backend when an error ends the stream.
pub type ErrorHandler = Box<dyn Fn() + Send + Sync + 'static>;

/// The sound card, as `MicCapture` needs it.
pub trait MicBackend: Send {
    /// Lists the devices. Includes devices without input channels; `select_input`
    /// skips them.
    fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError>;

    /// The stream format `open` will use for the device at `device`.
    fn format_of(&mut self, device: usize) -> Result<StreamFormat, InputOpenError>;

    /// Opens and starts a stream with exactly `format` (the value `format_of`
    /// returned), so the callback and the stream never disagree on rate or
    /// channels. The returned handle owns the stream; dropping it stops the
    /// stream and drops `on_block` before the drop returns.
    fn open(
        &mut self,
        device: usize,
        format: StreamFormat,
        on_block: BlockHandler,
        on_error: ErrorHandler,
    ) -> Result<Box<dyn Send>, InputOpenError>;
}

/// Which microphone to open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MicConfig {
    /// Exact device name, case sensitive. `None` means the default input device.
    pub device_name: Option<String>,
}

/// Chooses an input device. A name must match exactly (same case); the first
/// match wins. A name that matches nothing is an error, never the default
/// device. A device without input channels is never a match, and an empty name
/// matches nothing (a device whose name could not be read has an empty name).
pub fn select_input(
    devices: &[InputDeviceEntry],
    name: Option<&str>,
) -> Result<usize, InputOpenError> {
    let is_input = |d: &&InputDeviceEntry| d.input_channels > 0;
    let found = match name {
        Some("") => None,
        Some(wanted) => devices
            .iter()
            .position(|d| is_input(&d) && d.name == wanted),
        None => devices.iter().position(|d| is_input(&d) && d.is_default),
    };
    found.ok_or(InputOpenError::DeviceNotFound)
}

/// Maps an error of the audio I/O library to the type the setup report uses.
pub fn map_open_error(kind: cpal::ErrorKind) -> InputOpenError {
    match kind {
        cpal::ErrorKind::PermissionDenied => InputOpenError::PermissionDenied,
        cpal::ErrorKind::DeviceNotAvailable => InputOpenError::DeviceNotFound,
        _ => InputOpenError::Other,
    }
}

/// Whether an error report of the audio I/O library means that the stream ended.
/// An overload, a changed route and a refused real-time priority leave the stream
/// running. Every other kind ends it, a kind the library adds later included.
fn ends_stream(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

/// The error callback of a stream of the audio I/O library: `on_error` runs only
/// for a report that ends the stream. The callback allocates nothing and logs
/// nothing.
fn library_error_callback(on_error: ErrorHandler) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error: cpal::Error| {
        if ends_stream(error.kind()) {
            on_error()
        }
    }
}

/// A microphone as a `CaptureSource`.
pub struct MicCapture {
    method: CaptureMethod,
    device_name: Option<String>,
    backend: Box<dyn MicBackend>,
    level: InputLevel,
    counters: MicCounters,
    failed: Arc<AtomicBool>,
    stream: Option<Box<dyn Send>>,
}

impl MicCapture {
    pub fn new(config: MicConfig, backend: Box<dyn MicBackend>) -> Self {
        Self {
            method: CaptureMethod::Microphone {
                device_name: config.device_name.clone(),
            },
            device_name: config.device_name,
            backend,
            level: InputLevel::default(),
            counters: MicCounters::default(),
            failed: Arc::new(AtomicBool::new(false)),
            stream: None,
        }
    }

    /// The level reader. It stays valid across `start` and `stop`.
    pub fn level(&self) -> InputLevel {
        self.level.clone()
    }

    pub fn counters(&self) -> MicCounters {
        self.counters.clone()
    }

    pub fn stats(&self) -> MicStats {
        self.counters.snapshot()
    }

    /// True once an error ended the stream. Nothing reconnects.
    pub fn stream_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

impl CaptureSource for MicCapture {
    fn method(&self) -> &CaptureMethod {
        &self.method
    }

    fn start(&mut self, sink: RingProducer) -> Result<(), CaptureError> {
        if self.stream.is_some() {
            return Err(CaptureError::AlreadyStarted);
        }
        let open_error = CaptureError::InputOpen;
        let devices = self.backend.input_devices().map_err(open_error)?;
        let index = select_input(&devices, self.device_name.as_deref()).map_err(open_error)?;
        let format = self.backend.format_of(index).map_err(open_error)?;
        if format.sample == SampleFormat::Other {
            return Err(open_error(InputOpenError::Other));
        }
        let mut callback = MicCallback::new(sink, format.sample_rate, format.channels)
            .map_err(|_| open_error(InputOpenError::Other))?
            .with_shared(self.level.clone(), self.counters.clone());
        let on_block: BlockHandler = Box::new(move |block| match block {
            InputBlock::F32(data) => callback.process_f32(data),
            InputBlock::I16(data) => callback.process_i16(data),
        });
        let failed = Arc::clone(&self.failed);
        let on_error: ErrorHandler = Box::new(move || failed.store(true, Ordering::Relaxed));
        self.failed.store(false, Ordering::Relaxed);
        let stream = self
            .backend
            .open(index, format, on_block, on_error)
            .map_err(open_error)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        // Dropping the stream stops it and drops the callback, with the ring
        // producer inside it, here and not on the audio thread.
        drop(self.stream.take());
        Ok(())
    }
}

/// The real sound card, through the audio I/O library. Uses each device's
/// default input configuration; no format is forced.
pub struct SystemBackend {
    host: cpal::Host,
    /// The devices of the last `input_devices` call, so an index stays valid.
    devices: Vec<cpal::Device>,
}

impl SystemBackend {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
            devices: Vec::new(),
        }
    }

    fn device(&self, index: usize) -> Result<&cpal::Device, InputOpenError> {
        self.devices
            .get(index)
            .ok_or(InputOpenError::DeviceNotFound)
    }
}

impl Default for SystemBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn sample_format_of(format: cpal::SampleFormat) -> SampleFormat {
    match format {
        cpal::SampleFormat::F32 => SampleFormat::F32,
        cpal::SampleFormat::I16 => SampleFormat::I16,
        _ => SampleFormat::Other,
    }
}

impl MicBackend for SystemBackend {
    fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
        let default_id = self.host.default_input_device().and_then(|d| d.id().ok());
        let devices: Vec<cpal::Device> = self
            .host
            .devices()
            .map_err(|e| map_open_error(e.kind()))?
            .collect();
        let entries = devices
            .iter()
            .map(|d| InputDeviceEntry {
                name: d
                    .description()
                    .map(|description| description.name().to_string())
                    .unwrap_or_default(),
                input_channels: if d.supports_input() {
                    d.default_input_config().map_or(0, |c| c.channels())
                } else {
                    0
                },
                is_default: default_id.is_some() && d.id().ok() == default_id,
            })
            .collect();
        self.devices = devices;
        Ok(entries)
    }

    fn format_of(&mut self, device: usize) -> Result<StreamFormat, InputOpenError> {
        let config = self
            .device(device)?
            .default_input_config()
            .map_err(|e| map_open_error(e.kind()))?;
        Ok(StreamFormat {
            sample_rate: config.sample_rate(),
            channels: config.channels(),
            sample: sample_format_of(config.sample_format()),
        })
    }

    fn open(
        &mut self,
        device: usize,
        format: StreamFormat,
        mut on_block: BlockHandler,
        on_error: ErrorHandler,
    ) -> Result<Box<dyn Send>, InputOpenError> {
        let device = self.device(device)?;
        let config = cpal::StreamConfig {
            channels: format.channels,
            sample_rate: format.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let on_stream_error = library_error_callback(on_error);
        let stream = match format.sample {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| on_block(InputBlock::F32(data)),
                on_stream_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| on_block(InputBlock::I16(data)),
                on_stream_error,
                None,
            ),
            SampleFormat::Other => return Err(InputOpenError::Other),
        }
        .map_err(|e| map_open_error(e.kind()))?;
        stream.play().map_err(|e| map_open_error(e.kind()))?;
        Ok(Box::new(stream))
    }
}

impl MicCapture {
    /// A microphone on the real sound card.
    pub fn with_default_backend(config: MicConfig) -> Self {
        Self::new(config, Box::new(SystemBackend::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::ring_buffer;
    use std::sync::Mutex;

    /// A sound card with one input that keeps the error handler of its stream.
    #[derive(Default)]
    struct Card(Arc<Mutex<Option<ErrorHandler>>>);

    impl MicBackend for Card {
        fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
            Ok(vec![InputDeviceEntry {
                name: "Test Mic".to_string(),
                input_channels: 1,
                is_default: true,
            }])
        }

        fn format_of(&mut self, _device: usize) -> Result<StreamFormat, InputOpenError> {
            Ok(StreamFormat {
                sample_rate: 16_000,
                channels: 1,
                sample: SampleFormat::F32,
            })
        }

        fn open(
            &mut self,
            _device: usize,
            _format: StreamFormat,
            _on_block: BlockHandler,
            on_error: ErrorHandler,
        ) -> Result<Box<dyn Send>, InputOpenError> {
            *self.0.lock().unwrap() = Some(on_error);
            Ok(Box::new(()))
        }
    }

    /// Whether a running microphone is marked as failed after the library
    /// reported `kind` on its stream.
    fn failed_after(kind: cpal::ErrorKind) -> bool {
        let card = Card::default();
        let handler = Arc::clone(&card.0);
        let mut capture = MicCapture::new(MicConfig::default(), Box::new(card));
        let (producer, _consumer) = ring_buffer(1024);
        capture.start(producer).unwrap();
        let on_error = handler.lock().unwrap().take().unwrap();
        library_error_callback(on_error)(cpal::Error::new(kind));
        capture.stream_failed()
    }

    #[test]
    fn only_ending_stream_errors_mark_the_capture_failed() {
        use cpal::ErrorKind::*;
        // An overload, a changed route and a refused real-time priority.
        for kind in [Xrun, DeviceChanged, RealtimeDenied] {
            assert!(!failed_after(kind), "{kind:?} must not end the stream");
        }
        // A device that is not available, an invalid stream, the two
        // unclassified kinds, and every other kind.
        for kind in [
            DeviceNotAvailable,
            StreamInvalidated,
            BackendError,
            Other,
            DeviceBusy,
            HostUnavailable,
            InvalidInput,
            PermissionDenied,
            ResourceExhausted,
            UnsupportedConfig,
            UnsupportedOperation,
        ] {
            assert!(failed_after(kind), "{kind:?} must end the stream");
        }
    }
}
