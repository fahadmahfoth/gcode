//! Installing the history hook into a user's shell configuration.
//!
//! ADR 0007 decides that gcode observes the shell through a hook chained onto
//! `PROMPT_COMMAND` or `precmd`, integrated **additively** with whatever the
//! user already has. This module owns the half of that decision which touches
//! files: locating the right rc file, writing a marked block into it, and
//! taking that block out again byte for byte.
//!
//! # Two halves, one boundary
//!
//! [`plan_install`], [`plan_remove`], and [`status`] are pure. They take the
//! current contents of an rc file and return what the new contents would be.
//! They touch no disk and read no environment, so the byte-precise behaviour the
//! ADR requires is testable with string literals rather than with a temporary
//! home directory and a shell.
//!
//! [`install::init`] is the impure half. It reads a file, refuses when it cannot
//! be read, calls one of the pure functions, and writes the result back. That is
//! the only place in gcode that writes to `~/.bashrc` or `~/.zshrc`, and it
//! writes nothing outside the markers.
//!
//! # Why the hook file is copied rather than referenced in place
//!
//! The rc block sources a gcode-owned copy under `~/.gcode/shell/`, not the
//! source tree and not a path next to the binary. A packaged install may put the
//! binary anywhere, and it may move on upgrade; a block in the user's rc that
//! points at a stale path is a broken prompt, which ADR 0007 calls a P0
//! correctness bug. Embedding the hooks with `include_str!` means the block is
//! always valid for the binary version that wrote it.

pub mod install;

#[cfg(test)]
mod tests;

use std::ffi::OsStr;
use std::path::Path;

use crate::error::{Error, Result};

/// The first line of the block this module writes.
pub const BEGIN_MARKER: &str = "# >>> gcode init >>>";

/// The last line of the block this module writes.
pub const END_MARKER: &str = "# <<< gcode init <<<";

/// Prefix of the version line inside the block.
///
/// Doubles as the parse target for `--check`, which has to report the version
/// that installed the block. Parsing it from the file is the only way to know:
/// the block may have been written by an older gcode.
const VERSION_PREFIX: &str = "# gcode hook ";

/// Which shell to install into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// bash, hooked via `PROMPT_COMMAND`.
    Bash,
    /// zsh, hooked via `precmd_functions`.
    Zsh,
}

impl Kind {
    /// Both supported shells, for iteration in tests and help text.
    pub const ALL: [Self; 2] = [Self::Bash, Self::Zsh];

    /// Resolves a shell from the basename of `$SHELL`.
    ///
    /// The full path is matched on its file name only, because `$SHELL` is an
    /// absolute login-shell path and `/bin/bash`, `/usr/bin/bash`, and
    /// `/usr/local/bin/bash` are the same shell.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedShell`] for anything that is not bash or zsh,
    /// naming what was found. An unrecognised `$SHELL` is refused rather than
    /// guessed at, because guessing wrong means writing a hook into an rc file
    /// that will not run it and telling the user it worked.
    pub fn from_shell_var(value: &OsStr) -> Result<Self> {
        let found = Path::new(value).file_name().map_or_else(
            || value.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        match found.as_str() {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            _ => Err(Error::UnsupportedShell { shell: found }),
        }
    }

    /// The rc file this shell reads at login.
    #[must_use]
    pub fn rc_file_name(self) -> &'static str {
        match self {
            Self::Bash => ".bashrc",
            Self::Zsh => ".zshrc",
        }
    }

    /// The file name of the hook this shell needs.
    #[must_use]
    pub fn hook_file_name(self) -> &'static str {
        match self {
            Self::Bash => "gcode.bash",
            Self::Zsh => "gcode.zsh",
        }
    }

    /// The hook source, embedded at compile time.
    ///
    /// Embedding rather than reading `shell/` at runtime is what lets a single
    /// binary carry a hook that always matches its own version. See the module
    /// docs.
    #[must_use]
    pub fn hook_source(self) -> &'static str {
        match self {
            Self::Bash => include_str!("../../shell/gcode.bash"),
            Self::Zsh => include_str!("../../shell/gcode.zsh"),
        }
    }
}

/// What a plan did to the rc file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The block was appended. This is the first install.
    Added,
    /// An older or different block was replaced with the current one.
    Updated,
    /// The block was already present and identical. Nothing was written.
    Unchanged,
    /// The block was removed.
    Removed,
    /// There was no block to remove.
    NotInstalled,
}

impl Outcome {
    /// Whether this outcome required a write.
    #[must_use]
    pub fn changed(self) -> bool {
        matches!(self, Self::Added | Self::Updated | Self::Removed)
    }

    /// The word to print for this outcome.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
            Self::Removed => "removed",
            Self::NotInstalled => "not installed",
        }
    }
}

/// What `--check` found, without changing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// No block in the rc file.
    Missing,
    /// A block is present.
    Installed {
        /// The gcode version that wrote the block.
        version: String,
        /// Whether that version is the one running.
        current: bool,
    },
}

impl Status {
    /// Whether a usable block is in place.
    #[must_use]
    pub fn installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }
}

/// The byte range of a marked block, including the newline after the end marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    /// Where the begin-marker line starts.
    start: usize,
    /// Just past the newline that ends the end-marker line.
    end: usize,
}

/// A proposed new rc file, and what it would change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The full new contents of the rc file.
    pub content: String,
    /// The lines added or removed, in file order, for reporting.
    pub lines: Vec<String>,
    /// What the plan did.
    pub outcome: Outcome,
}

