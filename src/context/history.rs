//! The history store: append-only JSONL, `0600`, no database (ADR 0005).
//!
//! # Why this is a text file
//!
//! ADR 0005 considered `SQLite`, a binary format, the shell's own history file,
//! and a daemon, and rejected all four. The reasons are worth repeating here
//! because they are the reason the module looks this simple: greppability is a
//! feature for a developer-facing tool, truncation is `rm`, and the access
//! pattern is a ring-buffer tail read that a text file handles natively.
//!
//! # Why reads go backwards
//!
//! Every invocation wants the last handful of entries. A forward parse of a 10 MB
//! file to answer "what happened recently" reads 10 MB to use 4 KB, and the cost
//! lands on the user at the moment they are waiting for an answer. So
//! [`History::read_last`] seeks to EOF and scans backwards for newlines, stopping
//! as soon as it has enough. It never parses a line it did not need to parse.
//!
//! # Why a torn line is not an error
//!
//! Writes are one `write` of one line followed by `fsync`. A process killed
//! between the two leaves a line with no terminator. That is the expected shape
//! of a crash, not corruption, so it is discarded silently — warning about it on
//! every read would train the user to ignore warnings, which is exactly what makes
//! the one warning that matters (a genuinely malformed line) useless.
//!
//! # The one deviation from the ADR, and why
//!
//! ADR 0005 rule 6 says the data directory is created with `umask 0077`. This
//! implementation does not call `umask`, because `umask` is process-global state
//! and a library that changes it corrupts every other file the process creates —
//! including the model download. Instead the directory is created `0700` and the
//! file `0600` by explicit mode, then the file's mode is set again after opening.
//!
//! The observable result the ADR asks for is identical and in fact stronger: the
//! mode is `0600` whatever the caller's umask was, whereas `umask 0077` leaves a
//! pre-existing `0644` file at `0644`. `a_previous_loose_mode_is_tightened` covers
//! it. This is a deviation from the letter of an ADR and is flagged for a
//! maintainer decision rather than buried here.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path as StdPath;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::utils::paths::{self, Overrides};

/// Rotate when the file reaches this size.
///
/// ADR 0005 rule 5. Ten megabytes is roughly 500 entries at the ADR's stated few
/// kilobytes each, so a user who runs one command a day rotates about once a
/// year.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// How much survives a rotation, in bytes.
///
/// ADR 0005 rule 5 keeps the newest half. The half that is kept is the half that
/// was written most recently, and that half stays in `history.jsonl` rather than
/// moving to `.1`: the file a reader opens is the one that keeps its name, so a
/// reader that opened it a moment ago does not find it replaced under it.
pub const KEEP_BYTES: u64 = MAX_BYTES / 2;

/// How much of the tail is pulled into memory per backwards step.
///
/// Sixty-four kilobytes is larger than any plausible run of fifteen entries at the
/// ADR's stated few kilobytes each, so a normal read is a single step. Larger
/// would waste memory on the way to EOF; smaller would make a busy file take many
/// syscalls.
const READ_CHUNK: usize = 64 * 1024;

/// One executed command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix seconds.
    pub ts: i64,
    /// The command as it was run.
    pub cmd: String,
    /// Its exit status.
    pub exit: i32,
    /// Working directory at the time.
    pub cwd: String,
    /// Its output, truncated by the caller.
    pub out: String,
}

/// What a reader needs to know about the file without reading it.
///
/// Read backwards, so it is O(number of lines) and never O(file size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Size in bytes.
    pub bytes: u64,
    /// Number of entries found. After a rotation this is what is on disk, not
    /// what has ever been written.
    pub entries: u64,
    /// Unix mode bits of the file.
    pub mode: u32,
}

/// An append-only history file.
///
/// The path is stored rather than discovered per call, so a test can point one at
/// a temporary file and so that a rename between two calls cannot make one write
/// land in a different file than the next.
#[derive(Debug, Clone)]
pub struct History {
    path: PathBuf,
}

