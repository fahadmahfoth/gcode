//! Tests for the shell hook installer.
//!
//! The pure functions in the parent module are tested with string literals,
//! because the byte-precision the ADR requires is a property of the transformation
//! and not of the filesystem. The impure half gets its own temporary directory
//! and its own rc file, and no test here reads or writes a real `~/.bashrc`.
//!
//! # The cases that matter
//!
//! - Install twice produces a file identical to install once (ADR 0007).
//! - Remove after install returns the file byte for byte (ADR 0007).
//! - A user's own lines survive both, including their blank lines and their
//!   `PROMPT_COMMAND`.
//! - An unreadable rc file is refused rather than truncated.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;

use super::install::{changed_lines, Installer};
use super::{
    block, plan_install, plan_remove, status, Kind, Outcome, Plan, Status, BEGIN_MARKER, END_MARKER,
};

/// A synthetic hook path. Nothing here is read; it only has to be stable so the
/// expected blocks can be written out literally.
const HOOK: &str = "/home/user/.gcode/shell/gcode.bash";
const ZSH_HOOK: &str = "/home/user/.gcode/shell/gcode.zsh";

fn hook_path() -> PathBuf {
    PathBuf::from(HOOK)
}

/// A realistic user rc file: an alias, a blank line, and an existing
/// `PROMPT_COMMAND`. This is the shape that breaks naive installers.
const USER_RC: &str = "\
alias ll='ls -la'

PROMPT_COMMAND='history -a'
export EDITOR=vi
";

/// The rc file a first install should produce, written out in full.
fn installed_rc(block: &str) -> String {
    format!("{USER_RC}\n{block}")
}

#[test]
fn the_block_is_four_lines_with_the_markers_adr_0007_names() {
    let text = block(&hook_path());
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "unexpected block shape: {text}");
    assert_eq!(lines[0], BEGIN_MARKER);
    assert_eq!(lines[3], END_MARKER);
    assert!(text.ends_with('\n'), "appending must not join two lines");
}

#[test]
fn the_block_records_the_version_that_wrote_it() {
    // `--check` reports the installed version by parsing this line, and it can
    // only do that if install put it there.
    let text = block(&hook_path());
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "no version in: {text}"
    );
}

#[test]
fn the_hook_path_is_quoted_so_a_spaced_home_still_works() {
    let text = block(Path::new("/home/a b/.gcode/shell/gcode.bash"));
    assert!(
        text.contains("source '/home/a b/.gcode/shell/gcode.bash'"),
        "unquoted path in: {text}"
    );
}

#[test]
fn an_embedded_single_quote_in_the_path_is_escaped_not_closed() {
    // The failure this guards is a block that cannot be sourced at all, which
    // would break the user's prompt on the next shell start.
    let text = block(Path::new("/home/o'brien/.gcode/shell/gcode.bash"));
    let source_line = text.lines().nth(2).expect("source line");
    assert_eq!(
        source_line,
        r"source '/home/o'\''brien/.gcode/shell/gcode.bash'"
    );
}

#[test]
fn zsh_gets_its_own_hook_name_in_the_block() {
    // The block is generated per shell, so a zsh install must not reference the
    // bash hook. ADR 0007 point 2.
    let text = block(Path::new(ZSH_HOOK));
    assert!(text.contains("gcode.zsh"), "wrong hook in: {text}");
}

#[test]
fn install_appends_the_block_after_every_existing_line() {
    let plan = plan_install(USER_RC, &hook_path()).unwrap();
    assert_eq!(plan.outcome, Outcome::Added);
    assert_eq!(plan.content, installed_rc(&block(&hook_path())));
}

#[test]
fn install_appends_to_an_empty_file_without_a_leading_newline() {
    // A leading blank line in a fresh .bashrc is cosmetic, but it is the kind of
    // thing that makes a diff look wrong, so the empty case is its own branch.
    let plan = plan_install("", &hook_path()).unwrap();
    assert_eq!(plan.content, block(&hook_path()));
    assert!(!plan.content.starts_with('\n'), "stray leading newline");
}

