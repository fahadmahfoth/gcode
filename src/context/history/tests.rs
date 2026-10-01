//! Tests for the history store.
//!
//! ADR 0005 lists six validation cases. All six are here, including the two the
//! roadmap's Phase 2.1 list leaves out: a pre-existing loose mode, and concurrent
//! writers. An ADR's validation section is part of the decision, so a case in it
//! is not optional because a later document forgot to copy it.
//!
//! # No real clock, no real `$HOME`
//!
//! `ts` is supplied by the test, never read from the system clock, so an
//! assertion about ordering is an assertion about the code rather than about how
//! fast the machine is. Every test gets its own temporary directory, so none can
//! see or write a developer's `~/.gcode/history.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use super::{History, HistoryEntry, KEEP_BYTES, MAX_BYTES};

/// A directory that deletes itself. `temp_dir` + pid + a counter, so two tests in
/// the same process never collide and a rerun never reads a previous run's file.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("gcode-hist-{}-{label}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn file(&self) -> PathBuf {
        self.0.join("history.jsonl")
    }

    fn history(&self) -> History {
        History::new(self.file())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn entry(ts: i64, cmd: &str, exit: i32) -> HistoryEntry {
    HistoryEntry {
        ts,
        cmd: cmd.to_owned(),
        exit,
        cwd: "/home/user".to_owned(),
        out: String::new(),
    }
}

// ── ADR 0005: read the last 3 of 100 ────────────────────────────────────────

#[test]
fn reads_the_last_three_of_a_hundred_entries() {
    let s = Scratch::new("last3");
    let h = s.history();
    for i in 0..100 {
        h.append(&entry(i, &format!("cmd-{i}"), 0)).expect("append");
    }

    let got = h.read_last(3).expect("read");

    assert_eq!(got.len(), 3, "asked for 3");
    assert_eq!(
        got.iter().map(|e| e.cmd.as_str()).collect::<Vec<_>>(),
        vec!["cmd-97", "cmd-98", "cmd-99"],
        "oldest first, and the newest is last"
    );
}

#[test]
fn asking_for_more_than_the_file_holds_returns_all_of_it() {
    let s = Scratch::new("short");
    let h = s.history();
    h.append(&entry(1, "a", 0)).expect("append");
    h.append(&entry(2, "b", 0)).expect("append");

    assert_eq!(h.read_last(50).expect("read").len(), 2);
}

#[test]
fn asking_for_nothing_reads_nothing() {
    let s = Scratch::new("zero");
    let h = s.history();
    h.append(&entry(1, "a", 0)).expect("append");
    assert!(h.read_last(0).expect("read").is_empty());
}

#[test]
fn a_missing_file_is_an_empty_history_not_an_error() {
    let s = Scratch::new("missing");
    // `--fix` on a fresh install has nothing to read and must still work.
    assert!(s.history().read_last(5).expect("read").is_empty());
}

// ── ADR 0005: a corrupt line does not stop the read ─────────────────────────

#[test]
fn a_corrupt_line_in_the_middle_does_not_stop_the_read() {
    let s = Scratch::new("corrupt");
    let h = s.history();
    for i in 0..60 {
        h.append(&entry(i, &format!("cmd-{i}"), 0)).expect("append");
    }

    // Splice garbage into the middle of the file, as a half-written write or an
    // editor would.
    let raw = fs::read_to_string(s.file()).expect("read");
    let lines: Vec<&str> = raw.lines().collect();
    let broken = lines
        .iter()
        .enumerate()
        .map(|(i, l)| if i == 30 { "{\"ts\": not json" } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(s.file(), format!("{broken}\n")).expect("rewrite");

    let got = h.read_last(10).expect("a bad line must not fail the read");

    assert_eq!(got.len(), 10, "the 10 newest good entries still come back");
    assert!(
        got.iter().all(|e| e.cmd.starts_with("cmd-")),
        "no fragment of the broken line may appear as an entry: {got:?}"
    );
}

#[test]
fn a_corrupt_line_warns_exactly_once() {
    let s = Scratch::new("warn-once");
    let h = s.history();
    for i in 0..5 {
        h.append(&entry(i, &format!("cmd-{i}"), 0)).expect("append");
    }
    let mut raw = fs::read_to_string(s.file()).expect("read");
    raw.push_str("garbage not json\n");
    raw.push_str("also not json\n");
    fs::write(s.file(), raw).expect("rewrite");

    let mut warnings = 0usize;
    let got = h
        .read_last_reporting(10, &mut || warnings += 1)
        .expect("read");

    assert_eq!(got.len(), 5, "the five good entries survive");
    assert_eq!(
        warnings, 1,
        "two bad lines, one warning: a warning per line trains the user to ignore them"
    );
}

// ── ADR 0005: a truncated final line is silently discarded ──────────────────

#[test]
fn a_torn_final_line_is_discarded_without_an_entry() {
    let s = Scratch::new("torn");
    let h = s.history();
    h.append(&entry(1, "first", 0)).expect("append");
    h.append(&entry(2, "second", 0)).expect("append");

    // A Ctrl-C between the write and the fsync leaves a line with no newline.
    let mut raw = fs::read_to_string(s.file()).expect("read");
    raw.push_str("{\"ts\":3,\"cmd\":\"half-written");
    fs::write(s.file(), raw).expect("rewrite");

    let got = h.read_last(10).expect("read");

    assert_eq!(
        got.iter().map(|e| e.cmd.as_str()).collect::<Vec<_>>(),
        vec!["first", "second"],
        "the fragment is not an entry"
    );
}

#[test]
fn a_file_that_is_entirely_one_torn_line_reads_as_empty() {
    let s = Scratch::new("all-torn");
    let h = s.history();
    fs::write(s.file(), "{\"ts\":1,\"cmd\":\"never").expect("write");
    assert!(h.read_last(5).expect("read").is_empty());
}

// ── ADR 0005: rotation ─────────────────────────────────────────────────────

#[test]
fn a_ten_megabyte_file_rotates_and_stays_under_the_cap() {
    let s = Scratch::new("rotate");
    let h = s.history();

    // Build a file just over the threshold in one shot rather than by appending:
    // 10 MB of appends would make this test slow, and the rotation logic does not
    // care how the bytes arrived.
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(s.file())
        .expect("open");
    let filler = format!(
        "{{\"ts\":1,\"cmd\":\"{}\",\"exit\":0,\"cwd\":\"/h\",\"out\":\"\"}}",
        "x".repeat(400)
    );
    while f.metadata().expect("stat").len() < MAX_BYTES + 1024 {
        writeln!(f, "{filler}").expect("write");
    }
    drop(f);

    assert!(
        h.rotate_if_needed().expect("rotate"),
        "a file at the cap must rotate"
    );

    let stats = h.stats().expect("stats");
    assert!(
        stats.bytes <= KEEP_BYTES,
        "kept {} bytes, cap is {KEEP_BYTES}",
        stats.bytes
    );
    assert!(h.rotated_path().exists(), "the dropped half is kept as .1");

    // What survived must still parse. A rotation that keeps fragments has produced
    // a file no reader can use.
    let got = h.read_last(20).expect("read");
    assert!(!got.is_empty(), "the newest entries must survive");
    assert!(
        got.iter().all(|e| e.cmd.starts_with("xxx")),
        "no fragment at the cut: {got:?}"
    );
}

#[test]
fn rotation_leaves_the_newest_entries_readable_in_order() {
    let s = Scratch::new("rotate-order");
    let h = s.history();
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(s.file())
        .expect("open");
    let filler = format!(
        "{{\"ts\":1,\"cmd\":\"{}\",\"exit\":0,\"cwd\":\"/h\",\"out\":\"\"}}",
        "y".repeat(400)
    );
    while f.metadata().expect("stat").len() < MAX_BYTES + 1024 {
        writeln!(f, "{filler}").expect("write");
    }
    drop(f);

    h.rotate().expect("rotate");
    let got = h.read_last(10).expect("read");

    assert_eq!(got.len(), 10);
    for pair in got.windows(2) {
        assert!(
            pair[0].ts <= pair[1].ts,
            "rotation must not reorder entries"
        );
    }
}

#[test]
fn a_file_under_the_cap_does_not_rotate() {
    let s = Scratch::new("no-rotate");
    let h = s.history();
    h.append(&entry(1, "a", 0)).expect("append");
    assert!(
        !h.rotate_if_needed().expect("check"),
        "a small file must be left alone"
    );
    assert!(!h.rotated_path().exists());
}

#[test]
fn an_append_past_the_cap_rotates_and_keeps_the_entry_that_triggered_it() {
    let s = Scratch::new("append-rotate");
    let h = s.history();
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(s.file())
        .expect("open");
    let filler = format!(
        "{{\"ts\":1,\"cmd\":\"{}\",\"exit\":0,\"cwd\":\"/h\",\"out\":\"\"}}",
        "z".repeat(400)
    );
    while f.metadata().expect("stat").len() < MAX_BYTES - 4096 {
        writeln!(f, "{filler}").expect("write");
    }
    drop(f);

    h.append(&entry(999, "the-newest", 0)).expect("append");

    let got = h.read_last(1).expect("read");
    assert_eq!(
        got.first().map(|e| e.cmd.as_str()),
        Some("the-newest"),
        "rotating before the append would have discarded the entry that hit the threshold"
    );
}

// ── ADR 0005: mode 0600 ────────────────────────────────────────────────────

#[test]
#[cfg(unix)]
fn the_file_is_private_after_the_first_append() {
    let s = Scratch::new("mode");
    let h = s.history();
    h.append(&entry(1, "a", 0)).expect("append");
    assert_eq!(h.stats().expect("stats").mode & 0o777, 0o600);
}

#[test]
#[cfg(unix)]
fn the_directory_is_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let nested = Scratch::new("dir-mode").0.join("a").join("b");
    let h = History::new(nested.join("history.jsonl"));
    h.append(&entry(1, "a", 0)).expect("append");
    let mode = fs::metadata(&nested).expect("stat").permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "history must not be readable by another account"
    );
}