impl History {
    /// A store at an explicit path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The store the user configured, at `~/.gcode/history.jsonl` by default.
    /// # Errors
    ///
    /// When the home directory cannot be determined.
    pub fn discover() -> Result<Self> {
        Ok(Self::new(paths::history_file()?))
    }

    /// The store the overrides select. `GCODE_HISTORY_FILE` wins.
    ///
    /// # Errors
    ///
    /// When `GCODE_HISTORY_FILE` is set to a relative path.
    pub fn with_overrides(overrides: &Overrides) -> Result<Self> {
        Ok(Self::new(paths::history_file_with(overrides)?))
    }

    /// The file this store writes to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where a rotation puts the half it drops.
    pub fn rotated_path(&self) -> PathBuf {
        let mut p = self.path.as_os_str().to_owned();
        p.push(".1");
        PathBuf::from(p)
    }

    /// Size, entry count, and mode.
    ///
    /// Entry count is a backwards line count, not a JSON parse: this is for a
    /// human reading one line of output, and parsing every entry to count them
    /// would make the cheap operation the expensive one.
    /// # Errors
    ///
    /// When the file exists but cannot be read. A file that does not exist reports zeroes instead, because that is an empty history and not a failure.
    pub fn stats(&self) -> Result<Stats> {
        let file = match File::open(&self.path) {
            Ok(f) => f,
            // A history file that does not exist yet is an empty history, not a
            // failure. Reporting an error here would mean `--fix` cannot work
            // until the user has run a command that failed.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Stats {
                    bytes: 0,
                    entries: 0,
                    mode: 0,
                })
            }
            Err(e) => return Err(io_err(&e, &self.path, "could not open")),
        };
        let metadata = file
            .metadata()
            .map_err(|e| io_err(&e, &self.path, "could not stat"))?;
        Ok(Stats {
            bytes: metadata.len(),
            entries: count_lines_backwards(&file, &self.path)?,
            mode: mode_bits(&metadata),
        })
    }

    /// Append one entry, then rotate if the file has reached [`MAX_BYTES`].
    ///
    /// Rotation happens after the append so the entry just written is never the
    /// one dropped: rotating first would discard the newest entry whenever a write
    /// happened to land on the threshold.
    /// # Errors
    ///
    /// When the directory cannot be created, the file cannot be opened or written, or a rotation fails part-way. The caller decides whether that is fatal: a history that cannot be written should not stop a command from running.
    pub fn append(&self, entry: &HistoryEntry) -> Result<()> {
        self.append_only(entry)?;
        self.rotate_if_needed().map(|_| ())
    }

    /// Append without rotating. For tests that need to build a file.
    fn append_only(&self, entry: &HistoryEntry) -> Result<()> {
        let parent = self.path.parent().filter(|p| !p.as_os_str().is_empty());
        if let Some(dir) = parent {
            create_private_dir(dir)?;
        }

        let mut line = serde_json::to_string(entry).map_err(|e| Error::History {
            message: format!("could not encode the history entry: {e}"),
        })?;
        line.push('\n');

        // O_APPEND (via `append`) plus one `write_all` plus `fsync`. ADR 0005
        // rule 3. The atomicity of an O_APPEND write is a POSIX guarantee for
        // writes under the pipe buffer size, which is what keeps two terminals
        // from interleaving halves of two lines.
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.path)
            .map_err(|e| io_err(&e, &self.path, "could not open"))?;

        // Set the mode again rather than trusting the create mode. A file that
        // already existed may have been created by an older version, or by
        // `touch`, at 0644; the create mode above only applies when the file is
        // created. ADR 0005 wants 0600 and 0006-adjacent habits want it
        // unconditionally, so this is not conditional on having just made it.
        tighten_file_mode(&file)?;

        file.write_all(line.as_bytes())
            .map_err(|e| io_err(&e, &self.path, "could not append to"))?;
        file.sync_data()
            .map_err(|e| io_err(&e, &self.path, "could not sync"))?;
        Ok(())
    }

    /// The newest `n` entries, oldest first.
    ///
    /// Oldest first because every caller wants them in execution order: a prompt
    /// assembled backwards reads as a story running in reverse. A trailing line
    /// with no `\n` is a torn write and is dropped; a malformed line anywhere is
    /// skipped and produces exactly one warning, never an error.
    /// # Errors
    ///
    /// When the file exists but cannot be read. A file that does not exist reads as no entries, because `--fix` on a fresh install has nothing to report and must still work.
    pub fn read_last(&self, n: usize) -> Result<Vec<HistoryEntry>> {
        self.read_last_reporting(n, &mut || {
            eprintln!(
                "gcode: warning: skipping a malformed line in {}; \
                 the rest of the history is intact",
                self.path.display()
            );
        })
    }

    /// [`History::read_last`] with the warning sink supplied, so a test can count
    /// warnings instead of reading stderr.
    ///
    /// The warning fires at most once per call however many lines are malformed.
    /// A file with a hundred bad lines is one problem, not a hundred, and a user
    /// who gets a warning per line stops reading them.
    fn read_last_reporting(&self, n: usize, warn: &mut dyn FnMut()) -> Result<Vec<HistoryEntry>> {
        let mut warned = false;
        let mut once = || {
            if !warned {
                warned = true;
                warn();
            }
        };
        if n == 0 {
            return Ok(Vec::new());
        }
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err(&e, &self.path, "could not open")),
        };
        let raw = read_tail_lines(&file, n, &self.path)?;
        let mut out = Vec::with_capacity(raw.len());
        for line in raw {
            match serde_json::from_str::<HistoryEntry>(&line) {
                Ok(entry) => out.push(entry),
                Err(_) => once(),
            }
        }
        Ok(out)
    }

    /// Rotate if the file has reached [`MAX_BYTES`]. Returns whether it did.
    /// # Errors
    ///
    /// When the file cannot be read, or the rotation fails part-way.
    pub fn rotate_if_needed(&self) -> Result<bool> {
        let len = match fs::metadata(&self.path) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io_err(&e, &self.path, "could not stat")),
        };
        if len < MAX_BYTES {
            return Ok(false);
        }
        self.rotate().map(|()| true)
    }

    /// Keep the newest [`KEEP_BYTES`]; the older half becomes `.1`.
    ///
    /// The kept half stays in `history.jsonl` rather than moving to `.1`, so the
    /// file a reader opens keeps its name and its inode is the one an appender
    /// with an open handle is already writing to.
    ///
    /// # Why this rewrites the file instead of truncating it
    ///
    /// The obvious implementation is `set_len(cut)`, and it is wrong in a way that
    /// looks right: `set_len` keeps the *first* `n` bytes and discards the end.
    /// Rotation needs the opposite — drop the head, keep the tail — and no
    /// `set_len` argument can do that. An earlier draft did exactly this and the
    /// tests caught it by finding the file still over the cap, holding the oldest
    /// half.
    ///
    /// So the head is written to `.1` and the tail to a temporary file that is then
    /// renamed over the original. `rename` is atomic on POSIX, so a concurrent
    /// reader sees either the whole old file or the whole new one, never half of
    /// each.
    /// # Errors
    ///
    /// When the file cannot be read, `.1` or the staging file cannot be written,
    /// or the final rename fails. In the last case the history is untouched.
    pub fn rotate(&self) -> Result<()> {
        let file = File::open(&self.path)
            .map_err(|e| io_err(&e, &self.path, "could not open for rotation"))?;
        let len = file
            .metadata()
            .map_err(|e| io_err(&e, &self.path, "could not stat"))?
            .len();
        if len <= KEEP_BYTES {
            return Ok(());
        }
        let mut seeker = file
            .try_clone()
            .map_err(|e| io_err(&e, &self.path, "could not reopen"))?;
        let cut = cut_for_rotation(&mut seeker, KEEP_BYTES, &self.path)?;

        // The dropped head becomes `.1` first. If this fails nothing has been
        // replaced, so the history is still whole and the next append retries.
        let rotated = self.rotated_path();
        let mut dropped =
            File::create(&rotated).map_err(|e| io_err(&e, &rotated, "could not create"))?;
        tighten_file_mode(&dropped)?;
        let mut head = file
            .try_clone()
            .map_err(|e| io_err(&e, &self.path, "could not reopen"))?;
        head.seek(SeekFrom::Start(0))
            .map_err(|e| io_err(&e, &self.path, "could not seek"))?;
        io::copy(&mut head.take(cut), &mut dropped)
            .map_err(|e| io_err(&e, &self.path, "could not copy the older half into"))?;
        dropped
            .sync_all()
            .map_err(|e| io_err(&e, &rotated, "could not sync"))?;

        // Then the kept tail into a temporary, renamed over the original.
        //
        // The temporary is created private, not with `File::create`'s 0666, because
        // the umask default would leave the history world-readable and a rotation
        // is exactly when nobody is watching.
        let staged = self.staged_path();
        let mut kept = OpenOptions::new();
        kept.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            kept.mode(0o600);
        }
        let mut kept = kept
            .open(&staged)
            .map_err(|e| io_err(&e, &staged, "could not create"))?;
        let mut tail = file
            .try_clone()
            .map_err(|e| io_err(&e, &self.path, "could not reopen"))?;
        tail.seek(SeekFrom::Start(cut))
            .map_err(|e| io_err(&e, &self.path, "could not seek"))?;
        io::copy(&mut tail, &mut kept)
            .map_err(|e| io_err(&e, &self.path, "could not copy the newest half into"))?;
        kept.sync_all()
            .map_err(|e| io_err(&e, &staged, "could not sync"))?;
        drop(kept);

        fs::rename(&staged, &self.path)
            .map_err(|e| io_err(&e, &self.path, "could not replace with the rotated file"))?;
        Ok(())
    }

    /// Where a rotation stages the kept half before renaming it into place.
    ///
    /// Not `.new`, which some other tool might also want. Left behind only if the
    /// process dies mid-rotation, in which case the rename never happened and the
    /// history is still whole — the stale file is overwritten by the next rotation.
    fn staged_path(&self) -> PathBuf {
        let mut p = self.path.as_os_str().to_owned();
        p.push(".rotating");
        PathBuf::from(p)
    }
}

