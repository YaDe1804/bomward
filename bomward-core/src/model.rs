// SPDX-License-Identifier: AGPL-3.0-only

//! Normalized components and their identities.
//!
//! Identity is the Package URL without its version and without
//! version-dependent qualifiers ([ADR 0005](https://github.com/YaDe1804/bomward/blob/main/docs/adr/0005-component-identity.md),
//! ADR 0009). Each key holds a set of versions. A missing or invalid Package
//! URL uses `group` plus `name`. An invalid one also warns. `bom-ref` is not
//! a field of this model.
//!
//! [`Document::to_canonical_json`] encodes the model for tests. That encoding
//! is deterministic and it is **not** a public contract: field names, key
//! order, and the JSON shape may change. It is not a `CycloneDX` document and
//! it is not the report.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

/// One `CycloneDX` document after ingest.
///
/// Components are keyed by [`ComponentKey`]. The dependency graph uses those
/// same keys. `bom-ref` is not stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Components in key order.
    pub components: BTreeMap<ComponentKey, Component>,
    /// Edges declared by `dependencies`, after dangling refs are dropped.
    ///
    /// A key is present only when it has at least one resolved target.
    /// Cycles are stored as ordinary edges.
    pub dependencies: BTreeMap<ComponentKey, BTreeSet<ComponentKey>>,
    /// Warnings, sorted by key and then by [`WarningKind`].
    pub warnings: Vec<Warning>,
}

impl Document {
    /// JSON encoding of this document for golden tests.
    ///
    /// The same model always produces the same bytes. The shape is internal.
    /// See the module docs.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let components = self
            .components
            .iter()
            .map(|(key, component)| CanonicalComponent {
                key,
                versions: component
                    .versions
                    .iter()
                    .map(|(version, attributes)| CanonicalVersion {
                        version: &version.version,
                        epoch: version.epoch.as_deref(),
                        attributes,
                    })
                    .collect(),
                role: component.role,
            })
            .collect::<Vec<_>>();
        let dependencies = self
            .dependencies
            .iter()
            .map(|(from, to)| CanonicalEdges {
                from,
                to: to.iter().collect(),
            })
            .collect::<Vec<_>>();
        CanonicalDocument {
            components,
            dependencies,
            warnings: &self.warnings,
        }
        .serialize(serializer)
    }
}

#[derive(Serialize)]
struct CanonicalDocument<'a> {
    components: Vec<CanonicalComponent<'a>>,
    dependencies: Vec<CanonicalEdges<'a>>,
    warnings: &'a [Warning],
}

#[derive(Serialize)]
struct CanonicalComponent<'a> {
    key: &'a ComponentKey,
    versions: Vec<CanonicalVersion<'a>>,
    role: DependencyRole,
}

#[derive(Serialize)]
struct CanonicalVersion<'a> {
    version: &'a str,
    epoch: Option<&'a str>,
    attributes: &'a VersionAttributes,
}

#[derive(Serialize)]
struct CanonicalEdges<'a> {
    from: &'a ComponentKey,
    to: Vec<&'a ComponentKey>,
}

/// Identity of a component inside one review.
///
/// Ordering is the derived order of the fields. It is total and it does not
/// depend on the order of the input file.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKey {
    /// A purl that parsed and passed ADR 0009.
    Purl {
        /// Package type, already lowercased.
        ty: String,
        /// Namespace, when the type has one.
        namespace: Option<String>,
        /// Package name, after the type's case rule.
        name: String,
        /// Qualifiers that stay in the key, sorted by qualifier name.
        qualifiers: BTreeMap<String, String>,
        /// Subpath, when the purl has one.
        subpath: Option<String>,
    },
    /// Fallback when the purl is missing or invalid: `CycloneDX` `group` and `name`.
    Name {
        /// `group`, or the empty string when the field is absent.
        group: String,
        /// `name`.
        name: String,
    },
}

/// One component and the versions observed for its key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Component {
    /// Versions in `(version, epoch)` order.
    pub versions: BTreeMap<VersionId, VersionAttributes>,
    /// Where this component sits in the document's graph.
    pub role: DependencyRole,
}

/// The version half of a version-set element.
///
/// `epoch` is the rpm qualifier of that name. It is absent for every other
/// type, and for an rpm that did not set it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct VersionId {
    /// Package version. Empty when the document names none.
    pub version: String,
    /// rpm epoch, when the purl carried that qualifier.
    pub epoch: Option<String>,
}

