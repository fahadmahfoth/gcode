//! The model registry: what gcode is willing to download, and what it will trust.
//!
//! # Where the data comes from
//!
//! [`models/registry.toml`](../../../models/registry.toml) is the single source of
//! truth. It lives in the repository as a data file so that adding a model is a
//! reviewable diff rather than a change to Rust source, and it is embedded with
//! `include_str!` so the binary carries it. There is no search path, no
//! environment variable pointing at it, and no "registry not found" failure
//! mode. A user cannot end up with a registry newer than their binary, because
//! the registry *is* the binary.
//!
//! # Why it is validated twice
//!
//! `build.rs` enforces every rule at compile time, which is what stops a bad
//! registry from being committed. This module enforces them again at runtime,
//! which is what stops a bad registry from being *used* if the build check is
//! ever bypassed, for instance by a vendored build that skips build scripts.
//! The two checks are deliberately redundant: a supply chain guarantee with one
//! layer is one mistake away from none.
//!
//! # Why a checksum is not optional
//!
//! The registry exists to pin *what* gcode downloads. A URL alone says "fetch
//! this from whoever controls that host today"; a checksum says "and the bytes
//! must be exactly these". Remove the checksum and the registry is just a list of
//! suggestions, which is the failure the whole design is meant to prevent. So a
//! missing `sha256` is a hard error, in both checks.
//!
//! # User additions
//!
//! ADR 0010 allows a user to add their own entries, merged with lower
//! precedence than the built-ins. That is not implemented yet: the config schema
//! has no `[[model]]` section, and inventing one here would put a promise in
//! `docs/USAGE.md` before the merge rules are decided. It arrives with the
//! downloader in Phase 1.4, where the two have to agree on precedence.

use std::fmt;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::Error;

/// The registry, embedded at compile time.
///
/// `include_str!` on a relative path resolves against this file's directory, so
/// this reaches `models/registry.toml` at the repository root. `build.rs` checks
/// the same path; if one moves, the other has to move with it.
const REGISTRY_TOML: &str = include_str!("../../models/registry.toml");

/// Length of a SHA-256 digest in hexadecimal.
const SHA256_HEX_LEN: usize = 64;

/// Bounds on a declared context window, matching `config::validate` and
/// `build.rs`. A registry promising a window the rest of gcode refuses would let
/// `--context-size` be clamped to a value the model cannot hold.
const MIN_CONTEXT_SIZE: u32 = 512;
const MAX_CONTEXT_SIZE: u32 = 32_768;

/// One model gcode knows how to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    /// What the user types after `--use-model`.
    pub name: String,
    /// One line for `--list-models`. Absent for an entry the user added.
    pub description: Option<String>,
    /// Direct download URL. HTTPS only; the checksum is meaningless over a
    /// channel that is not authenticated.
    pub url: String,
    /// Lowercase hex SHA-256 of the file. Required, always.
    pub sha256: String,
    /// Size in bytes, shown before the download so it can be declined.
    pub size_bytes: u64,
    /// Whether this is the model used when the user names none. Exactly one
    /// entry may say so.
    #[serde(default)]
    pub default: bool,
    /// Native context window in tokens.
    pub context_size: u32,
    /// Thread count to suggest. A hint, never applied automatically.
    pub recommended_threads: Option<u32>,
    /// SPDX identifier, recorded for the credits screen and for legal clarity.
    pub license: String,
}

impl ModelEntry {
    /// A human-readable size, for `--list-models`.
    ///
    /// Binary units, because model files are sold in binary units and a download
    /// the user expected to be 400 MB turning out to be 429 MB is the kind of
    /// thing that makes someone think they were lied to.
    #[must_use]
    pub fn size_human(&self) -> String {
        const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
        // Counted in tenths, entirely in integers, at u128. A file size is the
        // one number in this tool a user has to be able to trust exactly, so
        // there is no f64: its mantissa cannot hold a u64, and the tenths are
        // carried through the divisions rather than recomputed from a remainder
        // that the loop has already discarded.
        let mut tenths = u128::from(self.size_bytes) * 10;
        let mut unit = 0;
        while tenths >= 1024 * 10 && unit + 1 < UNITS.len() {
            tenths /= 1024;
            unit += 1;
        }
        if unit == 0 {
            return format!("{} B", self.size_bytes);
        }
        let (whole, tenth) = (tenths / 10, tenths % 10);
        if tenth == 0 {
            format!("{whole} {}", UNITS[unit])
        } else {
            format!("{whole}.{tenth} {}", UNITS[unit])
        }
    }
}

