//! Bench harness M0-03: runs one stage on one WAV file and logs event times on one session clock.
//! A throwaway measurement tool (D-190): product code never imports it. The log carries event
//! names, times and numbers only (guardrail 6); no stage can write text into it.

pub mod args;
pub mod clock;
pub mod energy_ref;
pub mod log;
pub mod run;
pub mod stage;
pub mod synth;
pub mod wav;
