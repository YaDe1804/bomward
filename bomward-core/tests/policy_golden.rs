// SPDX-License-Identifier: AGPL-3.0-only

//! Example policies against the Lockward pair.
//!
//! The JSON files are an internal encoding of the verdict, same as the diff
//! goldens. They are not a public contract.

use std::path::{Path, PathBuf};

use bomward_core::diff::diff;
use bomward_core::ingest;
use bomward_core::policy::{self, PolicyLimits};

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn verdict(policy: &Path, old: &Path, new: &Path) -> Result<String, String> {
    let loaded =
        policy::read_path(policy, &PolicyLimits::default()).map_err(|error| error.to_string())?;
    let old_document = ingest::read_path(old, &bomward_core::Limits::default())
        .map_err(|error| error.to_string())?;
    let new_document = ingest::read_path(new, &bomward_core::Limits::default())
        .map_err(|error| error.to_string())?;
    Ok(policy::evaluate(&loaded, &diff(&old_document, &new_document)).to_canonical_json())
}

fn matches_golden(policy: &str, pair: &str, old: &str, expected: &str) -> Result<(), String> {
    let actual = verdict(
        &root(&format!("../examples/policies/{policy}.yaml")),
        &root(old),
        &root("tests/fixtures/lockward-new.cdx.json"),
    )?;
    let expected = expected.trim_end_matches('\n');
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{policy} against {pair} diverged\n{actual}"))
    }
}

#[test]
fn recommended_matches_the_lockward_pair() -> Result<(), String> {
    matches_golden(
        "recommended",
        "lockward",
        "tests/fixtures/lockward-old.cdx.json",
        include_str!("fixtures/golden/policy-recommended-lockward.json"),
    )
}

#[test]
fn recommended_matches_the_old_format_pair() -> Result<(), String> {
    matches_golden(
        "recommended",
        "lockward-old-format",
        "tests/fixtures/lockward-old-format.cdx.json",
        include_str!("fixtures/golden/policy-recommended-lockward-old-format.json"),
    )
}

#[test]
fn strict_matches_the_lockward_pair() -> Result<(), String> {
    matches_golden(
        "strict",
        "lockward",
        "tests/fixtures/lockward-old.cdx.json",
        include_str!("fixtures/golden/policy-strict-lockward.json"),
    )
}

#[test]
fn strict_matches_the_old_format_pair() -> Result<(), String> {
    matches_golden(
        "strict",
        "lockward-old-format",
        "tests/fixtures/lockward-old-format.cdx.json",
        include_str!("fixtures/golden/policy-strict-lockward-old-format.json"),
    )
}

#[test]
fn licenses_match_the_lockward_pair() -> Result<(), String> {
    matches_golden(
        "licenses",
        "lockward",
        "tests/fixtures/lockward-old.cdx.json",
        include_str!("fixtures/golden/policy-licenses-lockward.json"),
    )
}

#[test]
fn licenses_match_the_old_format_pair() -> Result<(), String> {
    matches_golden(
        "licenses",
        "lockward-old-format",
        "tests/fixtures/lockward-old-format.cdx.json",
        include_str!("fixtures/golden/policy-licenses-lockward-old-format.json"),
    )
}
