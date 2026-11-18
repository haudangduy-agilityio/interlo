//! Tauri commands, one module per area. Each command is a thin wrapper over a
//! plain function so tests do not need a Tauri runtime.

pub mod history;
pub mod meeting;
pub mod models;
pub mod notes;
pub mod settings;
pub mod setup;
pub mod start_checks;