#[test]
fn install_normalises_a_missing_trailing_newline_and_remove_restores_it() {
    // The awkward case: a file whose last line has no newline. Install has to add
    // one to append a block, and remove has to take it away again or the file is
    // not byte-identical.
    let original = "export EDITOR=vi";
    let installed = plan_install(original, &hook_path()).unwrap();
    assert!(
        installed.content.ends_with('\n'),
        "block needs a final newline"
    );
    let removed = plan_remove(&installed.content).unwrap();
    assert_eq!(
        removed.content, original,
        "trailing newline was not restored"
    );
}

#[test]
fn install_twice_produces_a_file_identical_to_install_once() {
    // ADR 0007 validation, verbatim.
    let once = plan_install(USER_RC, &hook_path()).unwrap();
    let twice = plan_install(&once.content, &hook_path()).unwrap();
    assert_eq!(twice.outcome, Outcome::Unchanged);
    assert_eq!(twice.content, once.content);
    assert!(
        changed_lines(&twice).is_empty(),
        "an unchanged install must not claim it added lines"
    );
}

#[test]
fn install_never_produces_two_blocks() {
    let mut content = plan_install(USER_RC, &hook_path()).unwrap().content;
    for _ in 0..5 {
        content = plan_install(&content, &hook_path()).unwrap().content;
    }
    assert_eq!(count(&content, BEGIN_MARKER), 1, "blocks accumulated");
}

#[test]
fn remove_after_install_is_byte_identical_to_the_original() {
    // ADR 0007 validation, verbatim.
    for original in [
        "",
        "\n",
        "alias ll='ls -la'\n",
        "alias ll='ls -la'",
        USER_RC,
        "PROMPT_COMMAND='history -a'\n\n\nalias x=1\n",
    ] {
        let installed = plan_install(original, &hook_path()).unwrap();
        let removed = plan_remove(&installed.content).unwrap();
        assert_eq!(
            removed.content, original,
            "round trip changed the file for {original:?}"
        );
    }
}

#[test]
fn remove_keeps_the_users_own_lines_and_their_blank_lines() {
    let installed = plan_install(USER_RC, &hook_path()).unwrap().content;
    let removed = plan_remove(&installed).unwrap();
    assert!(removed.content.contains("PROMPT_COMMAND='history -a'"));
    assert!(removed.content.contains("alias ll='ls -la'"));
    assert!(removed.content.contains("export EDITOR=vi"));
    assert_eq!(removed.outcome, Outcome::Removed);
    assert_eq!(
        removed.lines.len(),
        4,
        "must report the four lines it removed"
    );
}

#[test]
fn remove_when_nothing_is_installed_changes_nothing_and_says_so() {
    let plan = plan_remove(USER_RC).unwrap();
    assert_eq!(plan.outcome, Outcome::NotInstalled);
    assert_eq!(plan.content, USER_RC);
    assert!(changed_lines(&plan).is_empty(), "nothing was removed");
}

#[test]
fn remove_takes_the_separator_newline_but_not_the_users_blank_lines() {
    // The separator is the one newline this module inserted. Removing "all
    // preceding blank lines" would eat the user's own.
    let installed = plan_install(USER_RC, &hook_path()).unwrap().content;
    let removed = plan_remove(&installed).unwrap();
    assert!(
        removed.content.ends_with("export EDITOR=vi\n"),
        "expected the original ending, got {:?}",
        &removed.content[removed.content.len().saturating_sub(20)..]
    );
}

#[test]
fn install_replaces_an_older_block_in_place() {
    // A user who upgrades gcode, or moves their hook file, must get one block
    // pointing at the new path — not two blocks both claiming PROMPT_COMMAND.
    let stale = "PROMPT_COMMAND='history -a'\n\n\
        # >>> gcode init >>>\n# gcode hook 0.0.1\nsource '/old/path/gcode.bash'\n\
        # <<< gcode init <<<\n";
    let plan = plan_install(stale, &hook_path()).unwrap();
    assert_eq!(plan.outcome, Outcome::Updated);
    assert_eq!(count(&plan.content, BEGIN_MARKER), 1, "two blocks");
    assert!(plan.content.contains(HOOK), "not repointed");
    assert!(!plan.content.contains("/old/path"), "stale block survived");
    assert!(plan.content.contains("PROMPT_COMMAND='history -a'"));
}

