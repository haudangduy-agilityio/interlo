//! The microphone of one session: the device that the setup report names, behind
//! the source that the speech chain reads.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use audio_io::setup_report::microphone;
use audio_io::{ring_buffer, CaptureSource, DeviceInfo, MicCapture, MicConfig, RingConsumer};
use engine_host::live_chain::FrameSource;

/// The samples that wait between the audio callback and the chain: 4 s at 16 kHz.
const RING_SAMPLES: usize = 1 << 16;

/// An open capture. `stop` closes the microphone.
pub trait Capture: Send {
    fn stop(&mut self);
}

/// An open microphone: what the chain reads and what closes it.
pub struct Microphone {
    pub source: Box<dyn FrameSource>,
    pub capture: Box<dyn Capture>,
}

/// The microphone could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFailed;

/// How to open the setup microphone of `devices`, or `None` without one. The
/// default input is opened without a name; any other by its exact name.
pub fn mic_config(devices: &[DeviceInfo]) -> Option<MicConfig> {
    microphone(devices).map(|device| MicConfig {
        device_name: (!device.is_default_input).then(|| device.name.clone()),
    })
}

/// The microphone that the source and the capture share.
type Shared = Arc<Mutex<MicCapture>>;

fn lock(mic: &Shared) -> MutexGuard<'_, MicCapture> {
    mic.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the chain reads: the ring, and whether the stream still runs.
struct RingSource {
    ring: RingConsumer,
    mic: Shared,
}

impl FrameSource for RingSource {
    fn read(&mut self, out: &mut [f32]) -> usize {
        self.ring.pop_slice(out)
    }

    fn failed(&self) -> bool {
        lock(&self.mic).stream_failed()
    }
}

impl Drop for RingSource {
    fn drop(&mut self) {
        let dropped = self.ring.dropped_total();
        if dropped > 0 {
            // A code and a count only (docs/guardrails.md rule 6).
            eprintln!("E_CAPTURE_RING_FULL {dropped}");
        }
    }
}

struct MicStop(Shared);

impl Capture for MicStop {
    fn stop(&mut self) {
        // Stopping the microphone has no failure to report.
        let _ = lock(&self.0).stop();
    }
}

/// Start `mic` into a ring and give both ends of it.
pub fn open(mut mic: MicCapture) -> Result<Microphone, OpenFailed> {
    let (producer, ring) = ring_buffer(RING_SAMPLES);
    mic.start(producer).map_err(|_| OpenFailed)?;
    let mic = Arc::new(Mutex::new(mic));
    Ok(Microphone {
        source: Box::new(RingSource {
            ring,
            mic: Arc::clone(&mic),
        }),
        capture: Box::new(MicStop(mic)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_io::mic::{BlockHandler, ErrorHandler, InputDeviceEntry, SampleFormat, StreamFormat};
    use audio_io::setup_report::InputOpenError;
    use audio_io::{mic::InputBlock, MicBackend};
    use std::sync::{Arc, Mutex, PoisonError};

    /// What the stream of the fake sound card holds while it is open.
    type Open = Arc<Mutex<Option<(BlockHandler, ErrorHandler)>>>;

    /// A sound card with one default input.
    struct Card(Open);

    struct Stream(Open);

    impl Drop for Stream {
        fn drop(&mut self) {
            // A failed assertion drops the stream while the lock is poisoned: the
            // test must report that assertion, not abort in this destructor.
            self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        }
    }

    impl MicBackend for Card {
        fn input_devices(&mut self) -> Result<Vec<InputDeviceEntry>, InputOpenError> {
            Ok(vec![InputDeviceEntry {
                name: "Mic".to_string(),
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
            on_block: BlockHandler,
            on_error: ErrorHandler,
        ) -> Result<Box<dyn Send>, InputOpenError> {
            *self.0.lock().unwrap() = Some((on_block, on_error));
            Ok(Box::new(Stream(Arc::clone(&self.0))))
        }
    }

    #[test]
    fn the_source_reads_the_microphone_until_the_capture_stops() {
        let open_stream = Open::default();
        let mic = MicCapture::new(MicConfig::default(), Box::new(Card(open_stream.clone())));
        let Ok(mut microphone) = open(mic) else {
            panic!("the microphone does not open");
        };
        if let Some((on_block, on_error)) = open_stream.lock().unwrap().as_mut() {
            on_block(InputBlock::F32(&[0.25; 64]));
            let mut samples = [0.0; 128];
            let count = microphone.source.read(&mut samples);
            assert_eq!(count, 64);
            assert!(samples[..count].iter().all(|sample| *sample == 0.25));
            assert!(!microphone.source.failed());
            on_error();
            assert!(microphone.source.failed());
        } else {
            panic!("no stream was opened");
        }
        microphone.capture.stop();
        assert!(
            open_stream.lock().unwrap().is_none(),
            "the stream is closed"
        );
    }

    #[test]
    fn a_microphone_that_cannot_open_is_an_error() {
        let config = MicConfig {
            device_name: Some("Gone".to_string()),
        };
        let mic = MicCapture::new(config, Box::new(Card(Open::default())));
        assert_eq!(open(mic).err(), Some(OpenFailed));
    }
}
