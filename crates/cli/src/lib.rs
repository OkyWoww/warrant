//! # warrant-cli library
//!
//! Standardized exit codes and core CLI workflow coordination for Project WARRANT.

pub const EXIT_CLEAN: i32 = 0;
pub const EXIT_PROVEN_BUG: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

/// Standardized CLI exit code wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Clean,
    ProvenBug,
    Error,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        match self {
            Self::Clean => EXIT_CLEAN,
            Self::ProvenBug => EXIT_PROVEN_BUG,
            Self::Error => EXIT_ERROR,
        }
    }
}

pub mod demo;
pub mod eval_summary;
pub mod events;
pub mod reporter;
pub use demo::*;
pub use events::*;
pub use reporter::*;