#[test]
fn an_unterminated_block_is_refused_rather_than_repaired() {
    // Appending past it would leave two blocks; guessing where it ended would risk
    // deleting the user's own lines on the next remove.
    let broken = format!("alias x=1\n{BEGIN_MARKER}\nsource '{HOOK}'\n");
    for result in [
        plan_install(&broken, &hook_path()).map(|_| ()),
        plan_remove(&broken).map(|_| ()),
        status(&broken, &hook_path()).map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(Error::ShellBlockMalformed)),
            "accepted a malformed block"
        );
    }
}

#[test]
fn a_marker_that_is_only_a_prefix_is_not_a_marker() {
    // '# >>> gcode init >>> ' with trailing text, or a commented-out copy, must not
    // be mistaken for a real block. Getting this wrong means deleting a line the
    // user wrote.
    let lookalike = "# >>> gcode init >>> (disabled by hand)\n";
    let plan = plan_install(lookalike, &hook_path()).unwrap();
    assert_eq!(plan.outcome, Outcome::Added);
    assert_eq!(count(&plan.content, BEGIN_MARKER), 1, "wrongly detected");
}

#[test]
fn status_reports_missing_without_writing() {
    assert_eq!(status(USER_RC, &hook_path()).unwrap(), Status::Missing);
    assert!(!Status::Missing.installed());
}

#[test]
fn status_reports_the_installed_version() {
    let installed = plan_install(USER_RC, &hook_path()).unwrap().content;
    let found = status(&installed, &hook_path()).unwrap();
    assert_eq!(
        found,
        Status::Installed {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            current: true,
        }
    );
}

#[test]
fn status_distinguishes_an_older_install_from_the_running_one() {
    // `--check` exists so a user can find out they are on a stale hook before
    // blaming something else. Reporting "installed" alone would hide that.
    let stale = format!(
        "# >>> gcode init >>>\n# gcode hook 0.0.1\nsource '{HOOK}'\n# <<< gcode init <<<\n"
    );
    let found = status(&stale, &hook_path()).unwrap();
    assert_eq!(
        found,
        Status::Installed {
            version: "0.0.1".to_owned(),
            current: false,
        }
    );
    assert!(found.installed());
}

#[test]
fn status_says_unknown_rather_than_guessing_a_version() {
    // A block a user wrote by hand has no version line. Reporting a made-up one
    // would be worse than admitting it.
    let handmade = format!("{BEGIN_MARKER}\nsource '{HOOK}'\n{END_MARKER}\n");
    assert_eq!(
        status(&handmade, &hook_path()).unwrap(),
        Status::Installed {
            version: "unknown".to_owned(),
            current: false,
        }
    );
}

#[test]
fn both_shells_round_trip_through_the_same_code() {
    // The transformation is shell-agnostic; only the two names differ. If that
    // ever stops being true, this is the test that says so.
    for (kind, hook) in [(Kind::Bash, HOOK), (Kind::Zsh, ZSH_HOOK)] {
        let installed = plan_install(USER_RC, Path::new(hook)).unwrap();
        assert_eq!(installed.outcome, Outcome::Added, "{kind:?}");
        let removed = plan_remove(&installed.content).unwrap();
        assert_eq!(removed.content, USER_RC, "{kind:?}");
        assert!(status(&installed.content, Path::new(hook))
            .unwrap()
            .installed());
    }
}

#[test]
fn the_kind_parsing_matches_both_shells_and_refuses_the_rest() {
    use std::ffi::OsStr;
    for (value, expected) in [
        ("/bin/bash", Kind::Bash),
        ("/usr/bin/bash", Kind::Bash),
        ("/bin/zsh", Kind::Zsh),
        ("/usr/local/bin/zsh", Kind::Zsh),
        ("bash", Kind::Bash),
    ] {
        assert_eq!(Kind::from_shell_var(OsStr::new(value)).unwrap(), expected);
    }

    for bad in ["/usr/bin/fish", "/bin/sh", "/usr/bin/nushell", "/bin/dash"] {
        let error = Kind::from_shell_var(OsStr::new(bad)).unwrap_err();
        assert!(
            matches!(error, Error::UnsupportedShell { .. }),
            "accepted {bad}"
        );
        // The basename, because that is what the variant carries and what the
        // user needs to see. An error naming only "/usr/bin/fish" when the
        // message says "for /usr/bin/fish" is redundant; naming the command is
        // what saves them a trip to their config.
        let name = bad.rsplit('/').next().unwrap_or(bad);
        assert!(
            error.to_string().contains(name),
            "error hides the shell: {error}"
        );
    }
}

