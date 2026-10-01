//! Compiles the model registry into a hard guarantee.
//!
//! # Why this exists
//!
//! The registry is a list of URLs, checksums, and sizes that decides what gcode
//! will download and, more importantly, what it will *trust*. A registry with a
//! missing checksum, or with two entries both claiming to be the default, is not
//! a bug to be discovered on a user's machine: it is either a broken download or
//! a way to get an unexpected file fetched and hash-matched. Neither should be
//! reachable by shipping.
//!
//! ADR 0010 therefore makes these rules a build failure rather than a runtime
//! error, and this script is that build failure.
//!
//! # What is checked here, and what is not
//!
//! Everything that would make a shipped binary unsafe or unusable:
//!
//! - the file exists and parses
//! - every entry has a name, an HTTPS URL, a size, a licence
//! - every `sha256` is 64 lowercase hex characters, and not a placeholder
//! - exactly one entry sets `default = true`
//! - no two entries share a name
//! - no unrecognised key
//!
//! Two rules are deliberately left to [`src/model/registry.rs`], which checks them
//! again at run time: the context-window bounds, and the shape of a name beyond
//! kebab-case. Neither can make a download unsafe, and keeping them out of here
//! keeps this script small enough to read in one sitting. The supply-chain rules
//! are the ones that must never reach a user.
//!
//! Both sides use `toml`, so a rule written here and a rule there cannot
//! disagree about what the file means.
//!
//! # Legibility
//!
//! `include_str!` fails with "file not found" and says nothing about which
//! registry was missing. This script checks existence first and names the path.
//!
//! Every problem is reported, not just the first, because a contributor with
//! three wrong fields should learn about all three from one build.

use std::path::Path;
use std::process::ExitCode;

/// The registry, relative to the manifest. `include_str!` in the library uses the
/// same path; if one moves, the other must move with it.
const REGISTRY: &str = "models/registry.toml";

/// Length of a SHA-256 digest in hex characters.
const SHA256_HEX_LEN: usize = 64;

