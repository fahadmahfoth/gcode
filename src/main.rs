//! gcode — a local-first, offline natural-language-to-shell-command tool.
//!
//! This binary is wiring only. Business logic lives in the library
//! (`src/lib.rs`), so that `tests/` can exercise it directly instead of
//! spawning a process and parsing stdout.
//!
//! The only `println!` in the crate is here. Everything else that talks to a
//! terminal belongs in `ui/`, which does not exist yet (AGENTS.md section 4:
//! no `println!` outside `main.rs` and `ui/`, so that `--json` stays honest).

use anyhow::{bail, Result};

/// What the process should do, once the arguments have been understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Print the version and exit successfully.
    Version,
}

fn main() -> Result<()> {
    let owned: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();

    match parse(&args)? {
        Action::Version => print_version(),
    }

    Ok(())
}

/// Turns raw arguments into an [`Action`].
///
/// Phase 0 has exactly one flag. Everything else is refused rather than ignored,
/// because a silent no-op that exits 0 is indistinguishable from success: a typo
/// would look like a completed run. The real surface arrives with the CLI layer
/// in Phase 1.1, and this function is where it lands.
///
/// Arguments after the flag are tolerated so that `gcode --version` keeps
/// working once the parser grows more options.
///
/// # Errors
///
/// Returns an error when no arguments are given, or when the first argument is
/// not a flag this build knows about.
fn parse(args: &[&str]) -> Result<Action> {
    match args.split_first() {
        None => bail!(
            "no request given; the generate mode (-c) arrives with the CLI layer in Phase 1.1"
        ),
        Some((flag, _)) if matches!(*flag, "--version" | "-V") => Ok(Action::Version),
        Some((flag, _)) => bail!("unknown argument {flag:?}"),
    }
}

/// Prints the version, taken from the manifest so the two cannot drift.
fn print_version() {
    println!("gcode {}", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    use super::{parse, Action};

    #[test]
    fn long_version_flag_is_accepted() {
        assert_eq!(parse(&["--version"]).unwrap(), Action::Version);
    }

    #[test]
    fn short_version_flag_is_accepted() {
        assert_eq!(parse(&["-V"]).unwrap(), Action::Version);
    }

    #[test]
    fn trailing_arguments_do_not_break_the_version_flag() {
        assert_eq!(parse(&["--version", "--json"]).unwrap(), Action::Version);
    }

    #[test]
    fn no_arguments_is_an_error_not_a_silent_version_print() {
        // Bare `gcode` is the interactive mode documented in USAGE.md. Until
        // that mode exists, refusing is the honest answer: printing a version
        // and exiting 0 would read as a successful run.
        let error = parse(&[]).unwrap_err().to_string();
        assert!(error.contains("no request given"), "unhelpful: {error}");
    }

    #[test]
    fn an_unknown_flag_names_itself() {
        let error = parse(&["--dry-run"]).unwrap_err().to_string();
        assert!(error.contains("--dry-run"), "unhelpful: {error}");
    }

    #[test]
    fn a_near_miss_flag_is_still_rejected() {
        // `--versions` must not be silently accepted as `--version`.
        assert!(parse(&["--versions"]).is_err());
        assert!(parse(&["-v"]).is_err());
    }

    #[test]
    fn unknown_flags_fail_even_when_a_known_one_follows() {
        // `--json --version` is not `--version`; refuse rather than skip.
        assert!(parse(&["--json", "--version"]).is_err());
    }
}