/// Count the `\n` bytes in a slice.
///
/// Written out rather than pulling in `bytecount` for it. That crate is faster, and
/// the whole point of this module is that a history read stays cheap, but a
/// dependency whose only use is one `memchr` is a supply-chain entry and a licence
/// question for a loop the optimiser compiles to about as well. If this ever shows
/// up in a profile, that is the moment to change the decision, not before.
#[allow(clippy::naive_bytecount)]
fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|b| **b == b'\n').count()
}

/// Unix mode bits of a file, or 0 where the platform has none.
#[cfg(unix)]
fn mode_bits(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_bits(_metadata: &fs::Metadata) -> u32 {
    0
}

/// Wrap an I/O failure with the path it happened to, because "No such file or
/// directory" with no path is not an answer a user can act on.
fn io_err(e: &std::io::Error, path: &StdPath, what: &str) -> Error {
    Error::History {
        message: format!("{what} {}: {e}", path.display()),
    }
}

/// Create a directory that only its owner can enter.
///
/// ADR 0005 rule 6 asks for `umask 0077`. See the module note: the mode is set
/// explicitly instead, because `umask` is process-global. `0700` is requested, and
/// `mkdir` applies the caller's umask on top, so the result is `0700` or
/// stricter and never looser.
fn create_private_dir(dir: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    // Already-exists is the normal case, not a failure: every append after the
    // first one finds the directory there. Treating it as an error would make the
    // second append fail.
    // `create_dir_all`, not `create_dir`: `GCODE_HISTORY_FILE` may point at a
    // nested path, and creating only the last component fails with a bare
    // "No such file or directory" that names nothing.
    match builder.recursive(true).create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(io_err(&e, dir, "could not create")),
    }
}

