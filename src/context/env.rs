//! What the tool knows about the machine it is running on.
//!
//! Phase 2.2. This module answers "where am I and what is going on here" so the
//! prompt can say something specific: `ls` in a git repository with uncommitted
//! changes is a different request from `ls` in `/tmp`, and a model that cannot
//! tell them apart has to guess.
//!
//! # Three rules, all of them about not being wrong
//!
//! **Silence beats a guess.** A missing fact is `None` and renders as nothing.
//! There is no default branch name, no assumed `bash`, no "unknown" string
//! substituted for a fact we failed to read. A prompt that says `git_branch="?"`
//! teaches the model that question marks are a thing branches look like.
//!
//! **A slow fact is a missing fact.** The git probe has a deadline
//! ([`GIT_TIMEOUT`]). Network filesystems make `git status` arbitrarily slow, and a
//! user in a hung directory should get a command in the same time as a user who is
//! not. Overrunning the deadline kills the child and reports nothing.
//!
//! **Nothing here reaches the prompt unredacted.** This module deliberately does
//! not render anything. It produces facts, [`EnvSnapshot::into_context`] hands them
//! to [`Context`], and the prompt redacts and escapes every attribute on
//! the way out (ADR 0006). A branch name can contain anything a user typed, and a
//! commit subject is free text, so both are treated as untrusted input.
//!
//! # Not wired to a call site yet
//!
//! `build_prompt` has no production caller until Phase 1.5 lands the sampler, so
//! nothing calls [`EnvSnapshot::collect`] at runtime yet either. That is the same
//! reason the history store has no reader: these are the parts of the pipeline that
//! can be built and tested now, ahead of the engine that will consume them.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Env;
use crate::context::prompt::Context;

/// How long the whole git probe may take before it is abandoned.
///
/// One budget for every question, not one per question, because the number of
/// questions is an implementation detail and the latency guarantee is not.
///
/// **This is 250 ms, and the roadmap asked for 30 ms. That number was not reachable
/// and was changed after measuring it.** Two `git` subprocesses, issued together so
/// the cost is one spawn and not two, were timed on an idle machine and under a
/// 16-way CPU load:
///
/// | Condition | Concurrent `status` + `log` |
/// |---|---|
/// | Idle, warm | 20–30 ms |
/// | 16 busy loops | ~40 ms |
///
/// A budget below the floor means the probe reports nothing, always — the feature
/// would be dead code that still cost a fork. 250 ms is roughly ten times the worst
/// observed cost, which leaves room for a cold filesystem and a loaded laptop, and it
/// is still far below the point where a human waiting for a command notices. The
/// deadline still exists and still kills the child; what changed is that it is set
/// where it can actually be met.
pub const GIT_TIMEOUT: Duration = Duration::from_millis(250);

/// The facts about this machine, each optional because each can fail to be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot {
    /// The working directory, as the user would name it.
    pub cwd: Option<String>,
    /// Operating system, lowercased.
    pub os: Option<String>,
    /// CPU architecture.
    pub arch: Option<String>,
    /// The shell named by `$SHELL`, reduced to its program name.
    pub shell: Option<String>,
    /// The shell's version, first line only.
    pub shell_version: Option<String>,
    /// Repository facts. `None` when git is unavailable, disabled, timed out, or
    /// the directory is not a repository.
    pub git: Option<GitContext>,
}

/// The repository facts, each optional on its own.
///
/// Separate from the option around the whole struct because "this is a repository
/// and I could not read the branch" is a real state, and collapsing it into "not a
/// repository" would tell the model the user is not in a repository when they are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitContext {
    /// Current branch name.
    pub branch: Option<String>,
    /// Whether the working tree has uncommitted changes.
    pub dirty: Option<bool>,
    /// Subject line of the most recent commit.
    pub last_commit: Option<String>,
}

/// Whether history is switched off for this invocation.
///
/// Exposed here because this is where the question is asked, and the answer has to
/// be asked in exactly one place. The shell hooks check it too, since a hook runs
/// outside any gcode process and cannot ask this function.
#[must_use]
pub fn history_disabled(env: &Env) -> bool {
    env.no_history.unwrap_or(false)
}

