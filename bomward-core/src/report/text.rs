// SPDX-License-Identifier: AGPL-3.0-only

//! Shared words for the three report formats.
//!
//! A [`Phrase`] is either report prose or one untrusted SBOM string.
//! Formats escape values. They do not rewrite the prose.

use crate::diff::Direction;
use crate::model::{ComponentKey, DependencyRole, VersionId};
use crate::purl::canonical_purl;

/// One piece of a sentence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Phrase {
    /// English written by the report.
    Text(String),
    /// A string that came from a SBOM.
    Value(String),
}

/// `1 denial` or `2 denials`. Zero uses the plural.
pub(crate) fn counted(n: u64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// How a [`DependencyRole`] is spelled in prose and JSON.
pub(crate) fn role_id(role: DependencyRole) -> &'static str {
    match role {
        DependencyRole::Root => "root",
        DependencyRole::Direct => "direct",
        DependencyRole::Transitive => "transitive",
        DependencyRole::Unknown => "unknown",
    }
}

/// How a [`Direction`] is spelled. `Unknown` is two words.
pub(crate) fn direction_text(direction: Direction) -> &'static str {
    match direction {
        Direction::Upgrade => "upgrade",
        Direction::Downgrade => "downgrade",
        Direction::Unknown => "direction unknown",
    }
}

/// Readable package identity: ecosystem, decoded name, version, extras.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackageLabel {
    /// Purl type. Absent for a group/name key.
    pub(crate) ecosystem: Option<String>,
    /// Namespace plus name, or `group/name` when there is no purl.
    pub(crate) name: String,
    /// Version, with an rpm epoch written as `{epoch}:{version}`.
    pub(crate) version: String,
    /// Qualifiers that stayed in the key, in name order.
    pub(crate) qualifiers: Vec<(String, String)>,
    /// Purl subpath, when the key has one.
    pub(crate) subpath: Option<String>,
}

pub(crate) fn package_label(key: &ComponentKey, version: &VersionId) -> PackageLabel {
    let version_text = match &version.epoch {
        Some(epoch) => format!("{epoch}:{}", version.version),
        None => version.version.clone(),
    };
    match key {
        ComponentKey::Purl {
            ty,
            namespace,
            name,
            qualifiers,
            subpath,
        } => {
            let shown = match namespace {
                Some(namespace) => format!("{namespace}/{name}"),
                None => name.clone(),
            };
            PackageLabel {
                ecosystem: Some(ty.clone()),
                name: shown,
                version: version_text,
                qualifiers: qualifiers
                    .iter()
                    .map(|(qualifier, value)| (qualifier.clone(), value.clone()))
                    .collect(),
                subpath: subpath.clone(),
            }
        }
        ComponentKey::Name { group, name } => {
            let shown = if group.is_empty() {
                name.clone()
            } else {
                format!("{group}/{name}")
            };
            PackageLabel {
                ecosystem: None,
                name: shown,
                version: version_text,
                qualifiers: Vec::new(),
                subpath: None,
            }
        }
    }
}

/// Identity fields stored on a JSON unit. Strings are untruncated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Identity {
    pub(crate) purl: Option<String>,
    pub(crate) ecosystem: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) qualifiers: Vec<(String, String)>,
    pub(crate) subpath: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) epoch: Option<String>,
}

pub(crate) fn identity(key: &ComponentKey, version: Option<&VersionId>) -> Identity {
    let (version, epoch) = version.map_or((None, None), |version| {
        (Some(version.version.clone()), version.epoch.clone())
    });
    match key {
        ComponentKey::Purl {
            ty,
            namespace,
            name,
            qualifiers,
            subpath,
        } => Identity {
            purl: canonical_purl(key),
            ecosystem: Some(ty.clone()),
            namespace: namespace.clone(),
            name: Some(name.clone()),
            group: None,
            qualifiers: qualifiers
                .iter()
                .map(|(qualifier, value)| (qualifier.clone(), value.clone()))
                .collect(),
            subpath: subpath.clone(),
            version,
            epoch,
        },
        ComponentKey::Name { group, name } => Identity {
            purl: None,
            ecosystem: None,
            namespace: None,
            name: Some(name.clone()),
            group: Some(group.clone()),
            qualifiers: Vec::new(),
            subpath: None,
            version,
            epoch,
        },
    }
}

/// Join values with a comma. An empty list is the word `none`.
pub(crate) fn value_list(prefix: &str, values: &[String]) -> Vec<Phrase> {
    let mut phrases = vec![Phrase::Text(prefix.to_owned())];
    if values.is_empty() {
        phrases.push(Phrase::Text("none".to_owned()));
        return phrases;
    }
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            phrases.push(Phrase::Text(", ".to_owned()));
        }
        phrases.push(Phrase::Value(value.clone()));
    }
    phrases
}

#[cfg(test)]
mod tests {
    use super::counted;

    #[test]
    fn counts_use_the_singular_only_for_one() {
        assert_eq!(counted(0, "denial", "denials"), "0 denials");
        assert_eq!(counted(1, "denial", "denials"), "1 denial");
        assert_eq!(counted(2, "denial", "denials"), "2 denials");
        assert_eq!(counted(0, "warning", "warnings"), "0 warnings");
        assert_eq!(counted(1, "warning", "warnings"), "1 warning");
        assert_eq!(counted(2, "warning", "warnings"), "2 warnings");
        assert_eq!(counted(0, "allowed", "allowed"), "0 allowed");
        assert_eq!(counted(1, "allowed", "allowed"), "1 allowed");
        assert_eq!(counted(2, "allowed", "allowed"), "2 allowed");
        assert_eq!(
            counted(0, "allowed entry", "allowed entries"),
            "0 allowed entries"
        );
        assert_eq!(
            counted(1, "allowed entry", "allowed entries"),
            "1 allowed entry"
        );
        assert_eq!(
            counted(2, "allowed entry", "allowed entries"),
            "2 allowed entries"
        );
    }
}
