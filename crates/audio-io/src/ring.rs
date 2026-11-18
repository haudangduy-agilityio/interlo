//! Fixed-capacity single-producer single-consumer ring buffer of f32.
//!
//! All memory is allocated in `ring_buffer`. Push and pop never allocate,
//! lock or block. On overflow the producer drops the newest samples (the ones
//! that do not fit; the oldest unread samples are kept) and reports how many,
//! so a slow consumer never stalls capture. The capacity is rounded up to a
//! power of two so a slot index is a mask, not a division.
//! Samples are stored as `AtomicU32` bit patterns, which keeps the code safe.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared {
    slots: Box<[AtomicU32]>,
    /// `slots.len() - 1`; the length is a power of two.
    mask: usize,
    /// Total samples ever read (consumer writes).
    head: AtomicUsize,
    /// Total samples ever written (producer writes).
    tail: AtomicUsize,
    dropped: AtomicU64,
}

pub struct RingProducer {
    shared: Arc<Shared>,
}

pub struct RingConsumer {
    shared: Arc<Shared>,
}

/// Creates a ring buffer with room for at least `capacity` samples (at least
/// 1). The real capacity is `capacity` rounded up to a power of two.
pub fn ring_buffer(capacity: usize) -> (RingProducer, RingConsumer) {
    let capacity = capacity
        .max(1)
        .checked_next_power_of_two()
        .unwrap_or(1 << (usize::BITS - 1));
    let slots: Box<[AtomicU32]> = (0..capacity).map(|_| AtomicU32::new(0)).collect();
    let shared = Arc::new(Shared {
        mask: capacity - 1,
        slots,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
        dropped: AtomicU64::new(0),
    });
    (
        RingProducer {
            shared: Arc::clone(&shared),
        },
        RingConsumer { shared },
    )
}

impl RingProducer {
    pub fn capacity(&self) -> usize {
        self.shared.slots.len()
    }

    /// Total samples dropped by overflow since creation.
    pub fn dropped_total(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Writes as many samples as fit and returns how many were dropped.
    pub fn push_slice(&mut self, input: &[f32]) -> usize {
        let s = &*self.shared;
        let cap = s.slots.len();
        let mask = s.mask;
        let tail = s.tail.load(Ordering::Relaxed);
        let head = s.head.load(Ordering::Acquire);
        let free = cap - tail.wrapping_sub(head);
        let n = free.min(input.len());
        for (i, v) in input[..n].iter().enumerate() {
            s.slots[tail.wrapping_add(i) & mask].store(v.to_bits(), Ordering::Relaxed);
        }
        s.tail.store(tail.wrapping_add(n), Ordering::Release);
        let dropped = input.len() - n;
        if dropped > 0 {
            s.dropped.fetch_add(dropped as u64, Ordering::Relaxed);
        }
        dropped
    }
}

impl RingConsumer {
    pub fn capacity(&self) -> usize {
        self.shared.slots.len()
    }

    /// Samples ready to read.
    pub fn len(&self) -> usize {
        let s = &*self.shared;
        s.tail
            .load(Ordering::Acquire)
            .wrapping_sub(s.head.load(Ordering::Relaxed))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total samples dropped by overflow since creation.
    pub fn dropped_total(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Reads up to `out.len()` samples in order and returns the count read.
    pub fn pop_slice(&mut self, out: &mut [f32]) -> usize {
        let s = &*self.shared;
        let mask = s.mask;
        let head = s.head.load(Ordering::Relaxed);
        let tail = s.tail.load(Ordering::Acquire);
        let n = tail.wrapping_sub(head).min(out.len());
        for (i, slot) in out[..n].iter_mut().enumerate() {
            *slot = f32::from_bits(s.slots[head.wrapping_add(i) & mask].load(Ordering::Relaxed));
        }
        s.head.store(head.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn pop(&mut self) -> Option<f32> {
        let mut one = [0.0f32];
        (self.pop_slice(&mut one) == 1).then_some(one[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_push_order() {
        let (mut p, mut c) = ring_buffer(8);
        assert_eq!(p.push_slice(&[1.0, 2.0, 3.0]), 0);
        assert_eq!(c.pop(), Some(1.0));
        assert_eq!(c.pop(), Some(2.0));
        assert_eq!(c.pop(), Some(3.0));
        assert_eq!(c.pop(), None);
    }

    #[test]
    fn overflow_reports_dropped_and_keeps_old_samples() {
        let (mut p, mut c) = ring_buffer(4);
        let dropped = p.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(dropped, 2);
        assert_eq!(p.dropped_total(), 2);
        assert_eq!(c.dropped_total(), 2);
        let mut out = [0.0; 8];
        assert_eq!(c.pop_slice(&mut out), 4);
        assert_eq!(&out[..4], &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn full_buffer_never_blocks() {
        let (mut p, _c) = ring_buffer(2);
        assert_eq!(p.push_slice(&[1.0, 2.0]), 0);
        assert_eq!(p.push_slice(&[3.0]), 1);
        assert_eq!(p.dropped_total(), 1);
    }

    #[test]
    fn wraps_around_and_keeps_order() {
        let (mut p, mut c) = ring_buffer(3);
        p.push_slice(&[1.0, 2.0, 3.0]);
        let mut out = [0.0; 2];
        assert_eq!(c.pop_slice(&mut out), 2);
        assert_eq!(p.push_slice(&[4.0, 5.0]), 0);
        let mut rest = [0.0; 3];
        assert_eq!(c.pop_slice(&mut rest), 3);
        assert_eq!(rest, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn capacity_rounds_up_to_a_power_of_two() {
        for (asked, got) in [(0, 1), (1, 1), (2, 2), (3, 4), (5, 8), (8, 8), (9, 16)] {
            let (p, c) = ring_buffer(asked);
            assert_eq!(p.capacity(), got, "asked {asked}");
            assert_eq!(c.capacity(), got, "asked {asked}");
        }
    }

    #[test]
    fn rounded_capacity_wraps_and_drops_newest_on_overflow() {
        // Asked 5, got 8.
        let (mut p, mut c) = ring_buffer(5);
        let first: Vec<f32> = (0..8).map(|i| i as f32).collect();
        assert_eq!(p.push_slice(&first), 0);
        assert_eq!(p.push_slice(&[8.0, 9.0]), 2);
        let mut out = [0.0; 6];
        assert_eq!(c.pop_slice(&mut out), 6);
        assert_eq!(out, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        // Wrap past the end of the slot array.
        assert_eq!(p.push_slice(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0]), 0);
        let mut rest = [0.0; 8];
        assert_eq!(c.pop_slice(&mut rest), 8);
        assert_eq!(rest, [6.0, 7.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
    }

    #[test]
    fn len_and_capacity() {
        let (mut p, c) = ring_buffer(0);
        assert_eq!(p.capacity(), 1);
        assert_eq!(c.capacity(), 1);
        assert!(c.is_empty());
        p.push_slice(&[0.25]);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn works_across_threads() {
        let (mut p, mut c) = ring_buffer(16);
        let t = std::thread::spawn(move || {
            let mut sent = 0u32;
            while sent < 1000 {
                if p.push_slice(&[sent as f32]) == 0 {
                    sent += 1;
                }
            }
        });
        let mut next = 0u32;
        while next < 1000 {
            if let Some(v) = c.pop() {
                assert_eq!(v, next as f32);
                next += 1;
            }
        }
        t.join().unwrap();
    }
}
