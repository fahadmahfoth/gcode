//! Runs a command the user has already approved.
//!
//! This is the last stage of the pipeline and the only place a generated string
//! becomes a process. It performs no classification and makes no decision: every
//! caller reaches it through `runtime::gate`, which has already refused `CRITICAL`
//! and obtained consent. Keeping it a trait is what lets the suite prove that
//! without ever starting a real process.

use std::process::{Command, ExitStatus};

use crate::error::{Error, Result};

/// Runs one command and reports its exit status.
pub trait Executor {
    /// Runs `command` to completion.
    ///
    /// # Errors
    ///
    /// [`Error::Exec`] when the process could not be started or waited on. A
    /// command that runs and fails is not an error; its status is the `Ok` value.
    fn run(&self, command: &str) -> Result<i32>;
}

/// Runs a command with `sh -c`, inheriting stdin, stdout, and stderr.
///
/// The output is not captured: the user sees the command's own output, live, and
/// an interactive command works. Capturing it would need a pseudo-terminal, which
/// is a separate decision.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShellExecutor;

impl Executor for ShellExecutor {
    fn run(&self, command: &str) -> Result<i32> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
            .map_err(|source| Error::Exec {
                message: source.to_string(),
            })?;
        Ok(exit_code_of(status))
    }
}

/// The status a shell would report for `status`.
///
/// A process killed by a signal has no exit code; the shell convention is
/// `128 + signal`, which keeps "it was killed" distinguishable from "it exited
/// with 1". Anything else without a code reports `1`.
#[must_use]
pub fn exit_code_of(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_zero() {
        assert_eq!(ShellExecutor.run("true").expect("runs"), 0);
    }

    #[test]
    fn a_failing_command_reports_its_status_and_is_not_an_error() {
        assert_eq!(ShellExecutor.run("exit 7").expect("runs"), 7);
        assert_eq!(ShellExecutor.run("false").expect("runs"), 1);
    }

    #[test]
    fn a_command_is_one_shell_string_so_pipes_and_lists_work() {
        assert_eq!(
            ShellExecutor
                .run("printf a | grep -q a && exit 3")
                .expect("runs"),
            3
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_killed_command_reports_128_plus_the_signal() {
        assert_eq!(ShellExecutor.run("kill -9 $$").expect("runs"), 137);
    }

    #[test]
    fn a_command_that_does_not_exist_is_a_status_not_an_error() {
        assert_eq!(
            ShellExecutor
                .run("gcode-no-such-command-xyz 2>/dev/null")
                .expect("sh itself starts"),
            127
        );
    }
}
