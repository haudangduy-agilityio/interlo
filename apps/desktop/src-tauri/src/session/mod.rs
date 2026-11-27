//! The shell side of a speech session (SL-01): the setup answer with the checked
//! model files, and the start and the end of the one session.

pub mod capture;
pub mod controller;
mod engines;
pub(crate) mod flight;
pub mod machine;
pub mod model_check;
pub mod policy;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

pub use controller::Session;