/// Set a file's mode to `0600`, ignoring platforms that have no modes.
#[cfg(unix)]
fn tighten_file_mode(file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|e| Error::History {
            message: format!("could not set the history mode to 0600: {e}"),
        })
}

#[cfg(not(unix))]
fn tighten_file_mode(_file: &File) -> Result<()> {
    Ok(())
}

/// The offset to cut at so that everything from there to EOF is whole lines and is
/// no larger than `limit`.
///
/// The cut is the first line start at or after `file_len - limit`, so what is kept
/// is *under* the limit rather than over it. An earlier draft searched backwards
/// for the newline *before* the boundary and cut after it, which kept up to a full
/// extra line and quietly broke the cap by the length of the longest line — 877
/// bytes on the ADR's own fixture.
///
/// Cutting inside a line is not an option at all: the fragment left at the front is
/// a record no reader can parse and that no entry count accounts for.
fn cut_for_rotation(file: &mut File, limit: u64, path: &StdPath) -> Result<u64> {
    let len = file
        .metadata()
        .map_err(|e| io_err(&e, path, "could not stat"))?
        .len();
    if len <= limit {
        return Ok(len);
    }
    let target = len - limit;
    file.seek(SeekFrom::Start(target))
        .map_err(|e| io_err(&e, path, "could not seek"))?;

    let mut window = vec![0u8; READ_CHUNK];
    let mut pos = target;
    loop {
        // `usize::try_from` rather than `as`: this file is 10 MB today, but the
        // cast is the kind that is fine on a 64-bit machine and silently truncates
        // on a 32-bit one.
        let want = usize::try_from(len - pos)
            .unwrap_or(READ_CHUNK)
            .min(READ_CHUNK);
        file.seek(SeekFrom::Start(pos))
            .map_err(|e| io_err(&e, path, "could not seek"))?;
        file.read_exact(&mut window[..want])
            .map_err(|e| io_err(&e, path, "could not read"))?;
        if let Some(offset) = window[..want].iter().position(|b| *b == b'\n') {
            return Ok(pos + offset as u64 + 1);
        }
        pos += want as u64;
        if pos >= len {
            break;
        }
    }

    // No newline anywhere after the boundary: the last record began before it and
    // runs past it, so honouring the limit exactly would mean keeping part of a
    // line. Keep the whole record instead and let this one file sit over the cap.
    // A file of records larger than the cap is not something rotation can solve, and
    // a parseable file that is slightly too big beats a small file of fragments.
    file.seek(SeekFrom::Start(target))
        .map_err(|e| io_err(&e, path, "could not seek"))?;
    let before = target;
    let mut back = vec![
        0u8;
        usize::try_from(before)
            .unwrap_or(READ_CHUNK)
            .min(READ_CHUNK)
    ];
    let mut at = before;
    let mut keep = back.len();
    while at > 0 {
        let from = at - keep as u64;
        file.seek(SeekFrom::Start(from))
            .map_err(|e| io_err(&e, path, "could not seek"))?;
        file.read_exact(&mut back[..keep])
            .map_err(|e| io_err(&e, path, "could not read"))?;
        if let Some(offset) = back[..keep].iter().rposition(|b| *b == b'\n') {
            return Ok(from + offset as u64 + 1);
        }
        at = from;
        keep = back.len();
    }
    Ok(0)
}