#[test]
fn the_unsupported_shell_error_names_the_way_out() {
    let error = Error::UnsupportedShell {
        shell: "fish".to_owned(),
    };
    assert!(
        error.to_string().contains("--shell"),
        "no workaround offered"
    );
}

#[test]
fn each_hook_carries_the_name_its_shell_expects() {
    assert_eq!(Kind::Bash.rc_file_name(), ".bashrc");
    assert_eq!(Kind::Zsh.rc_file_name(), ".zshrc");
    assert_eq!(Kind::Bash.hook_file_name(), "gcode.bash");
    assert_eq!(Kind::Zsh.hook_file_name(), "gcode.zsh");
}

#[test]
fn the_embedded_hook_is_the_file_in_the_source_tree() {
    // include_str! means a hook change lands in the binary, but only if this test
    // fails when the two drift. It is the only thing tying the embedded copy to
    // the file a user reads.
    assert_eq!(
        Kind::Bash.hook_source(),
        include_str!("../../shell/gcode.bash")
    );
    assert_eq!(
        Kind::Zsh.hook_source(),
        include_str!("../../shell/gcode.zsh")
    );
    assert!(Kind::Bash.hook_source().contains("_gcode_capture"));
    assert!(Kind::Zsh.hook_source().contains("_gcode_capture"));
}

#[test]
fn outcome_reports_whether_it_wrote_anything() {
    assert!(Outcome::Added.changed());
    assert!(Outcome::Updated.changed());
    assert!(Outcome::Removed.changed());
    assert!(!Outcome::Unchanged.changed());
    assert!(!Outcome::NotInstalled.changed());
}

/// Counts lines that are exactly `needle`.
///
/// Substring counting would be wrong: a lookalike comment contains the marker as
/// a prefix, and the point of several tests here is that only an exact line
/// counts.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.lines().filter(|line| *line == needle).count()
}

/// A scratch home for the impure tests. Dropped when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "gcode-init-test-{}-{}-{name}",
            std::process::id(),
            // Distinct per test, since the name alone repeats across processes.
            NEXT.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            }),
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn rc(&self) -> PathBuf {
        self.0.join(".bashrc")
    }

    fn hook(&self) -> PathBuf {
        self.0.join(".gcode").join("shell").join("gcode.bash")
    }

    fn installer(&self) -> Installer {
        Installer::at(Kind::Bash, self.rc(), self.hook())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

thread_local! {
    /// A per-test counter, so two tests in one process cannot share a directory.
    static NEXT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[test]
fn init_writes_the_block_and_the_hook_file() {
    let scratch = Scratch::new("writes");
    fs::write(scratch.rc(), USER_RC).expect("write rc");

    assert_eq!(scratch.installer().init().unwrap().outcome, Outcome::Added);

    let written = fs::read_to_string(scratch.rc()).expect("read back rc");
    assert_eq!(written, installed_rc(&block(&scratch.hook())));
    assert!(
        scratch.hook().exists(),
        "hook file missing; the block would source nothing"
    );
    assert_eq!(
        fs::read_to_string(scratch.hook()).expect("read hook"),
        Kind::Bash.hook_source()
    );
}

#[test]
fn init_creates_the_rc_file_when_there_is_none() {
    let scratch = Scratch::new("creates");
    assert!(!scratch.rc().exists());
    assert_eq!(scratch.installer().init().unwrap().outcome, Outcome::Added);
    assert!(scratch.rc().exists(), "no rc file was created");
}

#[test]
fn init_twice_leaves_the_file_byte_identical() {
    // ADR 0007 validation against a real file, not just the pure function.
    let scratch = Scratch::new("twice");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    let installer = scratch.installer();

    installer.init().unwrap();
    let after_first = fs::read_to_string(scratch.rc()).expect("read back");
    assert_eq!(installer.init().unwrap().outcome, Outcome::Unchanged);
    assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), after_first);
}

