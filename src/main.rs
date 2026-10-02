//! gcode — a local-first, offline natural-language-to-shell-command tool.
//!
//! This binary is wiring only. Business logic lives in the library
//! (`src/runtime.rs`), so that `tests/` can exercise it directly instead of
//! spawning a process and parsing stdout.
//!
//! The only `println!` in the crate is here. Everything else that talks to a
//! terminal belongs in `ui/`, which keeps `--json` a serialisation of data
//! rather than a scrape of the human-facing renderer.

use std::sync::Arc;

use gcode::cli;
use gcode::config::RiskLevel;
use gcode::runtime::{run_configured, DenyAll, DEFAULT_CONFIRM_AT};
use gcode::safety::Risk;
use gcode::ui::prompt::Prompt;

/// Loads and resolves the configuration.
///
/// Split out so `main` stays a sequence of decisions rather than a sequence of
/// error handling, and so the failure path is one function a test can name.
fn load_effective_config(
    parsed: &gcode::cli::Parsed,
) -> gcode::Result<gcode::config::EffectiveConfig> {
    use gcode::config::{Config, Env, Overrides};

    // Two different `Overrides` types, and the names collide on purpose: the
    // paths module resolves where files live from the environment, while the
    // config module resolves values from the command line. Both are needed and
    // they answer different questions.
    let path_overrides = gcode::utils::paths::Overrides::from_env()?;
    let path = gcode::utils::paths::config_file_with(&path_overrides)?;

    // A missing file is not an error: it means "all defaults", which is what a
    // first run looks like.
    let file = Config::load(&path)?.unwrap_or_default();
    file.resolve(&Env::from_process(), &Overrides::from(parsed))
}

fn main() {
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

    // No model is loaded until Phase 1.5 lands the sampler, so the modes that
    // generate a command report that honestly rather than inventing a plausible
    // answer. `--explain` needs no model, which is why it works today.
    let engine: Option<Arc<dyn gcode::inference::InferenceEngine>> = None;

    // `always_confirm` is resolved once, here, so the run loop never touches the
    // filesystem and the CLI never guesses a threshold.
    //
    // A config that will not load is reported and then set to the documented
    // default. That is deliberate: refusing to run because a config file is
    // malformed would be safe but useless, and silently continuing at the
    // configured-but-unreadable threshold would be neither. The message means
    // the user learns about it either way.
    let config = match load_effective_config(&parsed) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("gcode: {error}");
            gcode::config::defaults()
        }
    };
    let always_confirm = Risk::from(config.always_confirm);
    debug_assert_eq!(
        DEFAULT_CONFIRM_AT,
        Risk::from(RiskLevel::Medium),
        "the run loop's default and the config default must agree"
    );

    // The downloader's HTTP client exists only when the feature is compiled in.
    // Without it the download mode reports that honestly; it never falls back to
    // a shell-out or a different transport (ADR 0001).
    #[cfg(feature = "download")]
    let transport: Option<Box<dyn gcode::model::download::Transport>> =
        Some(Box::new(gcode::model::download::HttpTransport::new()));
    #[cfg(not(feature = "download"))]
    let transport: Option<Box<dyn gcode::model::download::Transport>> = None;

    // Live progress on stderr, and never for `--json`: the progress line is not
    // part of the machine-readable contract.
    let mut progress = gcode::ui::progress::line;
    let on_progress: Option<&mut dyn FnMut(u64, u64)> = if parsed.json {
        None
    } else {
        Some(&mut progress)
    };

    // The prompt is only ever constructed for an interactive run. `--json` gets
    // `DenyAll`, which cannot consent to anything, so a machine-readable
    // invocation can never block on a human and never run an unconfirmed command.
    //
    // The borrow cannot simply be a `match` arm here because `Prompt` and
    // `DenyAll` are different types; the `DenyAll` variant is what makes the
    // "no prompt in a pipe" rule a compile-time fact about this function.
    let outcome = if parsed.json || !Prompt::is_interactive() {
        run_configured(
            &parsed,
            &config,
            engine,
            &mut DenyAll,
            always_confirm,
            transport.as_deref(),
            on_progress,
        )
    } else {
        run_configured(
            &parsed,
            &config,
            engine,
            &mut Prompt::stderr(),
            always_confirm,
            transport.as_deref(),
            on_progress,
        )
    };

    match outcome {
        Ok(output) => {
            if parsed.json {
                // `--list-models` is a list, so its JSON is an array rather than
                // the single object every command-producing mode emits. `run`
                // assembled the human table; this path re-renders the same
                // registry as JSON.
                if matches!(parsed.mode, gcode::cli::Mode::ListModels) {
                    match gcode::runtime::list_models_json() {
                        Ok(json) => {
                            println!("{json}");
                            return;
                        }
                        Err(error) => {
                            eprintln!("gcode: {error}");
                            std::process::exit(1);
                        }
                    }
                }
                // One object, one line, no colour, no prompt text anywhere.
                println!("{}", output.to_json());
                return;
            }
            if output.command.is_empty() {
                // A mode that produces no command — the shell-hook modes. Printing
                // an empty line and then "safe" would claim a risk level for a
                // file edit, which is not what those modes do.
                if let Some(explanation) = &output.explanation {
                    println!("{explanation}");
                }
                return;
            }
            // The command first, on its own line, so it can be selected and
            // copied without the risk line.
            println!("{}", output.command);
            eprintln!("{}", output.level);
            if let Some(explanation) = &output.explanation {
                eprintln!("{explanation}");
            }
        }
        Err(error) => {
            eprintln!("gcode: {error}");
            // 1, not 2: this is not a usage error. 2 is reserved for "you typed it
            // wrong", and a script that treats a blocked command as a typo will
            // show the user a usage message for a safety refusal.
            std::process::exit(1);
        }
    }
}