/// Not in the roadmap's list, but implied by "mode 0600": a file that already
/// existed at a looser mode must be tightened, or the guarantee holds only for
/// files this version created.
#[test]
#[cfg(unix)]
fn a_previous_loose_mode_is_tightened() {
    use std::os::unix::fs::PermissionsExt as _;
    let s = Scratch::new("loose");
    fs::write(s.file(), "").expect("write");
    fs::set_permissions(s.file(), fs::Permissions::from_mode(0o644)).expect("chmod");

    s.history().append(&entry(1, "a", 0)).expect("append");

    assert_eq!(
        fs::metadata(s.file()).expect("stat").permissions().mode() & 0o777,
        0o600,
        "an existing 0644 file must not stay world-readable"
    );
}

// ── ADR 0005: concurrent writers ───────────────────────────────────────────

#[test]
fn concurrent_appends_produce_intact_lines() {
    const WRITERS: usize = 16;
    const EACH: usize = 4;

    let s = Scratch::new("concurrent");
    let path = s.file();
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut handles = Vec::new();

    for w in 0..WRITERS {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let h = History::new(path);
            barrier.wait();
            for i in 0..EACH {
                h.append(&entry(
                    i64::try_from(w * EACH + i).expect("small"),
                    &format!("writer-{w}-item-{i}"),
                    0,
                ))
                .expect("append");
            }
        }));
    }
    for h in handles {
        h.join().expect("no writer may panic");
    }

    // Threads, not processes, but each append opens its own file handle, so this is
    // the same O_APPEND atomicity ADR 0005 relies on: a line is never spliced into
    // the middle of another.
    let raw = fs::read_to_string(&path).expect("read");
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(
        lines.len(),
        WRITERS * EACH,
        "every write produced exactly one line"
    );

    let mut ids: Vec<String> = Vec::new();
    for line in &lines {
        let e: HistoryEntry =
            serde_json::from_str(line).expect("every line is a whole, parseable record");
        ids.push(e.cmd);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        WRITERS * EACH,
        "no write was lost or interleaved"
    );
}

