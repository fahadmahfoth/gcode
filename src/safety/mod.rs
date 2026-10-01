//! Independent risk classification.
//!
//! The invariant this module exists to hold: **a command is classified from the
//! string that will be executed, never from the model's opinion** (ADR 0004).
//! It therefore has no access to the model, no I/O, and no configuration beyond
//! what is handed to it. `src/safety/` never imports `crate::inference`.
//!
//! Pipeline, in order:
//!
//! 1. [`normalise`] — strip comments, join continuations, collapse whitespace,
//!    split on `;` `&&` `||` `|` and classify every segment.
//! 2. [`crate::safety::patterns::BLOCKLIST`] — a hit is CRITICAL and stops
//!    everything (ADR 0008).
//! 3. [`crate::safety::patterns::PATTERNS`] — level per pattern, maximum wins.
//! 4. Structural checks — privilege, network+execute, taint.
//! 5. A floor for unknown commands.
//!
//! Every stage is cheap and pure, which is what makes the safety test suite
//! finish in milliseconds without a model.

pub mod classifier;
pub mod patterns;

pub use classifier::{
    classify, classify_in_env, classify_with, expand_vars, fetches_and_executes, normalise,
    split_segments,
};
pub use patterns::{BLOCKLIST, KNOWN_SAFE, PATTERNS};

use std::fmt;

/// How dangerous a command is.
///
/// `Ord` is deliberate: levels are compared, not enumerated (invariant 2 of
/// SAFETY.md, "the maximum wins"). The declaration order is the ascending
/// severity order, so `>` means "more dangerous".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Risk {
    /// Read-only, no side effects, bounded cost.
    Safe,
    /// Read-only, or a trivial reversible write.
    Low,
    /// Broad walk, package change, service restart, network fetch.
    Medium,
    /// Deletes data, changes permissions broadly, pipes network into a shell.
    High,
    /// Destroys a filesystem, wipes home, overwrites a raw disk, fork bombs.
    /// Hard blocked. There is no override.
    Critical,
}

impl Risk {
    /// The upper-case name, as it appears in `--json`, the man page, and docs.
    ///
    /// [`fmt::Display`] delegates here so the string exists in exactly one place.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    /// Whether a command at this level is runnable at all.
    ///
    /// The one question the whole module exists to answer correctly. No flag, no
    /// config key, and no environment variable reaches this.
    #[must_use]
    pub const fn is_runnable(self) -> bool {
        !matches!(self, Self::Critical)
    }

    /// Whether this level needs an explicit `y` before running.
    ///
    /// SAFE still prompts at low trust levels, so this is the *minimum*
    /// requirement rather than a decision. Consent is the default everywhere.
    #[must_use]
    pub const fn needs_consent(self) -> bool {
        true
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a command landed at its level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// The pattern that fired, e.g. `high.rm.recursive`.
    pub pattern_id: &'static str,
    /// The level that pattern contributes.
    pub level: Risk,
    /// A plain sentence for the user.
    pub message: &'static str,
    /// Which segment of the command fired it.
    pub segment: usize,
}

/// One `;`- or `&&`-separated piece of the command, and its own level.
///
/// Kept so the UI can show *which* half of a command is the problem. A user told
/// "HIGH" about `ls && rm -rf /` without being shown the second half has learned
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The segment as the user typed it.
    pub raw: String,
    /// The segment after normalisation, which is what the patterns saw.
    pub normalised: String,
    /// The highest level any pattern or check gave this segment.
    pub level: Risk,
    /// Reasons that fired on this segment.
    pub reasons: Vec<Reason>,
}

impl Segment {
    /// Whether this segment alone is unrunnable.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        !self.level.is_runnable()
    }
}

/// The classification of a whole command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The maximum across every segment. This is the command's level.
    pub level: Risk,
    /// Every reason from every segment.
    pub reasons: Vec<Reason>,
    /// The segments, in order.
    pub segments: Vec<Segment>,
}

impl Verdict {
    /// Whether the command may be executed. CRITICAL never may.
    #[must_use]
    pub fn is_runnable(&self) -> bool {
        self.level.is_runnable()
    }

