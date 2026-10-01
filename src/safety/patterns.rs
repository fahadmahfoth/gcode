//! The pattern table and the matcher that reads it.
//!
//! # Why not a regex table
//!
//! SAFETY.md sketches the table as `(regex, level, reason)`. This module is
//! data in the same sense, expressed as ordered token matchers instead. Three
//! reasons:
//!
//! 1. **No dependency in the module that must never be wrong.** `regex` pulls
//!    nine crates into a safety-critical path. This matcher is small enough to
//!    read in one sitting.
//! 2. **Near misses are structural, not accidental.** `rm -rf ./build` fails
//!    the root-delete rule because `./build` is not the token `/`, not because a
//!    regex author forgot to anchor.
//! 3. **Patterns stay data.** Adding a risk is a new row and a new test.
//!
//! # The matching model
//!
//! A segment is split into tokens (whitespace-separated, quotes kept). A
//! [`Matcher`] is an ordered list of [`Elem`]s that must all be found, in order,
//! somewhere in the token stream — gaps allowed. A [`Pattern`] matches when *any*
//! of its matchers does.
//!
//! Two deliberate choices:
//!
//! * **Command names are anchored to the head token.** [`Elem::Head`] only looks
//!   at the token that actually invokes the program, after wrappers and
//!   `VAR=value` prefixes. Without this, `echo rm -rf /` matches the root-delete
//!   rule, because the substring `rm` is somewhere in the segment.
//! * **Subsequence matching, not adjacency.** Being stricter means more false
//!   negatives, and a false negative here is a command classified one level too
//!   low.