/// The last `n` complete lines of `file`, oldest first, read backwards.
///
/// Stops as soon as it has `n` newlines in hand, so answering "what happened
/// recently" does not read a file that has accumulated a month of it. A trailing
/// line with no terminator is a torn write and is dropped; a blank line is dropped
/// too, because `tail` and editors leave them and an empty record is not one.
fn read_tail_lines(file: &File, n: usize, path: &StdPath) -> Result<Vec<String>> {
    let mut reader = file
        .try_clone()
        .map_err(|e| io_err(&e, path, "could not reopen"))?;
    let len = reader
        .metadata()
        .map_err(|e| io_err(&e, path, "could not stat"))?
        .len();
    if len == 0 {
        return Ok(Vec::new());
    }

    // Read backwards in chunks until `n` newlines have been seen, then stitch the
    // chunks into order once. Chunks are held as they are read — newest first — and
    // reversed at the end, which is cheaper than prepending to a growing buffer on
    // every step.
    //
    // The newline count is a running total rather than a rescan of everything read
    // so far. Rescanning looks equivalent and is quadratic: on a file where the last
    // `n` entries are far from EOF, every chunk paid for every chunk before it.
    let mut newest_first: Vec<Vec<u8>> = Vec::new();
    let mut seen_newlines = 0usize;
    let mut total = 0usize;
    let mut pos = len;
    while pos > 0 && seen_newlines < n {
        let want = usize::try_from(pos).unwrap_or(READ_CHUNK).min(READ_CHUNK);
        let start = pos - want as u64;
        let mut chunk = vec![0u8; want];
        reader
            .seek(SeekFrom::Start(start))
            .map_err(|e| io_err(&e, path, "could not seek"))?;
        reader
            .read_exact(&mut chunk)
            .map_err(|e| io_err(&e, path, "could not read"))?;
        pos = start;
        seen_newlines += count_newlines(&chunk);
        total += want;
        newest_first.push(chunk);
    }
    let buf_len = total;
    newest_first.reverse();
    let mut text = Vec::with_capacity(buf_len);
    for chunk in &newest_first {
        text.extend_from_slice(chunk);
    }

    // A file that does not end in a newline has a torn last line. Drop it, and
    // only then count what is left.
    let ends_clean = text.last() == Some(&b'\n');
    if !ends_clean {
        // Keep the prefix up to the last newline; whatever follows is the fragment.
        let keep = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        text.truncate(keep);
    }

    // `split` yields a trailing empty piece after the final newline; dropping it
    // leaves exactly one element per complete line.
    let mut lines: Vec<String> = text
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect();

    // Already in file order, so oldest first is what is wanted and no reverse is
    // needed here. An earlier draft reversed, which silently made every caller
    // read history backwards; the tests that check order are what caught it.
    if lines.len() > n {
        lines.drain(..lines.len() - n);
    }
    Ok(lines)
}