/// Renders the block that gets written into an rc file.
///
/// Four lines: the begin marker, a version line, the source command, the end
/// marker. Always ends with a newline, so appending it never joins two lines.
/// The source path is single-quoted with embedded quotes escaped, because a
/// home directory containing a space must not produce a block that cannot be
/// sourced.
///
/// No shell parameter: the two blocks differ only in the hook path, and taking
/// a [`Kind`] it does not read would let a caller silently write a bash install
/// pointing at the zsh hook.
#[must_use]
pub fn block(hook_path: &Path) -> String {
    format!(
        "{BEGIN_MARKER}\n{VERSION_PREFIX}{}\nsource {}\n{END_MARKER}\n",
        env!("CARGO_PKG_VERSION"),
        shell_quote(&hook_path.display().to_string()),
    )
}

/// Single-quotes a string for POSIX shells.
fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Works out the new rc file contents for an install.
///
/// Idempotent by construction: when the exact block this build would write is
/// already present the content comes back unchanged and the outcome is
/// [`Outcome::Unchanged`]. When a *different* block is present — an older gcode,
/// or a hook path the user has since moved — it is replaced in place rather than
/// duplicated, so a file never accumulates two blocks that both claim to own
/// `PROMPT_COMMAND`.
///
/// # Errors
///
/// Returns [`Error::ShellBlockMalformed`] when the file has a begin marker with
/// no end marker. Appending past it would leave two blocks and an unterminated
/// one; guessing where the user meant the block to stop would risk deleting their
/// own lines on the next remove.
pub fn plan_install(existing: &str, hook_path: &Path) -> Result<Plan> {
    let block = block(hook_path);
    let span = find_block(existing)?;

    match span {
        Some(span) if existing[span.start..span.end] == block => Ok(Plan {
            content: existing.to_owned(),
            lines: Vec::new(),
            outcome: Outcome::Unchanged,
        }),
        Some(span) => {
            let mut content = String::with_capacity(existing.len() + block.len());
            content.push_str(&existing[..span_without_separator(span, &existing[..span.start])]);
            content.push_str(&block);
            content.push_str(&existing[span.end..]);
            Ok(Plan {
                lines: block.lines().map(str::to_owned).collect(),
                content,
                outcome: Outcome::Updated,
            })
        }
        None => Ok(Plan {
            content: append(existing, &block),
            lines: block.lines().map(str::to_owned).collect(),
            outcome: Outcome::Added,
        }),
    }
}

/// Works out the new rc file contents for a removal.
///
/// Removes the block and exactly one newline before it, which is the newline
/// this module inserted. Removing a fixed one rather than "all preceding
/// blank lines" is what makes removal byte-precise: a user who had their own
/// blank line before the install gets it back.
///
/// # Errors
///
/// Returns [`Error::ShellBlockMalformed`] when a begin marker has no end marker.
pub fn plan_remove(existing: &str) -> Result<Plan> {
    let Some(span) = find_block(existing)? else {
        return Ok(Plan {
            content: existing.to_owned(),
            lines: Vec::new(),
            outcome: Outcome::NotInstalled,
        });
    };

    let head = span_without_separator(span, &existing[..span.start]);
    let mut content = String::with_capacity(existing.len());
    content.push_str(&existing[..head]);
    content.push_str(&existing[span.end..]);

    Ok(Plan {
        lines: existing[span.start..span.end]
            .lines()
            .map(str::to_owned)
            .collect(),
        content,
        outcome: Outcome::Removed,
    })
}

/// Reports what is installed, without writing anything.
///
/// # Errors
///
/// Returns [`Error::ShellBlockMalformed`] when a begin marker has no end marker.
pub fn status(existing: &str, hook_path: &Path) -> Result<Status> {
    match find_block(existing)? {
        None => Ok(Status::Missing),
        Some(span) => {
            let present = &existing[span.start..span.end];
            let version = present
                .lines()
                .find_map(|line| line.strip_prefix(VERSION_PREFIX))
                .unwrap_or("unknown")
                .trim()
                .to_owned();
            Ok(Status::Installed {
                current: present == block(hook_path),
                version,
            })
        }
    }
}

/// Appends a block, adding the one separating newline this module owns.
fn append(existing: &str, block: &str) -> String {
    let mut content = String::with_capacity(existing.len() + block.len() + 1);
    content.push_str(existing);
    if !existing.is_empty() {
        content.push('\n');
    }
    content.push_str(block);
    content
}

/// The start of a block including the single newline this module inserted.
///
/// `head` is the text before the begin marker. The newline is ours to remove
/// exactly when `head` is non-empty: install always inserted one there, whether
/// the user's last line was newline-terminated or not, so its removal is
/// reversible in both cases. An empty rc file never had one inserted.
fn span_without_separator(span: Span, head: &str) -> usize {
    if head.is_empty() {
        span.start
    } else {
        span.start - 1
    }
}

/// Locates the first marked block.
fn find_block(content: &str) -> Result<Option<Span>> {
    let mut offset = 0;
    let mut begin: Option<usize> = None;
    for line in content.split_inclusive('\n') {
        let trimmed = line.strip_suffix('\n').unwrap_or(line);
        if begin.is_none() {
            if trimmed == BEGIN_MARKER {
                begin = Some(offset);
            }
        } else if trimmed == END_MARKER {
            return Ok(Some(Span {
                start: begin.unwrap_or(offset),
                end: offset + line.len(),
            }));
        }
        offset += line.len();
    }

    match begin {
        None => Ok(None),
        Some(_) => Err(Error::ShellBlockMalformed),
    }
}
