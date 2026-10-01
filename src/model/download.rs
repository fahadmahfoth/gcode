//! Fetching a model file and proving it is the file the registry says it is.
//!
//! # Why a trait and not an HTTP client
//!
//! The roadmap asks for `mockito` to serve fixtures. This module puts the
//! network behind [`Transport`] instead, because a 400 MB download is the one
//! part of gcode that cannot be checked by reading it, and a test that must
//! stand up an HTTP server to catch an arithmetic mistake is a test that gets
//! skipped the first time the suite is slow. Everything except the socket is
//! testable in milliseconds against a fake, and the socket stays confined to
//! one file so CI's network-containment check keeps passing.
//!
//! # What this module will not do
//!
//! It will not execute anything, and it will not accept a file it cannot verify.
//! A model gcode could not verify has the same authority over the user's shell
//! as a command they are about to run, so an unverifiable download is refused
//! rather than trusted with a warning.

use std::fmt;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::registry::ModelEntry;

/// Extension of the incomplete file a download is written to.
///
/// The name is load-bearing: someone running `ls` in the model directory during
/// an interrupted download has to be able to tell it apart from a model gcode
/// believes in. Nothing ever opens a `.part` file as a model.
const PART_EXT: &str = "part";

/// File extension every model has, and the only one gcode will load.
const GGUF_EXT: &str = "gguf";

/// Bytes copied per loop iteration.
///
/// 256 KiB keeps the syscall count reasonable on a half-gigabyte transfer
/// without holding a meaningful amount of memory, which matters on the small
/// machines this project targets.
const CHUNK: usize = 256 * 1024;

/// Mode for a finished model: world-readable, not writable by anyone.
///
/// A model is data, not something to edit in place. `0600` would be theatre;
/// there is nothing secret in a public GGUF.
#[cfg(unix)]
const MODEL_MODE: u32 = 0o644;

/// Something that can move bytes from a URL to a reader.
///
/// Implemented for real behind the `download` feature, and in tests by a fake
/// serving memory. The trait hands back a reader rather than the bytes, because
/// a 400 MB body must never be buffered whole.
pub trait Transport: Send + Sync {
    /// Starts a transfer from `offset` bytes in.
    ///
    /// A zero `offset` is a plain request. A non-zero one asks for a byte
    /// range. An implementation that cannot serve a range must report
    /// [`Transport::supports_range`] as `false` rather than quietly restarting
    /// and appending a second copy of the file to the first.
    /// # Errors
    ///
    /// Returns an error when the transfer cannot be started at all. Errors
    /// partway through the body surface from the returned reader instead, so a
    /// caller that streams sees them while the partial is still resumable.
    fn get(&self, url: &str, offset: u64) -> io::Result<Box<dyn Read + Send>>;

    /// Whether this transport honours a non-zero `offset`.
    fn supports_range(&self) -> bool {
        false
    }
}

/// Where a model is, or where it should be written.
///
/// Returned by [`resolve`] so a caller can tell the user the path before a long
/// transfer starts, and so the download tests never touch a real data
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The final path, with no `.part` suffix.
    pub path: PathBuf,
    /// Where bytes go until the digest is confirmed.
    pub part: PathBuf,
    /// The directory that must exist before writing.
    pub dir: PathBuf,
}

impl Resolved {
    /// The three paths for `file_name` inside `dir`.
    fn in_dir(dir: &Path, file_name: &str) -> Self {
        Self {
            path: dir.join(file_name),
            part: dir.join(format!("{file_name}.{PART_EXT}")),
            dir: dir.to_path_buf(),
        }
    }
}

/// Everything [`resolve`] needs, injected rather than read from the
/// environment.
#[derive(Debug)]
pub struct ResolveInput<'a> {
    /// Registry name, from the config or `--model`.
    pub name: Option<&'a str>,
    /// An explicit path to a GGUF file, which bypasses the registry.
    pub path: Option<&'a Path>,
    /// Directory a model is downloaded into.
    pub models_dir: &'a Path,
    /// Extra directories to search, in order, for a model already on disk.
    pub search_dirs: &'a [PathBuf],
}

/// Finds a model without touching the network.
///
/// Precedence, highest first: an explicit configured path, a model already in a
/// search directory, the download directory. A configured path is taken at face
/// value: gcode will not hash 400 MB at startup, and a user pointing at a
/// specific file is telling gcode which one to use.
///
/// # Errors
///
/// Returns [`crate::Error::ModelNameRequired`] when there is neither a path nor
/// a name.
pub fn resolve(input: &ResolveInput<'_>) -> crate::Result<Resolved> {
    if let Some(path) = input.path {
        let name = path
            .file_name()
            .map_or_else(|| GGUF_EXT.to_owned(), |n| n.to_string_lossy().into_owned());
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        return Ok(Resolved::in_dir(
            dir.unwrap_or_else(|| Path::new(".")),
            &name,
        ));
    }

    let name = input.name.ok_or(crate::Error::ModelNameRequired)?;
    let file_name = file_name_for(name);

    for dir in input.search_dirs {
        if dir.join(&file_name).is_file() {
            return Ok(Resolved::in_dir(dir, &file_name));
        }
    }

    Ok(Resolved::in_dir(input.models_dir, &file_name))
}

