//! The stage interface. A stage gets the file as 20 ms chunks and reports events through a
//! sink that stamps them from the session clock, so a stage cannot choose its own time.

use crate::energy_ref::EnergyRef;
use crate::log::EventSink;

/// Samples in one chunk: 20 ms at 16 kHz.
pub const CHUNK_SAMPLES: usize = 320;
/// Length of one chunk in microseconds.
pub const CHUNK_US: u64 = 20_000;

pub trait Stage {
    /// The name used on the command line and in the log.
    fn name(&self) -> &'static str;
    /// One chunk of samples. `offset` is the index of its first sample in the file.
    fn on_chunk(&mut self, offset: u64, chunk: &[f32], sink: &mut dyn EventSink);
    /// The input is over: report what is still open.
    fn finish(&mut self, sink: &mut dyn EventSink);
}

/// The stage with this name, or `None`.
pub fn find(name: &str) -> Option<Box<dyn Stage>> {
    match name {
        "energy_ref" => Some(Box::new(EnergyRef::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_stage_is_refused() {
        assert!(find("nope").is_none());
        assert!(find("").is_none());
        assert_eq!(find("energy_ref").map(|s| s.name()), Some("energy_ref"));
    }
}
