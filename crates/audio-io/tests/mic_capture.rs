//! `MicCapture` on a fake backend: device choice by name, error mapping and the
//! start and stop contract of `CaptureSource` (AC-08, AC-09 format, AC-10, AC-11).
//! No sound card is needed.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::mic::{
    map_open_error, select_input, BlockHandler, ErrorHandler, InputBlock, InputDeviceEntry,
    MicBackend, MicCapture, MicConfig, SampleFormat, StreamFormat,
};
use audio_io::{
    mic_permission_from_error, ring_buffer, CaptureError, CaptureMethod, CaptureSource,
    InputOpenError, MicPermission,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle, ThreadId};
use std::time::{Duration, Instant};

fn entry(name: &str, channels: u16, is_default: bool) -> InputDeviceEntry {
    InputDeviceEntry {
        name: name.to_string(),
        input_channels: channels,
        is_default,
    }
}

fn three_mics() -> Vec<InputDeviceEntry> {
    vec![
        entry("Built-in Mic", 1, true),
        entry("USB Mic", 2, false),
        entry("USB Mic", 1, false),
    ]
}

// --- AC-08: the device is chosen by name ---

#[test]
fn name_match_is_exact_and_case_sensitive() {
    let devices = three_mics();
    assert_eq!(select_input(&devices, Some("USB Mic")), Ok(1));
    assert_eq!(
        select_input(&devices, Some("usb mic")),
        Err(InputOpenError::DeviceNotFound)
    );
}

#[test]
fn missing_name_is_not_found_and_never_the_default() {
    let devices = three_mics();
    assert_eq!(
        select_input(&devices, Some("Missing")),
        Err(InputOpenError::DeviceNotFound)
    );
}

#[test]
fn no_name_uses_the_default_input() {
    assert_eq!(select_input(&three_mics(), None), Ok(0));
    let no_default = vec![entry("A", 1, false), entry("B", 1, false)];
    assert_eq!(
        select_input(&no_default, None),
        Err(InputOpenError::DeviceNotFound)
    );
}

#[test]
fn duplicate_names_pick_the_first() {
    let devices = vec![entry("USB Mic", 2, false), entry("USB Mic", 1, false)];
    assert_eq!(select_input(&devices, Some("USB Mic")), Ok(0));
}

#[test]
fn output_only_device_is_not_an_input() {
    let devices = vec![entry("Speakers", 0, false), entry("Speakers", 1, false)];
    assert_eq!(select_input(&devices, Some("Speakers")), Ok(1));
    let only_output = vec![entry("Speakers", 0, true)];
    assert_eq!(
        select_input(&only_output, Some("Speakers")),
        Err(InputOpenError::DeviceNotFound)
    );
    assert_eq!(
        select_input(&only_output, None),
        Err(InputOpenError::DeviceNotFound)
    );
}

// --- AC-10: open errors map to the setup types ---

#[test]
fn permission_denied_maps_to_denied() {
    let mapped = map_open_error(cpal::ErrorKind::PermissionDenied);
    assert_eq!(mapped, InputOpenError::PermissionDenied);
    assert_eq!(mic_permission_from_error(mapped), MicPermission::Denied);
}

#[test]
fn device_not_available_maps_to_not_found() {
    assert_eq!(
        map_open_error(cpal::ErrorKind::DeviceNotAvailable),
        InputOpenError::DeviceNotFound
    );
}

#[test]
fn other_errors_map_to_other() {
    for kind in [
        cpal::ErrorKind::DeviceBusy,
        cpal::ErrorKind::HostUnavailable,
        cpal::ErrorKind::InvalidInput,
        cpal::ErrorKind::UnsupportedConfig,
        cpal::ErrorKind::ResourceExhausted,
    ] {
        let mapped = map_open_error(kind);
        assert_eq!(mapped, InputOpenError::Other, "{kind:?}");
        assert_eq!(mic_permission_from_error(mapped), MicPermission::Unknown);
    }
}

// --- A fake backend that drives the callback from its own thread ---

#[derive(Default)]
struct Probe {
    callback_thread: Mutex<Option<ThreadId>>,
    drop_thread: Mutex<Option<ThreadId>>,
    error_handler: Mutex<Option<ErrorHandler>>,
    opened: AtomicBool,
}

struct FakeBackend {
    devices: Vec<InputDeviceEntry>,
    format: StreamFormat,
    block_frames: usize,
    probe: Arc<Probe>,
}

impl FakeBackend {
    fn new(probe: Arc<Probe>) -> Self {
        Self {
            devices: three_mics(),
            format: StreamFormat {
                sample_rate: 16_000,
                channels: 1,
                sample: SampleFormat::F32,
            },
            block_frames: 160,
            probe,
        }
    }
}

struct FakeStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<BlockHandler>>,
    probe: Arc<Probe>,
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // The callback comes back here, so it is dropped on the thread that
            // drops the stream: the thread that called `stop`.
            let callback = thread.join().unwrap();
            drop(callback);
            *self.probe.drop_thread.lock().unwrap() = Some(thread::current().id());
        }
    }
}