/// The file name a configured name or path maps to.
///
/// A name that already ends in `.gguf` is used as written, so both
/// `--model qwen2.5-0.5b-instruct` and `--model ./local.gguf` work. A path keeps
/// only its last component, because a registry name is a single token on a
/// command line and must not be able to steer a write outside the model
/// directory.
fn file_name_for(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if last.is_empty() {
        return GGUF_EXT.to_owned();
    }
    if last.to_ascii_lowercase().ends_with(&format!(".{GGUF_EXT}")) {
        return last.to_owned();
    }
    format!("{last}.{GGUF_EXT}")
}

/// What is already on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    /// Nothing at the final path.
    Absent,
    /// Present, and hashes to what the registry expects.
    Verified,
    /// Present, and hashes to something else. Not yet deleted; only the caller
    /// knows whether removing it is safe.
    Mismatched,
    /// A `.part` file of this length is on disk.
    Partial(u64),
}

/// A hash that could not be computed.
///
/// Kept separate from a hash that did not match, because the two mean different
/// things to a user. "This is not the model" says delete something; "I could not
/// read it" says fix permissions. Collapsing them is how a permission problem
/// gets reported as a corrupt download, and the user deletes the wrong file.
#[derive(Debug)]
pub enum HashError {
    /// A file could not be opened or read.
    Io(io::Error),
    /// A partially written file is shorter than the registry says.
    Truncated {
        /// Bytes present.
        got: u64,
        /// Bytes the registry declares.
        want: u64,
    },
    /// A present file is longer than the registry says, so it is not this model.
    Oversized {
        /// Bytes present.
        got: u64,
        /// Bytes the registry declares.
        want: u64,
    },
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Truncated { got, want } => write!(
                f,
                "the file is {got} bytes but the registry declares {want}. An \
                 interrupted download is never accepted as a model"
            ),
            Self::Oversized { got, want } => write!(
                f,
                "the file is {got} bytes but the registry declares {want}. A file \
                 larger than the declared size is not the model the registry names"
            ),
        }
    }
}

impl std::error::Error for HashError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Truncated { .. } | Self::Oversized { .. } => None,
        }
    }
}

/// A failed transfer, described well enough to act on.
#[derive(Debug)]
pub struct DownloadError {
    /// The URL that failed.
    pub url: String,
    /// What went wrong.
    pub cause: io::Error,
    /// The digest the registry expects, when a file was received.
    pub expected: Option<String>,
    /// The digest of what arrived.
    pub actual: Option<String>,
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.expected, &self.actual) {
            (Some(expected), Some(actual)) => write!(
                f,
                "the file at {} is not the model the registry names.\n  \
                 expected sha256 {expected}\n  \
                 got      sha256 {actual}\n  \
                 The partial file has been deleted. If this repeats, either the \
                 transfer is being corrupted or the registry entry is wrong. \
                 Neither is a reason to keep the file.",
                self.url
            ),
            _ => write!(f, "could not download {}: {}", self.url, self.cause),
        }
    }
}

impl std::error::Error for DownloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// What a completed download did, for the caller to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Present and correct. No network access at all.
    AlreadyPresent,
    /// Bytes received and verified.
    Downloaded,
    /// A `.part` was appended to and the result verified.
    Resumed,
}

impl Outcome {
    /// Whether the network was used.
    pub fn touched_network(self) -> bool {
        matches!(self, Self::Downloaded | Self::Resumed)
    }
}

/// A [`DownloadError`] carrying a cause and no digests.
fn failed(url: &str, cause: impl Into<io::Error>) -> DownloadError {
    DownloadError {
        url: url.to_owned(),
        cause: cause.into(),
        expected: None,
        actual: None,
    }
}

/// A transfer that stopped early, saying so and saying what survives.
///
/// The resume advice is not decoration. Without it the obvious response to
/// "connection reset" is to delete the partial file, which throws away the only
/// good copy of the bytes gcode has and makes the user pay for the same 400 MB
/// twice.
fn stopped(url: &str, received: u64, total: u64, cause: &io::Error) -> DownloadError {
    DownloadError {
        url: url.to_owned(),
        cause: io::Error::new(
            cause.kind(),
            format!(
                "{cause}. The transfer stopped after {received} of {total} bytes; \
                 the partial file is kept, and the next attempt resumes from \
                 there rather than starting over"
            ),
        ),
        expected: None,
        actual: None,
    }
}

/// A download to perform.
pub struct Request<'a> {
    /// The model to fetch.
    pub entry: &'a ModelEntry,
    /// Destination paths.
    pub resolved: Resolved,
    /// Where bytes come from.
    pub transport: &'a dyn Transport,
    /// Tried when the primary URL yields the wrong bytes.
    pub mirror: Option<&'a str>,
    /// Called as `(received, expected_total)` bytes land, for a progress line
    /// on stderr. `expected_total` comes from the registry, not the server, so
    /// a progress bar cannot be stretched by a server that under-reports.
    pub on_progress: Option<&'a mut dyn FnMut(u64, u64)>,
}