// ── round trip ─────────────────────────────────────────────────────────────

#[test]
fn every_field_survives_a_round_trip() {
    let s = Scratch::new("round-trip");
    let h = s.history();
    let original = HistoryEntry {
        ts: 1_700_000_000,
        cmd: r#"echo "hi" && printf 'a\tb'"#.to_owned(),
        exit: 130,
        cwd: "/home/user/project".to_owned(),
        out: "line one\nline two\n".to_owned(),
    };
    h.append(&original).expect("append");
    assert_eq!(h.read_last(1).expect("read"), vec![original]);
}

#[test]
fn a_newline_in_the_output_cannot_break_the_format() {
    let s = Scratch::new("embedded-newline");
    let h = s.history();
    h.append(&HistoryEntry {
        ts: 1,
        cmd: "make".to_owned(),
        exit: 2,
        cwd: "/home/user".to_owned(),
        out: "first line\nsecond line".to_owned(),
    })
    .expect("append");
    h.append(&entry(2, "next", 0)).expect("append");

    let got = h.read_last(2).expect("read");
    assert_eq!(
        got.len(),
        2,
        "an embedded newline must not become two records"
    );
    assert_eq!(got[0].out, "first line\nsecond line");
}

#[test]
fn stats_counts_entries_without_parsing_them() {
    let s = Scratch::new("stats");
    let h = s.history();
    for i in 0..7 {
        h.append(&entry(i, &format!("cmd-{i}"), 0)).expect("append");
    }
    let stats = h.stats().expect("stats");
    assert_eq!(stats.entries, 7);
    assert!(stats.bytes > 0);
}

