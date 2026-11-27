//! meeting-core: protocol, session lifecycle, transcript coordinator and
//! egress guard for the meeting interpreter.
//!
//! Audio capture, provider calls and UI live outside this crate. They talk to
//! it through `protocol::Envelope` only.
//!
//! Must never: store audio; rewrite a final line; let text leave the Mac by
//! default.

pub mod coordinator;
pub mod egress;
pub mod policy;
pub mod protocol;
pub mod session;
pub mod start_checks;
