//! The microphone callback core without a sound card: level, ring behaviour,
//! conversion and counters (AC-02, AC-03, AC-09 and AC-17). This binary installs
//! no allocator, so the timing check of AC-03 is not slowed by one.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::{ring_buffer, MicCallback, RingConsumer, TARGET_SAMPLE_RATE};
use std::time::{Duration, Instant};

/// A sine of `amplitude` at `freq` Hz, `frames` frames, copied to `channels` channels.
fn sine(
    amplitude: f32,
    freq: f32,
    rate: u32,
    channels: usize,
    start_frame: usize,
    frames: usize,
) -> Vec<f32> {
    let mut v = Vec::with_capacity(frames * channels);
    for i in start_frame..start_frame + frames {
        let x = amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
        for _ in 0..channels {
            v.push(x);
        }
    }
    v
}

fn drain(consumer: &mut RingConsumer) -> usize {
    let mut buf = [0.0f32; 1024];
    let mut total = 0;
    loop {
        let n = consumer.pop_slice(&mut buf);
        if n == 0 {
            return total;
        }
        total += n;
    }
}

// --- AC-02: the level follows the sound ---

#[test]
fn level_follows_the_input_peak() {
    let (producer, mut consumer) = ring_buffer(32_768);
    let mut cb = MicCallback::new(producer, 48_000, 2).unwrap();
    let level = cb.level();
    for block in 0..100 {
        let data = sine(0.5, 440.0, 48_000, 2, block * 480, 480);
        cb.process_f32(&data);
        drain(&mut consumer);
    }
    let peak = level.peak_since_last_read();
    assert!((0.45..=0.55).contains(&peak), "peak was {peak}");
}

#[test]
fn level_read_resets_to_zero() {
    let (producer, _consumer) = ring_buffer(32_768);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    let level = cb.level();
    cb.process_f32(&[0.25; 160]);
    assert_eq!(level.peak_since_last_read(), 0.25);
    assert_eq!(level.peak_since_last_read(), 0.0, "no new input fed");
}

#[test]
fn silence_gives_zero_level() {
    let (producer, mut consumer) = ring_buffer(32_768);
    let mut cb = MicCallback::new(producer, 48_000, 2).unwrap();
    let level = cb.level();
    for _ in 0..100 {
        cb.process_f32(&[0.0; 960]);
        drain(&mut consumer);
    }
    assert_eq!(level.peak_since_last_read(), 0.0);
}

// --- AC-03: the ring never blocks the callback ---

#[test]
fn full_ring_never_blocks_the_callback() {
    const BLOCKS: usize = 2_000;
    const BLOCK: usize = 160;
    let (producer, consumer) = ring_buffer(1000);
    assert_eq!(consumer.capacity(), 1024);
    let mut cb = MicCallback::new(producer, TARGET_SAMPLE_RATE, 1).unwrap();
    let block = [0.5f32; BLOCK];
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    for _ in 0..BLOCKS {
        let t = Instant::now();
        cb.process_f32(&block);
        slowest = slowest.max(t.elapsed());
    }
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        slowest < Duration::from_millis(8),
        "slowest call {slowest:?}"
    );
    let stats = cb.stats();
    assert_eq!(stats.callbacks, BLOCKS as u64);
    assert_eq!(consumer.len(), 1024);
    assert_eq!(stats.samples_dropped, (BLOCKS * BLOCK) as u64 - 1024);
}

// --- AC-09: format, channels and rate ---

#[test]
fn stereo_is_averaged_to_mono() {
    let (producer, mut consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 2).unwrap();
    let mut data = Vec::new();
    for _ in 0..160 {
        data.extend_from_slice(&[1.0, 0.0]);
    }
    cb.process_f32(&data);
    let mut out = [0.0f32; 160];
    assert_eq!(consumer.pop_slice(&mut out), 160);
    assert!(out.iter().all(|&v| v == 0.5));
}

#[test]
fn i16_is_scaled_to_f32() {
    let (producer, mut consumer) = ring_buffer(16);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    cb.process_i16(&[32767, -32768, 0]);
    let mut out = [9.0f32; 3];
    assert_eq!(consumer.pop_slice(&mut out), 3);
    assert!((out[0] - 1.0).abs() < 0.0001, "{}", out[0]);
    assert_eq!(out[1], -1.0);
    assert_eq!(out[2], 0.0);
}

#[test]
fn rate_16k_passes_through() {
    let (producer, mut consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    let input: Vec<f32> = (0..160).map(|i| (i as f32) * 0.001 - 0.08).collect();
    cb.process_f32(&input);
    let mut out = [0.0f32; 160];
    assert_eq!(consumer.pop_slice(&mut out), 160);
    for (a, b) in input.iter().zip(out.iter()) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

/// Feeds 10 s of a tone in blocks of `block_frames` and returns the number of
/// samples that came out in the last 9 s (the first 1 s holds the converter delay).
fn output_after_the_first_second(rate: u32, block_frames: usize) -> usize {
    let (producer, mut consumer) = ring_buffer(32_768);
    let mut cb = MicCallback::new(producer, rate, 1).unwrap();
    let blocks_per_second = rate as usize / block_frames;
    let mut after_first = 0;
    let mut total = 0;
    for block in 0..blocks_per_second * 10 {
        let data = sine(0.3, 440.0, rate, 1, block * block_frames, block_frames);
        cb.process_f32(&data);
        total += drain(&mut consumer);
        if block + 1 == blocks_per_second {
            after_first = total;
        }
    }
    total - after_first
}

#[test]
fn rate_48k_gives_16k() {
    let n = output_after_the_first_second(48_000, 480);
    assert!(n.abs_diff(144_000) <= 40, "got {n}");
}

#[test]
fn rate_44k1_gives_16k() {
    let n = output_after_the_first_second(44_100, 441);
    assert!(n.abs_diff(144_000) <= 40, "got {n}");
}

// --- AC-17: counters, scratch edge, chunking ---

#[test]
fn counters_count_callbacks_and_input_frames() {
    let (producer, mut consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    for _ in 0..100 {
        cb.process_f32(&[0.1; 160]);
        drain(&mut consumer);
    }
    let stats = cb.stats();
    assert_eq!(stats.callbacks, 100);
    assert_eq!(stats.input_frames, 16_000);
    assert_eq!(stats.chunks_refused, 0);
}

#[test]
fn scratch_exactly_big_enough_is_accepted() {
    let (producer, consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 1)
        .unwrap()
        .with_scratch_frames(160);
    cb.process_f32(&[0.1; 160]);
    assert_eq!(cb.stats().chunks_refused, 0);
    assert_eq!(consumer.len(), 160);
}

#[test]
fn scratch_one_short_is_refused_and_counted() {
    let (producer, consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 1)
        .unwrap()
        .with_scratch_frames(159);
    cb.process_f32(&[0.1; 160]);
    assert_eq!(cb.stats().chunks_refused, 1);
    assert_eq!(consumer.len(), 0, "a refused chunk pushes nothing");
}

#[test]
fn a_long_block_is_chunked_not_refused() {
    let (producer, consumer) = ring_buffer(4096);
    assert_eq!(consumer.capacity(), 4096);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    cb.process_f32(&[0.1; 2500]);
    assert_eq!(cb.stats().chunks_refused, 0);
    assert_eq!(consumer.len(), 2500);
}

#[test]
fn dropped_samples_are_counted() {
    let (producer, consumer) = ring_buffer(8);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    cb.process_f32(&[0.1; 10]);
    assert_eq!(cb.stats().samples_dropped, 2);
    assert_eq!(consumer.len(), 8);
}
