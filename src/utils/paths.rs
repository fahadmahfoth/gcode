//! Where gcode keeps its files.
//!
//! Every path goes through this module (AGENTS.md section 4). Two reasons: a
//! platform change touches one file rather than a dozen, and there is exactly
//! one place to look when a user asks "where did that file go".
//!
//! # Layout
//!
//! | What | Linux | macOS |
//! |---|---|---|
//! | Config | `~/.config/gcode/config.toml` | `~/.config/gcode/config.toml` |
//! | Models | `~/.local/share/gcode/models/` | `~/Library/Application Support/gcode/models/` |
//! | History | `~/.gcode/history.jsonl` | `~/.gcode/history.jsonl` |
//!
//! History is deliberately outside the XDG data directory on every platform,
//! because [ADR 0005](../../docs/adr/0005-jsonl-history-store.md) fixes that
//! location and an accepted ADR is not changed quietly. `GCODE_HISTORY_FILE`
//! overrides it.
//!
//! # Testability
//!
//! Environment variables are process-global, and a test that calls
//! `std::env::set_var` while another thread reads it is a data race that
//! produces a heisenbug, not a test failure. So resolution is split in two:
//! [`Overrides::from_env`] is the only impure part, and every `*_with`
//! function is pure. The tests pass a synthetic [`Overrides`] and never read or
//! write the real environment or the real home directory.

use std::env;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The app name used to build every directory under the system base dirs.
const APP: &str = "gcode";

/// Which platform's directory layout to use.
///
/// Injected rather than read from `cfg!` at the point of use, so that the test
/// suite can verify the Linux layout on a Mac and the macOS layout on Linux.
/// Otherwise each platform's CI row only ever exercises its own branch, and a
/// layout that is wrong on the other platform stays wrong until release night.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// `$XDG_DATA_HOME`, else `~/.local/share`.
    Linux,
    /// `~/Library/Application Support`.
    MacOs,
    /// Whichever platform this binary was compiled for. The default.
    Current,
}

impl Os {
    /// The platform this binary was compiled for.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// Collapses [`Os::Current`] so callers can match on a concrete platform.
    #[must_use]
    fn concrete(self) -> Self {
        match self {
            Self::Current => Self::current(),
            concrete => concrete,
        }
    }
}

impl Default for Os {
    fn default() -> Self {
        Self::current()
    }
}