impl fmt::Display for ModelEntry {
    /// The name only.
    ///
    /// A registry row is a table, and a table wants the key, not a sentence. The
    /// description and size belong in columns chosen by whoever is printing, so
    /// `Display` stays the identifier.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// A validated set of models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    /// Entries in file order. The default is not moved to the front: file order
    /// is reviewable, and `--list-models` printing something other than the
    /// reviewed order would be a small lie.
    entries: Vec<ModelEntry>,
}

impl Registry {
    /// Parses and validates registry text.
    ///
    /// The same rules as `build.rs`, in the same order, so a registry that builds
    /// also loads. Exposed for the tests that need to prove each rule refuses.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::Invalid`] with every problem found, not just the
    /// first. A contributor fixing a registry should see all of it at once.
    pub fn parse(text: &str) -> Result<Self, RegistryError> {
        let file: RegistryFile = toml::from_str(text).map_err(|source| RegistryError::Invalid {
            problems: vec![format!("cannot be parsed as TOML: {source}")],
        })?;

        if file.model.is_empty() {
            return Err(RegistryError::Invalid {
                problems: vec!["declares no models at all".to_owned()],
            });
        }

        let mut problems = Vec::new();
        let mut defaults: Vec<&str> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();

        for (index, entry) in file.model.iter().enumerate() {
            check_entry(entry, index, &mut problems);

            if let Some(name) = entry.name.as_deref() {
                if seen.contains(&name) {
                    problems.push(format!("`{name}` appears more than once"));
                }
                seen.push(name);
            }
            if entry.default == Some(true) {
                // `label` is built per iteration and cannot be borrowed into a
                // vector that outlives the loop. The name is taken from the
                // entry, which `file` owns. A nameless entry is already a
                // problem, so the fallback is only for the message.
                defaults.push(entry.name.as_deref().unwrap_or("(unnamed)"));
            }
        }

        match defaults.len() {
            1 => {}
            0 => problems.push(
                "no model sets `default = true`. gcode must be able to download \
                 something without being told which model"
                    .to_owned(),
            ),
            _ => problems.push(format!(
                "{} models set `default = true` ({}). Exactly one must",
                defaults.len(),
                defaults.join(", ")
            )),
        }

        if !problems.is_empty() {
            return Err(RegistryError::Invalid { problems });
        }

        // Every field is `Some` at this point, and the checks above prove it, so
        // this is a restructure rather than a fallible step. A `filter_map` that
        // silently dropped an entry would be worse than the panic: it would
        // produce a registry that validates and is missing a model.
        let entries = file
            .model
            .into_iter()
            .map(|entry| ModelEntry {
                name: entry.name.unwrap_or_default(),
                description: entry.description,
                url: entry.url.unwrap_or_default(),
                sha256: entry.sha256.unwrap_or_default(),
                size_bytes: entry.size_bytes.unwrap_or_default(),
                default: entry.default.unwrap_or(false),
                context_size: entry.context_size.unwrap_or_default(),
                recommended_threads: entry.recommended_threads,
                license: entry.license.unwrap_or_default(),
            })
            .collect();

        Ok(Self { entries })
    }

    /// Every entry, in file order.
    #[must_use]
    pub fn entries(&self) -> &[ModelEntry] {
        &self.entries
    }

    /// The entry named `name`, if there is one.
    ///
    /// Exact and case-sensitive on purpose. A fuzzy match here would mean two
    /// names in a registry could resolve to the same model, and the one the user
    /// gets would depend on the order they happened to be typed.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ModelEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The default entry.
    ///
    /// `parse` guarantees exactly one exists, so this cannot fail. It returns
    /// `Option` rather than `expect` because the binary is not allowed to panic
    /// and a `Result` here would only be converted back into a panic at the call
    /// site.
    #[must_use]
    pub fn default_entry(&self) -> Option<&ModelEntry> {
        self.entries.iter().find(|entry| entry.default)
    }

    /// The default entry's name.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::NoDefault`] if the registry somehow has none,
    /// which `parse` rules out and only a hand-built `Registry` could cause.
    pub fn default_name(&self) -> Result<&str, RegistryError> {
        self.default_entry()
            .map(|entry| entry.name.as_str())
            .ok_or(RegistryError::NoDefault)
    }
}

/// The embedded registry, parsed once.
///
/// `OnceLock` rather than `LazyLock` because 1.75 is the MSRV and `LazyLock` is
/// 1.80. `get_or_init` is used so a parse failure is not cached as a panic and
/// re-panicked on every call: the error is returned each time instead, which is
/// what lets a caller report it.
static REGISTRY: OnceLock<Result<Registry, RegistryError>> = OnceLock::new();

