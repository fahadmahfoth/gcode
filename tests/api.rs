//! Proves the public surface works from outside the crate.
//!
//! Unit tests inside `src/` can reach private items, so they cannot catch a
//! module that was never made `pub`, a type that was made private, or a doc
//! link that only resolves internally. This test uses `gcode::...` paths and
//! nothing else, which is exactly what a downstream consumer sees.

use std::path::{Path, PathBuf};

use anyhow::Context;
use gcode::config::{self, Config, Env, RiskLevel};
use gcode::error::Error;
use gcode::utils::paths::{self, Overrides};

fn linux_like() -> Overrides {
    Overrides {
        home: Some(PathBuf::from("/home/user")),
        ..Overrides::default()
    }
}

/// A directory that removes itself, so a failing assertion cannot leave litter
/// behind for the next run.
///
/// Hand-rolled rather than pulled from a crate: the only thing needed is a name
/// no other process is using, and a test-only dependency would be a real cost
/// in the lockfile and the MSRV audit for the sake of one function.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        // The process id makes the name unique per run. The label makes it
        // readable in a failure. A timestamp would add nothing, because two
        // runs cannot overlap inside one test binary.
        let path = std::env::temp_dir().join(format!("gcode-test-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort. A test that fails mid-write may leave a file, and the
        // directory is already unique per run, so this cannot destroy anything
        // of value.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_config_path_is_reachable_from_outside_the_crate() {
    let path = paths::config_file_with(&linux_like()).unwrap();
    assert_eq!(path, PathBuf::from("/home/user/.config/gcode/config.toml"));
}

#[test]
fn the_history_path_is_reachable_from_outside_the_crate() {
    let path = paths::history_file_with(&linux_like()).unwrap();
    assert_eq!(path, PathBuf::from("/home/user/.gcode/history.jsonl"));
}

#[test]
fn the_error_type_is_matchable_by_consumers() {
    // If `Error` were not public, or if its variants changed shape, a consumer
    // could not branch on it. Phase 2's safety layer needs to do exactly this.
    let error: Error = Error::HomeDirUnavailable;
    assert!(error.to_string().contains("GCODE_CONFIG"));
}

#[test]
fn a_missing_home_directory_surfaces_as_a_typed_error() {
    let result = paths::config_file_with(&Overrides::default());
    assert!(matches!(result, Err(Error::HomeDirUnavailable)));
}

#[test]
fn the_two_layer_error_boundary_is_usable() {
    // `gcode::error::Result` is the library Result; the binary adds context
    // with anyhow. This is the shape every fallible call site will take.
    fn resolve() -> gcode::error::Result<PathBuf> {
        paths::history_file_with(&linux_like())
    }

    let with_context = resolve().context("reading the history location");
    assert!(with_context.is_ok());
}

// ── config, through the public API ──────────────────────────────────────────

/// Parses an argument list the way the binary does, and panics on bad input.
///
/// The binary unwraps a valid request, so a test that cannot parse its own
/// fixture has a bug in the test, not in gcode.
fn parse_request<const N: usize>(args: [&str; N]) -> gcode::cli::Parsed {
    gcode::cli::parse_from(args).expect("valid arguments")
}

#[test]
fn a_missing_config_file_is_not_an_error() {
    let dir = TempDir::new("missing-config");
    let path = dir.path().join("config.toml");
    assert_eq!(Config::load(&path).unwrap(), None);
}

#[test]
fn a_valid_config_file_loads() {
    let dir = TempDir::new("valid-config");
    let path = dir.write(
        "config.toml",
        "[model]\nname = \"kitty-bash-llm\"\n[safety]\nalways_confirm = \"HIGH\"\n",
    );
    let config = Config::load(&path).unwrap().expect("file exists");
    let effective = config
        .resolve(&Env::default(), &config::Overrides::default())
        .unwrap();
    assert_eq!(effective.model_name.as_deref(), Some("kitty-bash-llm"));
    assert_eq!(effective.always_confirm, RiskLevel::High);
}

#[test]
fn a_malformed_config_file_names_the_path_and_the_field() {
    // The roadmap makes this an acceptance criterion, and it is the difference
    // between a user fixing a typo in ten seconds and a user deleting a file.
    let dir = TempDir::new("malformed-config");
    let path = dir.write("config.toml", "[model]\ncontext_size = \"four thousand\"\n");

    let message = Config::load(&path).unwrap_err().to_string();
    assert!(message.contains("config.toml"), "no path: {message}");
    assert!(
        message.contains("model.context_size"),
        "no field: {message}"
    );
}

#[test]
fn an_unknown_config_key_is_named_too() {
    let dir = TempDir::new("unknown-key-config");
    let path = dir.write("config.toml", "[model]\ncontext_sizes = 4096\n");
    let message = Config::load(&path).unwrap_err().to_string();
    assert!(message.contains("context_sizes"), "no key: {message}");
}

#[test]
fn an_out_of_range_config_value_is_refused_with_its_range() {
    // A config file never passes through clap, so the CLI's bounds do not
    // protect this path. Refusing is still required.
    let dir = TempDir::new("range-config");
    let path = dir.write("config.toml", "[model]\ntemperature = 7.0\n");
    let config = Config::load(&path).unwrap().unwrap();
    let error = config
        .resolve(&Env::default(), &config::Overrides::default())
        .unwrap_err();
    assert!(matches!(error, Error::InvalidConfig { .. }));
    let message = error.to_string();
    assert!(message.contains("model.temperature"), "no field: {message}");
    assert!(message.contains("0.0"), "no range: {message}");
}

#[test]
fn always_confirm_cannot_be_critical() {
    let config: Config = toml::from_str("[safety]\nalways_confirm = \"CRITICAL\"\n").unwrap();
    let error = config
        .resolve(&Env::default(), &config::Overrides::default())
        .unwrap_err();
    assert!(matches!(error, Error::InvalidAlwaysConfirm));
}

#[test]
fn a_flag_beats_the_environment_and_the_file_end_to_end() {
    // The whole precedence chain through the public API: parse the flags, turn
    // them into overrides, load a file that disagrees, set an environment
    // variable that disagrees with both. The flag must win.
    let dir = TempDir::new("precedence-config");
    let path = dir.write("config.toml", "[model]\nname = \"from-file\"\n");

    let flags = parse_request(["gcode", "-c", "list files", "--model", "from-flag"]);
    let overrides = config::Overrides::from(&flags);

    let env = Env {
        model: Some("from-env".to_owned()),
        ..Env::default()
    };
    let file = Config::load(&path).unwrap().unwrap();
    let effective = file.resolve(&env, &overrides).unwrap();

    assert_eq!(effective.model_name.as_deref(), Some("from-flag"));
}

#[test]
fn the_environment_beats_the_file_end_to_end() {
    let dir = TempDir::new("env-config");
    let path = dir.write("config.toml", "[model]\nname = \"from-file\"\n");

    let flags = parse_request(["gcode", "-c", "list files"]);
    let env = Env {
        model: Some("from-env".to_owned()),
        ..Env::default()
    };
    let file = Config::load(&path).unwrap().unwrap();
    let effective = file
        .resolve(&env, &config::Overrides::from(&flags))
        .unwrap();

    assert_eq!(effective.model_name.as_deref(), Some("from-env"));
}

#[test]
fn the_file_beats_the_default_end_to_end() {
    let dir = TempDir::new("file-config");
    let path = dir.write("config.toml", "[model]\nname = \"from-file\"\n");

    let flags = parse_request(["gcode", "-c", "list files"]);
    let file = Config::load(&path).unwrap().unwrap();
    let effective = file
        .resolve(&Env::default(), &config::Overrides::from(&flags))
        .unwrap();

    assert_eq!(effective.model_name.as_deref(), Some("from-file"));
    // And the untouched defaults are still the documented ones.
    assert_eq!(effective.history_entries, 15);
    assert_eq!(effective.context_size, 4096);
}