    /// Whether the command is CRITICAL and therefore refused.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        !self.is_runnable()
    }

    /// The first segment that is itself unrunnable.
    #[must_use]
    pub fn blocking_segment(&self) -> Option<&Segment> {
        self.segments.iter().find(|s| s.is_blocked())
    }

    /// One message per reason, in order. Deduped: the same pattern firing on two
    /// segments of `rm -rf / && rm -rf /` is one problem, not two, and printing
    /// it twice reads as a second finding rather than a repeat.
    #[must_use]
    pub fn reason_messages(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for reason in &self.reasons {
            if !out.iter().any(|m| m == reason.message) {
                out.push(reason.message.to_owned());
            }
        }
        out
    }

    /// Every distinct pattern id that fired, in order.
    #[must_use]
    pub fn pattern_ids(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for reason in &self.reasons {
            if !out.contains(&reason.pattern_id) {
                out.push(reason.pattern_id);
            }
        }
        out
    }

    /// Every path this command names, deduplicated and in order.
    ///
    /// Derived from the normalised segments rather than by re-parsing the raw
    /// string, so a path inside a quoted argument is still found and a path in a
    /// stripped comment is not reported as touched.
    #[must_use]
    pub fn touched_paths(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for segment in &self.segments {
            for token in segment.normalised.split_whitespace() {
                let bare = token.trim_matches(|c| c == '\'' || c == '"');
                // A device node, a home reference, a dot-relative path, or
                // anything with a separator in it.
                let looks_like_path = bare.starts_with('/')
                    || bare.starts_with("./")
                    || bare.starts_with("../")
                    || bare == "~"
                    || bare.starts_with("~/")
                    || bare.starts_with("$HOME")
                    || bare.starts_with('%');
                if looks_like_path && !out.contains(&bare.to_owned()) {
                    out.push(bare.to_owned());
                }
            }
        }
        out
    }

    /// A plain-language explanation of what the command does and touches.
    ///
    /// For `--explain`. Describes and never suggests: it must not propose a
    /// different command, or `--explain` becomes a second generator whose output
    /// nothing has classified.
    #[must_use]
    pub fn explanation(&self) -> String {
        let mut s = String::new();
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("level: {}\n", self.level));
        let _ =
            std::fmt::Write::write_fmt(&mut s, format_args!("what it does: {}\n", self.summary()));

        let commands: Vec<&str> = self
            .segments
            .iter()
            .filter_map(|seg| seg.normalised.split_whitespace().next())
            .collect();
        if commands.is_empty() {
            s.push_str("what it runs: nothing — the command is empty after normalisation\n");
        } else {
            let mut unique: Vec<&str> = Vec::new();
            for c in &commands {
                if !unique.contains(c) {
                    unique.push(c);
                }
            }
            let _ = std::fmt::Write::write_fmt(
                &mut s,
                format_args!("what it runs: {}\n", unique.join(", ")),
            );
        }

        let paths = self.touched_paths();
        if paths.is_empty() {
            s.push_str("what it touches: no path — it names none\n");
        } else {
            let _ = std::fmt::Write::write_fmt(
                &mut s,
                format_args!("what it touches: {}\n", paths.join(", ")),
            );
        }

        if self.segments.len() > 1 {
            let parts: Vec<String> = self
                .segments
                .iter()
                .map(|seg| format!("{} [{:?}]", seg.normalised, seg.level))
                .collect();
            let _ = std::fmt::Write::write_fmt(
                &mut s,
                format_args!("segments: {}\n", parts.join(" | ")),
            );
        }

        if self.reasons.is_empty() {
            s.push_str("reasons: none — no destructive pattern matched\n");
        } else {
            // The header is plural even when there is exactly one reason, so the
            // shape of the output does not change with the number of findings. A
            // tool whose output changes shape is one that gets scraped.
            s.push_str("reasons:\n");
            for reason in &self.reasons {
                let _ = std::fmt::Write::write_fmt(
                    &mut s,
                    format_args!(
                        "  {} ({}) — {}\n",
                        reason.level, reason.pattern_id, reason.message
                    ),
                );
            }
        }
        s
    }

    /// A one-line summary for the prompt and for `--json`.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.reasons.is_empty() {
            return format!("{} — no destructive pattern matched", self.level);
        }
        let mut s = format!("{} — ", self.level);
        let first = &self.reasons[0];
        s.push_str(first.message);
        if self.reasons.len() > 1 {
            let _ = std::fmt::Write::write_fmt(
                &mut s,
                format_args!(" (and {} more)", self.reasons.len() - 1),
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_orders_by_severity() {
        assert!(Risk::Safe < Risk::Low);
        assert!(Risk::Low < Risk::Medium);
        assert!(Risk::Medium < Risk::High);
        assert!(Risk::High < Risk::Critical);
    }

    #[test]
    fn risk_names_are_uppercase() {
        assert_eq!(Risk::Safe.as_str(), "SAFE");
        assert_eq!(Risk::Low.as_str(), "LOW");
        assert_eq!(Risk::Medium.as_str(), "MEDIUM");
        assert_eq!(Risk::High.as_str(), "HIGH");
        assert_eq!(Risk::Critical.as_str(), "CRITICAL");
        assert_eq!(Risk::Critical.to_string(), "CRITICAL");
    }

    #[test]
    fn only_critical_is_unrunnable() {
        assert!(Risk::Safe.is_runnable());
        assert!(Risk::Low.is_runnable());
        assert!(Risk::Medium.is_runnable());
        assert!(Risk::High.is_runnable());
        assert!(!Risk::Critical.is_runnable());
    }

    #[test]
    fn every_level_requires_consent() {
        for level in [
            Risk::Safe,
            Risk::Low,
            Risk::Medium,
            Risk::High,
            Risk::Critical,
        ] {
            assert!(level.needs_consent(), "{level}");
        }
    }

    #[test]
    fn a_verdict_with_no_reasons_says_so() {
        let v = Verdict {
            level: Risk::Safe,
            reasons: vec![],
            segments: vec![],
        };
        assert_eq!(v.summary(), "SAFE — no destructive pattern matched");
        assert!(v.is_runnable());
    }

    #[test]
    fn a_verdict_with_many_reasons_counts_the_extras() {
        let r = |id: &'static str| Reason {
            pattern_id: id,
            level: Risk::High,
            message: "does something",
            segment: 0,
        };
        let v = Verdict {
            level: Risk::High,
            reasons: vec![r("a"), r("b"), r("c")],
            segments: vec![],
        };
        assert_eq!(v.summary(), "HIGH — does something (and 2 more)");
    }

    #[test]
    fn the_blocking_segment_is_the_first_that_is_critical() {
        let seg = |level| Segment {
            raw: "x".to_owned(),
            normalised: "x".to_owned(),
            level,
            reasons: vec![],
        };
        let v = Verdict {
            level: Risk::Critical,
            reasons: vec![],
            segments: vec![seg(Risk::Safe), seg(Risk::Critical), seg(Risk::Critical)],
        };
        assert!(v.is_blocked());
        assert_eq!(v.blocking_segment().map(|s| s.level), Some(Risk::Critical));
    }
}
