//! Redaction, applied before prompt assembly and nowhere else.
//!
//! ADR 0006 fixes this in one function for a reason: redaction scattered across
//! call sites is redaction that eventually gets skipped on the path somebody
//! forgot. History entries, filenames, and environment values all pass through
//! [`redact`] on their way into a prompt, and there is exactly one
//! implementation of what a secret looks like.
//!
//! # What is and is not a secret
//!
//! The patterns here are the shapes that turn up in a terminal and in
//! `history.jsonl`: API keys with a recognisable prefix, bearer tokens,
//! passwords assigned inline, and PEM private key blocks. gcode cannot know
//! your organisation's token format, so a pattern that misses is a miss — see
//! [docs/SAFETY.md](../../docs/SAFETY.md), which says so in as many words.
//!
//! The rule this module follows is that a false positive costs a `[REDACTED]` in
//! a prompt and a false negative costs a live credential in a model's context
//! window. It errs towards redacting.

use std::borrow::Cow;

/// Replaces a redacted span.
///
/// Deliberately not the empty string. A blank leaves the model wondering
/// whether the field was empty, and "why did this value disappear" is exactly
/// the kind of gap a prompt-injection attempt tries to talk its way into
/// filling. A visible marker also makes a leak obvious during snapshot review.
const MASK: &str = "[REDACTED]";

/// Longest history output tail placed in a prompt, in bytes.
///
/// The cap is on the end of the string, not the start. A build error's useful
/// part is its last line, and truncating from the front would keep the preamble
/// and discard the message.
pub const DEFAULT_OUTPUT_TAIL_BYTES: usize = 2048;

/// Most history entries placed in a prompt.
///
/// Fifteen is what fits in a 4 KB context alongside the instructions and the
/// request without squeezing the model's attention away from the actual
/// question.
pub const MAX_HISTORY_ENTRIES: usize = 15;

/// Opens a PEM block. Base64 bodies contain no dashes, so this is unambiguous.
const PEM_HEADER: &str = "-----BEGIN";

/// Marks text that was cut at the front, so the model does not read a truncated
/// error as a complete one.
const TRUNCATED: &str = "[...]";

/// Characters that end a credential.
///
/// The union of what a shell and a URL treat as a boundary. A character not in
/// this set is taken to be part of the secret, which is the direction to be
/// wrong in: a slightly wider mask costs nothing, a narrower one leaks.
const DELIMITERS: &str = " \t\n\r\"'`;|&<>()[]{},;:?#\\";

/// A prefix that marks the start of a credential, and what replaces it.
struct Rule {
    /// Matched literally.
    pattern: &'static str,
    /// Written in place of the pattern and the credential that follows it.
    replacement: &'static str,
    /// Why this shape counts as a secret, for whoever edits this file next.
    ///
    /// Never read at runtime. A pattern table whose rows are unexplained is a
    /// pattern table that loses a row to a careless cleanup, so the reasoning
    /// travels with the rule rather than in a commit message.
    #[allow(dead_code)]
    why: &'static str,
}

/// The rules, in application order.
///
/// PEM blocks are handled separately and first; a line rule applied before it
/// could cut a `key=` line out of the middle of a key block and leave base64
/// behind.
static RULES: &[Rule] = &[
    Rule {
        pattern: "Authorization: Bearer ",
        replacement: "Authorization: [REDACTED] ",
        why: "an Authorization header. The scheme is consumed with the secret \
              below: matching the header name alone leaves `Bearer <token>` \
              sitting safely in the prompt",
    },
    Rule {
        pattern: "Authorization:",
        replacement: "Authorization: [REDACTED]",
        why: "an Authorization header with no recognised scheme",
    },
    Rule {
        pattern: "sk-",
        replacement: "[REDACTED]",
        why: "an OpenAI-shaped API key",
    },
    Rule {
        pattern: "ghp_",
        replacement: "[REDACTED]",
        why: "a GitHub personal access token",
    },
    Rule {
        pattern: "gho_",
        replacement: "[REDACTED]",
        why: "a GitHub OAuth token",
    },
    Rule {
        pattern: "ghu_",
        replacement: "[REDACTED]",
        why: "a GitHub user-to-server token",
    },
    Rule {
        pattern: "ghs_",
        replacement: "[REDACTED]",
        why: "a GitHub server-to-server token",
    },
    Rule {
        pattern: "github_pat_",
        replacement: "[REDACTED]",
        why: "a fine-grained GitHub token",
    },
    Rule {
        pattern: "xoxb-",
        replacement: "[REDACTED]",
        why: "a Slack bot token",
    },
    Rule {
        pattern: "xoxp-",
        replacement: "[REDACTED]",
        why: "a Slack user token",
    },
    Rule {
        pattern: "AKIA",
        replacement: "[REDACTED]",
        why: "an AWS access key id",
    },
    Rule {
        pattern: "ASIA",
        replacement: "[REDACTED]",
        why: "an AWS temporary access key id",
    },
    Rule {
        pattern: "AIza",
        replacement: "[REDACTED]",
        why: "a Google API key",
    },
    Rule {
        pattern: "glpat-",
        replacement: "[REDACTED]",
        why: "a GitLab personal access token",
    },
    Rule {
        pattern: "ya29.",
        replacement: "[REDACTED]",
        why: "a Google OAuth access token",
    },
    Rule {
        pattern: "password=",
        replacement: "password=[REDACTED]",
        why: "a password assigned inline",
    },
    Rule {
        pattern: "passwd=",
        replacement: "passwd=[REDACTED]",
        why: "a password assigned inline",
    },
    Rule {
        pattern: "token=",
        replacement: "token=[REDACTED]",
        why: "a token assigned inline",
    },
    Rule {
        pattern: "api_key=",
        replacement: "api_key=[REDACTED]",
        why: "an API key assigned inline",
    },
    Rule {
        pattern: "api-key=",
        replacement: "api-key=[REDACTED]",
        why: "an API key assigned inline",
    },
    Rule {
        pattern: "apikey=",
        replacement: "apikey=[REDACTED]",
        why: "an API key assigned inline",
    },
    Rule {
        pattern: "secret=",
        replacement: "secret=[REDACTED]",
        why: "a secret assigned inline",
    },
];

