//! Error types for the gcode library.
//!
//! Two layers, two crates, on purpose (AGENTS.md section 4):
//!
//! - This module: `thiserror` types. A library hands back a typed error that a
//!   caller can match on. Adding a variant is a breaking change for that
//!   caller, which is why the enum is `#[non_exhaustive]`.
//! - The binary: `anyhow` context. A CLI adds "what I was trying to do" on the
//!   way out and never exposes the context type in an API.
//!
//! A variant exists only when some code path constructs it. Phase 0 does
//! filesystem-independent work, so there is no `Io` variant yet: it arrives
//! with the model downloader in Phase 1.3 and the history store in Phase 2.1.
//! An error variant with no constructor is a lie about the interface, and it
//! forces every caller to write a dead match arm.

/// Result alias for library operations.
///
/// The error is always [`Error`]. Callers that need context wrap this with
/// `anyhow::Context`, which they do in the binary, not here.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything the gcode library can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The user's home directory could not be determined.
    #[error(
        "could not determine the home directory; gcode needs it to find its \
         config, data, and model directories (set HOME, or use GCODE_CONFIG \
         and GCODE_HISTORY_FILE to point gcode at explicit locations)"
    )]
    HomeDirUnavailable,

    /// An environment variable held a value that is not usable as a path.
    #[error("{variable} is set to a non-absolute path, which gcode will not use")]
    InvalidEnvPath {
        /// The name of the offending variable.
        variable: &'static str,
    },
}