impl Request<'_> {
    /// A request with no mirror and no progress reporting.
    fn bare<'b>(
        entry: &'b ModelEntry,
        resolved: Resolved,
        transport: &'b dyn Transport,
    ) -> Request<'b> {
        Request {
            entry,
            resolved,
            transport,
            mirror: None,
            on_progress: None,
        }
    }
}

/// Ensures the entry's model exists at `request.resolved.path`, verified.
///
/// Idempotent, and entirely offline when the file is already correct. A file
/// that hashes to something else is deleted before anything else is attempted,
/// so a retry can never land beside a file gcode already knows is wrong.
///
/// # Errors
///
/// Returns a [`DownloadError`] carrying both digests when the bytes received do
/// not match the registry, after trying the mirror, and a bare cause when the
/// transport fails. A mismatch deletes the partial file; a transport failure
/// keeps it, so the next call resumes.
pub fn ensure(request: &mut Request<'_>) -> Result<Outcome, DownloadError> {
    let expected = request.entry.sha256.as_str();

    let mut start = 0u64;
    match inspect(&request.resolved, request.entry)? {
        Existing::Verified => return Ok(Outcome::AlreadyPresent),
        Existing::Mismatched => {
            // Removing a file we know is the wrong bytes is the one deletion
            // this module performs. A file gcode has verified is never deleted.
            fs::remove_file(&request.resolved.path).map_err(|e| failed(&request.entry.url, e))?;
        }
        Existing::Absent => {}
        Existing::Partial(n) => start = n,
    }

    // A `.part` of zero length is a failed previous attempt, not a prefix. It
    // carries no information, and resuming from it can only produce a shorter
    // file than starting over.
    if start == 0 && request.resolved.part.exists() {
        fs::remove_file(&request.resolved.part).map_err(|e| failed(&request.entry.url, e))?;
    }

    let resumed = start > 0 && request.transport.supports_range();
    let primary = request.entry.url.clone();
    // A digest that does not match is carried as a value rather than an error
    // so the mirror can be tried before anything is reported to the user.
    let attempt: Result<String, (String, io::Error)> =
        match transfer(request, &primary, start, resumed) {
            Ok(digest) if digest == expected => {
                promote(&request.resolved).map_err(|e| DownloadError {
                    url: primary.clone(),
                    cause: e,
                    expected: Some(expected.to_owned()),
                    actual: Some(digest),
                })?;
                return Ok(if resumed {
                    Outcome::Resumed
                } else {
                    Outcome::Downloaded
                });
            }
            Ok(digest) => Err((digest, io::Error::other("hash mismatch"))),
            // A transport failure leaves the `.part` alone: it is a good prefix, and
            // the next run resumes from it. That is the only reason to keep it.
            Err(e) => return Err(e),
        };

    // Wrong bytes from the primary. The partial is deleted before the mirror is
    // tried, so a mirror failure cannot leave a `.part` that a later resume
    // would append to and produce a file made of two different downloads.
    let Err((actual, cause)) = attempt else {
        unreachable!("a mismatched digest is returned as Err");
    };
    let _ = fs::remove_file(&request.resolved.part);

    if let Some(mirror) = request.mirror {
        match transfer(request, mirror, 0, false) {
            Ok(digest) if digest == expected => {
                promote(&request.resolved).map_err(|e| DownloadError {
                    url: mirror.to_owned(),
                    cause: e,
                    expected: Some(expected.to_owned()),
                    actual: Some(digest),
                })?;
                return Ok(Outcome::Downloaded);
            }
            Ok(digest) => {
                let _ = fs::remove_file(&request.resolved.part);
                return Err(DownloadError {
                    url: mirror.to_owned(),
                    cause: io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("the mirror also returned the wrong bytes, sha256 {digest}"),
                    ),
                    expected: Some(expected.to_owned()),
                    actual: Some(digest),
                });
            }
            Err(e) => return Err(e),
        }
    }

    Err(DownloadError {
        url: primary,
        cause,
        expected: Some(expected.to_owned()),
        actual: Some(actual),
    })
}

/// Decides what a download would have to do, without any network access.
fn inspect(resolved: &Resolved, entry: &ModelEntry) -> Result<Existing, DownloadError> {
    let expected = entry.sha256.as_str();

    if resolved.path.is_file() {
        // The declared size is checked before the digest. Hashing half a
        // gigabyte to learn the file is the wrong length wastes the user's
        // battery, and a size mismatch is already conclusive.
        let len = fs::metadata(&resolved.path)
            .map(|m| m.len())
            .map_err(|e| DownloadError {
                url: entry.url.clone(),
                cause: e,
                expected: None,
                actual: None,
            })?;
        if len != entry.size_bytes {
            return Ok(Existing::Mismatched);
        }
        return match hash_file(&resolved.path) {
            Ok(actual) if actual == expected => Ok(Existing::Verified),
            Ok(_) => Ok(Existing::Mismatched),
            Err(e) => Err(DownloadError {
                url: entry.url.clone(),
                cause: io::Error::other(e),
                expected: None,
                actual: None,
            }),
        };
    }

    // A `.part` at or past the declared size is not a usable prefix: the bytes
    // came from something that was not this model, or the file was appended to
    // twice. Both mean starting over, which is the same answer as no file.
    match fs::metadata(&resolved.part) {
        Ok(meta) if meta.len() > 0 && meta.len() < entry.size_bytes => {
            Ok(Existing::Partial(meta.len()))
        }
        _ => Ok(Existing::Absent),
    }
}