impl EnvSnapshot {
    /// Reads the real environment.
    ///
    /// Never fails. Every step that can fail yields `None` instead, because a
    /// caller that has to handle an error here will eventually handle it by
    /// ignoring it, and the correct behaviour here is to omit a fact.
    #[must_use]
    pub fn collect() -> Self {
        let env = Env::from_process();
        Self::collect_with(&env, &RealShell, None)
    }

    /// [`EnvSnapshot::collect`], with the environment, the shell, and the directory
    /// supplied.
    ///
    /// The seam that makes this testable. Reading `std::env` directly would mean
    /// tests that mutate the process environment, which race each other under
    /// `cargo test`; and `current_dir` is just as process-global, so a test could
    /// otherwise never point the whole collector at a fixture repository. `cwd` of
    /// `None` means the process working directory.
    #[must_use]
    pub fn collect_with(env: &Env, shell: &dyn ShellProbe, cwd: Option<&Path>) -> Self {
        // `GCODE_NO_ENV` is the coarse switch and is checked first, so a user who
        // set it gets a guarantee that does not depend on which of the other
        // switches happen to be misparsed.
        if env.no_env.unwrap_or(false) {
            return Self::default();
        }

        let dir: Option<std::path::PathBuf> = match cwd {
            Some(p) => Some(p.to_path_buf()),
            None => std::env::current_dir().ok(),
        };
        let cwd = dir.map(|p| p.to_string_lossy().into_owned());
        let shell_path = env.shell.clone();

        // `first_line` is applied here rather than inside `RealShell`, so the
        // one-line guarantee belongs to the snapshot instead of to one
        // implementation of the probe. `bash --version` prints four lines; a probe
        // that returns the whole banner gets the same treatment as one that
        // returns a trimmed string.
        let shell_version = shell_path
            .as_deref()
            .and_then(|p| shell.version(p))
            .as_deref()
            .and_then(first_line);

        let mut snapshot = Self {
            cwd,
            os: Some(std::env::consts::OS.to_owned()),
            arch: Some(std::env::consts::ARCH.to_owned()),
            shell: shell_path.as_deref().map(program_name),
            shell_version,
            git: None,
        };

        // A repository is a property of the directory, so the probe runs there
        // rather than wherever the process happened to be started.
        if env.no_git.unwrap_or(false) {
            return snapshot;
        }
        snapshot.git = match &snapshot.cwd {
            Some(dir) => probe_git(Path::new(dir), GIT_TIMEOUT),
            None => None,
        };
        snapshot
    }

    /// The facts as the prompt wants them.
    ///
    /// A conversion rather than a reimplementation: the prompt already has a
    /// `Context`, and building a second one here would be two places to keep in
    /// step. `output_tail_bytes` is left at zero, which the prompt reads as "use
    /// the default", because this module has no opinion about output size.
    #[must_use]
    pub fn into_context(self) -> Context {
        Context {
            cwd: self.cwd,
            os: self.os,
            arch: self.arch,
            shell: self.shell,
            shell_version: self.shell_version,
            git_branch: self.git.as_ref().and_then(|g| g.branch.clone()),
            git_dirty: self.git.as_ref().and_then(|g| g.dirty),
            git_last_commit: self.git.and_then(|g| g.last_commit),
            history: Vec::new(),
            output_tail_bytes: 0,
        }
    }
}

/// Runs the real shell to ask its version.
pub trait ShellProbe {
    /// The version string for the shell at `path`, or `None`.
    ///
    /// The first line only. `bash --version` prints four lines and zsh prints a
    /// licence paragraph; the prompt has no use for either and the model has no use
    /// for either, but the tokens spent are real.
    fn version(&self, path: &str) -> Option<String>;
}

/// [`ShellProbe`] that actually runs the shell.
pub struct RealShell;

impl ShellProbe for RealShell {
    fn version(&self, path: &str) -> Option<String> {
        let out = Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

/// The first line, trimmed, or `None` if it is blank.
fn first_line(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_owned())
    }
}

