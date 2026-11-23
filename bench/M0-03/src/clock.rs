//! The session clock and the sleeper. Both are traits so tests pass fake ones.

use std::time::{Duration, Instant};

/// A monotonic clock in microseconds. Only differences between two readings mean anything.
pub trait Clock {
    fn now_us(&self) -> u64;
}

/// Waits for a number of microseconds. `realtime` pacing is the only caller.
pub trait Sleeper {
    fn sleep_us(&self, us: u64);
}

/// The real clock: microseconds since this value was made.
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_us(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// The real sleeper. A sleep can overshoot but never ends early.
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep_us(&self, us: u64) {
        std::thread::sleep(Duration::from_micros(us));
    }
}

#[cfg(test)]
pub mod fake {
    use super::{Clock, Sleeper};
    use std::cell::Cell;

    /// Returns its current value on each read, then adds `step`. `advance` moves it by hand.
    pub struct FakeClock {
        now: Cell<u64>,
        step: u64,
    }

    impl FakeClock {
        pub fn new(step: u64) -> Self {
            Self {
                now: Cell::new(0),
                step,
            }
        }

        pub fn advance(&self, us: u64) {
            self.now.set(self.now.get() + us);
        }
    }

    impl Clock for FakeClock {
        fn now_us(&self) -> u64 {
            let value = self.now.get();
            self.now.set(value + self.step);
            value
        }
    }

    /// Advances the fake clock by the time it was asked to sleep and counts the calls.
    pub struct FakeSleeper<'a> {
        clock: &'a FakeClock,
        calls: Cell<u32>,
    }

    impl<'a> FakeSleeper<'a> {
        pub fn new(clock: &'a FakeClock) -> Self {
            Self {
                clock,
                calls: Cell::new(0),
            }
        }

        pub fn calls(&self) -> u32 {
            self.calls.get()
        }
    }

    impl Sleeper for FakeSleeper<'_> {
        fn sleep_us(&self, us: u64) {
            self.calls.set(self.calls.get() + 1);
            self.clock.advance(us);
        }
    }

    #[test]
    fn fake_clock_returns_values_in_order() {
        let clock = FakeClock::new(1000);
        assert_eq!(clock.now_us(), 0);
        assert_eq!(clock.now_us(), 1000);
        assert_eq!(clock.now_us(), 2000);
        clock.advance(500);
        assert_eq!(clock.now_us(), 3500);
    }
}