/// Attributes stored on one version. Every field is a sorted set.
///
/// Two occurrences of the same key and version are united. A set that grows
/// on the second occurrence produces a [`WarningKind::ConflictingAttributes`]
/// warning. See ADR 0010.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct VersionAttributes {
    /// `license.id`, `license.name`, and `expression` strings.
    pub licenses: BTreeSet<String>,
    /// `hashes` entries that have both `alg` and `content`.
    pub hashes: BTreeSet<ContentHash>,
    /// `supplier.name` values.
    pub suppliers: BTreeSet<String>,
    /// `CycloneDX` `scope` strings, not the graph role.
    pub scopes: BTreeSet<String>,
    /// `CycloneDX` component `type` strings.
    pub types: BTreeSet<String>,
    /// `checksum` qualifiers removed from the key.
    pub checksums: BTreeSet<String>,
    /// `download_url` qualifiers removed from the key.
    pub download_urls: BTreeSet<String>,
    /// `vcs_url` qualifiers removed from the key.
    pub vcs_urls: BTreeSet<String>,
    /// `file_name` qualifiers removed from the key.
    pub file_names: BTreeSet<String>,
    /// `vers` qualifiers removed from the key.
    pub vers: BTreeSet<String>,
}

impl VersionAttributes {
    pub(crate) fn absorb(&mut self, extra: &Self) -> bool {
        extend_set(&mut self.licenses, &extra.licenses)
            | extend_set(&mut self.hashes, &extra.hashes)
            | extend_set(&mut self.suppliers, &extra.suppliers)
            | extend_set(&mut self.scopes, &extra.scopes)
            | extend_set(&mut self.types, &extra.types)
            | extend_set(&mut self.checksums, &extra.checksums)
            | extend_set(&mut self.download_urls, &extra.download_urls)
            | extend_set(&mut self.vcs_urls, &extra.vcs_urls)
            | extend_set(&mut self.file_names, &extra.file_names)
            | extend_set(&mut self.vers, &extra.vers)
    }
}

fn extend_set<T>(target: &mut BTreeSet<T>, extra: &BTreeSet<T>) -> bool
where
    T: Clone + Ord,
{
    let before = target.len();
    target.extend(extra.iter().cloned());
    target.len() > before
}

/// One hash from a component, stored as the document wrote it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct ContentHash {
    /// Algorithm name, for example `SHA-256`.
    pub alg: String,
    /// Hash contents.
    pub content: String,
}

/// How a component relates to the root of this document.
///
/// AP2 decides how a report prints these. Ingest only records them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyRole {
    /// `metadata.component`.
    Root,
    /// Listed in the root's `dependsOn`.
    Direct,
    /// Reachable from a direct dependency. The walk uses a visited set.
    Transitive,
    /// Not the root and not reachable, or the document has no graph.
    #[default]
    Unknown,
}

/// One counted warning. Warnings do not change the exit code.
///
/// Sorting is by [`Warning::key`] (a missing key first) and then by
/// [`Warning::kind`].
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct Warning {
    /// Component the warning is about, when it has one.
    pub key: Option<ComponentKey>,
    /// Stable kind.
    pub kind: WarningKind,
}

/// Warning kinds from ADR 0010, in sort order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningKind {
    /// A purl was present and did not parse. Id: `invalid-purl`.
    InvalidPurl,
    /// A `ref` or `dependsOn` matched no component. Id: `orphan-ref`.
    OrphanRef,
    /// Two components declared the same `bom-ref`. Id: `duplicate-bom-ref`.
    DuplicateBomRef,
    /// No root component, or no `dependencies` field. Id: `role-unknown`.
    ///
    /// This is the graph role. It is not the `CycloneDX` `scope` field.
    RoleUnknown,
    /// The same key and version carried different attributes.
    /// Id: `conflicting-attributes`.
    ConflictingAttributes,
    /// The component `version` and the purl version differ. Id: `version-mismatch`.
    ///
    /// The stored version is the purl version.
    VersionMismatch,
}

impl WarningKind {
    /// Stable identifier for reports and tests.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::InvalidPurl => "invalid-purl",
            Self::OrphanRef => "orphan-ref",
            Self::DuplicateBomRef => "duplicate-bom-ref",
            Self::RoleUnknown => "role-unknown",
            Self::ConflictingAttributes => "conflicting-attributes",
            Self::VersionMismatch => "version-mismatch",
        }
    }

    /// Human-readable text. The invalid-purl sentence is fixed by ADR 0005.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidPurl => "invalid purl; fell back to group and name",
            Self::OrphanRef => "dependency ref does not match a component",
            Self::DuplicateBomRef => "duplicate bom-ref; kept the first component",
            Self::RoleUnknown => "component role is unknown",
            Self::ConflictingAttributes => "same version carried different attributes",
            Self::VersionMismatch => "component version differs from purl version",
        }
    }
}
