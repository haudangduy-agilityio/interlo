//! Resampling contract. Real converters come in a later task.
//!
//! Which side of the audio callback (guardrail 8):
//!
//! - `Resampler::process_into` is CALLBACK SAFE. It writes into a scratch
//!   slice that the caller sized before the stream started (`max_output_len`),
//!   so it never allocates. An implementation must not lock, log or block
//!   either.
//! - `Resampler::process` is CONTROL SIDE ONLY. It grows a `Vec`, which can
//!   allocate, so it belongs to file import, tests and set-up code, never to
//!   the callback.
//! - `PassthroughResampler` implements both; the same holds for every converter
//!   added later.
//! - The ring buffer in `ring.rs` is callback safe on both ends.

/// Sample rate the recognition pipeline consumes.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// The scratch slice given to `Resampler::process_into` is too short for the
/// output of this block. Nothing was written: a block is never cut short.
/// Plain data, so the callback can return it without allocating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("scratch buffer holds {capacity} samples; this block needs {needed}")]
pub struct ScratchTooSmall {
    pub needed: usize,
    pub capacity: usize,
}

/// Converts a block of mono f32 samples.
pub trait Resampler {
    /// Most samples one call can produce for `input_len` input samples. The
    /// caller sizes its scratch buffer with this before the stream starts.
    fn max_output_len(&self, input_len: usize) -> usize;

    /// Callback-safe conversion: writes the output to the start of `out` and
    /// returns how many samples were written. Never allocates. When `out` is
    /// shorter than `max_output_len(input.len())` it writes nothing and
    /// returns `ScratchTooSmall`, so a caller counts the failure instead of
    /// playing a cut block.
    fn process_into(&mut self, input: &[f32], out: &mut [f32]) -> Result<usize, ScratchTooSmall>;

    /// Control-side conversion: appends to `out`, growing it when needed (this
    /// can allocate, so never call it from an audio callback).
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let start = out.len();
        out.resize(start + self.max_output_len(input.len()), 0.0);
        match self.process_into(input, &mut out[start..]) {
            Ok(written) => out.truncate(start + written),
            // Cannot happen: the slice was sized by `max_output_len`.
            Err(_) => out.truncate(start),
        }
    }
}

/// Copies samples unchanged. Valid when the input already runs at
/// `TARGET_SAMPLE_RATE`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PassthroughResampler;

impl Resampler for PassthroughResampler {
    fn max_output_len(&self, input_len: usize) -> usize {
        input_len
    }

    fn process_into(&mut self, input: &[f32], out: &mut [f32]) -> Result<usize, ScratchTooSmall> {
        let needed = self.max_output_len(input.len());
        let capacity = out.len();
        let Some(dst) = out.get_mut(..needed) else {
            return Err(ScratchTooSmall { needed, capacity });
        };
        dst.copy_from_slice(input);
        Ok(needed)
    }
}

/// A sample rate the converter cannot be built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sample rate {0} Hz is not supported")]
pub struct UnsupportedRate(pub u32);

/// Converts mono f32 from any input rate to `TARGET_SAMPLE_RATE`.
///
/// CALLBACK SAFE: everything is allocated in `new`; `process_into` copies,
/// accumulates and runs the FFT converter without allocating. The input block
/// may have any length: samples that do not fill the converter's fixed input
/// chunk wait in an accumulator for the next call. At the target rate the
/// samples pass through unchanged. The converter adds a fixed delay of about
/// 10 ms.
pub struct RateConverter {
    kind: ConverterKind,
}

enum ConverterKind {
    Pass,
    Fft(Box<FftState>),
}

struct FftState {
    fft: rubato::Fft<f32>,
    /// Input samples waiting for a full chunk.
    pending: Box<[f32]>,
    pending_len: usize,
    chunk_in: usize,
    out_max: usize,
}

impl RateConverter {
    pub fn new(rate_in: u32) -> Result<Self, UnsupportedRate> {
        if rate_in == TARGET_SAMPLE_RATE {
            return Ok(Self {
                kind: ConverterKind::Pass,
            });
        }
        let rate = usize::try_from(rate_in).map_err(|_| UnsupportedRate(rate_in))?;
        // 10 ms of input per chunk; the converter rounds it to a size that fits
        // the rate pair.
        let hint = (rate / 100).max(1);
        let fft = rubato::Fft::<f32>::new(
            rate,
            TARGET_SAMPLE_RATE as usize,
            hint,
            1,
            rubato::FixedSync::Input,
        )
        .map_err(|_| UnsupportedRate(rate_in))?;
        use rubato::Resampler as _;
        let chunk_in = fft.input_frames_next();
        let out_max = fft.output_frames_max();
        let pending = vec![0.0; fft.input_frames_max().max(chunk_in)].into_boxed_slice();
        Ok(Self {
            kind: ConverterKind::Fft(Box::new(FftState {
                fft,
                pending,
                pending_len: 0,
                chunk_in,
                out_max,
            })),
        })
    }
}

