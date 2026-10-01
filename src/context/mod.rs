//! Assembling what the model sees, and the redaction that happens first.
//!
//! The ordering in this module is the whole point. Redaction runs before
//! assembly, in one function, because redaction applied at the call site is
//! redaction that gets skipped on the path somebody forgot (ADR 0006).

pub mod history;
pub mod prompt;
pub mod redact;
