//! Guards for real-time audio callbacks (guardrail 8, NFR-05, D-115).
//!
//! A callback wraps its work in [`section`]. Inside a section, in a debug
//! build:
//!
//! - an allocation is a violation, seen by [`GuardAllocator`] (a global
//!   allocator wrapper that a binary installs with `#[global_allocator]`);
//! - taking an [`RtMutex`] is a violation.
//!
//! A violation is counted per thread and for the whole process. In mode
//! [`ViolationMode::Abort`] (the default) it also aborts the process, so a debug
//! build fails at once; tests and the manual-run program use
//! [`ViolationMode::Count`] and read the counters. In a release build a section
//! is a plain call and nothing is checked.
//!
//! Only allocations are seen, not frees: a value dropped inside a section is
//! not counted.
//!
//! The guard sees only what runs on the calling thread inside the section. A
//! lock or an allocation inside another crate's code that the callback calls is
//! seen too when it goes through the global allocator, but a lock that does not
//! use [`RtMutex`] is not seen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{LockResult, Mutex, MutexGuard};

/// What a violation does besides being counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationMode {
    /// Abort the process (the default).
    Abort,
    /// Only count.
    Count,
}

const MODE_ABORT: u8 = 0;
const MODE_COUNT: u8 = 1;

static MODE: AtomicU8 = AtomicU8::new(MODE_ABORT);
static TOTAL_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static TOTAL_LOCKS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    // Plain `Cell`s with a const start value: reading them never allocates,
    // which matters because the allocator reads them.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static LOCKS: Cell<u64> = const { Cell::new(0) };
    static SECTIONS: Cell<u64> = const { Cell::new(0) };
}

/// Violation counts. Two separate counters: an allocation never counts as a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Violations {
    pub allocations: u64,
    pub locks: u64,
}

/// Sets what a violation does. Process-wide; the default is `Abort`.
pub fn set_violation_mode(mode: ViolationMode) {
    let value = match mode {
        ViolationMode::Abort => MODE_ABORT,
        ViolationMode::Count => MODE_COUNT,
    };
    MODE.store(value, Ordering::Relaxed);
}

/// Violations seen on the calling thread.
pub fn thread_violations() -> Violations {
    Violations {
        allocations: ALLOCATIONS.try_with(Cell::get).unwrap_or(0),
        locks: LOCKS.try_with(Cell::get).unwrap_or(0),
    }
}

/// Violations seen on every thread since the process started.
pub fn total_violations() -> Violations {
    Violations {
        allocations: TOTAL_ALLOCATIONS.load(Ordering::Relaxed),
        locks: TOTAL_LOCKS.load(Ordering::Relaxed),
    }
}

/// Sections the calling thread has entered, in a debug build (0 in a release
/// build). A test reads it to show that a callback runs inside a section.
pub fn sections_entered() -> u64 {
    SECTIONS.try_with(Cell::get).unwrap_or(0)
}

/// True while the calling thread is inside a [`section`] in a debug build.
pub fn in_section() -> bool {
    cfg!(debug_assertions) && DEPTH.try_with(|d| d.get() > 0).unwrap_or(false)
}

/// Runs a callback body as a real-time section. Sections nest; the section
/// ends when the outermost call returns or unwinds.
#[cfg(debug_assertions)]
pub fn section<R>(f: impl FnOnce() -> R) -> R {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            let _ = DEPTH.try_with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
    let _ = DEPTH.try_with(|d| d.set(d.get() + 1));
    let _ = SECTIONS.try_with(|c| c.set(c.get() + 1));
    let _leave = Leave;
    f()
}

/// Release build: a plain call, nothing is checked.
#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn section<R>(f: impl FnOnce() -> R) -> R {
    f()
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
enum Kind {
    Allocation,
    Lock,
}

/// Counts one violation and aborts in the default mode. Allocates nothing.
#[cfg(debug_assertions)]
fn record(kind: Kind) {
    match kind {
        Kind::Allocation => {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            TOTAL_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        Kind::Lock => {
            let _ = LOCKS.try_with(|c| c.set(c.get() + 1));
            TOTAL_LOCKS.fetch_add(1, Ordering::Relaxed);
        }
    }
    if MODE.load(Ordering::Relaxed) == MODE_ABORT {
        std::process::abort();
    }
}

/// A global allocator that forwards to the system allocator and records an
/// allocation made inside a [`section`]. Install it in a debug binary or a test
/// binary with `#[global_allocator]`. In a release build it only forwards.
#[derive(Debug, Clone, Copy, Default)]
pub struct GuardAllocator;

#[inline(always)]
fn check_allocation() {
    #[cfg(debug_assertions)]
    if in_section() {
        record(Kind::Allocation);
    }
}

// SAFETY: every call is forwarded unchanged to the system allocator with the
// same arguments, so its contract holds. The only addition is `check_allocation`,
// which reads thread-local cells and atomics and never allocates, so the
// allocator does not call itself. The tests in `tests/realtime_guard.rs` cover it.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for GuardAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        check_allocation();
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        check_allocation();
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        check_allocation();
        System.realloc(ptr, layout, new_size)
    }
}

/// A `Mutex` for control-side code that a callback must never take: locking it
/// inside a [`section`] is a violation. Outside a section it is a plain mutex.
#[derive(Debug, Default)]
pub struct RtMutex<T> {
    inner: Mutex<T>,
}

impl<T> RtMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        #[cfg(debug_assertions)]
        if in_section() {
            record(Kind::Lock);
        }
        self.inner.lock()
    }
}