/// Removes credential-shaped strings from `input`.
///
/// Returns a borrowed `Cow` and allocates only when something was actually
/// redacted, because this runs over every history entry of every invocation and
/// the common case is text with nothing sensitive in it.
///
/// # Examples
///
/// ```
/// use gcode::context::redact::redact;
///
/// assert_eq!(
///     redact("export TOKEN=sk-abcdefghijklmnopqrstuvwx"),
///     "export TOKEN=[REDACTED]"
/// );
/// assert_eq!(redact("ls -la"), "ls -la");
/// ```
#[must_use]
pub fn redact(input: &str) -> Cow<'_, str> {
    // Nothing to do for the common case. The rules re-check individually; this
    // only avoids allocating a copy for clean text.
    if !mentions_a_secret(input) {
        return Cow::Borrowed(input);
    }

    let mut out = input.to_owned();
    strip_pem_blocks(&mut out);
    for rule in RULES {
        apply_line_rule(&mut out, rule);
    }
    Cow::Owned(out)
}

/// A cheap test for whether anything could match.
///
/// Checks the PEM header as well as the line rules. It must, or a private key
/// block sails past the early-out and reaches the model untouched.
fn mentions_a_secret(input: &str) -> bool {
    input.contains(PEM_HEADER) || RULES.iter().any(|r| input.contains(r.pattern))
}

/// Replaces the credential beginning at `rule.pattern` with
/// `rule.replacement`.
///
/// The credential ends at the first delimiter after the pattern, not at the end
/// of the line. Cutting to end-of-line would keep the secret out of the prompt
/// at the cost of silently deleting the rest of the user's command, and
/// `token=abc123 && ls` becoming `[REDACTED]` is a worse failure than the one it
/// prevents.
///
/// The delimiter set is the union of what a shell and a URL treat as a boundary.
/// Anything not in it is treated as part of the secret, which is the direction
/// to be wrong in.
///
/// Loops rather than replacing once, so two secrets on one line both go.
fn apply_line_rule(text: &mut String, rule: &Rule) {
    let mut cursor = 0;
    while let Some(rel) = text.get(cursor..).and_then(|t| t.find(rule.pattern)) {
        let start = cursor + rel;
        let rest = &text[start + rule.pattern.len()..];
        let payload_len = rest
            .find(|c: char| DELIMITERS.contains(c))
            .unwrap_or(rest.len());

        // An assignment with no value is a search pattern, not a credential:
        // `grep -r 'token=' src/` is somebody looking for tokens, and masking it
        // would silently rewrite the command they ran. A rule whose pattern ends
        // at a delimiter with nothing after it has nothing to hide, so it is
        // skipped and the scan resumes past it.
        if payload_len == 0 {
            cursor = start + rule.pattern.len();
            continue;
        }

        let mut next = String::with_capacity(text.len());
        next.push_str(&text[..start]);
        next.push_str(rule.replacement);
        next.push_str(&rest[payload_len..]);
        *text = next;

        // Resume past the replacement. No replacement contains its own pattern,
        // so this cannot loop; stepping past the mask also stops the next
        // iteration from re-finding the pattern just written.
        cursor = start + rule.replacement.len();
    }
}

