# ADR 0009 — Package URL normalization

- Status: accepted
- Date: 2026-11-11

## Context

ADR 0005 defines the component key: a Package URL with the version and
the version-dependent qualifiers removed, normalized the way ECMA-427
and the type rules require. Invalid purls are not input errors. This ADR
chooses who parses the string.

Checked on 2026-11-11:

- `packageurl` 0.7.1 (2026-09-21), MIT, `rust-version` 1.87, edition
  2024. Repository: <https://github.com/scm-rs/packageurl.rs>, tag
  `v0.7.1`. Direct dependencies: `percent-encoding` 2 and `thiserror` 2.
  `memchr` and `serde` are optional and bomward does not enable them.
- The crate's `src/types.rs` says the per-type rules are transcribed from
  the purl-spec type definitions. `validate` returns success for a type
  that is not in that table ("unknown types have none").
- `purl` 0.1.6 (2025-03-13), MIT, no `rust-version`.
  Repository: <https://github.com/phylum-dev/purl>. Its generic parser
  does not apply type rules. The optional `package-type` feature applies
  a shorter list and, on purpose, diverges from the spec: PyPI names are
  rewritten with PEP 503, and npm names are not lowercased. `cyclonedx-bom`
  0.9.0 depends on `purl` 0.1.3.

## Decision

bomward parses with `packageurl` 0.7.1 and then applies ADR 0005 itself.

The crate already:

- lowercases `type`;
- percent-decodes components (RFC 3986);
- lowercases qualifier keys and drops empty qualifier values;
- rejects a `subpath` segment that is empty, `.`, or `..`;
- applies the type table below.

bomward then:

- rejects the purl when a qualifier key occurs twice (see below);
- removes `version`, `checksum`, `download_url`, `vcs_url`, `file_name`,
  `vers`, and `epoch` from the key;
- keeps those removed qualifiers, and `epoch`, as attributes of the
  version pair `(version, epoch)`;
- stores the remaining qualifiers in a `BTreeMap` ordered by key. The
  crate's qualifier map is a `HashMap`. Nothing that becomes a key or a
  golden encoding iterates that map.

Any error from the crate, and a duplicate qualifier key, is an invalid
purl. ADR 0005 then applies: fall back to `group` plus `name`, and emit
one warning. The warning text is `invalid purl; fell back to group and name`.
The purl string is not copied into the warning or the error.

### Types the crate normalizes

These types get the crate's namespace rule and case folding. bomward does
not add a second normalizer for them.

| Rule | Types |
| --- | --- |
| namespace lowercased | `apk`, `bitbucket`, `composer`, `deb`, `github`, `golang`, `hex`, `qpkg`, `rpm` |
| name lowercased | `bitbucket`, `composer`, `deb`, `github`, `hex`, `npm`, `pypi` |
| version lowercased | `huggingface` |
| namespace required | `composer`, `cpan`, `huggingface`, `swift`, `vscode-extension` |
| namespace prohibited | `bitnami`, `cargo`, `chrome-extension`, `cocoapods`, `conda`, `cran`, `gem`, `hackage`, `julia`, `mlflow`, `nuget`, `oci`, `otp`, `pub`, `pypi`, `vcpkg` |
| extra checks in the crate | `chrome-extension` (32-character lowercase name and a numeric version shape), `cpan` (name must not contain `::`), `julia` (qualifier `uuid` required), `mlflow` (name lowercased when `repository_url` contains `databricks`) |

A type that is not in this table gets generic normalization only: the
crate lowercases the type, percent-decodes, lowercases qualifier keys,
drops empty values, and validates the subpath. bomward does not invent
type rules for it in v0.1.

PyPI names are lowercased. `packageurl` 0.7.1 also rewrites `_` to `-`
in a PyPI name (`PackageUrl::new`). bomward keeps that behavior.
It does **not** add PEP 503 on top: `.` is kept, and runs of separators
are not collapsed. That fuller rewrite is what `purl` 0.1.6 does, and
it diverges from the type definition this crate transcribes. `a_b` and
`a-b` are one key; `a.b` stays `a.b`.

### Duplicate qualifier keys

`packageurl` 0.7.1 parses qualifiers into a `HashMap`, so a repeated key
keeps the last value and `from_str` still succeeds. ADR 0005 says a
duplicate qualifier key makes the purl invalid. bomward scans the
qualifier string and treats a repeated key as an invalid purl. Empty
values stay "drop", which is what the crate already does and what
ADR 0005 says.

## Rationale

The type table is the part that goes stale, and it is the part this
crate already tracks against the purl-spec suite. Reimplementing ECMA-427
would duplicate percent-decoding and the subpath rules for no gain.
`purl` 0.1.6 is older, its type list is shorter, and its PyPI and npm
rules disagree with the definitions ADR 0005 cites.

The one rule the chosen crate does not enforce is the duplicate
qualifier key. That check is a scan of one string. It stays in bomward
so the key matches ADR 0005.

## Alternatives

- Hand-written parser. Rejected: the type table and the percent-encoding
  rules are exactly the code that drifts from the spec.
- `purl` 0.1.6. Rejected: last release 2025-03-13, and the type feature
  diverges from the spec on PyPI and npm.
- Enable `packageurl`'s `serde` feature. Rejected: the SBOM string is
  parsed in one place. A second deserializer is unused surface.

## Consequences

- A new purl type is generic until `packageurl` learns its rules and
  bomward bumps the crate. That bump is recorded in the changelog.
- PEP 503 is a known gap. Closing it is an amendment of this ADR, not a
  silent extra fold in the key.
- `bomward-core` gains `packageurl`, `percent-encoding`, and `thiserror`
  is already present. No HTTP or process crate.
