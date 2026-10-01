//! User interface helpers (non-interactive when possible).
//!
//! Phase 3.6 is the prompt, but the run loop must be able to refuse to prompt in
//! non-interactive contexts. This is a thin, dependency-free module: it never
//! reaches the model, and it does not read a credential store.

pub mod prompt;