/// Removes whole PEM blocks, header through footer inclusive.
///
/// A PEM body is base64, which contains no dashes at all. That makes the
/// detection unusually reliable: the header, the footer, and nothing else in the
/// block can produce a run of five dashes. The footer is therefore found by
/// scanning from the end of the header *line*, since the header line itself
/// begins with one.
fn strip_pem_blocks(text: &mut String) -> bool {
    const DASHES: &str = "-----";
    let mut changed = false;
    let mut cursor = 0;

    while let Some(rel) = text.get(cursor..).and_then(|t| t.find(PEM_HEADER)) {
        let start = cursor + rel;

        // Past the header line, which is where the footer can legitimately
        // appear. Searching from `start` instead would match the header's own
        // leading dashes and treat every block as if it were empty.
        let after_header = if let Some(nl) = text[start..].find('\n') {
            start + nl + 1
        } else {
            // A header with no body on this line and no newline after it: there
            // is nothing to redact but the header.
            {
                let mut next = String::with_capacity(text.len());
                next.push_str(&text[..start]);
                next.push_str(MASK);
                *text = next;
                return true;
            }
        };

        let Some(rel_end) = text.get(after_header..).and_then(|t| t.find(DASHES)) else {
            // Unterminated: the body would have followed, so everything from the
            // header onward is suspect and cannot be trusted to be safe.
            let mut next = String::with_capacity(text.len());
            next.push_str(&text[..start]);
            next.push_str(MASK);
            *text = next;
            return true;
        };

        let footer = after_header + rel_end;
        let tail = &text[footer..];
        let line = tail.find('\n').unwrap_or(tail.len());

        let mut next = String::with_capacity(text.len());
        next.push_str(&text[..start]);
        next.push_str(MASK);
        next.push_str(&tail[line..]);
        *text = next;
        changed = true;

        // Resume past the mask so the loop cannot re-find this header.
        cursor = start + MASK.len();
    }
    changed
}

