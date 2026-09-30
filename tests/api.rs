//! Proves the public surface works from outside the crate.
//!
//! Unit tests inside `src/` can reach private items, so they cannot catch a
//! module that was never made `pub`, a type that was made private, or a doc
//! link that only resolves internally. This test uses `gcode::...` paths and
//! nothing else, which is exactly what a downstream consumer sees.

use std::path::PathBuf;

use anyhow::Context;
use gcode::error::Error;
use gcode::utils::paths::{self, Overrides};

fn linux_like() -> Overrides {
    Overrides {
        home: Some(PathBuf::from("/home/user")),
        ..Overrides::default()
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