impl MicBackend for FakeBackend {
    fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
        Ok(self.devices.clone())
    }

    fn format_of(&mut self, _device: usize) -> Result<StreamFormat, InputOpenError> {
        Ok(self.format)
    }

    fn open(
        &mut self,
        _device: usize,
        format: StreamFormat,
        mut on_block: BlockHandler,
        on_error: ErrorHandler,
    ) -> Result<Box<dyn Send>, InputOpenError> {
        self.probe.opened.store(true, Ordering::Relaxed);
        *self.probe.error_handler.lock().unwrap() = Some(on_error);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in_thread = Arc::clone(&stop);
        let probe = Arc::clone(&self.probe);
        let frames = self.block_frames;
        let samples = frames * usize::from(format.channels);
        let thread = thread::spawn(move || {
            *probe.callback_thread.lock().unwrap() = Some(thread::current().id());
            let block = vec![0.25f32; samples];
            while !stop_in_thread.load(Ordering::Relaxed) {
                on_block(InputBlock::F32(&block));
                thread::sleep(Duration::from_millis(1));
            }
            on_block
        });
        Ok(Box::new(FakeStream {
            stop,
            thread: Some(thread),
            probe: Arc::clone(&self.probe),
        }))
    }
}

fn capture_with(probe: &Arc<Probe>, name: Option<&str>) -> MicCapture {
    MicCapture::new(
        MicConfig {
            device_name: name.map(str::to_string),
        },
        Box::new(FakeBackend::new(Arc::clone(probe))),
    )
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

// --- AC-08 (start) and AC-09 (format) ---

#[test]
fn start_with_a_missing_name_is_not_found_and_pushes_nothing() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, Some("Missing"));
    let (producer, consumer) = ring_buffer(1024);
    assert_eq!(
        cap.start(producer),
        Err(CaptureError::InputOpen(InputOpenError::DeviceNotFound))
    );
    assert!(
        !probe.opened.load(Ordering::Relaxed),
        "no device was opened"
    );
    assert_eq!(consumer.len(), 0);
}

#[test]
fn unsupported_sample_format_is_an_open_error() {
    let probe = Arc::new(Probe::default());
    let mut backend = FakeBackend::new(Arc::clone(&probe));
    backend.format.sample = SampleFormat::Other;
    let mut cap = MicCapture::new(MicConfig::default(), Box::new(backend));
    let (producer, consumer) = ring_buffer(1024);
    assert_eq!(
        cap.start(producer),
        Err(CaptureError::InputOpen(InputOpenError::Other))
    );
    assert!(!probe.opened.load(Ordering::Relaxed));
    assert_eq!(consumer.len(), 0);
}

// --- AC-11: start and stop follow the source contract ---

#[test]
fn start_returns_without_the_consumer() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    // A tiny ring that nobody reads: it fills at once and must not stall start.
    let (producer, consumer) = ring_buffer(64);
    let started = Instant::now();
    cap.start(producer).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    wait_until("ring full", || consumer.len() == consumer.capacity());
    wait_until("samples dropped", || cap.stats().samples_dropped > 0);
    cap.stop().unwrap();
}

#[test]
fn second_start_is_refused() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    let (p1, _c1) = ring_buffer(1024);
    cap.start(p1).unwrap();
    let (p2, c2) = ring_buffer(1024);
    assert_eq!(cap.start(p2), Err(CaptureError::AlreadyStarted));
    assert_eq!(c2.len(), 0);
    cap.stop().unwrap();
}

#[test]
fn nothing_is_pushed_after_stop() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    let (producer, mut consumer) = ring_buffer(1 << 20);
    cap.start(producer).unwrap();
    wait_until("some samples", || consumer.len() >= 320);
    cap.stop().unwrap();
    let mut buf = vec![0.0f32; 1 << 20];
    let at_stop = consumer.pop_slice(&mut buf);
    assert!(at_stop >= 320);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(consumer.len(), 0, "a sample arrived after stop returned");
}

#[test]
fn callback_is_dropped_on_the_stopping_thread() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    let (producer, consumer) = ring_buffer(4096);
    cap.start(producer).unwrap();
    wait_until("callback ran", || !consumer.is_empty());
    let stopping_thread = thread::current().id();
    cap.stop().unwrap();
    let dropped_on = probe.drop_thread.lock().unwrap().expect("never dropped");
    let ran_on = probe.callback_thread.lock().unwrap().expect("never ran");
    assert_eq!(dropped_on, stopping_thread);
    assert_ne!(dropped_on, ran_on);
}

#[test]
fn stop_before_start_is_ok() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    assert_eq!(cap.stop(), Ok(()));
}

#[test]
fn method_names_the_microphone() {
    let probe = Arc::new(Probe::default());
    let named = capture_with(&probe, Some("USB Mic"));
    assert_eq!(
        named.method(),
        &CaptureMethod::Microphone {
            device_name: Some("USB Mic".to_string())
        }
    );
    let default = capture_with(&probe, None);
    assert_eq!(
        default.method(),
        &CaptureMethod::Microphone { device_name: None }
    );
}

#[test]
fn stream_error_sets_the_failed_flag() {
    let probe = Arc::new(Probe::default());
    let mut cap = capture_with(&probe, None);
    assert!(!cap.stream_failed());
    let (producer, _consumer) = ring_buffer(1024);
    cap.start(producer).unwrap();
    assert!(!cap.stream_failed());
    {
        let guard = probe.error_handler.lock().unwrap();
        (guard.as_ref().expect("no error handler"))();
    }
    assert!(cap.stream_failed());
    cap.stop().unwrap();
}