/// The SHA-256 of the empty string, and of nothing else.
///
/// It is the value every "put a plausible-looking hash here" placeholder reaches
/// for, because it is the first SHA-256 anyone ever typed. A real model file
/// cannot have it. Naming it here means a placeholder cannot ship by accident.
const PLACEHOLDER_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Where this is going. `cargo:key=value` rather than `cargo::key=value`, because
/// the double-colon form needs Rust 1.77 and ADR 0002 sets the floor at 1.75.
/// Cargo still accepts the single-colon form and does not warn about it.
fn main() -> ExitCode {
    println!("cargo:rerun-if-changed={REGISTRY}");
    println!("cargo:rerun-if-changed=build.rs");

    // `PROFILE` is "debug" for a development build and "release" for one that
    // will be shipped. The placeholder check is release-only so a contributor can
    // build, test, and run the tool before the real model data lands, while no
    // artefact a user could install is ever produced from an unfilled registry.
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");

    match check(Path::new(REGISTRY), release) {
        Ok(()) => ExitCode::SUCCESS,
        Err(problems) => {
            // stderr rather than `cargo:warning`, because this is a failure and a
            // warning is skippable with `--cap-lints`. Cargo prints "build script
            // failed" on top of this, so the message only has to say what is
            // wrong with the file.
            eprintln!(
                "the model registry at {REGISTRY} is not usable:\n\n{}\n\n\
                 Fix the file rather than relaxing the rule. Every check here exists \
                 because breaking it is either a broken download or an unverified \
                 file that gcode would go on to treat as trusted.",
                problems
                    .iter()
                    .map(|problem| format!("  - {problem}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            ExitCode::FAILURE
        }
    }
}

/// Runs every build-time rule, collecting all problems.
fn check(path: &Path, release: bool) -> Result<(), Vec<String>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Err(vec![
            "cannot be read. It is the source of truth for every model gcode will \
             download (ADR 0010) and is embedded with include_str!, so the binary \
             cannot be built without it"
                .to_owned(),
        ]);
    };

    let registry: RegistryFile = match toml::from_str(&text) {
        Ok(registry) => registry,
        Err(error) => return Err(vec![format!("cannot be parsed as TOML: {error}")]),
    };

    if registry.model.is_empty() {
        return Err(vec!["declares no models at all".to_owned()]);
    }

    let mut problems = Vec::new();
    let mut defaults: Vec<&str> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for (index, entry) in registry.model.iter().enumerate() {
        check_entry(entry, index, release, &mut problems);

        if let Some(name) = entry.name.as_deref() {
            if seen.contains(&name) {
                problems.push(format!("`{name}` appears more than once"));
            }
            seen.push(name);
        }
        if entry.default == Some(true) {
            defaults.push(entry.name.as_deref().unwrap_or("unnamed"));
        }
    }

    match defaults.len() {
        1 => {}
        0 => problems.push(
            "no model sets `default = true`. gcode must be able to download something \
             without being told which model"
                .to_owned(),
        ),
        _ => problems.push(format!(
            "{} models set `default = true` ({}). Exactly one must",
            defaults.len(),
            defaults.join(", ")
        )),
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// The rules for one entry, each pushing its own problem.
fn check_entry(entry: &RegistryEntry, index: usize, release: bool, problems: &mut Vec<String>) {
    let label = match entry.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => {
            problems.push(format!(
                "model #{index} has no `name`, so it cannot be selected with --use-model"
            ));
            format!("model #{index}")
        }
    };

    match entry.url.as_deref() {
        None => problems.push(format!("{label} has no `url`")),
        Some(url) if !url.starts_with("https://") => problems.push(format!(
            "{label} has a `url` that is not HTTPS. A checksum only protects the file \
             if the channel is authenticated"
        )),
        Some(_) => {}
    }

    match entry.sha256.as_deref() {
        None => problems.push(format!(
            "{label} has no `sha256`. A model without a pinned hash cannot be verified, \
             and an unverifiable download is not something gcode will perform"
        )),
        Some(hash) if hash.len() != SHA256_HEX_LEN => problems.push(format!(
            "{label} has a {}-character `sha256`, not {SHA256_HEX_LEN}",
            hash.len()
        )),
        Some(hash) if !is_lower_hex(hash) => problems.push(format!(
            "{label} has a `sha256` that is not lowercase hexadecimal"
        )),
        Some(hash) if hash == PLACEHOLDER_SHA256 && release => problems.push(format!(
            "{label} still has the placeholder checksum, which is the SHA-256 of the \
             empty string and therefore of no model. Replace it with a real model \
             whose checksum you have verified by downloading the file once"
        )),
        Some(_) => {}
    }

    match entry.size_bytes {
        None => problems.push(format!(
            "{label} has no `size_bytes`, so the user cannot be told how much they are \
             about to download"
        )),
        Some(0) => problems.push(format!("{label} has `size_bytes = 0`")),
        Some(_) => {}
    }

    if entry.license.as_deref().unwrap_or_default().is_empty() {
        problems.push(format!(
            "{label} has no `license`, and gcode is required to be able to say what \
             licence a model is under"
        ));
    }
}

/// Whether `text` is lowercase hexadecimal.
fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The shape of `models/registry.toml`.
///
/// Every field is optional so that a missing one becomes a named problem rather
/// than a serde error the contributor has to decode. Deserialising into required
/// fields would report the first failure in file order, so a contributor would
/// fix one, build again, and meet the next.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    #[serde(default, rename = "model")]
    model: Vec<RegistryEntry>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryEntry {
    name: Option<String>,
    // Present so that `deny_unknown_fields` accepts them. This script has no use
    // for a description, a thread hint, or a licence string; the library reads
    // all three.
    #[allow(dead_code)]
    description: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    default: Option<bool>,
    #[allow(dead_code)]
    context_size: Option<u32>,
    #[allow(dead_code)]
    recommended_threads: Option<u32>,
    license: Option<String>,
}
