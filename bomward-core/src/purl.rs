// SPDX-License-Identifier: AGPL-3.0-only

//! Package URL parsing for the component key.
//!
//! `packageurl` 0.7.1 parses and applies its type table. This module removes
//! the version-dependent qualifiers and rejects a repeated qualifier key,
//! which that crate keeps as the last value. ADR 0009 records both choices.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use packageurl::PackageUrl;

use crate::model::ComponentKey;

/// A purl split into the key and the version attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedPurl {
    pub(crate) key: ComponentKey,
    pub(crate) version: Option<String>,
    pub(crate) epoch: Option<String>,
    pub(crate) checksums: BTreeSet<String>,
    pub(crate) download_urls: BTreeSet<String>,
    pub(crate) vcs_urls: BTreeSet<String>,
    pub(crate) file_names: BTreeSet<String>,
    pub(crate) vers: BTreeSet<String>,
}

/// Parse `raw` into a key, or `None` when ADR 0005 calls the purl invalid.
pub(crate) fn parse_purl(raw: &str) -> Option<ParsedPurl> {
    if qualifier_keys_repeat(raw) {
        return None;
    }
    let purl = PackageUrl::from_str(raw).ok()?;
    let mut qualifiers = BTreeMap::new();
    let mut checksums = BTreeSet::new();
    let mut download_urls = BTreeSet::new();
    let mut vcs_urls = BTreeSet::new();
    let mut file_names = BTreeSet::new();
    let mut vers = BTreeSet::new();
    let mut epoch = None;
    for (key, value) in purl.qualifiers() {
        let value = value.as_ref().to_owned();
        match key.as_ref() {
            "checksum" => {
                checksums.insert(value);
            }
            "download_url" => {
                download_urls.insert(value);
            }
            "vcs_url" => {
                vcs_urls.insert(value);
            }
            "file_name" => {
                file_names.insert(value);
            }
            "vers" => {
                vers.insert(value);
            }
            "epoch" => {
                epoch = Some(value);
            }
            other => {
                qualifiers.insert(other.to_owned(), value);
            }
        }
    }
    Some(ParsedPurl {
        key: ComponentKey::Purl {
            ty: purl.ty().to_owned(),
            namespace: purl.namespace().map(str::to_owned),
            name: purl.name().to_owned(),
            qualifiers,
            subpath: purl.subpath().map(str::to_owned),
        },
        version: purl.version().map(str::to_owned),
        epoch,
        checksums,
        download_urls,
        vcs_urls,
        file_names,
        vers,
    })
}