/// Environment overrides for path resolution.
///
/// [`Default`] resolves using the real environment. Tests build one by hand so
/// they are hermetic.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Overrides {
    /// The user's home directory. `None` means it could not be determined.
    pub home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME`, if set.
    pub xdg_config_home: Option<PathBuf>,
    /// `XDG_DATA_HOME`, if set.
    pub xdg_data_home: Option<PathBuf>,
    /// `GCODE_CONFIG`, a full path to the config file.
    pub config_file: Option<PathBuf>,
    /// `GCODE_HISTORY_FILE`, a full path to the history file.
    pub history_file: Option<PathBuf>,
    /// The platform layout to resolve for. Defaults to [`Os::Current`].
    pub os: Os,
    /// Overrides the macOS Application Support base outright. `None` derives it
    /// from `home` and `os`.
    pub application_support: Option<PathBuf>,
}

impl Overrides {
    /// Reads the real environment.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidEnvPath`] when `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
    /// `GCODE_CONFIG`, or `GCODE_HISTORY_FILE` is set to a relative path.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            home: dirs::home_dir(),
            xdg_config_home: env_path("XDG_CONFIG_HOME")?,
            xdg_data_home: env_path("XDG_DATA_HOME")?,
            config_file: env_path("GCODE_CONFIG")?,
            history_file: env_path("GCODE_HISTORY_FILE")?,
            os: Os::current(),
            application_support: None,
        })
    }
}

/// Reads an environment variable that must be an absolute path if present.
///
/// Returns `Ok(None)` when unset or empty, because an empty variable is
/// conventionally treated as unset by every other tool. A relative value is an
/// error: gcode runs from whatever directory the user happens to be in, so a
/// relative config path would resolve differently depending on the cwd.
///
/// # Errors
///
/// Returns [`Error::InvalidEnvPath`] when the variable is set to a path that
/// is not absolute.
fn env_path(variable: &'static str) -> Result<Option<PathBuf>> {
    match env::var_os(variable) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                Ok(Some(path))
            } else {
                Err(Error::InvalidEnvPath { variable })
            }
        }
    }
}

/// The directory holding `config.toml`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn config_dir() -> Result<PathBuf> {
    config_dir_with(&Overrides::from_env()?)
}

/// The directory holding `config.toml`, resolved against `overrides`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn config_dir_with(overrides: &Overrides) -> Result<PathBuf> {
    // macOS is included in the XDG fallback on purpose. dirs::config_dir()
    // returns ~/Library/Application Support there, but gcode's documented
    // config location is ~/.config/gcode on every platform, and a config file
    // is something a user edits by hand. One location for all platforms is
    // easier to document and easier to remember.
    match overrides.xdg_config_home.clone() {
        Some(base) => Ok(base.join(APP)),
        None => Ok(home(overrides)?.join(".config").join(APP)),
    }
}

/// The full path to the config file.
///
/// `GCODE_CONFIG` wins over the discovered location, so a user can keep a
/// project-local config without changing any global state.
///
/// # Errors
///
/// Returns [`Error::InvalidEnvPath`] when `GCODE_CONFIG` is set to a relative
/// path, or [`Error::HomeDirUnavailable`] when no explicit path is set and the
/// home directory cannot be determined.
pub fn config_file() -> Result<PathBuf> {
    config_file_with(&Overrides::from_env()?)
}

/// The full path to the config file, resolved against `overrides`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn config_file_with(overrides: &Overrides) -> Result<PathBuf> {
    match overrides.config_file.clone() {
        Some(path) => Ok(path),
        None => Ok(config_dir_with(overrides)?.join("config.toml")),
    }
}

/// The data directory: models and anything else that persists.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn data_dir() -> Result<PathBuf> {
    data_dir_with(&Overrides::from_env()?)
}

/// The data directory, resolved against `overrides`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn data_dir_with(overrides: &Overrides) -> Result<PathBuf> {
    if let Some(base) = overrides.xdg_data_home.clone() {
        return Ok(base.join(APP));
    }
    let base = match overrides.application_support.clone() {
        Some(base) => base,
        None => match overrides.os.concrete() {
            Os::MacOs => home(overrides)?.join("Library").join("Application Support"),
            _ => home(overrides)?.join(".local").join("share"),
        },
    };
    Ok(base.join(APP))
}

/// The directory holding downloaded model weights.
///
/// # Errors
///
/// Propagates whatever [`data_dir`] returns.
pub fn models_dir() -> Result<PathBuf> {
    models_dir_with(&Overrides::from_env()?)
}

/// The directory holding downloaded model weights, resolved against `overrides`.
///
/// # Errors
///
/// Propagates whatever [`data_dir_with`] returns.
pub fn models_dir_with(overrides: &Overrides) -> Result<PathBuf> {
    Ok(data_dir_with(overrides)?.join("models"))
}

/// The directory holding `history.jsonl`.
///
/// Separate from [`data_dir`] because ADR 0005 fixes this location on every
/// platform and an accepted ADR is not changed quietly.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn history_dir() -> Result<PathBuf> {
    history_dir_with(&Overrides::from_env()?)
}

/// The history directory, resolved against `overrides`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn history_dir_with(overrides: &Overrides) -> Result<PathBuf> {
    Ok(home(overrides)?.join(format!(".{APP}")))
}

/// The full path to the append-only history file.
///
/// # Errors
///
/// Returns [`Error::InvalidEnvPath`] when `GCODE_HISTORY_FILE` is set to a
/// relative path, or [`Error::HomeDirUnavailable`] when no explicit path is set
/// and the home directory cannot be determined.
pub fn history_file() -> Result<PathBuf> {
    history_file_with(&Overrides::from_env()?)
}

/// The full path to the history file, resolved against `overrides`.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when the home directory cannot be
/// determined.
pub fn history_file_with(overrides: &Overrides) -> Result<PathBuf> {
    match overrides.history_file.clone() {
        Some(path) => Ok(path),
        None => Ok(history_dir_with(overrides)?.join("history.jsonl")),
    }
}

/// The home directory, or [`Error::HomeDirUnavailable`] with the recovery hint.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when no home directory is known.
fn home(overrides: &Overrides) -> Result<&Path> {
    overrides.home.as_deref().ok_or(Error::HomeDirUnavailable)
}

/// The config file path as a string, for embedding in an error message.
///
/// [`Error::ModelNameRequired`](crate::Error::ModelNameRequired) names the file
/// a user has to edit, and a message that cannot say where that is sends them
/// looking. Falls back to the conventional location rather than propagating: the
/// error is already being reported, and a second failure would replace a useful
/// message with a nested one.
pub fn config_file_display() -> String {
    config_file().map_or_else(
        |_| ".config/gcode/config.toml".to_owned(),
        |p| p.display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linux_like() -> Overrides {
        Overrides {
            home: Some(PathBuf::from("/home/user")),
            os: Os::Linux,
            ..Overrides::default()
        }
    }

    fn macos_like() -> Overrides {
        Overrides {
            home: Some(PathBuf::from("/Users/user")),
            os: Os::MacOs,
            ..Overrides::default()
        }
    }

    #[test]
    fn config_dir_follows_xdg_config_home() {
        let o = Overrides {
            xdg_config_home: Some(PathBuf::from("/tmp/xdg-config")),
            ..linux_like()
        };
        assert_eq!(
            config_dir_with(&o).unwrap(),
            PathBuf::from("/tmp/xdg-config/gcode")
        );
    }

    #[test]
    fn config_dir_falls_back_to_dot_config_on_linux() {
        assert_eq!(
            config_dir_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.config/gcode")
        );
    }

    #[test]
    fn config_dir_falls_back_to_dot_config_on_macos() {
        // Documented as ~/.config/gcode on every platform, deliberately not
        // the Application Support directory that dirs would return here.
        assert_eq!(
            config_dir_with(&macos_like()).unwrap(),
            PathBuf::from("/Users/user/.config/gcode")
        );
    }

    #[test]
    fn config_file_is_derived_from_config_dir() {
        assert_eq!(
            config_file_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.config/gcode/config.toml")
        );
    }

    #[test]
    fn gcode_config_overrides_the_discovered_config_file() {
        let o = Overrides {
            config_file: Some(PathBuf::from("/srv/gcode/project.toml")),
            ..linux_like()
        };
        assert_eq!(
            config_file_with(&o).unwrap(),
            PathBuf::from("/srv/gcode/project.toml")
        );
    }

    #[test]
    fn data_dir_follows_xdg_data_home() {
        let o = Overrides {
            xdg_data_home: Some(PathBuf::from("/tmp/xdg-data")),
            ..linux_like()
        };
        assert_eq!(
            data_dir_with(&o).unwrap(),
            PathBuf::from("/tmp/xdg-data/gcode")
        );
    }

    #[test]
    fn data_dir_falls_back_to_local_share_on_linux() {
        assert_eq!(
            data_dir_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.local/share/gcode")
        );
    }

    #[test]
    fn data_dir_falls_back_to_application_support_on_macos() {
        assert_eq!(
            data_dir_with(&macos_like()).unwrap(),
            PathBuf::from("/Users/user/Library/Application Support/gcode")
        );
    }

    #[test]
    fn models_dir_sits_inside_the_data_dir() {
        assert_eq!(
            models_dir_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.local/share/gcode/models")
        );
    }

    #[test]
    fn history_dir_is_outside_the_data_dir_on_every_platform() {
        // ADR 0005 fixes ~/.gcode/history.jsonl for all platforms. This test
        // exists to make that split visible: if it ever changes, this fails.
        for o in [linux_like(), macos_like()] {
            let history = history_dir_with(&o).unwrap();
            let data = data_dir_with(&o).unwrap();
            assert!(!history.starts_with(&data), "history escaped the data dir");
        }
    }

    #[test]
    fn history_file_is_derived_from_history_dir() {
        assert_eq!(
            history_file_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.gcode/history.jsonl")
        );
    }

    #[test]
    fn gcode_history_file_overrides_the_discovered_location() {
        let o = Overrides {
            history_file: Some(PathBuf::from("/tmp/h.jsonl")),
            ..linux_like()
        };
        assert_eq!(
            history_file_with(&o).unwrap(),
            PathBuf::from("/tmp/h.jsonl")
        );
    }

    #[test]
    fn current_agrees_with_the_compilation_target() {
        // If this fails, `Os::current` and the `cfg!` it mirrors have drifted.
        assert_eq!(
            Os::current(),
            if cfg!(target_os = "macos") {
                Os::MacOs
            } else {
                Os::Linux
            }
        );
    }

    #[test]
    fn current_collapses_to_a_concrete_platform() {
        assert_ne!(Os::current().concrete(), Os::Current);
        assert_eq!(Os::Linux.concrete(), Os::Linux);
        assert_eq!(Os::MacOs.concrete(), Os::MacOs);
    }

    #[test]
    fn an_explicit_application_support_base_wins() {
        let o = Overrides {
            os: Os::Linux,
            application_support: Some(PathBuf::from("/srv/appdata")),
            ..linux_like()
        };
        assert_eq!(
            data_dir_with(&o).unwrap(),
            PathBuf::from("/srv/appdata/gcode")
        );
    }

    #[test]
    fn the_default_overrides_use_the_host_platform() {
        assert_eq!(Overrides::default().os, Os::current());
    }

    #[test]
    fn both_platform_layouts_are_verifiable_on_either_host() {
        // The two assertions below are the reason `Overrides` carries an `os`
        // field. Without it, one of them could only ever run on its own CI row.
        assert_eq!(
            data_dir_with(&linux_like()).unwrap(),
            PathBuf::from("/home/user/.local/share/gcode")
        );
        assert_eq!(
            data_dir_with(&macos_like()).unwrap(),
            PathBuf::from("/Users/user/Library/Application Support/gcode")
        );
    }

    #[test]
    fn a_missing_home_directory_is_an_error_not_a_panic() {
        let o = Overrides::default();
        for result in [config_dir_with(&o), data_dir_with(&o), history_dir_with(&o)] {
            assert!(matches!(result, Err(Error::HomeDirUnavailable)));
        }
    }

    #[test]
    fn the_error_message_tells_the_user_what_to_do() {
        let message = Error::HomeDirUnavailable.to_string();
        assert!(message.contains("GCODE_CONFIG"), "no workaround offered");
    }

    #[test]
    fn a_relative_env_path_is_rejected_rather_than_resolved() {
        // A relative path would resolve against the cwd, so the same config
        // would be a different file depending on where gcode was run from.
        //
        // This is one of only three tests that touch the process environment.
        // The rest use synthetic Overrides, so nothing here can race another
        // test. The variable names are unique to these tests for the same
        // reason.
        const PROBE: &str = "GCODE_TEST_RELATIVE_PATH_PROBE";

        env::set_var(PROBE, "relative.toml");
        let relative = env_path(PROBE);
        env::remove_var(PROBE);

        assert!(matches!(
            relative,
            Err(Error::InvalidEnvPath { variable }) if variable == PROBE
        ));
    }

    #[test]
    fn an_absolute_env_path_is_accepted_and_an_empty_one_means_unset() {
        const PROBE: &str = "GCODE_TEST_PATH_PROBE";

        env::set_var(PROBE, "/srv/gcode/config.toml");
        let absolute = env_path(PROBE);
        env::set_var(PROBE, "");
        let empty = env_path(PROBE);
        env::remove_var(PROBE);

        assert_eq!(
            absolute.unwrap(),
            Some(PathBuf::from("/srv/gcode/config.toml"))
        );
        assert_eq!(empty.unwrap(), None, "empty is conventionally unset");
    }

    #[test]
    fn an_unset_variable_yields_none() {
        const PROBE: &str = "GCODE_TEST_DEFINITELY_UNSET_PROBE";
        env::remove_var(PROBE);
        assert_eq!(env_path(PROBE).unwrap(), None);
    }
}