/// One transfer, hashing as the bytes stream past.
///
/// The digest comes from the same bytes on their way to disk. Hashing afterwards
/// means reading 400 MB twice, and on a slow disk that is the difference
/// between a download and a wait.
fn transfer(
    request: &mut Request<'_>,
    url: &str,
    offset: u64,
    resumed: bool,
) -> Result<String, DownloadError> {
    let total = request.entry.size_bytes;
    fs::create_dir_all(&request.resolved.dir).map_err(|e| failed(url, e))?;

    // Everything below is relative to `start`, not `offset`. A rejected resume
    // has to ask for byte zero and truncate the partial: handing a transport
    // that ignored Range a non-zero offset and then appending its full body to
    // the surviving prefix yields a file containing two copies of the model.
    let start = if resumed { offset } else { 0 };

    let mut body = request
        .transport
        .get(url, start)
        .map_err(|e| failed(url, e))?;

    let mut file = open_part(&request.resolved.part, start).map_err(|e| failed(url, e))?;
    let mut hasher = if start > 0 {
        seed_hasher(&request.resolved.part, start)
    } else {
        Sha256::new()
    };

    let mut buf = vec![0u8; CHUNK];
    let mut received = start;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(stopped(url, received, total, &e)),
        };
        file.write_all(&buf[..n])
            .map_err(|e| stopped(url, received, total, &e))?;
        hasher.update(&buf[..n]);
        received += n as u64;
        if let Some(cb) = request.on_progress.as_mut() {
            cb(received, total);
        }
    }

    if received < total {
        // Reported as an interrupted transfer rather than left to the digest, so
        // the caller keeps the `.part` and resumes instead of discarding a good
        // prefix and starting the whole 400 MB again.
        return Err(failed(
            url,
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "the connection ended after {received} of {total} bytes; the \
                     partial file is kept and the next attempt resumes from it"
                ),
            ),
        ));
    }

    // sync before the rename. A rename is atomic with respect to other
    // processes, but it is not a flush: without this, a crash between the
    // rename and the flush can leave a correctly named file full of zeroes,
    // which the next run's hash will catch but the user will not enjoy.
    file.sync_all().map_err(|e| failed(url, e))?;

    Ok(hex(&hasher.finalize()))
}

/// Opens the `.part` file for writing at `offset`, truncating when not resuming.
fn open_part(part: &Path, offset: u64) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true);
    if offset > 0 {
        let mut file = opts.open(part)?;
        file.set_len(offset)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(file)
    } else {
        opts.truncate(true);
        opts.open(part)
    }
}

/// Re-hashes an existing prefix so a resumed transfer can produce the digest of
/// the whole file rather than only of the new bytes.
fn seed_hasher(part: &Path, offset: u64) -> Sha256 {
    let mut hasher = Sha256::new();
    let Ok(mut file) = File::open(part) else {
        return hasher;
    };
    let mut buf = vec![0u8; CHUNK];
    let mut left = offset;
    while left > 0 {
        let capped = left.min(CHUNK as u64);
        let Ok(want) = usize::try_from(capped) else {
            break;
        };
        // A short read or an I/O error stops the scan. Either way the resulting
        // digest is not the digest of the prefix, so the transfer that follows
        // cannot match the registry and the file is refused. Failing towards a
        // wrong digest rather than towards a panic is the whole point.
        match file.read(&mut buf[..want]) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                left -= n as u64;
            }
        }
    }
    hasher
}

/// Moves a verified `.part` into place and sets its mode.
///
/// Rename first, then the mode. The window in which the file is visible under
/// its final name with a temporary mode is harmless; the reverse order would
/// leave a complete, correctly named file with the wrong mode if the process
/// died in between.
fn promote(resolved: &Resolved) -> io::Result<()> {
    fs::rename(&resolved.part, &resolved.path)?;
    fs::set_permissions(&resolved.path, model_permissions())
}

/// The mode a finished model gets.
#[cfg(unix)]
fn model_permissions() -> Permissions {
    use std::os::unix::fs::PermissionsExt;
    Permissions::from_mode(MODEL_MODE)
}

/// Windows has no mode bits, and no native Windows support in v1.x (ADR 0013).
#[cfg(not(unix))]
fn model_permissions() -> Permissions {
    Permissions::from_read_only(false)
}