impl Resampler for RateConverter {
    fn max_output_len(&self, input_len: usize) -> usize {
        match &self.kind {
            ConverterKind::Pass => input_len,
            // Worst case: the accumulator already holds `chunk_in - 1` samples.
            ConverterKind::Fft(s) => input_len.div_ceil(s.chunk_in) * s.out_max,
        }
    }

    fn process_into(&mut self, input: &[f32], out: &mut [f32]) -> Result<usize, ScratchTooSmall> {
        let needed = self.max_output_len(input.len());
        let capacity = out.len();
        if capacity < needed {
            return Err(ScratchTooSmall { needed, capacity });
        }
        let s = match &mut self.kind {
            ConverterKind::Pass => {
                out[..needed].copy_from_slice(input);
                return Ok(needed);
            }
            ConverterKind::Fft(s) => s,
        };
        use rubato::Resampler as _;
        let mut consumed = 0;
        let mut written = 0;
        while consumed < input.len() {
            let take = (s.chunk_in - s.pending_len).min(input.len() - consumed);
            s.pending[s.pending_len..s.pending_len + take]
                .copy_from_slice(&input[consumed..consumed + take]);
            s.pending_len += take;
            consumed += take;
            if s.pending_len < s.chunk_in {
                continue;
            }
            s.pending_len = 0;
            let free = out.len() - written;
            let (Ok(src), Ok(mut dst)) = (
                rubato::audioadapter_buffers::direct::InterleavedSlice::new(
                    &s.pending[..s.chunk_in],
                    1,
                    s.chunk_in,
                ),
                rubato::audioadapter_buffers::direct::InterleavedSlice::new_mut(
                    &mut out[written..],
                    1,
                    free,
                ),
            ) else {
                continue;
            };
            if let Ok((_, produced)) = s.fft.process_into_buffer(&src, &mut dst, None) {
                written += produced;
            }
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Counts allocations made by the current thread only, so tests that run
    /// in parallel do not disturb each other.
    struct CountingAlloc;

    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    // SAFETY: every call is forwarded unchanged to the system allocator; the
    // only addition is a thread-local counter, which does not allocate. The
    // test `process_into_does_not_allocate` covers this allocator.
    #[allow(unsafe_code)]
    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
            System.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAlloc = CountingAlloc;

    fn allocations_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
        let before = ALLOCATIONS.with(Cell::get);
        let out = f();
        (out, ALLOCATIONS.with(Cell::get) - before)
    }

    #[test]
    fn target_rate_is_16k() {
        assert_eq!(TARGET_SAMPLE_RATE, 16_000);
    }

    #[test]
    fn passthrough_appends_input() {
        let mut r = PassthroughResampler;
        let mut out = vec![0.5];
        r.process(&[1.0, 2.0], &mut out);
        assert_eq!(out, vec![0.5, 1.0, 2.0]);
    }

    #[test]
    fn process_into_writes_the_block_to_the_start_of_the_scratch() {
        let mut r = PassthroughResampler;
        let mut scratch = [9.0; 4];
        assert_eq!(r.process_into(&[1.0, 2.0], &mut scratch), Ok(2));
        assert_eq!(scratch, [1.0, 2.0, 9.0, 9.0]);
    }

    #[test]
    fn process_into_refuses_a_scratch_that_is_too_short_and_writes_nothing() {
        let mut r = PassthroughResampler;
        let mut scratch = [9.0; 1];
        assert_eq!(
            r.process_into(&[1.0, 2.0], &mut scratch),
            Err(ScratchTooSmall {
                needed: 2,
                capacity: 1
            })
        );
        assert_eq!(scratch, [9.0], "no partial block");
        // The boundary: exactly `max_output_len` samples is enough.
        let mut exact = [0.0; 2];
        assert_eq!(r.process_into(&[1.0, 2.0], &mut exact), Ok(2));
    }

    #[test]
    fn process_into_does_not_allocate() {
        let mut r = PassthroughResampler;
        let input = vec![0.25f32; 480];
        let mut scratch = vec![0.0f32; r.max_output_len(input.len())];
        // Warm-up call so lazy one-time set-up is not counted.
        r.process_into(&input, &mut scratch).unwrap();

        let (result, allocations) = allocations_during(|| {
            let mut last = Ok(0);
            for _ in 0..100 {
                last = r.process_into(&input, &mut scratch);
            }
            last
        });

        assert_eq!(result, Ok(480));
        assert_eq!(allocations, 0, "the callback-safe path allocated");
    }

    #[test]
    fn the_counting_allocator_sees_the_vec_path_grow() {
        // Guards the test above: if the counter were broken it would read 0
        // here too, and the no-allocation result would prove nothing.
        let mut r = PassthroughResampler;
        let input = vec![0.25f32; 480];
        let mut out = Vec::new();
        let ((), allocations) = allocations_during(|| r.process(&input, &mut out));
        assert!(allocations > 0, "the counter did not see an allocation");
    }
}