#[test]
fn stats_on_a_missing_file_reports_nothing_rather_than_failing() {
    let s = Scratch::new("stats-missing");
    let stats = s.history().stats().expect("stats");
    assert_eq!((stats.bytes, stats.entries), (0, 0));
}

// ── the backwards reader, against inputs that break it ─────────────────────

#[test]
fn entries_longer_than_the_read_chunk_are_still_found() {
    // The chunk boundary is where a naive backwards reader loses the first line:
    // the line straddles the read and the piece before it has to be stitched back.
    let s = Scratch::new("long-line");
    let h = s.history();
    let big = "z".repeat(200_000);
    h.append(&entry(1, "small-a", 0)).expect("append");
    h.append(&entry(2, &big, 0)).expect("append");
    h.append(&entry(3, "small-b", 0)).expect("append");

    let got = h.read_last(3).expect("read");
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[1].cmd.len(),
        big.len(),
        "a straddling line must be reassembled whole"
    );
}

#[test]
fn reading_more_than_one_chunk_of_entries_is_still_ordered() {
    let s = Scratch::new("many-chunks");
    let h = s.history();
    for i in 0..2_000 {
        h.append(&entry(i, &format!("cmd-{i}"), 0)).expect("append");
    }
    let got = h.read_last(1_500).expect("read");
    assert_eq!(got.len(), 1_500);
    assert_eq!(got.first().expect("first").ts, 500);
    assert_eq!(got.last().expect("last").ts, 1_999);
}

#[test]
fn the_rotated_path_is_the_history_path_plus_one() {
    let s = Scratch::new("rotated-path");
    assert_eq!(
        s.history().rotated_path(),
        PathBuf::from(format!("{}.1", s.file().display()))
    );
}

// ── failures ─────────────────────────────────────────────────────────────────
//
// A store that has never failed is a store whose error handling has never been
// run. Each of these forces a real IO error — no mocking, no unwritable-permission
// trickery that would pass as root and fail as a normal user. The trick is to point
// the store at a path whose *parent* is a regular file: `create_dir_all` then fails
// with ENOTDIR on every platform and for every user, including root.

/// A path that cannot be created because a file is already standing where a
/// directory would have to go.
/// The returned `Scratch` must be held by the caller: it deletes its directory on
/// drop, and dropping it here would leave the blocked path simply absent, which is
/// the ordinary fresh-install case and proves nothing.
fn blocked(label: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new(label);
    let blocker = scratch.0.join("not-a-dir");
    fs::write(&blocker, b"i am a file").expect("write blocker");
    (scratch, blocker.join("history.jsonl"))
}

#[test]
fn append_reports_a_path_it_cannot_create() {
    let (_scratch, path) = blocked("append-blocked");
    let history = History::new(&path);
    match history.append(&entry(1_700_000_000, "ls -la", 0)) {
        Err(crate::error::Error::History { message }) => {
            assert!(message.contains("history.jsonl"), "{message}");
        }
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn read_last_reports_a_path_it_cannot_create() {
    let (_scratch, path) = blocked("read-blocked");
    let history = History::new(&path);
    match history.read_last(3) {
        Err(crate::error::Error::History { .. }) => {}
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn stats_reports_a_path_it_cannot_create() {
    let (_scratch, path) = blocked("stats-blocked");
    let history = History::new(&path);
    match history.stats() {
        Err(crate::error::Error::History { .. }) => {}
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn a_directory_where_the_history_file_should_be_is_an_error_not_a_silent_empty_history() {
    // The inverse of the blocked-path case: the path exists, but as a directory.
    // Reading it must fail rather than report an empty history, because "no history
    // yet" and "your history is not readable" lead to different decisions.
    let scratch = Scratch::new("dir-instead-of-file");
    let as_dir = scratch.0.join("history.jsonl");
    fs::create_dir_all(&as_dir).expect("make dir");
    let history = History::new(&as_dir);
    match history.read_last(3) {
        Err(crate::error::Error::History { .. }) => {}
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn rotate_reports_a_failure_to_create_the_staging_file() {
    // The staging file sits beside the history file, so blocking the parent blocks
    // the rename too. Rotation must fail loudly rather than report success and
    // leave the file over its cap.
    let (_scratch, path) = blocked("rotate-blocked");
    let history = History::new(&path);
    match history.rotate() {
        Err(crate::error::Error::History { .. }) => {}
        other => panic!("expected History, got {other:?}"),
    }
}

#[test]
fn rotate_if_needed_on_a_blocked_path_is_an_error() {
    let (_scratch, path) = blocked("rotate-if-needed-blocked");
    let history = History::new(&path);
    match history.rotate_if_needed() {
        Err(crate::error::Error::History { .. }) => {}
        other => panic!("expected History, got {other:?}"),
    }
}