/// ADR 0005: a repeated qualifier key makes the purl invalid.
///
/// `packageurl` 0.7.1 stores qualifiers in a `HashMap`, so it would keep the
/// last value. Empty values are ignored here, matching the crate, which drops
/// them before the key is built.
fn qualifier_keys_repeat(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("pkg:") else {
        return false;
    };
    let rest = match rest.rfind('#') {
        Some(index) => rest.get(..index).unwrap_or(rest),
        None => rest,
    };
    let Some(start) = rest.rfind('?') else {
        return false;
    };
    let qualifiers = rest.get(start + 1..).unwrap_or("");
    let mut seen = BTreeSet::new();
    for pair in qualifiers.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if value.is_empty() {
            continue;
        }
        if !seen.insert(key.to_ascii_lowercase()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{parse_purl, qualifier_keys_repeat};
    use crate::model::ComponentKey;
    use packageurl::PackageUrl;
    use std::str::FromStr;

    fn purl_key(raw: &str) -> Result<ComponentKey, String> {
        parse_purl(raw)
            .map(|parsed| parsed.key)
            .ok_or_else(|| format!("expected a valid purl: {raw}"))
    }

    fn canonical(key: &ComponentKey) -> Result<String, String> {
        let ComponentKey::Purl {
            ty,
            namespace,
            name,
            qualifiers,
            subpath,
        } = key
        else {
            return Err("name key".to_owned());
        };
        let mut built =
            PackageUrl::new(ty.as_str(), name.as_str()).map_err(|error| error.to_string())?;
        if let Some(namespace) = namespace {
            built
                .with_namespace(namespace.as_str())
                .map_err(|error| error.to_string())?;
        }
        for (qualifier, value) in qualifiers {
            built
                .add_qualifier(qualifier.as_str(), value.as_str())
                .map_err(|error| error.to_string())?;
        }
        if let Some(subpath) = subpath {
            built
                .with_subpath(subpath.as_str())
                .map_err(|error| error.to_string())?;
        }
        Ok(built.to_string())
    }

    #[test]
    fn normalization_is_idempotent() -> Result<(), String> {
        let samples = [
            "pkg:npm/%40scope/Left-Pad@1.2.3",
            "pkg:generic/Foo@1?Arch=x86&distro=Alpine",
            "pkg:golang/GitHub.com/Org/Repo@v1.2.3",
            "pkg:rpm/Fedora/curl@7.71.1-1?epoch=1&arch=x86_64",
            "pkg:pypi/A_B@1.0",
        ];
        for sample in samples {
            let parsed = purl_key(sample)?;
            let once = canonical(&parsed)?;
            let again = purl_key(&once)?;
            if parsed != again {
                return Err(format!("{sample} -> {once}"));
            }
            let twice = canonical(&again)?;
            if once != twice {
                return Err(format!("{once} != {twice}"));
            }
        }
        Ok(())
    }

    #[test]
    fn qualifier_order_does_not_change_the_key() -> Result<(), String> {
        let left = purl_key("pkg:generic/a@1?arch=x&distro=alpine")?;
        let right = purl_key("pkg:generic/a@1?distro=alpine&arch=x")?;
        if left != right {
            return Err(format!("{left:?} != {right:?}"));
        }
        Ok(())
    }

    #[test]
    fn version_and_stripped_qualifiers_are_not_in_the_key() -> Result<(), String> {
        let plain = purl_key("pkg:npm/left-pad@1.0.0")?;
        let other = purl_key(
            "pkg:npm/left-pad@9.9.9?checksum=sha256:aa&download_url=https://example.invalid/a&vcs_url=git+https://example.invalid&file_name=a.tgz&vers=1.0&epoch=2&arch=x64",
        )?;
        let ComponentKey::Purl {
            qualifiers,
            name,
            ty,
            ..
        } = &other
        else {
            return Err("expected a purl key".to_owned());
        };
        if ty != "npm" || name != "left-pad" {
            return Err(format!("identity changed: {other:?}"));
        }
        if qualifiers.len() != 1 || qualifiers.get("arch").map(String::as_str) != Some("x64") {
            return Err(format!("qualifiers: {qualifiers:?}"));
        }
        let ComponentKey::Purl {
            qualifiers: plain_qualifiers,
            ..
        } = &plain
        else {
            return Err("expected a purl key".to_owned());
        };
        if plain_qualifiers.contains_key("arch") {
            return Err("plain key picked up arch".to_owned());
        }
        let with_epoch =
            parse_purl("pkg:rpm/fedora/curl@7.71.1-1?epoch=1&arch=x86_64").ok_or("rpm")?;
        let other_epoch =
            parse_purl("pkg:rpm/fedora/curl@7.71.1-1?epoch=2&arch=x86_64").ok_or("rpm 2")?;
        if with_epoch.key != other_epoch.key || with_epoch.epoch == other_epoch.epoch {
            return Err("epoch should change the version pair only".to_owned());
        }
        Ok(())
    }

    #[test]
    fn duplicate_qualifier_keys_are_invalid() {
        assert!(qualifier_keys_repeat("pkg:generic/a@1?arch=x&arch=y"));
        assert!(parse_purl("pkg:generic/a@1?arch=x&Arch=y").is_none());
        assert!(parse_purl("pkg:generic/a@1?arch=&distro=alpine").is_some());
    }

    #[test]
    fn pypi_underscore_becomes_a_dash_and_dot_stays() -> Result<(), String> {
        let dashed = purl_key("pkg:pypi/a_b@1")?;
        let already = purl_key("pkg:pypi/a-b@1")?;
        if dashed != already {
            return Err(format!("{dashed:?} != {already:?}"));
        }
        let dotted = purl_key("pkg:pypi/a.b@1")?;
        if dotted == dashed {
            return Err("dot was collapsed".to_owned());
        }
        Ok(())
    }

    #[test]
    fn crate_round_trip_is_available() {
        let parsed = PackageUrl::from_str("pkg:npm/%40scope/left-pad@1.0.0");
        assert!(parsed.is_ok());
    }
}
