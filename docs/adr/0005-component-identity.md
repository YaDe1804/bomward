# ADR 0005 — Component identity

- Status: accepted
- Date: 2026-11-11

## Context

A diff has to know when a component in the new SBOM is the same
component as one in the old SBOM. `bom-ref` looks like an identifier and
is not one across documents: tools regenerate it, and two generators do
not agree. Using it would report every rebuild as a remove plus an add.

The same component also has many versions. A second version in one SBOM
is a set membership change, not a broken document. Package URLs in the
wild are sometimes garbage; rejecting the whole file on one bad `purl`
hides every other change in that review.

Checked against the Package URL specification on 2026-11-11:

- ECMA-427, 1st edition, published 2025-12-10. Syntax, percent-encoding
  (RFC 3986), and canonical form.
- The recommended qualifiers in that standard, also listed at
  <https://www.packageurl.org/docs/purl/common-qualifiers>:
  `checksum`, `download_url`, `file_name`, `repository_url`, `vcs_url`,
  `vers`.
- The `rpm` type definition still stores `epoch` as a qualifier. The
  version field is only `version-release`. `deb` stores the epoch inside
  the version string (`1:2.4.47-2`).
  <https://github.com/package-url/purl-spec/blob/main/docs/types/definitions/rpm-definition.md>

## Decision

The primary key is the Package URL after normalization, **without** the
version and without version-dependent qualifiers.

Removed from the key:

| Piece | Why it is version-dependent |
| --- | --- |
| `version` | The package version itself. |
| `checksum` | Digest of one artifact. The spec's form is `algorithm:hex` of a built file. |
| `download_url` | Direct URL of one download. Spec examples point at one release archive. |
| `vcs_url` | VCS location. The spec allows a commit, tag, or branch after `@`. |
| `file_name` | Archive name. Spec examples are `Django-1.11.1.tar.gz` and the matching wheel: the version and the artifact are in the name. |
| `vers` | A version range. The spec says it replaces the version and is mutually exclusive with it. |
| `epoch` | Not a core recommended qualifier. The `rpm` type keeps it beside the version because RPM order is `epoch:version-release`. `deb` already folds epoch into `version`, so stripping `version` covers deb. |

Kept in the key: `type`, `namespace`, `name`, every other qualifier
(`arch`, `distro`, `repository_url`, and type-specific keys such as a
Maven classifier), and `subpath`.

Normalization follows the spec before the key is built:

- `type` is lowercased.
- Components are percent-decoded (RFC 3986) and then compared.
- Qualifier keys are lowercased, empty values are dropped, and duplicate
  keys make the purl invalid.
- Qualifiers are ordered by key. Order in the source string does not
  matter.
- `subpath` has no leading or trailing slash. A `.` or `..` segment is
  invalid.
- Namespace and name also follow that type's `normalization_rules`
  (for example the `rpm` namespace is lowercased, the `rpm` name is not).

The model stores a **set of versions** for each key. One version in that
set is the pair `(version, epoch)`. `epoch` is absent for types that do
not use it, and for an rpm with no epoch qualifier. The diff compares
the two sets: a version only on the new side is added, a version only
on the old side is removed. License, hash, and supplier are attributes
of a version that is in both sets, not extra keys and not extra versions.
`checksum`, `download_url`, `vcs_url`, `file_name`, and `vers` stay on
that version as attributes.

The same version pair twice in one document is one set element. That is
not an input error.

A `purl` that is present but not valid is **not** an input error. The
key falls back to `group` plus `name`, and the report carries a warning.
Warnings are counted and listed. They do not change the exit code.
A missing `group` is the empty string. A missing `name` on that fallback
still rejects the document (exit 2). A missing `purl` uses the same
`group` plus `name` key and does not warn.

`bom-ref` is not a key. It may be used only to walk the dependency graph
**inside the document that declared it**.

## Rationale

`purl` is the identifier that survives a rebuild and is shared across
generators. The version has to stay out of the key or a bump would look
like a removal and an addition. The qualifiers above move with the
version or with one built file, so leaving them in the key would split
one component into a new identity on every release or rebuild.

`epoch` is the case the core list does not name. On rpm it is not inside
`version`, and it still changes version order. It is removed from the
key and kept on the version pair, so epoch 1 and epoch 2 stay two
versions of one component. `repository_url` and `arch` stay in the key:
they say which repository or architecture the component is, not which
release it is.

Several versions per key are normal (a direct dependency and its
duplicates, or a lockfile that lists more than one version). Comparing
sets reports that without rejecting the file.

A broken `purl` used to reject the document. One bad string then blocked
the review. The fallback still yields a stable key, and the warning
keeps the bad string visible in the report.

## Alternatives

- `bom-ref` as the key. Rejected: it is not stable across two SBOMs.
- `purl` including the version. Rejected: version changes would not be
  visible as changes of one component.
- Keep `checksum`, `download_url`, `vcs_url`, `file_name`, `vers`, or
  `epoch` in the key. Rejected: each one changes when the release or the
  built file changes, so the join would miss the version update.
- Reject the document when a `purl` does not parse. Rejected on
  amendment: the rest of the SBOM is still reviewable, and the warning
  is the signal.
- Reject two versions of one key. Rejected on amendment: that is the
  case the version set exists for.
- `name` only, ignoring `group` and `purl`. Rejected: unrelated
  components share names across ecosystems and publishers.

## Consequences

- Ingest parses purls, normalizes them, strips the version-dependent
  pieces, and builds the version set.
- The diff joins on this key and never on `bom-ref`. It compares version
  sets, then attributes of versions that are in both sets.
- Direct versus transitive still uses each document's own graph.
- Every report format has a warning count and a warning list, sorted by
  key then by message, so the output stays deterministic.
- The warning text for this case is `invalid purl; fell back to group and name`.