/// Hashes a file on disk without loading it into memory.
fn hash_file(path: &Path) -> Result<String, HashError> {
    let mut file = File::open(path).map_err(HashError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(HashError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// The SHA-256 of `bytes`, for tests and for the registry's own empty-string
/// sentinel.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Lowercase hex, written out so a digest renders identically everywhere it
/// appears, including inside an error message a user is reading.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 0x0f)] as char);
    }
    out
}

/// Builds a request for `entry` against `transport`.
///
/// Exists so tests and callers share one construction path, and so no caller
/// can forget a field.
pub fn request<'a>(
    entry: &'a ModelEntry,
    resolved: Resolved,
    transport: &'a dyn Transport,
) -> Request<'a> {
    Request::bare(entry, resolved, transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::registry::Registry;
    use std::sync::Mutex;

    /// A hand-rolled temp directory.
    ///
    /// `tempfile` was removed at 1.2 because its `getrandom 0.4` line needs
    /// Rust 1.85, above the 1.75 floor. The point of these tests is not the
    /// directory; it is worth less than a dependency that breaks the MSRV.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            // Process id plus an atomic counter: unique within a run, and
            // unique across concurrent `cargo test` invocations without needing
            // randomness.
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("gcode-test-{}-{tag}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    use std::sync::atomic::{AtomicU64, Ordering};

    /// A transport serving fixed bytes, recording what it was asked for.
    struct FakeTransport {
        bodies: Vec<(String, Vec<u8>)>,
        supports_range: bool,
        /// `(url, offset)` per call, in order.
        calls: Mutex<Vec<(String, u64)>>,
        /// Bytes still owed before the body errors, to simulate a dropped
        /// connection mid-transfer.
        truncate_after: Option<usize>,
    }

    impl FakeTransport {
        fn new(bodies: Vec<(&str, &[u8])>) -> Self {
            Self {
                bodies: bodies
                    .into_iter()
                    .map(|(u, b)| (u.to_owned(), b.to_vec()))
                    .collect(),
                supports_range: true,
                calls: Mutex::new(Vec::new()),
                truncate_after: None,
            }
        }

        fn without_range(mut self) -> Self {
            self.supports_range = false;
            self
        }

        /// Serve at most this many bytes, then report a broken connection.
        fn truncating_after(mut self, n: usize) -> Self {
            self.truncate_after = Some(n);
            self
        }

        fn calls(&self) -> Vec<(String, u64)> {
            self.calls.lock().expect("calls").clone()
        }
    }

    /// Reads its body, then fails. What a socket does when the peer goes away:
    /// an error, not a short read that looks like a complete response.
    struct FailingReader {
        body: Vec<u8>,
        served: usize,
        fail_after: usize,
    }

    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.served >= self.fail_after {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "connection reset by peer",
                ));
            }
            let n = usize::min(buf.len(), self.fail_after - self.served).min(self.body.len());
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "connection reset by peer",
                ));
            }
            buf[..n].copy_from_slice(&self.body[..n]);
            self.body.drain(..n);
            self.served += n;
            Ok(n)
        }
    }

    /// Serves one body, optionally sliced at `offset`.
    impl Transport for FakeTransport {
        fn get(&self, url: &str, offset: u64) -> io::Result<Box<dyn Read + Send>> {
            self.calls
                .lock()
                .expect("calls")
                .push((url.to_owned(), offset));
            let body = self
                .bodies
                .iter()
                .find(|(u, _)| u == url)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("no such url: {url}"))
                })?;
            let cap = usize::try_from(u64::try_from(body.len()).unwrap_or(u64::MAX))
                .unwrap_or(usize::MAX);
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(cap);
            let sliced = body[start..].to_vec();
            Ok(match self.truncate_after {
                Some(n) if n < sliced.len() => Box::new(FailingReader {
                    body: sliced,
                    served: 0,
                    fail_after: n,
                }) as Box<dyn Read + Send>,
                _ => Box::new(io::Cursor::new(sliced)),
            })
        }

        fn supports_range(&self) -> bool {
            self.supports_range
        }
    }

    /// A registry entry pointing at `body`, with a correct digest for it.
    fn entry_for(name: &str, url: &str, body: &[u8]) -> ModelEntry {
        let toml = format!(
            "[[model]]\n\
             name = \"{name}\"\n\
             url = \"{url}\"\n\
             sha256 = \"{}\"\n\
             size_bytes = {}\n\
             default = true\n\
             context_size = 4096\n\
             license = \"MIT\"\n",
            sha256_hex(body),
            body.len()
        );
        let text = format!("{toml}__end__\n");
        let cut = text.find("__end__").expect("marker");
        Registry::parse(&text[..cut])
            .expect("fixture entry must be valid")
            .default_entry()
            .expect("fixture has a default")
            .clone()
    }

    /// An entry whose declared digest does not match the bytes on offer.
    fn entry_with_wrong_digest(name: &str, url: &str, body: &[u8]) -> ModelEntry {
        let mut e = entry_for(name, url, body);
        // A syntactically valid digest of something else entirely.
        e.sha256 = sha256_hex(b"not the model you are looking for");
        e
    }

    fn resolved_in(dir: &TempDir) -> Resolved {
        Resolved::in_dir(dir.path(), "fixture.gguf")
    }

    // ── resolve ────────────────────────────────────────────────────────────

    #[test]
    fn a_bare_name_gets_the_gguf_extension() {
        let dir = TempDir::new("name");
        let out = resolve(&ResolveInput {
            name: Some("qwen2.5-0.5b-instruct"),
            path: None,
            models_dir: dir.path(),
            search_dirs: &[],
        })
        .expect("resolve");
        assert_eq!(out.path, dir.path().join("qwen2.5-0.5b-instruct.gguf"));
    }

    #[test]
    fn a_name_that_already_names_a_gguf_is_left_alone() {
        let dir = TempDir::new("gguf");
        let out = resolve(&ResolveInput {
            name: Some("local.gguf"),
            path: None,
            models_dir: dir.path(),
            search_dirs: &[],
        })
        .expect("resolve");
        assert_eq!(out.path, dir.path().join("local.gguf"));
    }

    #[test]
    fn an_explicit_path_wins_over_everything() {
        let dir = TempDir::new("path");
        let explicit = dir.path().join("somewhere").join("mine.gguf");
        let out = resolve(&ResolveInput {
            name: Some("ignored"),
            path: Some(&explicit),
            models_dir: Path::new("/nonexistent"),
            search_dirs: &[],
        })
        .expect("resolve");
        assert_eq!(out.path, explicit);
    }

    /// A name carrying a path separator must not be able to steer a write out
    /// of the model directory. The name comes from a command line, so this is
    /// the boundary that matters.
    #[test]
    fn a_name_with_a_separator_cannot_escape_the_model_directory() {
        let dir = TempDir::new("escape");
        for name in ["../../etc/passwd", "/etc/passwd", "a/../../b"] {
            let out = resolve(&ResolveInput {
                name: Some(name),
                path: None,
                models_dir: dir.path(),
                search_dirs: &[],
            })
            .expect("resolve");
            assert_eq!(
                out.path.parent(),
                Some(dir.path()),
                "`{name}` escaped the model directory: {:?}",
                out.path
            );
        }
    }

    #[test]
    fn a_search_directory_holding_the_model_wins_over_the_download_dir() {
        let search = TempDir::new("search");
        let download = TempDir::new("download");
        fs::write(search.path().join("m.gguf"), b"x").expect("write");
        let out = resolve(&ResolveInput {
            name: Some("m"),
            path: None,
            models_dir: download.path(),
            search_dirs: &[search.path().to_path_buf()],
        })
        .expect("resolve");
        assert_eq!(out.path, search.path().join("m.gguf"));
    }

    #[test]
    fn search_directories_are_searched_in_order() {
        let first = TempDir::new("first");
        let second = TempDir::new("second");
        let download = TempDir::new("dl");
        fs::write(first.path().join("m.gguf"), b"a").expect("write");
        fs::write(second.path().join("m.gguf"), b"b").expect("write");
        let out = resolve(&ResolveInput {
            name: Some("m"),
            path: None,
            models_dir: download.path(),
            search_dirs: &[first.path().to_path_buf(), second.path().to_path_buf()],
        })
        .expect("resolve");
        assert_eq!(out.path, first.path().join("m.gguf"));
    }

    #[test]
    fn a_missing_download_directory_does_not_stop_resolve() {
        let out = resolve(&ResolveInput {
            name: Some("m"),
            path: None,
            models_dir: Path::new("/nonexistent/gcode/models"),
            search_dirs: &[],
        })
        .expect("resolve");
        assert_eq!(
            out.path,
            Path::new("/nonexistent/gcode/models").join("m.gguf")
        );
    }

    #[test]
    fn no_name_and_no_path_is_a_typed_error() {
        let err = resolve(&ResolveInput {
            name: None,
            path: None,
            models_dir: Path::new("/tmp"),
            search_dirs: &[],
        })
        .expect_err("must not invent a model");
        assert!(matches!(err, crate::Error::ModelNameRequired));
        let text = err.to_string();
        assert!(text.contains("--model"), "must say what to do: {text}");
    }

    // ── ensure: the paths that matter ──────────────────────────────────────

    #[test]
    fn a_correct_file_is_used_without_touching_the_network() {
        let dir = TempDir::new("present");
        let body = b"the model bytes";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        fs::write(&resolved.path, body).expect("write");

        let transport = FakeTransport::new(vec![]);
        let mut req = request(&entry, resolved, &transport);
        let outcome = ensure(&mut req).expect("already present");

        assert_eq!(outcome, Outcome::AlreadyPresent);
        assert!(!outcome.touched_network());
        assert!(transport.calls().is_empty(), "must not call out");
    }

    #[test]
    fn a_finished_file_is_renamed_into_place_and_hashed() {
        let dir = TempDir::new("download");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);

        let mut req = request(&entry, resolved.clone(), &transport);
        let outcome = ensure(&mut req).expect("download");

        assert_eq!(outcome, Outcome::Downloaded);
        assert!(outcome.touched_network());
        assert_eq!(fs::read(&resolved.path).expect("read"), body);
        assert!(!resolved.part.exists(), ".part must not survive");
        assert_eq!(
            transport.calls(),
            vec![("https://example.invalid/m.gguf".into(), 0)]
        );
    }

    #[test]
    fn a_partial_file_is_resumed_from_its_own_length() {
        let dir = TempDir::new("resume");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);

        // An earlier attempt wrote 10 of the 20 bytes and died.
        fs::write(&resolved.part, &body[..10]).expect("write");

        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        let outcome = ensure(&mut req).expect("resume");

        assert_eq!(outcome, Outcome::Resumed);
        assert_eq!(
            transport.calls(),
            vec![("https://example.invalid/m.gguf".into(), 10)],
            "must ask for the remaining bytes only"
        );
        assert_eq!(fs::read(&resolved.path).expect("read"), body);
    }

    /// The digest of a resumed file has to cover the whole file, not just the
    /// new bytes. A hasher seeded with only the tail would happily accept a
    /// prefix from one download and a suffix from another.
    #[test]
    fn a_resumed_file_is_hashed_across_both_halves() {
        let dir = TempDir::new("resume-hash");
        let head = b"first half ";
        let tail = b"second half";
        let mut body = head.to_vec();
        body.extend_from_slice(tail);
        let entry = entry_for("m", "https://example.invalid/m.gguf", &body);
        let resolved = resolved_in(&dir);
        fs::write(&resolved.part, head).expect("write");

        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", &body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("resume");
        assert_eq!(
            fs::read(&resolved.path).expect("read"),
            body,
            "the resumed file must be the whole model, not head+tail twice"
        );
    }

    #[test]
    fn a_transport_without_range_support_starts_over() {
        let dir = TempDir::new("norange");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        fs::write(&resolved.part, &body[..10]).expect("write");

        // The server ignores Range and sends the whole body. Appending it to the
        // existing prefix would produce 30 bytes, so the prefix is discarded.
        let transport =
            FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]).without_range();
        let mut req = request(&entry, resolved.clone(), &transport);
        let outcome = ensure(&mut req).expect("download");

        assert_eq!(outcome, Outcome::Downloaded);
        assert_eq!(fs::read(&resolved.path).expect("read"), body);
    }

    // ── ensure: the paths that must fail ───────────────────────────────────

    #[test]
    fn wrong_bytes_are_deleted_and_the_error_names_both_digests() {
        let dir = TempDir::new("mismatch");
        let served = b"these are not the model";
        let entry = entry_with_wrong_digest("m", "https://example.invalid/m.gguf", served);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", served)]);

        let mut req = request(&entry, resolved.clone(), &transport);
        let err = ensure(&mut req).expect_err("must refuse");

        assert_eq!(err.actual.as_deref(), Some(sha256_hex(served).as_str()));
        assert_eq!(err.expected.as_deref(), Some(entry.sha256.as_str()));
        let text = err.to_string();
        assert!(
            text.contains(&entry.sha256),
            "must print the expected: {text}"
        );
        assert!(
            text.contains(&sha256_hex(served)),
            "must print what arrived: {text}"
        );
        assert!(!resolved.part.exists(), "the bad bytes must be deleted");
        assert!(!resolved.path.exists(), "nothing may be promoted");
    }

    #[test]
    fn a_mirror_is_tried_when_the_primary_returns_the_wrong_bytes() {
        let dir = TempDir::new("mirror");
        let good = b"the real model bytes";
        let bad = b"a captive portal or a wrong registry entry";
        let entry = entry_for("m", "https://example.invalid/m.gguf", good);

        let transport = FakeTransport::new(vec![
            ("https://example.invalid/m.gguf", bad),
            ("https://mirror.invalid/m.gguf", good),
        ]);
        let resolved = resolved_in(&dir);
        let mut req = request(&entry, resolved.clone(), &transport);
        req.mirror = Some("https://mirror.invalid/m.gguf");

        let outcome = ensure(&mut req).expect("mirror saves it");
        assert_eq!(outcome, Outcome::Downloaded);
        assert_eq!(fs::read(&resolved.path).expect("read"), good);
        assert_eq!(
            transport.calls(),
            vec![
                ("https://example.invalid/m.gguf".into(), 0),
                ("https://mirror.invalid/m.gguf".into(), 0),
            ]
        );
    }

    #[test]
    fn a_mirror_that_also_returns_the_wrong_bytes_is_refused() {
        let dir = TempDir::new("mirror-bad");
        let entry = entry_for("m", "https://example.invalid/m.gguf", b"expected");
        let bad = b"wrong here too";
        let transport = FakeTransport::new(vec![
            ("https://example.invalid/m.gguf", bad),
            ("https://mirror.invalid/m.gguf", bad),
        ]);
        let resolved = resolved_in(&dir);
        let mut req = request(&entry, resolved.clone(), &transport);
        req.mirror = Some("https://mirror.invalid/m.gguf");

        let err = ensure(&mut req).expect_err("must refuse");
        assert!(err.actual.is_some());
        assert!(!resolved.path.exists());
        assert!(!resolved.part.exists());
    }

    #[test]
    fn a_wrong_sized_file_is_replaced_rather_than_hashed() {
        let dir = TempDir::new("size");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        // Right name, wrong length: a truncated or foreign file.
        fs::write(&resolved.path, b"0123456789").expect("write");

        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("re-download");

        assert_eq!(fs::read(&resolved.path).expect("read"), body);
        assert_eq!(transport.calls().len(), 1, "must re-download once");
    }

    /// A `.part` at or past the declared size cannot be a prefix of this model.
    /// Resuming onto it would produce a file longer than the registry says, and
    /// a file that long is not this model by definition.
    #[test]
    fn an_oversized_partial_is_discarded_not_appended_to() {
        let dir = TempDir::new("oversize-part");
        let body = b"0123456789";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        fs::write(&resolved.part, b"far too many bytes to be a prefix").expect("write");

        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        let outcome = ensure(&mut req).expect("fresh download");

        assert_eq!(outcome, Outcome::Downloaded);
        assert_eq!(fs::read(&resolved.path).expect("read"), body);
    }

    #[test]
    fn a_dropped_connection_keeps_the_prefix_for_the_next_attempt() {
        let dir = TempDir::new("dropped");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        let transport =
            FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]).truncating_after(8);

        let mut req = request(&entry, resolved.clone(), &transport);
        let err = ensure(&mut req).expect_err("truncated transfer");

        // The prefix is worth keeping: the next run resumes from 8. A dropped
        // connection is not a corrupt file, and treating it as one would throw
        // away the only good copy of 8 bytes and start the 400 MB again.
        assert_eq!(fs::metadata(&resolved.part).expect("part").len(), 8);
        assert!(!resolved.path.exists());
        let text = err.to_string();
        assert!(
            text.contains("resumes from"),
            "the error must say the prefix survives: {text}"
        );
        assert!(
            !text.contains("sha256"),
            "a short transfer is not a hash mismatch: {text}"
        );
        assert_eq!(
            err.actual, None,
            "no digest is claimed for a transfer that never finished"
        );
    }

    #[test]
    fn a_transport_error_names_the_url() {
        let dir = TempDir::new("urlerr");
        let body = b"0123456789";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let transport = FakeTransport::new(vec![]);
        let resolved = resolved_in(&dir);
        let mut req = request(&entry, resolved, &transport);

        let err = ensure(&mut req).expect_err("no such url");
        assert!(err.url.contains("example.invalid"), "{err}");
    }

    #[test]
    fn progress_is_reported_against_the_registry_size() {
        let dir = TempDir::new("progress");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);

        let seen: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
        let mut recorder = |received: u64, total: u64| {
            seen.lock().expect("seen").push((received, total));
        };
        let mut req = request(&entry, resolved, &transport);
        req.on_progress = Some(&mut recorder);
        ensure(&mut req).expect("download");

        let seen = seen.into_inner().expect("seen");
        assert!(!seen.is_empty(), "progress must be reported");
        assert_eq!(
            seen.last().copied(),
            Some((20, 20)),
            "must end at full size"
        );
        assert!(
            seen.iter().all(|(_, total)| *total == 20),
            "the total comes from the registry, not the server: {seen:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_finished_model_is_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("mode");
        let body = b"0123456789";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("download");

        let mode = fs::metadata(&resolved.path)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644, "got {:o}", mode & 0o777);
    }

    #[test]
    fn the_download_directory_is_created_if_absent() {
        let dir = TempDir::new("mkdir");
        let nested = dir.path().join("a").join("b");
        let body = b"0123456789";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = Resolved::in_dir(&nested, "m.gguf");
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("download");
        assert!(resolved.path.is_file());
    }

    #[test]
    fn two_runs_of_the_same_command_download_once() {
        let dir = TempDir::new("idempotent");
        let body = b"0123456789abcdefghij";
        let entry = entry_for("m", "https://example.invalid/m.gguf", body);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);

        let first = {
            let mut req = request(&entry, resolved.clone(), &transport);
            ensure(&mut req).expect("first")
        };
        let second = {
            let mut req = request(&entry, resolved, &transport);
            ensure(&mut req).expect("second")
        };

        assert_eq!(first, Outcome::Downloaded);
        assert_eq!(second, Outcome::AlreadyPresent);
        assert_eq!(transport.calls().len(), 1, "the network is used once");
    }

    #[test]
    fn a_file_larger_than_the_registry_says_is_never_accepted() {
        let dir = TempDir::new("toolarge");
        let body = b"0123456789";
        let mut entry = entry_for("m", "https://example.invalid/m.gguf", body);
        entry.size_bytes = 5;
        let resolved = resolved_in(&dir);
        // Longer than declared, and hashing to the right value, which is the
        // shape of a registry entry that went stale.
        fs::write(&resolved.path, body).expect("write");

        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("re-download");
        assert_eq!(transport.calls().len(), 1);
    }

    // ── the digest itself ──────────────────────────────────────────────────

    #[test]
    fn sha256_matches_the_published_vectors() {
        // NIST/RFC test vectors. If this drifts, every pinned checksum in the
        // registry is meaningless and nothing else will notice.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(&b"a".repeat(1_000_000)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn a_digest_is_lowercase_hex_of_the_right_width() {
        let d = sha256_hex(b"anything");
        assert_eq!(d.len(), 64);
        assert!(d
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    /// The chunk size is an implementation detail, but a body larger than it
    /// must be hashed across several reads. A 3 MiB body against a 256 KiB
    /// buffer catches an off-by-one in the loop that a small fixture never
    /// would.
    #[test]
    fn a_body_larger_than_one_chunk_hashes_correctly() {
        let dir = TempDir::new("big");
        let body: Vec<u8> = (0..(CHUNK * 3 + 17))
            .map(|i| u8::try_from(i % 251).expect("byte"))
            .collect();
        let entry = entry_for("m", "https://example.invalid/m.gguf", &body);
        let resolved = resolved_in(&dir);
        let transport = FakeTransport::new(vec![("https://example.invalid/m.gguf", &body)]);
        let mut req = request(&entry, resolved.clone(), &transport);
        ensure(&mut req).expect("download");
        assert_eq!(fs::read(&resolved.path).expect("read").len(), body.len());
    }
}