/// Truncates to at most `limit` bytes, keeping the end.
///
/// Byte-oriented, not character-oriented, on purpose: the limit exists to bound
/// the bytes handed to the model, and slicing on a character boundary while
/// exceeding the byte budget would defeat it. A cut landing mid-UTF-8 is moved
/// forward to a boundary so the prompt stays valid text.
///
/// The leading marker tells the model the text started mid-stream, so it does
/// not read a truncated error as a complete one.
///
/// # Examples
///
/// ```
/// use gcode::context::redact::tail_bytes;
///
/// assert_eq!(tail_bytes("abcdef", 3), "[...]def");
/// assert_eq!(tail_bytes("abc", 10), "abc");
/// ```
#[must_use]
pub fn tail_bytes(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_owned();
    }
    let mut start = s.len() - limit;
    // Walk forward to a character boundary. `is_char_boundary` is false at most
    // three bytes into a multi-byte sequence, so this terminates immediately.
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("{TRUNCATED}{}", &s[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── the shapes the roadmap names ────────────────────────────────────────

    #[test]
    fn an_openai_shaped_key_is_redacted() {
        let out = redact("sk-abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(out, "[REDACTED]");
        assert!(!out.contains("abcdefghij"), "leaked: {out}");
    }

    #[test]
    fn a_bearer_token_is_redacted() {
        let out = redact("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abc");
        assert!(!out.contains("eyJhbGci"), "leaked: {out}");
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn a_pem_private_key_block_is_redacted_whole() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow...\n-----END RSA PRIVATE KEY-----\n";
        let out = redact(pem);
        assert!(!out.contains("MIIEow"), "leaked: {out}");
        assert!(!out.contains("BEGIN RSA"), "leaked: {out}");
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn text_around_a_pem_block_survives() {
        let input = "before\n-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\nafter\n";
        let out = redact(input);
        assert!(out.starts_with("before"), "{out}");
        assert!(out.ends_with("after\n"), "{out}");
        assert!(!out.contains("AAAA"), "{out}");
    }

    /// An unterminated block means the body was about to follow. Leaving
    /// everything after the header in place would leak the key body.
    #[test]
    fn an_unterminated_pem_block_is_redacted() {
        let out = redact("-----BEGIN PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n");
        assert!(!out.contains("MIIEow"), "leaked: {out}");
    }

    // ── other credential shapes ────────────────────────────────────────────

    #[test]
    fn a_github_token_is_redacted() {
        assert!(!redact("ghp_16CharsOfNonsense0000000000").contains("16Chars"));
        assert!(!redact("github_pat_11ABCDEFG0abcdefghijkl").contains("11ABC"));
    }

    #[test]
    fn an_aws_key_id_is_redacted() {
        assert!(!redact("AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE").contains("IOSFODNN"));
    }

    #[test]
    fn a_slack_token_is_redacted() {
        assert!(!redact("xoxb-123456789012-abcdefghijkl").contains("abcdefghij"));
    }

    #[test]
    fn an_inline_password_is_redacted() {
        let out = redact("mysql -u root --password=hunter2 db");
        assert!(!out.contains("hunter2"), "leaked: {out}");
        assert!(out.contains("mysql"), "the command survives: {out}");
    }

    #[test]
    fn an_inline_token_is_redacted() {
        assert!(!redact("curl -H 'token=abc123def456'").contains("abc123"));
    }

    #[test]
    fn an_inline_api_key_is_redacted() {
        assert!(!redact("api_key=zzzzzzzzzzzz").contains("zzzzzzzz"));
    }

    // ── the false-positive side ────────────────────────────────────────────

    /// Redacting ordinary commands would make the history useless. This is the
    /// test that keeps the rules from over-reaching.
    #[test]
    fn ordinary_commands_are_untouched() {
        for cmd in [
            "npm run build",
            "git commit -m 'fix the thing'",
            "ls -la /home/user/projects",
            "grep -r 'token=' src/",
            "echo done",
            "pytest -q",
            "cargo test --all-features",
            "awk '{print $1}' file.txt",
        ] {
            assert_eq!(redact(cmd), cmd, "over-redacted: {cmd}");
        }
    }

    #[test]
    fn an_empty_string_is_untouched() {
        assert_eq!(redact(""), "");
    }

    #[test]
    fn text_without_secrets_is_borrowed_not_copied() {
        assert!(matches!(redact("ls -la"), Cow::Borrowed(_)));
    }

    #[test]
    fn text_with_a_secret_is_owned() {
        assert!(matches!(redact("token=abc123456"), Cow::Owned(_)));
    }

    /// A credential does not contain a newline, so cutting to end-of-line keeps
    /// the rest of the command the user actually ran.
    #[test]
    fn only_the_secret_on_a_line_is_removed() {
        let out = redact("cd /tmp && token=abcdefghij && ls");
        assert!(!out.contains("abcdefghij"), "{out}");
        assert!(out.contains("cd /tmp"), "{out}");
        assert!(out.contains("ls"), "{out}");
    }

    #[test]
    fn two_secrets_on_two_lines_are_both_removed() {
        let out = redact("token=aaaaaaaaaaaaaaaa\nsecret=bbbbbbbbbbbb\nls");
        assert!(!out.contains("aaaaaaaaaa"), "{out}");
        assert!(!out.contains("bbbbbbbbbb"), "{out}");
        assert!(out.contains("ls"));
    }

    /// Two secrets on one line must both go, which is why the rule loops.
    #[test]
    fn two_secrets_on_one_line_are_both_removed() {
        let out = redact("curl -H 'token=aaaaaaaaaaaa' -H 'sk-bbbbbbbbbbbb'");
        assert!(!out.contains("aaaaaaaaaaaa"), "{out}");
        assert!(!out.contains("bbbbbbbbbbbb"), "{out}");
    }

    // ── truncation ─────────────────────────────────────────────────────────

    #[test]
    fn a_short_string_is_returned_whole() {
        assert_eq!(tail_bytes("abc", 10), "abc");
    }

    #[test]
    fn a_long_string_keeps_its_end() {
        assert_eq!(tail_bytes("abcdef", 3), "[...]def");
    }

    /// The end of a build error is the useful part. Truncating from the front
    /// would keep the preamble and throw away the message.
    #[test]
    fn the_end_is_kept_because_that_is_where_the_error_is() {
        let input = "compiling 400 files\nerror: cannot find value `foo`";
        let out = tail_bytes(input, 20);
        assert!(out.ends_with("`foo`"), "{out}");
        assert!(out.starts_with("[...]"), "{out}");
    }

    /// A multi-byte character must not be cut in half, or the prompt stops being
    /// valid UTF-8 and the model sees replacement characters.
    #[test]
    fn a_truncation_inside_a_multibyte_character_stays_valid() {
        let input = "αααααααααααααα";
        for limit in 0..40 {
            let out = tail_bytes(input, limit);
            assert!(
                std::str::from_utf8(out.as_bytes()).is_ok(),
                "limit {limit} produced invalid text: {out:?}"
            );
        }
    }

    #[test]
    fn truncation_respects_the_byte_budget() {
        let input = "x".repeat(10_000);
        let out = tail_bytes(&input, 2048);
        assert!(out.len() <= 2048 + 5, "budget blown: {}", out.len());
    }

    /// Order matters and is worth asserting: truncating first could cut a
    /// rule's marker off the end of a long line and leave the payload.
    #[test]
    fn redaction_runs_before_truncation_or_the_secret_survives() {
        let input = format!("error: token={} and more text follows", "s".repeat(50));
        let tail = tail_bytes(&redact(&input), 40);
        assert!(!tail.contains(&"s".repeat(20)), "leaked: {tail}");
    }
}
