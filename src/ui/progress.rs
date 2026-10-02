//! A one-line progress indicator for a long download.
//!
//! It writes to stderr, never stdout, so `--json` stays a clean contract and a
//! piped command substitution never receives progress text. When stderr is not a
//! terminal — a CI log, a redirect — it writes nothing at all: a carriage-return
//! line that is not on a terminal is megabytes of `\r` in a file nobody reads.

use std::io::{IsTerminal, Write};

/// Renders `received` of `total` bytes as a single updating line.
///
/// Called from the downloader's progress hook, once per chunk. Emits a final
/// newline when the transfer completes, so the shell prompt does not end up on
/// the same line as the last percentage.
pub fn line(received: u64, total: u64) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let percent = received.saturating_mul(100).checked_div(total).unwrap_or(0);
    let mut err = std::io::stderr().lock();
    let _ = write!(
        err,
        "\r  downloading {:>7} / {} MiB ({percent:>3}%)",
        mib(received),
        mib(total)
    );
    if received >= total {
        let _ = writeln!(err);
    }
    let _ = err.flush();
}

/// Bytes as mebibytes to one decimal, counted in integers.
///
/// The same reasoning as the registry's `size_human`: a model size is a number a
/// user is asked to trust, so it is not routed through an `f64` that cannot hold
/// a `u64` exactly.
fn mib(bytes: u64) -> String {
    let tenths = u128::from(bytes) * 10 / (1024 * 1024);
    format!("{}.{}", tenths / 10, tenths % 10)
}