/// The embedded registry.
///
/// # Errors
///
/// Returns [`Error::Registry`] if the embedded registry fails validation, which
/// `build.rs` is meant to make unreachable.
pub fn registry() -> Result<&'static Registry, Error> {
    match REGISTRY.get_or_init(|| Registry::parse(REGISTRY_TOML)) {
        Ok(registry) => Ok(registry),
        // RegistryError is not Clone, and a &'static reference to it cannot be
        // manufactured from the error value itself. The message is reconstructed
        // instead, which is why it is a method rather than a derived Debug.
        Err(error) => Err(Error::Registry {
            message: error.to_string(),
        }),
    }
}

/// Why a registry cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// The registry broke one or more rules. Every problem is reported.
    #[error("the model registry is not valid:\n  - {}", problems.join("\n  - "))]
    Invalid {
        /// One message per violated rule, in file order.
        problems: Vec<String>,
    },
    /// A `Registry` with no default, which `parse` cannot produce.
    #[error("the model registry has no default model")]
    NoDefault,
}

/// Every rule for one entry, each pushing its own problem.
///
/// A function rather than an inline block so that `parse` reads as a list of
/// rules and the detail lives underneath. The same shape is in `build.rs`.
fn check_entry(entry: &RegistryEntry, index: usize, problems: &mut Vec<String>) {
    let label = match entry.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => {
            problems.push(format!(
                "model #{index} has no `name`, so it cannot be selected with --use-model"
            ));
            format!("model #{index}")
        }
    };

    if let Some(name) = entry.name.as_deref() {
        if !is_kebab_case(name) {
            problems.push(format!(
                "`{name}` is not kebab-case; a name is typed on the command line, so \
                 lowercase words separated by hyphens is the only shape that survives a \
                 shell without quoting"
            ));
        }
    }

    match entry.url.as_deref() {
        None => problems.push(format!("{label} has no `url`")),
        Some(url) if !url.starts_with("https://") => problems.push(format!(
            "{label} has a `url` that is not HTTPS. A checksum only protects the \
             file if the channel is authenticated"
        )),
        Some(_) => {}
    }

    match entry.sha256.as_deref() {
        None => problems.push(format!(
            "{label} has no `sha256`. A model without a pinned hash cannot be \
             verified, and an unverifiable download is not something gcode will \
             perform"
        )),
        Some(hash) if hash.len() != SHA256_HEX_LEN => problems.push(format!(
            "{label} has a {}-character `sha256`, not {SHA256_HEX_LEN}",
            hash.len()
        )),
        Some(hash) if !is_lower_hex(hash) => problems.push(format!(
            "{label} has a `sha256` that is not lowercase hexadecimal"
        )),
        Some(_) => {}
    }

    match entry.size_bytes {
        None => problems.push(format!(
            "{label} has no `size_bytes`, so the user cannot be told how much they \
             are about to download"
        )),
        Some(0) => problems.push(format!("{label} has `size_bytes = 0`")),
        Some(_) => {}
    }

    if let Some(context) = entry.context_size {
        if !(MIN_CONTEXT_SIZE..=MAX_CONTEXT_SIZE).contains(&context) {
            problems.push(format!(
                "{label} has `context_size = {context}`, outside \
                 {MIN_CONTEXT_SIZE}..={MAX_CONTEXT_SIZE}"
            ));
        }
    }

    if entry.license.as_deref().unwrap_or_default().is_empty() {
        problems.push(format!(
            "{label} has no `license`, and gcode is required to be able to say what \
             licence a model is under"
        ));
    }
}

/// The on-disk shape.
///
/// Every field is optional so that a missing one becomes a named problem rather
/// than a serde error naming a type. Deserialising into required fields would
/// report the first failure in file order, and a contributor would fix one, run
/// again, and meet the next.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    #[serde(default, rename = "model")]
    model: Vec<RegistryEntry>,
}

/// The on-disk shape of one entry. See [`RegistryFile`] for why this is separate
/// from [`ModelEntry`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryEntry {
    name: Option<String>,
    description: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    default: Option<bool>,
    context_size: Option<u32>,
    recommended_threads: Option<u32>,
    license: Option<String>,
}

/// Lowercase ASCII alphanumerics, hyphens, and dots, starting and ending on an
/// alphanumeric.
///
/// Dots are allowed because real model names carry version numbers:
/// `qwen2.5-0.5b-instruct` and `gemma-2-2b` are what a user would type, and
/// `docs/models.md` lists them. A rule that rejected the names of the models
/// gcode intends to ship would be a rule that gets weakened at the first
/// conflict.
fn is_kebab_case(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'.')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.starts_with('.')
        && !name.ends_with('.')
}

