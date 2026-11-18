//! A slow consumer never stalls capture: pushes stay bounded in time, every
//! sample is either read or counted as dropped, and the drop count reaches a
//! diagnostics event.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::{ring_buffer, RingProducer, TARGET_SAMPLE_RATE};
use diagnostics::{code, event, export_json_lines, LogEvent};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// 10 ms of mono audio at the target rate.
const BLOCK_SAMPLES: usize = (TARGET_SAMPLE_RATE / 100) as usize;
/// Blocks the capture thread pushes: 20 s of audio.
const BLOCKS: usize = 2_000;
/// Ring capacity: 100 ms of audio, so a stalled consumer overflows quickly.
const RING_SAMPLES: usize = BLOCK_SAMPLES * 10;
/// The longest single push the test accepts. A push is a copy of 160 samples
/// (microseconds); the bound is loose on purpose so a loaded CI machine does
/// not fail it, yet it is far below one 10 ms block of real time.
const MAX_PUSH: Duration = Duration::from_millis(8);
/// A blocked producer would never finish; the test fails after this wait.
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);

/// Pushes `BLOCKS` blocks as fast as possible and reports the slowest push.
fn capture_run(mut producer: RingProducer) -> (RingProducer, Duration, usize) {
    let block = vec![0.5f32; BLOCK_SAMPLES];
    let mut slowest = Duration::ZERO;
    let mut dropped_by_pushes = 0usize;
    for _ in 0..BLOCKS {
        let start = Instant::now();
        dropped_by_pushes += producer.push_slice(&block);
        slowest = slowest.max(start.elapsed());
    }
    (producer, slowest, dropped_by_pushes)
}

/// The event the capture side would hand to diagnostics for a drop count. The
/// dropped audio length is rounded up, so one dropped sample is never 0 ms.
fn overflow_event(dropped_samples: u64) -> LogEvent {
    let rate = u64::from(TARGET_SAMPLE_RATE);
    let dropped_ms = (dropped_samples * 1_000).div_ceil(rate);
    LogEvent::new(event::CAPTURE_OVERFLOW)
        .with_code(code::CAPTURE_RING_FULL)
        .with_timing("dropped", dropped_ms)
}

#[test]
fn a_stalled_consumer_never_blocks_capture_and_the_drops_are_counted() {
    let (producer, consumer) = ring_buffer(RING_SAMPLES);
    let capacity = producer.capacity();
    let (done_tx, done_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();

    // The consumer reads nothing until it is released: the slowest case.
    let consumer_thread = thread::spawn(move || {
        let mut consumer = consumer;
        let _ = release_rx.recv();
        let mut out = vec![0.0f32; capacity];
        let read = consumer.pop_slice(&mut out);
        (read, consumer.dropped_total())
    });
    let capture_thread = thread::spawn(move || {
        let result = capture_run(producer);
        let _ = done_tx.send(());
        result
    });

    // If a push waited for the consumer, the capture thread would never
    // finish: the timeout turns that hang into a failure.
    done_rx
        .recv_timeout(FINISH_TIMEOUT)
        .expect("capture finished while the consumer was stalled");
    let (producer, slowest, dropped_by_pushes) = capture_thread.join().unwrap();
    assert!(
        slowest < MAX_PUSH,
        "slowest push took {slowest:?}; the bound is {MAX_PUSH:?}"
    );

    release_tx.send(()).unwrap();
    let (read, consumer_view) = consumer_thread.join().unwrap();

    let pushed = BLOCKS * BLOCK_SAMPLES;
    assert_eq!(read, capacity, "the ring kept its oldest unread samples");
    assert_eq!(dropped_by_pushes, pushed - capacity);
    // The drop count is readable from both ends and equals what pushes reported.
    assert_eq!(producer.dropped_total(), dropped_by_pushes as u64);
    assert_eq!(consumer_view, dropped_by_pushes as u64);

    // The count reaches a diagnostics event, without any text.
    let json = export_json_lines(&[overflow_event(producer.dropped_total())], false).unwrap();
    let line: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(line["name"], event::CAPTURE_OVERFLOW);
    assert_eq!(line["code"], code::CAPTURE_RING_FULL);
    let expected_ms = (dropped_by_pushes as u64 * 1_000).div_ceil(u64::from(TARGET_SAMPLE_RATE));
    assert_eq!(line["timings_ms"]["dropped"], expected_ms);
    assert!(line["text"].is_null());
}

#[test]
fn a_slow_consumer_gets_or_loses_every_sample_and_nothing_is_double_counted() {
    let (producer, mut consumer) = ring_buffer(RING_SAMPLES);
    let capture_thread = thread::spawn(move || capture_run(producer));

    // Slow reader: a small read, then a sleep, until capture is done and the
    // ring is empty.
    let mut read_total = 0usize;
    let mut scratch = vec![0.0f32; BLOCK_SAMPLES / 4];
    while !capture_thread.is_finished() || !consumer.is_empty() {
        read_total += consumer.pop_slice(&mut scratch);
        thread::sleep(Duration::from_micros(200));
    }
    let (producer, slowest, dropped_by_pushes) = capture_thread.join().unwrap();

    let pushed = BLOCKS * BLOCK_SAMPLES;
    assert_eq!(
        read_total + dropped_by_pushes,
        pushed,
        "every sample was read or counted as dropped"
    );
    assert!(
        dropped_by_pushes > 0,
        "the reader was slow enough to overflow"
    );
    assert_eq!(producer.dropped_total(), dropped_by_pushes as u64);
    assert!(slowest < MAX_PUSH, "slowest push took {slowest:?}");
}
