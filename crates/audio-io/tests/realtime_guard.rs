//! The two callback guards: an allocation inside a callback
//! section and a lock taken inside one are violations; the same actions outside
//! a section are not. This is the only test binary that installs the guard
//! allocator.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use audio_io::realtime::{
    section, sections_entered, set_violation_mode, thread_violations, total_violations,
    GuardAllocator, RtMutex, ViolationMode,
};
use std::process::Command;
use std::thread;

#[global_allocator]
static GUARD: GuardAllocator = GuardAllocator;

/// Set by the parent test so the child run knows to do the real work.
const CHILD_ENV: &str = "INTERLO_RT_GUARD_CHILD";

fn count_mode() {
    set_violation_mode(ViolationMode::Count);
}

/// A bad callback: it allocates.
fn allocating_callback() -> usize {
    let mut v: Vec<u8> = Vec::with_capacity(64);
    v.push(1);
    let b = Box::new(7u32);
    v.len() + usize::try_from(*b).unwrap_or(0)
}

#[test]
fn allocation_in_a_callback_section_is_a_violation() {
    count_mode();
    let before = thread_violations();
    let result = section(allocating_callback);
    let after = thread_violations();
    assert_eq!(result, 8);
    #[cfg(debug_assertions)]
    {
        assert!(
            after.allocations > before.allocations,
            "the bad callback was not caught: {before:?} -> {after:?}"
        );
        assert_eq!(after.locks, before.locks);
    }
    #[cfg(not(debug_assertions))]
    assert_eq!(after, before, "release builds check nothing");
}

#[test]
fn allocation_outside_a_section_is_not_a_violation() {
    count_mode();
    let before = thread_violations();
    let _ = allocating_callback();
    let after = thread_violations();
    assert_eq!(after, before);
}

#[test]
fn nested_sections_stay_inside_until_the_outer_one_ends() {
    count_mode();
    let before = thread_violations();
    section(|| {
        section(|| ());
        // Still inside the outer section: this allocation counts.
        let _ = allocating_callback();
    });
    let mid = thread_violations();
    let _ = allocating_callback();
    let after = thread_violations();
    #[cfg(debug_assertions)]
    assert!(mid.allocations > before.allocations);
    assert_eq!(after, mid, "the section ended with the outer call");
}

#[test]
fn a_panic_inside_a_section_still_leaves_the_section() {
    count_mode();
    let result = std::panic::catch_unwind(|| section(|| panic!("callback failed")));
    assert!(result.is_err());
    let before = thread_violations();
    let _ = allocating_callback();
    assert_eq!(thread_violations(), before, "the flag was left set");
}

#[test]
fn total_violations_include_other_threads() {
    count_mode();
    let total_before = total_violations();
    let mine_before = thread_violations();
    thread::spawn(|| {
        section(allocating_callback);
    })
    .join()
    .unwrap();
    assert_eq!(thread_violations(), mine_before, "per-thread counter moved");
    #[cfg(debug_assertions)]
    assert!(total_violations().allocations > total_before.allocations);
}

#[test]
fn the_guard_sees_an_allocation_made_on_purpose() {
    // Guards the zero results of the other tests: a counter that never moves
    // would pass them all.
    count_mode();
    let before = thread_violations();
    section(|| {
        let _b = std::hint::black_box(Box::new([1u8, 2, 3]));
    });
    #[cfg(debug_assertions)]
    assert!(thread_violations().allocations > before.allocations);
    #[cfg(not(debug_assertions))]
    assert_eq!(thread_violations(), before);
}

#[test]
fn lock_in_a_callback_section_is_a_violation() {
    count_mode();
    let m = RtMutex::new(0u32);
    // The first lock of a std mutex allocates its system lock on this Mac, so
    // take it once outside a section; this test is about the lock, not that.
    *m.lock().unwrap() += 1;
    let before = thread_violations();
    section(|| {
        *m.lock().unwrap() += 1;
    });
    let after = thread_violations();
    #[cfg(debug_assertions)]
    {
        assert_eq!(after.locks, before.locks + 1);
        assert_eq!(after.allocations, before.allocations);
    }
    #[cfg(not(debug_assertions))]
    assert_eq!(after, before);
}

#[test]
fn lock_outside_a_section_is_allowed() {
    count_mode();
    let m = RtMutex::new(0u32);
    let before = thread_violations();
    *m.lock().unwrap() += 1;
    assert_eq!(thread_violations(), before);
    assert_eq!(*m.lock().unwrap(), 1);
}

#[test]
fn section_runs_the_callback_in_release() {
    // Both profiles: the callback runs once and returns its value.
    let mut calls = 0;
    let out = section(|| {
        calls += 1;
        41 + calls
    });
    assert_eq!(out, 42);
    assert_eq!(calls, 1);
}

// --- Child-process checks: the default mode aborts the process ---

