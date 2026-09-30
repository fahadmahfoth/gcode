//! gcode — a local-first, offline natural-language-to-shell-command tool.
//!
//! This binary is wiring only. Business logic lives in the library
//! (`src/lib.rs`), so that `tests/` can exercise it directly instead of
//! spawning a process and parsing stdout.
//!
//! The only `println!` in the crate is here. Everything else that talks to a
//! terminal belongs in `ui/`, which does not exist yet (AGENTS.md section 4:
//! no `println!` outside `main.rs` and `ui/`, so that `--json` stays honest).

use anyhow::Result;

use gcode::cli::{self, Mode};

fn main() -> Result<()> {
    // `parse_from` exits the process itself for `--help` and `--version`, which
    // is the one behaviour that has to bypass the Result type: both are
    // successful outcomes with nothing to hand back.
    let parsed = match cli::parse_from(std::env::args_os()) {
        Ok(parsed) => parsed,
        // clap raises `ErrorKind::DisplayHelp` and `DisplayVersion` for
        // `--help` and `--version` and returns them here as `Err`. They are
        // successful outcomes that happen to travel as errors, so they are
        // separated from a real usage error: a script that runs
        // `gcode --version` must not see a failure exit code.
        Err(cli::ParseOutcome::Handled(outcome)) => {
            print!("{outcome}");
            std::process::exit(0);
        }
        Err(cli::ParseOutcome::Usage(message)) => {
            eprintln!("gcode: {message}");
            // 2, not 1: this is a usage error, and the exit code is what a
            // script keys on to tell "you typed it wrong" from "it broke".
            std::process::exit(2);
        }
    };

    // Each mode lands in Phase 1.2 through 1.8. Until then each one names
    // itself and says what is missing, rather than pretending to have run.
    // `not_implemented` always fails, so reaching the end of this function is
    // currently unreachable; returning Ok keeps that from being a lie if a
    // future mode genuinely succeeds without further work here.
    match parsed.mode {
        Mode::Generate { .. } => not_implemented("generate"),
        Mode::Fix => not_implemented("fix"),
        Mode::Complete { .. } => not_implemented("complete"),
        Mode::Explain { .. } => not_implemented("explain"),
        Mode::Interactive => not_implemented("interactive"),
    }
}

/// Reports that a parsed mode has no implementation yet.
///
/// Returns `Result` rather than calling `exit` directly so that every arm of
/// the `match` above has the same type. Adding a `Mode` variant later makes the
/// match non-exhaustive, so it cannot be forgotten.
///
/// # Errors
///
/// Always returns an error.
fn not_implemented(mode: &str) -> Result<()> {
    anyhow::bail!(
        "the {mode} mode is parsed but not implemented yet; \
         the inference pipeline lands in Phase 1"
    )
}

#[cfg(test)]
mod tests {
    use super::not_implemented;

    #[test]
    fn an_unimplemented_mode_names_itself() {
        let message = not_implemented("generate").unwrap_err().to_string();
        assert!(message.contains("generate"), "unhelpful: {message}");
    }

    #[test]
    fn an_unimplemented_mode_says_it_is_not_done() {
        // The whole point of the message is that it cannot be misread as a
        // successful run.
        let message = not_implemented("fix").unwrap_err().to_string();
        assert!(message.contains("not implemented"), "unhelpful: {message}");
    }
}