/// Whether `text` is lowercase hexadecimal.
fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::{ModelEntry, Registry, RegistryError, MAX_CONTEXT_SIZE, MIN_CONTEXT_SIZE};

    /// A hash that is syntactically valid. Never a real file's hash: a fixture
    /// that verified nothing should not look like it did.
    ///
    /// Contains letters on purpose. An all-zero hash uppercases to itself, so a
    /// test that uppercases this one would pass for the wrong reason.
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// A registry with one valid default entry, as a starting point for edits.
    fn valid() -> String {
        format!(
            r#"
[[model]]
name = "test-model"
url = "https://example.invalid/test-model.gguf"
sha256 = "{HASH}"
size_bytes = 1000
default = true
context_size = 4096
license = "MIT"
"#
        )
    }

    fn problems(text: &str) -> String {
        match Registry::parse(text) {
            Ok(_) => panic!("expected the registry to be refused:\n{text}"),
            Err(RegistryError::Invalid { problems }) => problems.join("\n"),
            Err(other) => panic!("wrong error kind: {other}"),
        }
    }

    // ── the required rules ──────────────────────────────────────────────────

    #[test]
    fn a_valid_registry_parses() {
        let registry = Registry::parse(&valid()).unwrap();
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.default_name().unwrap(), "test-model");
        assert_eq!(registry.get("test-model").unwrap().size_bytes, 1000);
    }

    #[test]
    fn two_defaults_are_refused() {
        // Required by the roadmap, and the check that would otherwise fail only
        // when a user asked for a model and got whichever came first.
        let text = format!(
            "{}\n[[model]]\nname = \"other\"\nurl = \"https://example.invalid/other.gguf\"\n\
             sha256 = \"{HASH}\"\nsize_bytes = 2000\ndefault = true\ncontext_size = 4096\n\
             license = \"MIT\"\n",
            valid()
        );
        let message = problems(&text);
        assert!(
            message.contains("2 models set `default = true`"),
            "{message}"
        );
    }

    #[test]
    fn no_default_is_refused() {
        let text = valid().replace("default = true", "default = false");
        let message = problems(&text);
        assert!(
            message.contains("no model sets `default = true`"),
            "{message}"
        );
    }

    #[test]
    fn a_missing_hash_is_refused() {
        // Required by the roadmap. This is the check that turns the registry from
        // a list of suggestions into a list of things that can be verified.
        let text = valid().replace(&format!("sha256 = \"{HASH}\"\n"), "");
        let message = problems(&text);
        assert!(message.contains("no `sha256`"), "{message}");
    }

    #[test]
    fn a_short_hash_is_refused() {
        let text = valid().replace(HASH, &HASH[..32]);
        let message = problems(&text);
        assert!(message.contains("32-character `sha256`"), "{message}");
    }

    #[test]
    fn an_uppercase_hash_is_refused() {
        // Length is right, bytes are not hex in the case this code accepts.
        // Refusing is better than normalising silently, because a registry that
        // is hand-edited should say so rather than quietly differ from its
        // checksum in the downloader.
        let text = valid().replace(HASH, &HASH.to_uppercase());
        let message = problems(&text);
        assert!(message.contains("not lowercase hexadecimal"), "{message}");
    }

    #[test]
    fn a_non_https_url_is_refused() {
        let text = valid().replace("https://example.invalid", "http://example.invalid");
        let message = problems(&text);
        assert!(message.contains("not HTTPS"), "{message}");
    }

    #[test]
    fn a_zero_size_is_refused() {
        let text = valid().replace("size_bytes = 1000", "size_bytes = 0");
        let message = problems(&text);
        assert!(message.contains("size_bytes = 0"), "{message}");
    }

    #[test]
    fn a_missing_size_is_refused() {
        let text = valid().replace("size_bytes = 1000\n", "");
        let message = problems(&text);
        assert!(message.contains("no `size_bytes`"), "{message}");
    }

    #[test]
    fn an_unknown_key_is_refused() {
        // Consistent with the config layer: a registry with a typo in a checksum
        // field would otherwise look validated and protect nothing.
        let text = valid().replace("license = \"MIT\"", "licence = \"MIT\"");
        assert!(!problems(&text).is_empty());
    }

    #[test]
    fn an_empty_registry_is_refused() {
        let message = problems("model = []\n");
        assert!(message.contains("no models"), "{message}");
    }

    #[test]
    fn an_unparseable_registry_is_refused() {
        let message = problems("[[model]\nname = ");
        assert!(message.contains("cannot be parsed"), "{message}");
    }

    #[test]
    fn a_duplicate_name_is_refused() {
        let text = format!(
            "{}\n{}",
            valid(),
            valid().replace("size_bytes = 1000", "size_bytes = 2000")
        );
        let message = problems(&text);
        assert!(message.contains("appears more than once"), "{message}");
    }

    // ── context window bounds ───────────────────────────────────────────────

    #[test]
    fn the_context_window_bounds_match_the_config_layer() {
        // The two modules reject the same values. A registry that promised a
        // window the config layer refuses would make `--context-size` clamp into
        // a value the model cannot hold.
        for good in [MIN_CONTEXT_SIZE, 4096, MAX_CONTEXT_SIZE] {
            let text = valid().replace("context_size = 4096", &format!("context_size = {good}"));
            assert!(Registry::parse(&text).is_ok(), "rejected {good}");
        }
        for bad in [0, MIN_CONTEXT_SIZE - 1, MAX_CONTEXT_SIZE + 1] {
            let text = valid().replace("context_size = 4096", &format!("context_size = {bad}"));
            assert!(Registry::parse(&text).is_err(), "accepted {bad}");
        }
    }

    // ── names ───────────────────────────────────────────────────────────────

    #[test]
    fn a_name_must_be_kebab_case() {
        for good in ["qwen2.5-0.5b-instruct", "a", "model-2", "x1-y2"] {
            let text = valid().replace("test-model", good);
            assert!(Registry::parse(&text).is_ok(), "rejected {good}");
        }
        for bad in [
            "Test-Model",
            "test_model",
            "-leading",
            "trailing-",
            "with space",
        ] {
            let text = valid().replace("test-model", bad);
            assert!(Registry::parse(&text).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn a_name_is_looked_up_exactly() {
        // A fuzzy match would mean two names could resolve to one model and the
        // result would depend on typing order.
        let registry = Registry::parse(&valid()).unwrap();
        assert!(registry.get("test-model").is_some());
        assert!(registry.get("Test-Model").is_none());
        assert!(registry.get("test").is_none());
        assert!(registry.get("test-model ").is_none());
    }

    // ── display ─────────────────────────────────────────────────────────────

    #[test]
    fn an_entry_displays_as_its_name() {
        let entry = ModelEntry {
            name: "test-model".to_owned(),
            description: Some("A test model".to_owned()),
            url: "https://example.invalid/m.gguf".to_owned(),
            sha256: HASH.to_owned(),
            size_bytes: 417_386_752,
            default: true,
            context_size: 4096,
            recommended_threads: Some(4),
            license: "MIT".to_owned(),
        };
        assert_eq!(entry.to_string(), "test-model");
    }

    #[test]
    fn sizes_are_shown_in_binary_units() {
        let entry = |bytes| ModelEntry {
            name: "m".to_owned(),
            description: None,
            url: "https://example.invalid/m.gguf".to_owned(),
            sha256: HASH.to_owned(),
            size_bytes: bytes,
            default: true,
            context_size: 4096,
            recommended_threads: None,
            license: "MIT".to_owned(),
        };
        // Binary units, because that is how model files are sold. A "400 MB"
        // file that turns out to be 429 MB is how a user decides they were lied
        // to.
        assert_eq!(entry(512).size_human(), "512 B");
        assert_eq!(entry(417_386_752).size_human(), "398 MiB");
        assert_eq!(entry(1_099_511_627_776).size_human(), "1 TiB");
    }

    // ── the embedded registry ───────────────────────────────────────────────

    #[test]
    fn the_embedded_registry_is_valid() {
        // Proves the shipped file passes the same rules the tests above check.
        // If this fails, `build.rs` should have failed first; the point is that
        // a bypassed build check still cannot ship a broken registry.
        let registry = super::registry().expect("the embedded registry must be valid");
        assert!(
            !registry.entries().is_empty(),
            "the shipped registry declares no models"
        );
        let default = registry.default_entry().expect("one default is required");
        assert!(
            registry.get(&default.name).is_some(),
            "the default must be reachable by name"
        );
    }

    #[test]
    fn the_embedded_default_has_a_sha256() {
        let registry = super::registry().unwrap();
        let default = registry.default_entry().unwrap();
        assert_eq!(
            default.sha256.len(),
            super::SHA256_HEX_LEN,
            "the shipped default model has a malformed checksum"
        );
    }
}