/// `/usr/local/bin/bash` becomes `bash`.
///
/// The bare name is what the prompt wants: `$SHELL` is a path on every system
/// tested here, and a path in the context line spends tokens on the fact that the
/// user has a `/bin` directory.
fn program_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// Asks git what it can, under one deadline.
///
/// Two subprocesses at most, not four. The roadmap's budget for this is 30 ms, and a
/// `git` spawn costs several milliseconds on its own, so four calls could not fit
/// inside it on any real machine — an earlier version did exactly that and reported
/// nothing at all, because every call ran out of budget. `git status` answers for
/// both the branch and the dirty flag in one go, so that is the call that happens
/// first; the commit subject is the optional second one.
///
/// Returns `None` only when git could not be run, timed out on the first call, or
/// the directory is not a repository. Once one fact has been read it is kept: a
/// deadline that expires between the two calls costs the subject, not the branch.
fn probe_git(cwd: &Path, timeout: Duration) -> Option<GitContext> {
    use std::sync::mpsc;

    let start = Instant::now();
    let deadline = start + timeout;
    let remaining = || deadline.saturating_duration_since(Instant::now());

    // Run both calls concurrently. On warm systems, `git status` and `git log` on
    // a small repo can run in parallel, and this makes the 30 ms budget
    // sufficient without dropping either fact.
    let (status_tx, status_rx) = mpsc::channel();
    let (log_tx, log_rx) = mpsc::channel();

    let cwd_s = cwd.to_path_buf();
    let d_s = deadline;
    std::thread::spawn(move || {
        let r = git_output(&cwd_s, &["status", "--porcelain=v2", "--branch"], d_s);
        let _ = status_tx.send(r);
    });

    let cwd_l = cwd.to_path_buf();
    let d_l = deadline;
    std::thread::spawn(move || {
        let r = git_output(&cwd_l, &["log", "-1", "--pretty=%s"], d_l);
        let _ = log_tx.send(r);
    });

    let status_out = status_rx.recv_timeout(remaining()).ok().flatten()?;
    let mut branch = None;
    let mut dirty = false;
    for line in status_out.lines() {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            let name = name.trim();
            if !name.is_empty() && name != "(detached)" {
                branch = Some(name.to_owned());
            }
        } else if !line.starts_with('#') {
            dirty = true;
        }
    }

    let last_commit = log_rx.recv_timeout(remaining()).ok().flatten();
    let last_commit = last_commit
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    Some(GitContext {
        branch,
        dirty: Some(dirty),
        last_commit,
    })
}

/// One `git` call, or `None` for a failure, a non-zero exit, or an overrun.
///
/// The child is killed on overrun rather than left running: a `git status` stuck on
/// a dead network mount would otherwise outlive the process that spawned it.
///
/// stdout is drained on a separate thread. Without that, a git that writes more
/// than a pipe buffer blocks on the write while we sit in `try_wait`, and the
/// deadline would fire on a process that was about to answer.
fn git_output(cwd: &Path, args: &[&str], deadline: Instant) -> Option<String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Never let git read the user's stdin. A `git status` that decided to ask a
        // question would hang the tool behind a prompt the user cannot see.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // git writes its complaints to stderr, and this module is silent on
        // purpose. A user in a non-repository should not be told about it.
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut stdout = stdout;
        // A read error here means the pipe broke, which only happens if the child
        // was killed. The empty buffer is the right answer to that.
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                // 1 ms, not a blocking `wait`: the deadline has to be observed while
                // the child is still running, and a blocking wait could not do that.
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };

    // The exit status decides whether this is an answer or a failure. It cannot be
    // left to the caller to infer: outside a repository git exits 128 and prints
    // nothing, and an empty string is a perfectly good answer to a different
    // question — "which files changed?" in a repository with a clean tree. Reading
    // that as success would report every directory as a clean repository.
    if !status.success() {
        return None;
    }

    // The child has exited, so its stdout pipe is closed and the reader thread is
    // already finishing. This wait is a backstop for a thread that somehow has not
    // been scheduled, not a normal path, so it is short and it does not extend the
    // deadline: the budget was for the child, and the child is gone.
    let buf = rx
        .recv_timeout(Duration::from_millis(50))
        .ok()
        .unwrap_or_default();
    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests;
