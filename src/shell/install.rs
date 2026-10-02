//! The impure half of shell installation: the only code that writes to
//! `~/.bashrc` or `~/.zshrc`.
//!
//! Every decision about *what* the file should contain is made by the pure
//! functions in the parent module. This module does three things and no more: it
//! reads the current contents, it refuses when that read is not possible, and it
//! writes back exactly what it was told to.
//!
//! # Refusing is the point
//!
//! An rc file that exists but cannot be read is not an empty rc file. Truncating
//! it would delete the user's shell configuration — aliases, PATH edits, an
//! nvm loader — and then report success. So [`init`] returns
//! [`Error::ShellRcUnreadable`] and writes nothing.
//!
//! # Ownership of the hook file
//!
//! The hook itself is written to `~/.gcode/shell/`, owned by gcode. Removal
//! deletes the block and the file, in that order: if the block were removed
//! first and the file write then failed, the user's shell would be clean but
//! stale hook files would accumulate and `--check` would have nothing to verify.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::utils::paths::{self, Overrides};

use super::{plan_install, plan_remove, status, Kind, Outcome, Plan, Status};

/// A path, a resolved shell, and enough context to report what happened.
#[derive(Debug, Clone)]
pub struct Installer {
    /// The shell being configured.
    pub kind: Kind,
    /// The rc file to edit.
    pub rc_path: PathBuf,
    /// The gcode-owned copy of the hook.
    pub hook_path: PathBuf,
}

impl Installer {
    /// Resolves an installer for `kind`, taking every path from `overrides`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::HomeDirUnavailable`] when no home directory is known.
    pub fn resolve(kind: Kind, overrides: &Overrides) -> Result<Self> {
        Ok(Self {
            kind,
            rc_path: paths::shell_rc_file_with(overrides, kind)?,
            hook_path: paths::shell_hook_dir_with(overrides)?.join(kind.hook_file_name()),
        })
    }

    /// Builds an installer from explicit paths, for tests and for `--shell` with
    /// an explicit rc file.
    #[must_use]
    pub fn at(kind: Kind, rc_path: PathBuf, hook_path: PathBuf) -> Self {
        Self {
            kind,
            rc_path,
            hook_path,
        }
    }

    /// Installs the hook, or reports that it is already there.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ShellRcUnreadable`] if the rc file exists but cannot be
    /// read, [`Error::ShellRcWrite`] if it exists but cannot be written, and
    /// [`Error::ShellHookWrite`] if the hook file cannot be written.
    ///
    /// Returns the whole [`Plan`], not just the [`Outcome`], because the caller
    /// has to print the exact lines that were added. Returning only the outcome
    /// would force the caller to re-derive them from the file it just wrote.
    pub fn init(&self) -> Result<Plan> {
        let existing = read_rc(&self.rc_path)?;
        let plan = plan_install(&existing, &self.hook_path)?;
        if plan.outcome != Outcome::Unchanged {
            write_hook(&self.hook_path, self.kind)?;
            write_rc(&self.rc_path, &plan.content)?;
        }
        Ok(plan)
    }

    /// Reports the installed state without writing anything.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ShellRcUnreadable`] if the rc file exists but cannot be
    /// read, and propagates [`Error::ShellBlockMalformed`].
    pub fn check(&self) -> Result<Status> {
        let existing = read_rc(&self.rc_path)?;
        status(&existing, &self.hook_path)
    }

    /// Removes the block and the hook file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ShellRcUnreadable`] and [`Error::ShellRcWrite`] as
    /// [`Self::init`] does.
    pub fn remove(&self) -> Result<Plan> {
        let existing = read_rc(&self.rc_path)?;
        let plan = plan_remove(&existing)?;
        if plan.outcome == Outcome::Removed {
            write_rc(&self.rc_path, &plan.content)?;
            if self.hook_path.exists() {
                fs::remove_file(&self.hook_path).map_err(|source| Error::ShellHookWrite {
                    path: self.hook_path.clone(),
                    source,
                })?;
            }
        }
        Ok(plan)
    }
}

/// Reads an rc file, treating a missing one as empty.
fn read_rc(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(Error::ShellRcUnreadable {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Writes the hook file, creating its directory, and never leaving a stale file.
fn write_hook(path: &Path, kind: Kind) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::ShellHookWrite {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, kind.hook_source()).map_err(|source| Error::ShellHookWrite {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes the rc file, keeping whatever mode it already had.
///
/// `fs::write` truncates an existing file in place, so the user's `0600` stays
/// `0600` and their `0644` stays `0644`. A new file gets gcode's own umask.
fn write_rc(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|source| Error::ShellRcWrite {
        path: path.to_path_buf(),
        source,
    })
}

/// The lines a plan touched, for the exact-lines requirement.
///
/// Returns an empty list for [`Outcome::Unchanged`] and [`Outcome::NotInstalled`],
/// because nothing was touched and printing lines that were not added would be a
/// lie about what the tool did.
#[must_use]
pub fn changed_lines(plan: &Plan) -> &[String] {
    if plan.outcome.changed() {
        &plan.lines
    } else {
        &[]
    }
}