/// Runs this test binary again for one child test with the child variable set
/// and returns its exit status. The parent never sets a mode for the child, so
/// the default mode applies there.
fn run_child(test_name: &str) -> std::process::ExitStatus {
    let exe = std::env::current_exe().unwrap();
    Command::new(exe)
        .args(["--exact", test_name, "--test-threads=1", "--nocapture"])
        .env(CHILD_ENV, "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
}

#[cfg(unix)]
fn signal_of(status: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[test]
fn child_allocates_in_a_section() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return; // parent run: the real work is in the child
    }
    // No set_violation_mode here: the default must abort.
    section(allocating_callback);
    // Only a release build gets here.
}

#[test]
fn child_locks_in_a_section() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let m = RtMutex::new(0u32);
    section(|| {
        *m.lock().unwrap() += 1;
    });
}

#[cfg(all(unix, debug_assertions))]
#[test]
fn allocation_in_the_default_mode_aborts_the_process() {
    let status = run_child("child_allocates_in_a_section");
    assert_eq!(
        signal_of(status),
        Some(6),
        "expected SIGABRT, got {status:?}"
    );
}

#[cfg(all(unix, debug_assertions))]
#[test]
fn lock_in_the_default_mode_aborts_the_process() {
    let status = run_child("child_locks_in_a_section");
    assert_eq!(
        signal_of(status),
        Some(6),
        "expected SIGABRT, got {status:?}"
    );
}

#[cfg(all(unix, debug_assertions))]
#[test]
fn a_child_without_a_violation_exits_normally() {
    // Control for the two tests above: the child harness itself does not abort.
    let status = run_child("child_without_a_violation");
    assert!(status.success(), "{status:?}");
}

#[test]
fn child_without_a_violation() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    section(|| ());
}

// --- The real callback core passes the guard ---

use audio_io::{ring_buffer, MicCallback};

/// Feeds many blocks through a real `MicCallback` in Count mode and returns the
/// violations seen on this thread. The ring is drained outside the section.
fn violations_of_a_real_callback(rate: u32, channels: u16, block_frames: usize, i16_input: bool) {
    count_mode();
    let (producer, mut consumer) = ring_buffer(65_536);
    let mut cb = MicCallback::new(producer, rate, channels).unwrap();
    let samples = block_frames * usize::from(channels);
    let f32_block: Vec<f32> = (0..samples)
        .map(|i| ((i % 64) as f32 - 32.0) / 64.0)
        .collect();
    let i16_block: Vec<i16> = (0..samples).map(|i| ((i % 64) as i16 - 32) * 500).collect();
    let mut drain = vec![0.0f32; 4096];
    // Warm-up outside the measurement: one-time lazy set-up is not the callback's.
    if i16_input {
        cb.process_i16(&i16_block);
    } else {
        cb.process_f32(&f32_block);
    }
    while consumer.pop_slice(&mut drain) > 0 {}
    let before = thread_violations();
    for _ in 0..200 {
        if i16_input {
            cb.process_i16(&i16_block);
        } else {
            cb.process_f32(&f32_block);
        }
        while consumer.pop_slice(&mut drain) > 0 {}
    }
    assert_eq!(
        thread_violations(),
        before,
        "the callback allocated or took a lock ({rate} Hz, {channels} ch)"
    );
    assert_eq!(cb.stats().chunks_refused, 0);
}

#[test]
fn mic_callback_does_not_allocate_48k_stereo_f32() {
    violations_of_a_real_callback(48_000, 2, 512, false);
}

#[test]
fn mic_callback_does_not_allocate_44k1_mono_i16() {
    violations_of_a_real_callback(44_100, 1, 441, true);
}

#[test]
fn mic_callback_does_not_allocate_16k_mono_f32() {
    violations_of_a_real_callback(16_000, 1, 160, false);
}

#[test]
fn mic_callback_does_not_allocate_96k_four_channels_f32() {
    violations_of_a_real_callback(96_000, 4, 1000, false);
}

#[test]
fn mic_callback_does_not_allocate_for_a_block_longer_than_a_chunk() {
    violations_of_a_real_callback(48_000, 2, 2500, false);
}

#[test]
fn the_guard_catches_a_bad_microphone_style_callback() {
    // Control for the tests above: the same section around a callback that
    // builds a Vec per block is a violation, so a zero above means something.
    count_mode();
    let before = thread_violations();
    section(|| {
        let v: Vec<f32> = vec![0.0; 480];
        std::hint::black_box(&v);
    });
    #[cfg(debug_assertions)]
    assert!(thread_violations().allocations > before.allocations);
    #[cfg(not(debug_assertions))]
    assert_eq!(thread_violations(), before);
}

#[test]
fn the_microphone_callback_runs_inside_a_section() {
    // Without this, removing the section from the callback would leave every
    // "no violation" test above green: nothing would be watching.
    count_mode();
    let (producer, _consumer) = ring_buffer(1024);
    let mut cb = MicCallback::new(producer, 16_000, 1).unwrap();
    let before = sections_entered();
    cb.process_f32(&[0.1; 160]);
    cb.process_i16(&[100; 160]);
    #[cfg(debug_assertions)]
    assert_eq!(sections_entered(), before + 2);
    #[cfg(not(debug_assertions))]
    assert_eq!(sections_entered(), before);
}