#[test]
fn remove_after_init_restores_the_file_byte_for_byte() {
    // ADR 0007 validation against a real file.
    let scratch = Scratch::new("roundtrip");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    let installer = scratch.installer();

    installer.init().unwrap();
    assert_eq!(installer.remove().unwrap().outcome, Outcome::Removed);
    assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), USER_RC);
    assert!(!scratch.hook().exists(), "hook file left behind");
}

#[test]
fn remove_when_nothing_is_installed_is_a_no_op() {
    let scratch = Scratch::new("noinstall");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    assert_eq!(
        scratch.installer().remove().unwrap().outcome,
        Outcome::NotInstalled
    );
    assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), USER_RC);
}

#[test]
fn check_reports_missing_and_installed_without_writing() {
    let scratch = Scratch::new("check");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    let installer = scratch.installer();

    assert_eq!(installer.check().unwrap(), Status::Missing);
    installer.init().unwrap();
    let found = installer.check().unwrap();
    assert!(found.installed());
    assert_eq!(
        found,
        Status::Installed {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            current: true,
        }
    );
}

#[test]
fn check_does_not_create_the_rc_file() {
    // "Reports without changing anything" includes not creating files. A check
    // that left a .bashrc behind would be a change.
    let scratch = Scratch::new("checknone");
    assert_eq!(scratch.installer().check().unwrap(), Status::Missing);
    assert!(!scratch.rc().exists(), "check created an rc file");
}

#[test]
fn init_refuses_an_unreadable_rc_file_instead_of_truncating_it() {
    // The failure this prevents is the worst one in this module: a user's whole
    // shell configuration deleted, followed by a success message.
    let scratch = Scratch::new("unreadable");
    fs::write(scratch.rc(), USER_RC).expect("write rc");

    set_mode(&scratch.rc(), 0o000);

    let result = scratch.installer().init();

    // Restore readability before asserting on the contents: the check that the
    // file is untouched has to be able to read it.
    set_mode(&scratch.rc(), 0o600);

    #[cfg(unix)]
    if !is_root() {
        assert!(
            matches!(result, Err(Error::ShellRcUnreadable { .. })),
            "wrote to an unreadable rc file: {result:?}"
        );
        assert_eq!(
            fs::read_to_string(scratch.rc()).unwrap(),
            USER_RC,
            "the file was modified"
        );
    }
    #[cfg(not(unix))]
    let _ = result;
}

#[test]
fn check_also_refuses_an_unreadable_rc_file() {
    // A check that reported "missing" for a file it could not read would be a
    // lie, and would send the user to install over a config it never saw.
    let scratch = Scratch::new("checkunreadable");
    fs::write(scratch.rc(), USER_RC).expect("write rc");

    set_mode(&scratch.rc(), 0o000);

    #[cfg(unix)]
    if !is_root() {
        assert!(matches!(
            scratch.installer().check(),
            Err(Error::ShellRcUnreadable { .. })
        ));
    }

    set_mode(&scratch.rc(), 0o600);
}

#[test]
fn init_preserves_the_rc_files_existing_mode() {
    // fs::write truncates in place. Tightening it would be gcode changing
    // something the user chose; loosening it would be a security regression.
    let scratch = Scratch::new("mode");
    fs::write(scratch.rc(), USER_RC).expect("write rc");

    set_mode(&scratch.rc(), 0o600);
    scratch.installer().init().unwrap();
    assert_eq!(mode_of(&scratch.rc()), 0o600, "mode changed on install");
}

/// Sets a file's permission bits, or does nothing on a platform without them.
fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// The file's permission bits, or `u32::MAX` where the platform has none.
fn mode_of(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).expect("stat").permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        u32::MAX
    }
}

