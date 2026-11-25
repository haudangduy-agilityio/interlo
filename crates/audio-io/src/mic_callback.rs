//! The microphone callback core: everything the sound card's callback does with
//! one block of input, without any sound card (M1-06, FR-01).
//!
//! CALLBACK SAFE (guardrail 8, NFR-05, D-115). `process_f32` and `process_i16`
//! mix the block down to mono, track the peak level, convert it to
//! `TARGET_SAMPLE_RATE` and push it to the ring. Every buffer is allocated in
//! `MicCallback::new`. The body runs inside `realtime::section`, so a debug
//! build counts any allocation or guarded lock taken here. Shared state is
//! atomic counters only; nothing here blocks, waits or logs.
//!
//! A long block is cut into chunks of at most `MAX_CHUNK_FRAMES` input frames, so
//! the scratch buffers stay small whatever block size the driver picks. A chunk
//! whose output does not fit the scratch buffer is refused and counted, never
//! cut short.

use crate::realtime;
use crate::resample::{RateConverter, Resampler, UnsupportedRate};
use crate::ring::RingProducer;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Most input frames one chunk holds. The scratch buffers are sized for it.
pub const MAX_CHUNK_FRAMES: usize = 1024;

/// A microphone callback cannot be built for this format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MicSetupError {
    #[error(transparent)]
    Rate(#[from] UnsupportedRate),
    #[error("the input has no channels")]
    NoChannels,
}

/// Peak input level since the last read. Cheap to clone; the control side reads
/// it while the callback writes it.
#[derive(Debug, Clone, Default)]
pub struct InputLevel {
    /// Bit pattern of a non-negative f32. For non-negative floats the bit
    /// order is the numeric order, so `fetch_max` keeps the larger peak.
    peak_bits: Arc<AtomicU32>,
}

impl InputLevel {
    /// Highest absolute mono sample since the previous call, then resets to 0.
    pub fn peak_since_last_read(&self) -> f32 {
        f32::from_bits(self.peak_bits.swap(0, Ordering::Relaxed))
    }

    fn record(&self, peak: f32) {
        self.peak_bits.fetch_max(peak.to_bits(), Ordering::Relaxed);
    }
}

/// A snapshot of the callback counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MicStats {
    /// Calls to `process_f32` or `process_i16`.
    pub callbacks: u64,
    /// Input frames received, all channels counted as one frame.
    pub input_frames: u64,
    /// Output samples the ring refused because it was full.
    pub samples_dropped: u64,
    /// Chunks refused because the scratch buffer was too small.
    pub chunks_refused: u64,
}

#[derive(Debug, Default)]
struct Counters {
    callbacks: AtomicU64,
    input_frames: AtomicU64,
    samples_dropped: AtomicU64,
    chunks_refused: AtomicU64,
}

/// Reads the callback counters from the control side.
#[derive(Debug, Clone, Default)]
pub struct MicCounters {
    inner: Arc<Counters>,
}

impl MicCounters {
    pub fn snapshot(&self) -> MicStats {
        MicStats {
            callbacks: self.inner.callbacks.load(Ordering::Relaxed),
            input_frames: self.inner.input_frames.load(Ordering::Relaxed),
            samples_dropped: self.inner.samples_dropped.load(Ordering::Relaxed),
            chunks_refused: self.inner.chunks_refused.load(Ordering::Relaxed),
        }
    }
}

/// A sample format the sound card can hand to the callback.
trait InputSample: Copy {
    fn to_f32(self) -> f32;
}

impl InputSample for f32 {
    #[inline(always)]
    fn to_f32(self) -> f32 {
        self
    }
}

impl InputSample for i16 {
    #[inline(always)]
    fn to_f32(self) -> f32 {
        f32::from(self) / 32768.0
    }
}

pub struct MicCallback {
    producer: RingProducer,
    converter: RateConverter,
    channels: usize,
    /// One chunk of mixed-down input.
    mono: Box<[f32]>,
    /// One chunk of converted output.
    out: Box<[f32]>,
    level: InputLevel,
    counters: MicCounters,
}

impl MicCallback {
    /// Builds the callback core for an input of `channels` channels at
    /// `rate_in` Hz. All buffers are allocated here.
    pub fn new(producer: RingProducer, rate_in: u32, channels: u16) -> Result<Self, MicSetupError> {
        if channels == 0 {
            return Err(MicSetupError::NoChannels);
        }
        let converter = RateConverter::new(rate_in)?;
        let out_len = converter.max_output_len(MAX_CHUNK_FRAMES);
        Ok(Self {
            producer,
            converter,
            channels: usize::from(channels),
            mono: vec![0.0; MAX_CHUNK_FRAMES].into_boxed_slice(),
            out: vec![0.0; out_len].into_boxed_slice(),
            level: InputLevel::default(),
            counters: MicCounters::default(),
        })
    }

    /// Sets the output scratch size in samples. For tests of the refusal edge:
    /// the default fits every chunk.
    #[must_use]
    pub fn with_scratch_frames(mut self, frames: usize) -> Self {
        self.out = vec![0.0; frames].into_boxed_slice();
        self
    }

    /// Uses level and counter handles made earlier, so the control side can hold
    /// them before the callback exists.
    #[must_use]
    pub fn with_shared(mut self, level: InputLevel, counters: MicCounters) -> Self {
        self.level = level;
        self.counters = counters;
        self
    }

    /// The level reader; clone it before the callback moves to the audio thread.
    pub fn level(&self) -> InputLevel {
        self.level.clone()
    }

    /// The counter reader; clone it before the callback moves to the audio thread.
    pub fn counters(&self) -> MicCounters {
        self.counters.clone()
    }

    pub fn stats(&self) -> MicStats {
        self.counters.snapshot()
    }

    /// One block of interleaved f32 input.
    pub fn process_f32(&mut self, data: &[f32]) {
        realtime::section(|| self.process(data));
    }

    /// One block of interleaved i16 input.
    pub fn process_i16(&mut self, data: &[i16]) {
        realtime::section(|| self.process(data));
    }

    fn process<T: InputSample>(&mut self, data: &[T]) {
        let channels = self.channels;
        let frames_total = data.len() / channels;
        self.counters
            .inner
            .callbacks
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .inner
            .input_frames
            .fetch_add(frames_total as u64, Ordering::Relaxed);
        let usable = &data[..frames_total * channels];
        for chunk in usable.chunks(MAX_CHUNK_FRAMES * channels) {
            let frames = chunk.len() / channels;
            let mut peak = 0.0f32;
            for (slot, frame) in self.mono.iter_mut().zip(chunk.chunks_exact(channels)) {
                let mut sum = 0.0f32;
                for sample in frame {
                    sum += sample.to_f32();
                }
                let mixed = if channels == 1 {
                    sum
                } else {
                    sum / channels as f32
                };
                peak = peak.max(mixed.abs());
                *slot = mixed;
            }
            self.level.record(peak);
            match self
                .converter
                .process_into(&self.mono[..frames], &mut self.out)
            {
                Ok(written) => {
                    let dropped = self.producer.push_slice(&self.out[..written]);
                    if dropped > 0 {
                        self.counters
                            .inner
                            .samples_dropped
                            .fetch_add(dropped as u64, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    self.counters
                        .inner
                        .chunks_refused
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}