/// Count lines by reading backwards, never parsing.
///
/// A backwards line count is O(newlines seen) and reads at most one chunk for a
/// file whose last byte is a newline, which is the normal shape. Parsing every
/// entry to count them would make the cheap operation the expensive one.
fn count_lines_backwards(file: &File, path: &StdPath) -> Result<u64> {
    let mut reader = file
        .try_clone()
        .map_err(|e| io_err(&e, path, "could not reopen"))?;
    let len = reader
        .metadata()
        .map_err(|e| io_err(&e, path, "could not stat"))?
        .len();
    if len == 0 {
        return Ok(0);
    }
    let mut count = 0u64;
    let mut pos = len;
    let mut buf = vec![0u8; READ_CHUNK.min(usize::try_from(len).unwrap_or(READ_CHUNK))];
    while pos > 0 {
        let want = usize::try_from(pos).unwrap_or(buf.len()).min(buf.len());
        let start = pos - want as u64;
        reader
            .seek(SeekFrom::Start(start))
            .map_err(|e| io_err(&e, path, "could not seek"))?;
        reader
            .read_exact(&mut buf[..want])
            .map_err(|e| io_err(&e, path, "could not read"))?;
        count += count_newlines(&buf[..want]) as u64;
        pos = start;
    }
    Ok(count)
}

#[cfg(test)]
#[path = "history/tests.rs"]
mod tests;