#[test]
fn the_hook_file_is_written_where_the_block_points() {
    // Two paths that must agree. When they drift, the block sources nothing and
    // the prompt silently stops recording.
    let scratch = Scratch::new("agree");
    let installer = scratch.installer();
    installer.init().unwrap();
    let rc = fs::read_to_string(scratch.rc()).expect("read rc");
    assert!(
        rc.contains(&scratch.hook().display().to_string()),
        "block does not reference the hook that was written: {rc}"
    );
}

#[test]
fn the_installer_resolves_paths_from_the_home_directory() {
    use crate::utils::paths::{Os, Overrides};
    let overrides = Overrides {
        home: Some(PathBuf::from("/home/user")),
        os: Os::Linux,
        ..Overrides::default()
    };
    let installer = Installer::resolve(Kind::Bash, &overrides).unwrap();
    assert_eq!(installer.rc_path, Path::new("/home/user/.bashrc"));
    assert_eq!(
        installer.hook_path,
        Path::new("/home/user/.gcode/shell/gcode.bash")
    );
}

#[test]
fn an_unknown_home_directory_is_an_error_not_a_panic() {
    use crate::utils::paths::Overrides;
    for kind in Kind::ALL {
        assert!(matches!(
            Installer::resolve(kind, &Overrides::default()),
            Err(Error::HomeDirUnavailable)
        ));
    }
}

#[test]
fn remove_is_safe_to_run_twice() {
    let scratch = Scratch::new("removetwice");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    let installer = scratch.installer();
    installer.init().unwrap();

    assert_eq!(installer.remove().unwrap().outcome, Outcome::Removed);
    assert_eq!(installer.remove().unwrap().outcome, Outcome::NotInstalled);
    assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), USER_RC);
}

/// True when the test is running as uid 0, which can read a `0000` file.
#[cfg(unix)]
fn is_root() -> bool {
    // std has no uid accessor, and adding a libc dependency to ask one question
    // about test setup is not a trade worth making. The euid probe is the
    // cheapest thing that answers it without a new crate.
    std::fs::read_to_string("/proc/self/status").is_ok_and(|status| status.contains("Uid:\t0\t"))
}

#[test]
fn a_malformed_block_leaves_the_file_untouched_on_disk() {
    // The impure half must not write when the pure half refuses, or the refusal
    // only protects half the cases.
    let scratch = Scratch::new("malformed");
    let broken = format!("alias x=1\n{BEGIN_MARKER}\nsource '{HOOK}'\n");
    fs::write(scratch.rc(), &broken).expect("write rc");

    assert!(matches!(
        scratch.installer().init(),
        Err(Error::ShellBlockMalformed)
    ));
    assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), broken);
}

#[test]
fn install_then_remove_is_stable_across_repeated_cycles() {
    // Byte precision has to hold for the second cycle, not only the first: a
    // second install sees a file that install itself produced.
    let scratch = Scratch::new("cycles");
    fs::write(scratch.rc(), USER_RC).expect("write rc");
    let installer = scratch.installer();
    for _ in 0..3 {
        installer.init().unwrap();
        installer.remove().unwrap();
        assert_eq!(fs::read_to_string(scratch.rc()).unwrap(), USER_RC);
    }
}

#[test]
fn a_plan_reports_the_lines_it_will_change() {
    // "Prints the exact lines it added" is only checkable if the plan carries
    // them rather than the caller re-deriving them.
    let plan: Plan = plan_install(USER_RC, &hook_path()).unwrap();
    assert_eq!(
        changed_lines(&plan),
        [
            BEGIN_MARKER,
            format!("# gcode hook {}", env!("CARGO_PKG_VERSION")).as_str(),
            format!("source '{HOOK}'").as_str(),
            END_MARKER,
        ]
    );
}

#[test]
fn install_into_a_file_whose_last_line_is_a_partial_quote_still_works() {
    // The rc files people actually have. A block appended to this must not be
    // swallowed by an unterminated quote on the line above it.
    let awkward = "export PROMPT='\nalias x=1\n";
    let installed = plan_install(awkward, &hook_path()).unwrap();
    assert!(installed.content.contains(BEGIN_MARKER));
    let removed = plan_remove(&installed.content).unwrap();
    assert_eq!(removed.content, awkward);
}