/// One condition inside a matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elem {
    /// The command the segment actually invokes is one of these.
    ///
    /// This is the only element that names a program, and it never matches an
    /// argument. `rmdir` does not match `Head(["rm"])`.
    Head(&'static [&'static str]),
    /// A token equal to any one of these words, anywhere.
    Any(&'static [&'static str]),
    /// A token equal to any one of these words, ignoring case. For SQL keywords,
    /// which are upper-case and usually quoted into one token.
    WordCaseInsensitive(&'static [&'static str]),
    /// A token *starting* with any one of these. Needed for device names, where
    /// `/dev/sda1` and `/dev/sdb` are both dangerous and neither equals
    /// `/dev/sd`.
    AnyPrefix(&'static [&'static str]),
    /// The head command starts with one of these prefixes. `["mkfs"]` matches
    /// `mkfs`, `mkfs.ext4`, and `mke2fs`, but not `echo mkfs.ext4`.
    HeadPrefix(&'static [&'static str]),
    /// An option cluster containing every one of these letters, in any order, at
    /// any case. `Flags("rf")` matches `-rf`, `-fr`, and `-Rf`.
    ///
    /// Letters may be spread across several consecutive option tokens, so
    /// `rm -r -f x` matches. Never matches a token that is not an option.
    Flags(&'static str),
    /// Any option token at all.
    AnyFlags,
    /// A literal substring anywhere in the segment, including inside quotes.
    Text(&'static str),
    /// The same, after every whitespace character is removed.
    ///
    /// For shell constructs whose exact spelling varies, like
    /// `:(){ :|:& };:` against `:(){:|:&};:`. Whitespace inside a brace group is
    /// not load-bearing to the shell.
    Compact(&'static str),
    /// The filesystem root: `/`, `/*`, `/.`, or `//`. Not `./`, not `/tmp`.
    RootPath,
    /// The home directory itself: `~`, `~/`, `~/..`, `$HOME`, `$HOME/`. Not
    /// `~/Documents`.
    HomeRoot,
    /// Any path at or under the home directory: `~`, `~/x`, `$HOME/x`.
    HomePath,
    /// Any token that looks like a path, i.e. contains a `/`.
    AnyPath,
    /// The token is a `dd of=` target on a real block device, i.e. under
    /// `/dev/` but not the null device.
    DdDevice,
    /// The token does not mention `/dev/null`.
    NotNull,
    /// The nested element must be found *anywhere* in the token stream, without
    /// consuming a token in sequence. Needed for `find / -name x`, where the
    /// root is not adjacent to the head command.
    Contains(&'static Elem),
}

const ROOT: Elem = Elem::RootPath;
/// `Contains(&RootPath)`, for the recursive-walk rules.
const CONTAINS_ROOT: Elem = Elem::Contains(&ROOT);

/// An ordered set of conditions that must all hold.
#[derive(Debug, Clone, Copy)]
pub struct Matcher {
    /// The conditions, in order.
    pub elems: &'static [Elem],
}

impl Matcher {
    /// Builds a matcher from its parts.
    #[must_use]
    pub const fn new(elems: &'static [Elem]) -> Self {
        Self { elems }
    }
}

/// One row: an identifier, a level, and a sentence for the user.
#[derive(Debug, Clone, Copy)]
pub struct Pattern {
    /// Stable identifier, used in tests, docs, and `--explain` output.
    pub id: &'static str,
    /// The level this row contributes.
    pub level: Risk,
    /// What the user is told when it fires. Says what the command does, never
    /// just "dangerous".
    pub reason: &'static str,
    /// Any one of these matching is enough.
    pub matchers: &'static [Matcher],
}

use super::Risk;

// ── Vocabulary shared with the structural checks ───────────────────────────

/// Wrappers that run a command without being it.
pub const WRAPPERS: &[&str] = &["env", "command", "nohup", "time", "exec", "stdbuf", "nice"];
/// Privileged tools, matched in any position: `sudo` can sit mid-pipeline.
pub const PRIVILEGE_ANY: &[&str] = &["sudo", "doas", "pkexec", "runuser"];
/// Privileged tools, matched only as the head command. `su` appears inside words
/// often enough that any-position matching over-fires on `grep su notes.txt`.
pub const PRIVILEGE_HEAD: &[&str] = &["su"];
/// Commands that read from the network.
pub const FETCH: &[&str] = &["curl", "wget", "fetch", "aria2c", "http", "httpie"];
/// Commands that run code the user did not write.
pub const EXECUTE: &[&str] = &[
    "sh", "bash", "zsh", "ksh", "dash", "fish", "csh", "tcsh", "python", "python3", "node", "perl",
    "ruby", "php", "java", "source", "eval", "exec",
];

// ── The blocklist ───────────────────────────────────────────────────────────
//
// Evaluated before level assignment. A hit is CRITICAL and stops everything
// (ADR 0008). Kept short on purpose: SAFETY.md is explicit that a long
// blocklist is a false sense of security.

/// Raw block devices. `/dev/null`, `/dev/zero`, `/dev/std*`, and the tty devices
/// are excluded on purpose — writing to them is ordinary and harmless, and a
/// blocklist that fires on `echo x > /dev/null` would be one people turn off.
const DEVICES: &[&str] = &[
    "/dev/sd",
    "/dev/nvme",
    "/dev/hd",
    "/dev/vd",
    "/dev/xvd",
    "/dev/disk",
    "/dev/rdisk",
    "/dev/loop",
    "/dev/ram",
    "/dev/mapper",
];

/// Blocklist rows that must see the **whole command** rather than one segment.
///
/// A fork bomb is written `:(){ :|:& };:`. Bash lexes the trailing `;` as a
/// separator, so a per-segment pass sees `:(){ :|:& }` and `:`, and no single
/// segment contains the whole construct. Per-segment rows would therefore miss
/// it, which is the one blocklist row that must never be missed.
pub static COMMAND_SCOPE: &[Pattern] = &[Pattern {
    id: "block.forkbomb",
    level: Risk::Critical,
    reason: "fork bomb, which hangs the machine",
    matchers: &[Matcher::new(&[
        Elem::Compact(":(){"),
        Elem::Compact(":|:&};:"),
    ])],
}];

/// Unoverridable destructive commands. Short, and it does not grow on request.
pub static BLOCKLIST: &[Pattern] = &[
    Pattern {
        id: "block.rm.root",
        level: Risk::Critical,
        reason: "recursively deletes the filesystem root",
        matchers: &[
            Matcher::new(&[Elem::Head(&["rm"]), Elem::Flags("rf"), Elem::RootPath]),
            Matcher::new(&[
                Elem::Head(&["rm"]),
                Elem::Flags("rf"),
                Elem::Text("--no-preserve-root"),
                Elem::RootPath,
            ]),
        ],
    },
    Pattern {
        id: "block.rm.home",
        level: Risk::Critical,
        reason: "recursively deletes your home directory",
        matchers: &[
            Matcher::new(&[Elem::Head(&["rm"]), Elem::Flags("rf"), Elem::HomeRoot]),
            Matcher::new(&[
                Elem::Head(&["rm"]),
                Elem::Flags("rf"),
                Elem::Any(&["~/*", "$HOME/*", "~/."]),
            ]),
        ],
    },
    Pattern {
        id: "block.mkfs",
        level: Risk::Critical,
        reason: "formats a filesystem, erasing everything on it",
        matchers: &[Matcher::new(&[Elem::HeadPrefix(&[
            "mkfs", "mke2fs", "newfs",
        ])])],
    },
    Pattern {
        id: "block.dd.device",
        level: Risk::Critical,
        reason: "overwrites a raw block device",
        matchers: &[Matcher::new(&[Elem::Head(&["dd"]), Elem::DdDevice])],
    },
    Pattern {
        id: "block.redirect.device",
        level: Risk::Critical,
        reason: "redirects output into a raw block device",
        matchers: &[Matcher::new(&[
            Elem::Any(&[">", ">>"]),
            Elem::AnyPrefix(DEVICES),
        ])],
    },
    Pattern {
        id: "block.chmod.777.root",
        level: Risk::Critical,
        reason: "makes the whole filesystem world-writable",
        matchers: &[Matcher::new(&[
            Elem::Head(&["chmod"]),
            Elem::Flags("r"),
            Elem::Any(&["777"]),
            Elem::RootPath,
        ])],
    },
    Pattern {
        id: "block.write.device",
        level: Risk::Critical,
        reason: "writes directly to a raw block device",
        matchers: &[Matcher::new(&[
            Elem::Head(&["tee", "cat"]),
            Elem::AnyPrefix(DEVICES),
        ])],
    },
];

// ── The pattern table ───────────────────────────────────────────────────────

/// Commands ranked below the floor unless a pattern raises them.
pub static PATTERNS: &[Pattern] = &[
    // ── HIGH ───────────────────────────────────────────────────────────────
    Pattern {
        id: "high.rm.recursive",
        level: Risk::High,
        reason: "recursively deletes files, which cannot be undone",
        matchers: &[Matcher::new(&[Elem::Head(&["rm"]), Elem::Flags("r")])],
    },
    Pattern {
        id: "high.rm.force",
        level: Risk::High,
        reason: "deletes files without asking for confirmation",
        matchers: &[Matcher::new(&[Elem::Head(&["rm"]), Elem::Flags("f")])],
    },
    Pattern {
        id: "high.chown.recursive",
        level: Risk::High,
        reason: "changes ownership of a whole tree",
        matchers: &[Matcher::new(&[
            Elem::Head(&["chown", "chgrp"]),
            Elem::Flags("r"),
        ])],
    },
    Pattern {
        id: "high.permissions.broad",
        level: Risk::High,
        reason: "changes permissions recursively",
        matchers: &[Matcher::new(&[
            Elem::Head(&["chmod", "chown", "chgrp"]),
            Elem::Flags("r"),
        ])],
    },
    Pattern {
        id: "high.firewall.flush",
        level: Risk::High,
        reason: "flushes every firewall rule, removing all filtering",
        matchers: &[
            Matcher::new(&[
                Elem::Head(&["iptables", "ip6tables", "nft"]),
                Elem::Flags("f"),
            ]),
            Matcher::new(&[Elem::Head(&["ufw", "firewall-cmd"]), Elem::Any(&["flush"])]),
            Matcher::new(&[Elem::Head(&["nft"]), Elem::Any(&["flush", "destroy"])]),
            Matcher::new(&[Elem::Head(&["ufw"]), Elem::Flags("f")]),
        ],
    },
    Pattern {
        id: "high.power",
        level: Risk::High,
        reason: "changes the machine's power state, cutting off other users",
        matchers: &[Matcher::new(&[Elem::Head(&[
            "shutdown", "reboot", "halt", "poweroff",
        ])])],
    },
    Pattern {
        id: "high.git.forcepush",
        level: Risk::High,
        reason: "force-pushes, which can destroy other people's history",
        matchers: &[Matcher::new(&[
            Elem::Head(&["git"]),
            Elem::Any(&["push"]),
            Elem::Flags("f"),
        ])],
    },
    Pattern {
        id: "high.git.clean",
        level: Risk::High,
        reason: "removes untracked files git is protecting you from",
        matchers: &[Matcher::new(&[
            Elem::Head(&["git"]),
            Elem::Any(&["clean"]),
            Elem::AnyFlags,
        ])],
    },
    Pattern {
        id: "high.docker.prune",
        level: Risk::High,
        reason: "removes containers, images, or volumes in bulk",
        matchers: &[Matcher::new(&[
            Elem::Head(&["docker", "podman"]),
            Elem::Any(&["system", "image", "container", "volume", "network"]),
            Elem::Any(&["prune"]),
        ])],
    },
    Pattern {
        id: "high.kill.init",
        level: Risk::High,
        reason: "kills process 1, which stops the whole system",
        matchers: &[Matcher::new(&[
            Elem::Head(&["kill"]),
            Elem::Flags("9"),
            Elem::Any(&["1"]),
        ])],
    },
    Pattern {
        id: "high.chmod.777",
        level: Risk::High,
        reason: "makes a path world-writable",
        matchers: &[Matcher::new(&[Elem::Head(&["chmod"]), Elem::Any(&["777"])])],
    },
    Pattern {
        id: "high.mount.system",
        level: Risk::High,
        reason: "changes what filesystems are mounted",
        matchers: &[Matcher::new(&[Elem::Head(&["mount", "umount"])])],
    },
    // ── MEDIUM ─────────────────────────────────────────────────────────────
    Pattern {
        id: "medium.fetch",
        level: Risk::Medium,
        reason: "fetches data from the network",
        matchers: &[Matcher::new(&[Elem::Head(FETCH)])],
    },
    Pattern {
        id: "medium.exec",
        level: Risk::Medium,
        reason: "runs a program, so it does whatever that program does",
        matchers: &[Matcher::new(&[Elem::Head(EXECUTE)])],
    },
    Pattern {
        id: "medium.install",
        level: Risk::Medium,
        reason: "installs packages, which can run install scripts",
        matchers: &[Matcher::new(&[
            Elem::Head(&[
                "npm", "pnpm", "yarn", "pip", "pip3", "gem", "cargo", "go", "composer",
            ]),
            Elem::Any(&["install"]),
        ])],
    },
    Pattern {
        id: "medium.history.clear",
        level: Risk::Medium,
        reason: "clears your shell history",
        matchers: &[Matcher::new(&[Elem::Head(&["history"]), Elem::Flags("c")])],
    },
    Pattern {
        id: "medium.walk.root",
        level: Risk::Medium,
        reason: "walks the filesystem from the root, which is slow and noisy",
        matchers: &[Matcher::new(&[
            Elem::Head(&["find", "du", "grep"]),
            CONTAINS_ROOT,
        ])],
    },
    Pattern {
        id: "medium.packages",
        level: Risk::Medium,
        reason: "installs or removes system packages",
        matchers: &[Matcher::new(&[
            Elem::Head(&[
                "apt", "apt-get", "yum", "dnf", "apk", "brew", "pacman", "port",
            ]),
            Elem::Any(&["install", "remove", "purge", "uninstall", "erase"]),
        ])],
    },
    Pattern {
        id: "medium.service",
        level: Risk::Medium,
        reason: "stops or restarts a system service",
        matchers: &[
            Matcher::new(&[
                Elem::Head(&["systemctl"]),
                Elem::Any(&["stop", "restart", "reload", "disable"]),
            ]),
            Matcher::new(&[Elem::Head(&["service"]), Elem::Any(&["stop", "restart"])]),
        ],
    },
    Pattern {
        id: "medium.destroy.contents",
        level: Risk::Medium,
        reason: "destroys the contents of a file",
        matchers: &[Matcher::new(&[Elem::Head(&[
            "shred", "truncate", "unlink",
        ])])],
    },
    Pattern {
        id: "medium.permissions",
        level: Risk::Medium,
        reason: "changes permissions or ownership",
        matchers: &[Matcher::new(&[Elem::Head(&["chmod", "chown", "chgrp"])])],
    },
    Pattern {
        id: "medium.git.destructive",
        level: Risk::Medium,
        reason: "discards uncommitted work",
        matchers: &[
            Matcher::new(&[
                Elem::Head(&["git"]),
                Elem::Any(&["reset", "checkout"]),
                Elem::AnyFlags,
            ]),
            // `git restore <path>` always discards, flagged or not.
            Matcher::new(&[Elem::Head(&["git"]), Elem::Any(&["restore"])]),
        ],
    },
    Pattern {
        id: "medium.write.system",
        level: Risk::Medium,
        reason: "writes into a system path",
        matchers: &[
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/etc/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/usr/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/bin/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/sbin/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/boot/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/sys/")]),
            Matcher::new(&[Elem::Head(WRITERS), Elem::Text("/Library/")]),
        ],
    },
    Pattern {
        id: "medium.schedule",
        level: Risk::Medium,
        reason: "changes scheduled jobs, which run without you",
        matchers: &[Matcher::new(&[Elem::Head(&["crontab", "at", "batch"])])],
    },
    Pattern {
        id: "medium.kill",
        level: Risk::Medium,
        reason: "terminates a process",
        matchers: &[
            Matcher::new(&[Elem::Head(&["kill"])]),
            Matcher::new(&[Elem::Head(&["pkill", "killall"])]),
        ],
    },
    Pattern {
        id: "medium.database",
        level: Risk::Medium,
        reason: "changes database contents",
        matchers: &[Matcher::new(&[
            Elem::Head(&["psql", "mysql", "sqlite3"]),
            // The verb is usually quoted, and SQL keywords are upper-case, so
            // the token is lower-cased before the comparison.
            Elem::WordCaseInsensitive(&["drop", "truncate", "delete", "update", "alter"]),
        ])],
    },
    Pattern {
        id: "medium.user.account",
        level: Risk::Medium,
        reason: "changes user accounts or groups",
        matchers: &[Matcher::new(&[Elem::Head(&[
            "useradd", "usermod", "groupadd", "userdel", "groupdel", "passwd", "dscl",
        ])])],
    },
    // ── LOW ────────────────────────────────────────────────────────────────
    Pattern {
        id: "low.rm.single",
        level: Risk::Low,
        reason: "deletes a file",
        matchers: &[Matcher::new(&[Elem::Head(&["rm"]), Elem::AnyPath])],
    },
    Pattern {
        id: "low.rm.bare",
        level: Risk::Low,
        reason: "deletes files",
        matchers: &[Matcher::new(&[Elem::Head(&["rm"])])],
    },
    Pattern {
        id: "low.mv",
        level: Risk::Low,
        reason: "moves or renames a path",
        matchers: &[Matcher::new(&[Elem::Head(&["mv", "cp", "rsync"])])],
    },
    Pattern {
        id: "low.mkdir",
        level: Risk::Low,
        reason: "creates a directory or an empty file",
        matchers: &[Matcher::new(&[Elem::Head(&["mkdir", "touch", "ln"])])],
    },
];

/// Commands that create or replace a file at a path.
pub const WRITERS: &[&str] = &["cp", "mv", "install", "ln", "tee", "dd", "rsync"];

/// Head commands that are read-only and bounded. Anything not in this list and
/// not raised by a pattern gets [`Risk::Low`] rather than [`Risk::Safe`].
///
/// This is the floor that makes the table safe to extend: a new destructive
/// binary that nobody has written a pattern for yet cannot be classified SAFE
/// just because it is unknown. It costs a confirmation prompt, which is cheap.
/// It is an addition beyond SAFETY.md's table, and the reason is that a table
/// which calls unknown commands SAFE fails open, which is the wrong direction
/// for the one module whose whole job is not failing open.
pub static KNOWN_SAFE: &[&str] = &[
    "ls",
    "cat",
    "pwd",
    "echo",
    "printf",
    "which",
    "whoami",
    "id",
    "uname",
    "hostname",
    "uptime",
    "date",
    "df",
    "du",
    "stat",
    "file",
    "tree",
    "lsblk",
    "free",
    "ps",
    "top",
    "man",
    "wc",
    "head",
    "tail",
    "sort",
    "uniq",
    "cut",
    "tr",
    "tee",
    "diff",
    "basename",
    "dirname",
    "realpath",
    "md5sum",
    "sha256sum",
    "seq",
    "true",
    "false",
    "test",
    "env",
    "printenv",
    "jq",
    "awk",
    "sed",
    "grep",
    "rg",
    "find",
    "xargs",
    "yes",
    "sleep",
    "less",
    "more",
    "tput",
    "command",
    "type",
    "alias",
    "hash",
    "jobs",
    "fg",
    "bg",
    "nohup",
    "git",
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "pip",
    "python3",
    "node",
    "rustc",
    "go",
    "make",
    "lsattr",
    "getfacl",
    "locale",
    "tty",
    "who",
    "w",
    "last",
    "dmesg",
    "lscpu",
    "lspci",
    "lsusb",
    "rev",
    "file",
];

// ── Tokenising ──────────────────────────────────────────────────────────────

/// Splits a segment into tokens, keeping quoted regions together.
///
/// Byte-wise on purpose: the only bytes this inspects are ASCII quotes and ASCII
/// whitespace, and every byte of a multi-byte UTF-8 sequence is `>= 0x80`, so the
/// slice boundaries can only ever land on a character boundary.
pub fn tokenize(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'\'' || c == b'"' {
                    quote = Some(c);
                } else if c.is_ascii_whitespace() {
                    if start < i {
                        out.push(&s[start..i]);
                    }
                    start = i + 1;
                }
            }
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push(&s[start..]);
    }
    out
}

/// Index of the token that actually invokes the program.
///
/// Skips leading options, `VAR=value` prefixes, and wrappers such as `env`, then
/// reduces a path to its base name so `/bin/ls` is `ls`.
#[must_use]
pub fn head_token_index(toks: &[&str]) -> Option<usize> {
    for (i, t) in toks.iter().enumerate() {
        if t.starts_with('-') && !t.contains('=') {
            continue;
        }
        if t.contains('=') && !t.starts_with('=') {
            let name = t.split('=').next().unwrap_or(t);
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
        }
        let base = t.rsplit('/').next().unwrap_or(t);
        if base.is_empty() {
            continue;
        }
        // `sudo`, `env`, and friends are not the command; skip them so
        // `sudo ufw flush` heads at `ufw` and `sudo rm -rf /` heads at `rm`.
        if WRAPPERS.contains(&base) || PRIVILEGE_ANY.contains(&base) {
            continue;
        }
        return Some(i);
    }
    None
}

/// The command the segment actually invokes.
#[must_use]
pub fn head_command(toks: &[&str]) -> Option<String> {
    head_token_index(toks).map(|i| toks[i].rsplit('/').next().unwrap_or(toks[i]).to_owned())
}

// ── Matching ────────────────────────────────────────────────────────────────

/// True when `token` is exactly the empty device string.
fn is_not_null(token: &str) -> bool {
    !token.contains("/dev/null")
}

/// True when `token` satisfies `elem`, given the whole segment for substring
/// elements.
fn token_matches(elem: &Elem, token: &str, whole: &str) -> bool {
    match elem {
        Elem::Head(words) => {
            let base = token.rsplit('/').next().unwrap_or(token);
            words.contains(&base)
        }
        Elem::Any(words) => words.contains(&token),
        Elem::WordCaseInsensitive(words) => {
            // Shell quoting survives tokenising, so `'DROP TABLE users'` is one
            // token. Compare every word inside it, unquoted and lower-cased.
            let bare = token
                .trim_matches(|c| c == '\'' || c == '"')
                .to_ascii_lowercase();
            bare.split_whitespace().any(|w| words.contains(&w))
        }
        Elem::AnyPrefix(prefixes) => prefixes.iter().any(|p| token.starts_with(p)),
        Elem::HeadPrefix(prefixes) => prefixes.iter().any(|p| token.starts_with(p)),
        Elem::Flags(letters) => {
            token.starts_with('-')
                && letters
                    .chars()
                    .all(|c| token.to_ascii_lowercase().contains(c))
        }
        Elem::AnyFlags => token.starts_with('-') && token.len() > 1,
        Elem::Text(t) => whole.contains(t),
        Elem::Compact(t) => {
            let squashed: String = whole.chars().filter(|c| !c.is_whitespace()).collect();
            squashed.contains(t)
        }
        Elem::RootPath => matches!(token, "/" | "/*" | "/." | "//"),
        Elem::HomeRoot => matches!(token, "~" | "~/" | "~/.." | "$HOME" | "$HOME/" | "$HOME/.."),
        Elem::HomePath => {
            token == "~"
                || token.starts_with("~/")
                || token == "$HOME"
                || token.starts_with("$HOME/")
        }
        Elem::AnyPath => token.contains('/'),
        Elem::DdDevice => token.starts_with("of=/dev/") && is_not_null(token),
        Elem::NotNull => is_not_null(token),
        // Handled by the caller, which scans the whole stream. Answering against
        // the single token keeps this total rather than panicking on an
        // invariant the caller is responsible for.
        Elem::Contains(inner) => token_matches(inner, token, token),
    }
}

/// True when `elems` are all present, in order, in `toks`.
fn matcher_matches(elems: &[Elem], toks: &[&str], whole: &str) -> bool {
    let head = head_token_index(toks);
    let mut ti = 0usize;
    for elem in elems {
        if let Elem::Contains(inner) = elem {
            if !toks.iter().any(|t| token_matches(inner, t, whole)) {
                return false;
            }
            continue;
        }
        // Substring elements look at the whole segment, so they do not consume a
        // token. Treating them as consuming meant a segment with one token could
        // satisfy at most one of them, which silently broke `psql -c 'DROP TABLE
        // users'` and every multi-element literal matcher.
        if matches!(elem, Elem::Text(_) | Elem::Compact(_) | Elem::NotNull) {
            if !token_matches(elem, "", whole) {
                return false;
            }
            continue;
        }
        if matches!(elem, Elem::Head(_) | Elem::HeadPrefix(_)) {
            let Some(idx) = head else { return false };
            if !token_matches(elem, toks[idx], whole) || idx < ti {
                return false;
            }
            ti = idx + 1;
            continue;
        }
        // `Flags` may spread across consecutive option tokens, so it consumes a
        // run rather than a single token.
        if let Elem::Flags(letters) = elem {
            let mut run = String::new();
            let mut j = ti;
            while j < toks.len() && toks[j].starts_with('-') && toks[j].len() > 1 {
                run.push_str(&toks[j].to_ascii_lowercase());
                j += 1;
            }
            if letters.chars().all(|c| run.contains(c)) && j > ti {
                ti = j;
                continue;
            }
            return false;
        }
        let mut found = false;
        while ti < toks.len() {
            if token_matches(elem, toks[ti], whole) {
                found = true;
                ti += 1;
                break;
            }
            ti += 1;
        }
        if !found {
            return false;
        }
    }
    true
}

/// True when `segment` fires any of `pattern`'s matchers.
#[must_use]
pub fn pattern_matches(pattern: &Pattern, segment: &str) -> bool {
    let toks = tokenize(segment);
    pattern
        .matchers
        .iter()
        .any(|m| matcher_matches(m.elems, &toks, segment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::Risk;

    fn pattern(id: &str) -> &'static Pattern {
        PATTERNS
            .iter()
            .chain(BLOCKLIST.iter())
            .chain(COMMAND_SCOPE.iter())
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("no pattern {id}"))
    }

    fn fires(id: &str, segment: &str) -> bool {
        pattern_matches(pattern(id), segment)
    }

    // ── the matcher itself ────────────────────────────────────────────────

    #[test]
    fn a_quoted_token_stays_one_token() {
        assert_eq!(
            tokenize("grep 'a b c' file"),
            vec!["grep", "'a b c'", "file"]
        );
        assert_eq!(tokenize("echo \"x y\""), vec!["echo", "\"x y\""]);
    }

    #[test]
    fn tokenizing_utf8_does_not_split_a_character() {
        let toks = tokenize("lsملف عربي rm");
        assert_eq!(toks.len(), 3, "{toks:?}");
        assert_eq!(toks[0], "lsملف");
    }

    #[test]
    fn the_head_command_skips_options_assignments_and_wrappers() {
        assert_eq!(head_command(&tokenize("ls")).as_deref(), Some("ls"));
        assert_eq!(head_command(&tokenize("-la")).as_deref(), None);
        assert_eq!(head_command(&tokenize("VAR=1 ls")).as_deref(), Some("ls"));
        assert_eq!(
            head_command(&tokenize("env FOO=1 ls")).as_deref(),
            Some("ls")
        );
        assert_eq!(
            head_command(&tokenize("./bin/tool")).as_deref(),
            Some("tool")
        );
        assert_eq!(
            head_command(&tokenize("sudo rm -rf x")).as_deref(),
            Some("rm")
        );
    }

    #[test]
    fn a_head_element_never_matches_an_argument() {
        const EL: [Elem; 1] = [Elem::Head(&["rm"])];
        const M: [Matcher; 1] = [Matcher::new(&EL)];
        let seg = "echo rm";
        assert!(!matcher_matches(M[0].elems, &tokenize(seg), seg));
        let seg = "rm x";
        assert!(matcher_matches(M[0].elems, &tokenize(seg), seg));
        let seg = "rmdir x";
        assert!(!matcher_matches(M[0].elems, &tokenize(seg), seg));
        let seg = "sudo rm x";
        assert!(matcher_matches(M[0].elems, &tokenize(seg), seg));
    }

    #[test]
    fn flags_spread_across_several_option_tokens() {
        const EL: [Elem; 2] = [Elem::Head(&["rm"]), Elem::Flags("rf")];
        const M: [Matcher; 1] = [Matcher::new(&EL)];
        for yes in [
            "rm -rf x",
            "rm -fr x",
            "rm -Rf x",
            "rm -r -f x",
            "rm -rfv x",
        ] {
            assert!(matcher_matches(M[0].elems, &tokenize(yes), yes), "{yes}");
        }
        for no in ["rm -r x", "rm -f x", "rm x", "rm rf x"] {
            assert!(!matcher_matches(M[0].elems, &tokenize(no), no), "{no}");
        }
    }

    #[test]
    fn a_flag_never_matches_a_non_option_token() {
        const EL: [Elem; 2] = [Elem::Head(&["grep"]), Elem::Flags("r")];
        const M: [Matcher; 1] = [Matcher::new(&EL)];
        assert!(!matcher_matches(
            M[0].elems,
            &tokenize("grep rf foo"),
            "grep rf foo"
        ));
    }

    #[test]
    fn the_root_path_element_is_exact() {
        for yes in ["/", "/*", "/.", "//"] {
            assert!(token_matches(&Elem::RootPath, yes, yes), "{yes}");
        }
        for no in ["./build", "/tmp", "~", "/home/user", "."] {
            assert!(!token_matches(&Elem::RootPath, no, no), "{no}");
        }
    }

    #[test]
    fn the_home_path_element_covers_tilde_and_dollar_home() {
        for yes in ["~", "~/proj", "$HOME", "$HOME/proj"] {
            assert!(token_matches(&Elem::HomePath, yes, yes), "{yes}");
        }
        for no in ["/home/user", "echo"] {
            assert!(!token_matches(&Elem::HomePath, no, no), "{no}");
        }
    }

    #[test]
    fn a_compact_element_ignores_whitespace() {
        let a = ":(){ :|:& };:";
        let b = ":(){:|:&};:";
        assert!(token_matches(&Elem::Compact(":(){"), a, a));
        assert!(token_matches(&Elem::Compact(":|:&};:"), b, b));
        assert!(!token_matches(&Elem::Compact(":(){"), "echo hi", "echo hi"));
    }

    // ── blocklist: positive for every row ──────────────────────────────────

    #[test]
    fn the_blocklist_catches_rm_rf_root() {
        assert!(fires("block.rm.root", "rm -rf /"));
        assert!(fires("block.rm.root", "rm -fr /"));
        assert!(fires("block.rm.root", "rm -rf /*"));
        assert!(fires("block.rm.root", "rm -rf --no-preserve-root /"));
        assert!(fires("block.rm.root", "sudo rm -rf /"));
    }

    #[test]
    fn the_blocklist_catches_rm_rf_home() {
        assert!(fires("block.rm.home", "rm -rf ~"));
        assert!(fires("block.rm.home", "rm -rf ~/"));
        assert!(fires("block.rm.home", "rm -rf $HOME"));
        assert!(fires("block.rm.home", "rm -rf ~/*"));
    }

    #[test]
    fn the_blocklist_catches_every_filesystem_formatter() {
        for cmd in [
            "mkfs /dev/sda1",
            "mkfs.ext4 /dev/sda1",
            "mke2fs /dev/sda1",
            "newfs /dev/da0",
        ] {
            assert!(fires("block.mkfs", cmd), "{cmd}");
        }
    }

    #[test]
    fn the_blocklist_catches_dd_onto_a_device() {
        assert!(fires("block.dd.device", "dd if=/dev/zero of=/dev/sda"));
        assert!(fires("block.dd.device", "dd of=/dev/nvme0n1 if=image.iso"));
    }

    #[test]
    fn the_blocklist_catches_a_redirect_into_a_device() {
        assert!(fires("block.redirect.device", "> /dev/sda"));
        assert!(fires("block.redirect.device", "echo x >> /dev/sdb1"));
    }

    #[test]
    fn the_fork_bomb_row_needs_command_scope() {
        // Whitespace-independent, and matched against the whole command.
        let p = pattern("block.forkbomb");
        assert!(pattern_matches(p, ":(){ :|:& };:"));
        assert!(pattern_matches(p, ":(){:|:&};:"));
        // On one segment it cannot fire, which is why it lives in COMMAND_SCOPE.
        assert!(!pattern_matches(p, ":(){ :|:& }"));
    }

    #[test]
    fn the_blocklist_catches_chmod_777_on_root() {
        assert!(fires("block.chmod.777.root", "chmod -R 777 /"));
        assert!(fires("block.chmod.777.root", "chmod -R 777 /*"));
    }

    #[test]
    fn the_blocklist_catches_writing_a_device() {
        assert!(fires("block.write.device", "tee /dev/sdb < image.iso"));
        assert!(fires("block.write.device", "cat image.iso > /dev/nvme0n1"));
    }

    // ── blocklist: near-miss negative for every row ───────────────────────

    #[test]
    fn rm_on_a_subdirectory_is_not_a_root_delete() {
        for no in [
            "rm -rf ./build",
            "rm -rf build",
            "rm -rf /tmp/scratch",
            "rmdir /tmp/x",
        ] {
            assert!(!fires("block.rm.root", no), "{no}");
        }
    }

    /// The home blocklist stops at the home directory itself. Deleting one folder
    /// inside it is HIGH, not CRITICAL.
    #[test]
    fn rm_on_one_folder_inside_home_is_high_not_critical() {
        for cmd in ["rm -rf ~/Documents", "rm -rf ~/projects"] {
            assert!(!fires("block.rm.home", cmd), "{cmd}");
        }
        let v = crate::safety::classify("rm -rf ~/Documents");
        assert_eq!(v.level, Risk::High, "{:?}", v.reasons);
        assert!(v.is_runnable());
        for no in ["rm -rf ./dist", "rm -rf /home/user/proj/node_modules"] {
            assert!(!fires("block.rm.home", no), "{no}");
        }
    }

    #[test]
    fn mkfs_in_a_word_is_not_a_formatter() {
        for no in ["echo mkfs.ext4", "man mkfs", "df -h"] {
            assert!(!fires("block.mkfs", no), "{no}");
        }
    }

    /// Writing to `/dev/null` is the single most common shell redirection there
    /// is. A blocklist that fires on it would be one people turn off.
    #[test]
    fn dd_reading_a_device_or_writing_null_is_not_a_device_write() {
        assert!(!fires("block.dd.device", "dd if=/dev/zero of=/dev/null"));
        assert!(!fires(
            "block.dd.device",
            "dd if=/dev/zero of=./out.img bs=1M count=10"
        ));
        assert!(!fires("block.dd.device", "dd if=./in.img of=./out.img"));
        assert!(!fires("block.redirect.device", "echo hi > /dev/null"));
        assert!(!fires("block.write.device", "tee /dev/null"));
    }

    #[test]
    fn a_redirect_to_a_file_is_not_a_device_write() {
        for no in ["echo hi > /tmp/out.txt", "echo hi > out.txt", "tee out.txt"] {
            assert!(!fires("block.redirect.device", no), "{no}");
            assert!(!fires("block.write.device", no), "{no}");
        }
    }

    #[test]
    fn a_function_named_colon_is_not_a_fork_bomb() {
        let p = pattern("block.forkbomb");
        for no in ["echo hi", ":(){ echo hi; }", ":(){ echo ':|'; }"] {
            assert!(!pattern_matches(p, no), "{no}");
        }
    }

    #[test]
    fn chmod_777_on_a_project_is_not_the_root() {
        for no in ["chmod -R 777 ./public", "chmod 777 build.sh"] {
            assert!(!fires("block.chmod.777.root", no), "{no}");
        }
    }

    /// The near miss that a substring table would get wrong.
    #[test]
    fn a_dangerous_command_quoted_as_text_is_not_a_command() {
        for no in ["echo 'rm -rf /'", "grep -r 'rm -rf' .", "cat notes.md"] {
            assert!(!fires("block.rm.root", no), "{no}");
            assert!(!fires("block.mkfs", no), "{no}");
        }
    }

    // ── pattern table: positive and near-miss per row ─────────────────────

    #[test]
    fn high_rm_recursive_positive_and_negative() {
        assert!(fires("high.rm.recursive", "rm -r dist"));
        assert!(fires("high.rm.recursive", "rm -rf ./build"));
        assert!(!fires("high.rm.recursive", "rm dist"));
        assert!(!fires("high.rm.recursive", "rmdir dist"));
        assert!(!fires("high.rm.recursive", "echo rm -rf x"));
    }

    #[test]
    fn high_rm_force_positive_and_negative() {
        assert!(fires("high.rm.force", "rm -f a.txt"));
        assert!(!fires("high.rm.force", "rm a.txt"));
        assert!(!fires("high.rm.force", "echo rm -f"));
    }

    #[test]
    fn high_chown_recursive_positive_and_negative() {
        assert!(fires("high.chown.recursive", "chown -R me:me ."));
        assert!(!fires("high.chown.recursive", "chown me:me file.txt"));
        assert!(!fires("high.chown.recursive", "cat chown"));
    }

    #[test]
    fn high_permissions_broad_positive_and_negative() {
        assert!(fires("high.permissions.broad", "chmod -R u+w /srv/app"));
        assert!(!fires("high.permissions.broad", "chmod u+w /srv/app"));
    }

    #[test]
    fn high_firewall_flush_positive_and_negative() {
        assert!(fires("high.firewall.flush", "iptables -F"));
        assert!(fires("high.firewall.flush", "sudo ufw flush"));
        assert!(fires("high.firewall.flush", "nft flush ruleset"));
        assert!(!fires("high.firewall.flush", "iptables -L -n"));
        assert!(!fires("high.firewall.flush", "echo -F"));
        assert!(!fires("high.firewall.flush", "ufw status"));
    }

    #[test]
    fn high_power_positive_and_negative() {
        assert!(fires("high.power", "sudo shutdown -h now"));
        assert!(fires("high.power", "reboot"));
        assert!(!fires("high.power", "echo reboot"));
        assert!(!fires("high.power", "cat shutdown.log"));
    }

    #[test]
    fn high_git_force_push_positive_and_negative() {
        assert!(fires("high.git.forcepush", "git push --force"));
        assert!(fires("high.git.forcepush", "git push -f origin main"));
        assert!(fires(
            "high.git.forcepush",
            "git push --force-with-lease origin main"
        ));
        assert!(!fires("high.git.forcepush", "git push origin main"));
        assert!(!fires(
            "high.git.forcepush",
            "git push --dry-run origin main"
        ));
        assert!(!fires("high.git.forcepush", "git log --oneline"));
    }

    #[test]
    fn high_git_clean_positive_and_negative() {
        assert!(fires("high.git.clean", "git clean -fd"));
        assert!(fires("high.git.clean", "git clean -n"));
        assert!(!fires("high.git.clean", "git clean"));
        assert!(!fires("high.git.clean", "git commit -m clean"));
    }

    #[test]
    fn high_docker_prune_positive_and_negative() {
        assert!(fires("high.docker.prune", "docker system prune -af"));
        assert!(fires("high.docker.prune", "podman image prune"));
        assert!(!fires("high.docker.prune", "docker ps"));
        assert!(!fires("high.docker.prune", "docker image ls"));
    }

    #[test]
    fn high_kill_init_positive_and_negative() {
        assert!(fires("high.kill.init", "kill -9 1"));
        assert!(!fires("high.kill.init", "kill -9 4242"));
        assert!(!fires("high.kill.init", "pkill -9 node"));
    }

    #[test]
    fn high_chmod_777_positive_and_negative() {
        assert!(fires("high.chmod.777", "chmod 777 /srv/app"));
        assert!(!fires("high.chmod.777", "chmod 755 script.sh"));
        assert!(!fires("high.chmod.777", "chmod +x script.sh"));
    }

    #[test]
    fn high_mount_system_positive_and_negative() {
        assert!(fires("high.mount.system", "sudo mount /dev/sdb1 /mnt"));
        assert!(!fires("high.mount.system", "findmnt /mnt"));
        assert!(!fires("high.mount.system", "echo mount"));
    }

    #[test]
    fn medium_fetch_positive_and_negative() {
        assert!(fires("medium.fetch", "curl https://example.com"));
        assert!(fires("medium.fetch", "wget -q https://example.com/f"));
        assert!(!fires("medium.fetch", "echo curl"));
        assert!(!fires("medium.fetch", "cat curl.sh"));
    }

    #[test]
    fn medium_exec_positive_and_negative() {
        assert!(fires("medium.exec", "bash setup.sh"));
        assert!(fires("medium.exec", "sh -c 'echo hi'"));
        assert!(!fires("medium.exec", "echo bash"));
        assert!(!fires("medium.exec", "cat bashrc"));
    }

    #[test]
    fn medium_install_positive_and_negative() {
        assert!(fires("medium.install", "npm install"));
        assert!(fires("medium.install", "pip install requests"));
        assert!(!fires("medium.install", "npm run build"));
        assert!(!fires("medium.install", "cargo build --release"));
        assert!(!fires("medium.install", "npm list"));
    }

    #[test]
    fn medium_history_clear_positive_and_negative() {
        assert!(fires("medium.history.clear", "history -c"));
        assert!(!fires("medium.history.clear", "history | head"));
        assert!(!fires("medium.history.clear", "echo history"));
    }

    #[test]
    fn medium_walk_root_positive_and_negative() {
        assert!(fires("medium.walk.root", "find / -name '*.log'"));
        assert!(fires("medium.walk.root", "du -sh /*"));
        assert!(fires("medium.walk.root", "grep -r x /"));
        assert!(!fires("medium.walk.root", "find . -name '*.log'"));
        assert!(!fires("medium.walk.root", "ls /"));
        assert!(!fires("medium.walk.root", "find /tmp -name x"));
    }

    #[test]
    fn medium_packages_positive_and_negative() {
        assert!(fires("medium.packages", "sudo apt install ripgrep"));
        assert!(fires("medium.packages", "brew uninstall node"));
        assert!(!fires("medium.packages", "apt list"));
        assert!(!fires("medium.packages", "echo apt install"));
    }

    #[test]
    fn medium_service_positive_and_negative() {
        assert!(fires("medium.service", "sudo systemctl restart nginx"));
        assert!(fires("medium.service", "service nginx stop"));
        assert!(!fires("medium.service", "systemctl cat nginx"));
        assert!(!fires("medium.service", "systemctl status nginx"));
        assert!(!fires("medium.service", "echo restart"));
    }

    #[test]
    fn medium_destroy_contents_positive_and_negative() {
        assert!(fires("medium.destroy.contents", "shred -u secrets.txt"));
        // `truncate -l` sets a length, but it still discards the tail, so the
        // row fires. The near miss is the command being only mentioned.
        assert!(fires("medium.destroy.contents", "truncate -l 20 names.txt"));
        assert!(!fires("medium.destroy.contents", "echo shred"));
        assert!(!fires("medium.destroy.contents", "cat secrets.txt"));
    }

    #[test]
    fn medium_permissions_positive_and_negative() {
        assert!(fires("medium.permissions", "chmod 600 ~/.ssh/config"));
        assert!(!fires("medium.permissions", "ls -l"));
        assert!(!fires("medium.permissions", "echo chmod"));
    }

    #[test]
    fn medium_git_destructive_positive_and_negative() {
        assert!(fires("medium.git.destructive", "git reset --hard HEAD~1"));
        assert!(fires("medium.git.destructive", "git checkout -- ."));
        assert!(fires("medium.git.destructive", "git restore src/main.rs"));
        assert!(!fires("medium.git.destructive", "git reset"));
        assert!(!fires("medium.git.destructive", "git log --oneline"));
    }

    #[test]
    fn medium_write_system_positive_and_negative() {
        assert!(fires(
            "medium.write.system",
            "cp nginx.conf /etc/nginx/nginx.conf"
        ));
        assert!(fires("medium.write.system", "mv ./key /usr/local/bin/key"));
        assert!(fires("medium.write.system", "ln -s ./a /etc/a"));
        for no in ["cp ./a ./b", "cp ./a ./config/etc", "echo /etc/passwd"] {
            assert!(!fires("medium.write.system", no), "{no}");
        }
    }

    #[test]
    fn medium_schedule_positive_and_negative() {
        assert!(fires("medium.schedule", "crontab -e"));
        assert!(fires("medium.schedule", "at midnight"));
        assert!(!fires("medium.schedule", "cat crontab"));
        assert!(!fires("medium.schedule", "echo at"));
    }

    #[test]
    fn medium_kill_positive_and_negative() {
        assert!(fires("medium.kill", "kill 4242"));
        assert!(fires("medium.kill", "pkill node"));
        assert!(fires("medium.kill", "killall node"));
        assert!(!fires("medium.kill", "echo kill"));
    }

    #[test]
    fn medium_database_positive_and_negative() {
        assert!(fires("medium.database", "psql -c 'DROP TABLE users'"));
        assert!(fires(
            "medium.database",
            "sqlite3 db.sqlite 'DELETE FROM t'"
        ));
        assert!(!fires("medium.database", "psql -c 'SELECT 1'"));
        assert!(!fires("medium.database", "echo psql"));
    }

    #[test]
    fn medium_user_account_positive_and_negative() {
        assert!(fires("medium.user.account", "sudo usermod -aG docker me"));
        assert!(fires("medium.user.account", "useradd --help"));
        assert!(!fires("medium.user.account", "cat passwd"));
        assert!(!fires("medium.user.account", "echo passwd"));
    }

    #[test]
    fn low_rows_positive_and_negative() {
        assert!(fires("low.rm.single", "rm ./a.txt"));
        assert!(fires("low.rm.bare", "rm a.txt"));
        assert!(!fires("low.rm.bare", "echo rm"));
        assert!(fires("low.mv", "mv ./a ./b"));
        assert!(!fires("low.mv", "echo mv"));
        assert!(fires("low.mkdir", "mkdir -p out"));
        assert!(!fires("low.mkdir", "echo mkdir"));
    }

    // ── table integrity ───────────────────────────────────────────────────

    #[test]
    fn every_pattern_has_a_unique_id_a_reason_and_a_matcher() {
        let mut ids: Vec<&str> = Vec::new();
        for p in PATTERNS
            .iter()
            .chain(BLOCKLIST.iter())
            .chain(COMMAND_SCOPE.iter())
        {
            assert!(!p.id.is_empty(), "a pattern has no id");
            assert!(!ids.contains(&p.id), "duplicate pattern id: {}", p.id);
            ids.push(p.id);
            assert!(
                p.reason.len() > 8,
                "reason is too terse to be useful: {}",
                p.reason
            );
            assert!(
                !p.reason.contains("danger") && !p.reason.contains("unsafe"),
                "reason should say what it does, not that it is risky: {}",
                p.reason
            );
            assert!(!p.matchers.is_empty(), "{} has no matchers", p.id);
        }
    }

    #[test]
    fn every_blocklist_row_is_critical() {
        for p in BLOCKLIST.iter().chain(COMMAND_SCOPE.iter()) {
            assert_eq!(p.level, Risk::Critical, "{} is not CRITICAL", p.id);
        }
    }

    /// Invariant 7 of AGENTS.md: every pattern gets a positive and a near-miss
    /// negative. Named here so a new row without tests fails the build.
    #[test]
    fn every_pattern_row_has_a_positive_and_a_negative_test() {
        let covered = [
            "block.rm.root",
            "block.rm.home",
            "block.mkfs",
            "block.dd.device",
            "block.redirect.device",
            "block.forkbomb",
            "block.chmod.777.root",
            "block.write.device",
            "high.rm.recursive",
            "high.rm.force",
            "high.chown.recursive",
            "high.permissions.broad",
            "high.firewall.flush",
            "high.power",
            "high.git.forcepush",
            "high.git.clean",
            "high.docker.prune",
            "high.kill.init",
            "high.chmod.777",
            "high.mount.system",
            "medium.fetch",
            "medium.exec",
            "medium.install",
            "medium.history.clear",
            "medium.walk.root",
            "medium.packages",
            "medium.service",
            "medium.destroy.contents",
            "medium.permissions",
            "medium.git.destructive",
            "medium.write.system",
            "medium.schedule",
            "medium.kill",
            "medium.database",
            "medium.user.account",
            "low.rm.single",
            "low.rm.bare",
            "low.mv",
            "low.mkdir",
        ];
        // Set equality rather than a length check. The declared list is the
        // contract: a row with no entry is an untested pattern, and an entry with
        // no row means a test was written for something that no longer exists.
        let mut in_table: Vec<&str> = PATTERNS
            .iter()
            .chain(BLOCKLIST.iter())
            .chain(COMMAND_SCOPE.iter())
            .map(|p| p.id)
            .collect();
        let mut declared: Vec<&str> = covered.to_vec();
        in_table.sort_unstable();
        declared.sort_unstable();
        assert_eq!(
            in_table, declared,
            "the declared test list does not match the table"
        );
    }
}
