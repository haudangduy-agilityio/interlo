//! Single-producer single-consumer ring of window levels, written by the capture
//! callback and read by the main thread. Atomics only: no allocation, no lock and no
//! log on the producer side (guardrail 8).

use crate::levels::WindowLevels;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Slots in the ring: 25 s of 100 ms windows (starting value).
pub const RING_SLOTS: usize = 256;

pub struct StatsRing {
    slots: [[AtomicU64; 2]; RING_SLOTS],
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicU64,
    callbacks: AtomicU64,
}

impl Default for StatsRing {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsRing {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| [AtomicU64::new(0), AtomicU64::new(0)]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            callbacks: AtomicU64::new(0),
        }
    }

    /// Producer side. Returns false (and counts a drop) when the ring is full.
    pub fn push(&self, w: WindowLevels) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING_SLOTS {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let slot = &self.slots[head % RING_SLOTS];
        slot[0].store(w.rms_dbfs.to_bits(), Ordering::Relaxed);
        slot[1].store(w.tone_dbfs.to_bits(), Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    /// Consumer side.
    pub fn pop(&self) -> Option<WindowLevels> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let slot = &self.slots[tail % RING_SLOTS];
        let w = WindowLevels {
            rms_dbfs: f64::from_bits(slot[0].load(Ordering::Relaxed)),
            tone_dbfs: f64::from_bits(slot[1].load(Ordering::Relaxed)),
        };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(w)
    }

    /// Producer side: count one capture callback.
    pub fn note_callback(&self) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(x: f64) -> WindowLevels {
        WindowLevels {
            rms_dbfs: x,
            tone_dbfs: x - 1.0,
        }
    }

    #[test]
    fn values_come_out_in_order() {
        let r = StatsRing::new();
        assert!(r.push(w(-10.0)));
        assert!(r.push(w(-20.0)));
        assert_eq!(r.pop(), Some(w(-10.0)));
        assert_eq!(r.pop(), Some(w(-20.0)));
        assert_eq!(r.pop(), None);
    }

    #[test]
    fn full_ring_drops_and_counts() {
        let r = StatsRing::new();
        for i in 0..RING_SLOTS {
            assert!(r.push(w(i as f64)));
        }
        assert!(!r.push(w(999.0)));
        assert_eq!(r.dropped(), 1);
        assert_eq!(r.pop(), Some(w(0.0)));
        assert!(r.push(w(1000.0)));
    }

    #[test]
    fn ring_wraps_around_many_times() {
        let r = StatsRing::new();
        for i in 0..(RING_SLOTS * 3 + 7) {
            assert!(r.push(w(i as f64)));
            assert_eq!(r.pop(), Some(w(i as f64)));
        }
        assert_eq!(r.dropped(), 0);
    }

    #[test]
    fn minus_120_survives_the_bit_round_trip() {
        let r = StatsRing::new();
        r.push(w(-120.0));
        assert_eq!(r.pop().map(|x| x.rms_dbfs), Some(-120.0));
    }

    #[test]
    fn callbacks_are_counted() {
        let r = StatsRing::new();
        r.note_callback();
        r.note_callback();
        assert_eq!(r.callbacks(), 2);
    }
}
