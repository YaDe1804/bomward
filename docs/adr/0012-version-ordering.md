# ADR 0012 — Version ordering

- Status: accepted
- Date: 2026-11-12

## Context

[ADR 0011](0011-change-model.md) gives `version-changed` a direction:
`upgrade`, `downgrade`, or `unknown`. The direction is a policy input.
A downgrade reported as `upgrade` would let a rule allow the wrong
change. Reporting `unknown` does not.

No single order is true for every purl type. npm and Cargo publish
Semantic Versioning. Maven, Debian, and PyPI do not. rpm carries an
epoch that outranks the version string and is not part of SemVer.

Checked on 2026-11-12:

- Semantic Versioning 2.0.0, section 11 (precedence):
  <https://semver.org/spec/v2.0.0.html>
- `semver` 1.0.28 (David Tolnay, MIT OR Apache-2.0, MSRV 1.68).
  `Version::parse` accepts only a three-part SemVer string.
  `Version::cmp_precedence` compares major, minor, patch, and
  pre-release, and ignores build metadata.
  `Ord` on `Version` also compares build metadata, so it is not the
  precedence relation.
  <https://docs.rs/semver/1.0.28/semver/struct.Version.html>
- The rpm purl type stores `epoch` as a qualifier. ADR 0005 keeps that
  qualifier on `VersionId` and out of the component key.

## Decision

`version-changed` carries one direction. bomward never guesses.

The direction is decided in this order:

1. **Epoch.** If the two epoch strings differ, compare them as base-10
   integers with no sign and no leading-zero rejection beyond what a
   `u64` parse accepts. A higher epoch is `upgrade`, a lower epoch is
   `downgrade`. If either epoch is not a `u64`, the direction is
   `unknown` and the version strings are not consulted.
   An absent epoch is `0` only for purl type `rpm`. For every other
   type, an absent epoch is skipped. Two absent epochs are equal and
   fall through.
2. **SemVer precedence, only for `npm` and `cargo`.** Both version
   strings must parse with `semver` 1.0.28 `Version::parse`. The
   comparison is `Version::cmp_precedence`, not `Ord`.
   - A greater new version is `upgrade`.
   - A lesser new version is `downgrade`.
   - Equal precedence is `unknown`. Build metadata does not change
     precedence, so `1.0.0+aaa` to `1.0.0+bbb` is still
     `version-changed` (the strings differ) and the direction is
     `unknown`.
3. **Everything else is `unknown`.** That includes `rpm` once the
   epochs are equal, every type other than `npm` and `cargo`, and any
   `npm` or `cargo` string that does not parse. A string that looks
   like SemVer on a `pypi` or `generic` key is still `unknown`.

Pre-release and build metadata follow SemVer 2.0.0 as implemented by
`cmp_precedence`:

- A pre-release is less than the same major.minor.patch without one.
- Pre-release identifiers compare left to right. Numeric identifiers
  compare as integers. Non-numeric identifiers compare as ASCII.
  A numeric identifier is less than a non-numeric one. A longer
  pre-release that starts with an equal prefix is greater.
- The example in the specification holds:
  `1.0.0-alpha` < `1.0.0-alpha.1` < `1.0.0-alpha.beta` < `1.0.0-beta`
  < `1.0.0-beta.2` < `1.0.0-beta.11` < `1.0.0-rc.1` < `1.0.0`.
- Leading zeros, a `v` prefix, and fewer than three numeric components
  fail `Version::parse`. The direction is `unknown`.

The closed type list is `npm` and `cargo`. Adding a type requires a
change to this ADR.

The crate is `semver` 1.0.28 with default features only. The `serde`
feature stays off. bomward does not reimplement precedence.

## Rationale

`cmp_precedence` is the relation section 11 of the SemVer specification
defines. `Ord` would call a build-metadata change an upgrade or a
downgrade, which is a direction the specification refuses.

A hand-written comparator was rejected because the pre-release rules
(numeric versus ASCII, numeric less than non-numeric, length) are easy
to get wrong, and a wrong answer becomes a policy decision. The crate
has no required dependency when `serde` is off, and its license is
already on the allow-list in `deny.toml`.

Loose parsers that accept `v1.2.3` or `1.2` would turn a non-SemVer
string into a direction. A parse failure is the `unknown` this ADR
wants.

Ordering `golang`, `gem`, or `maven` because many of those versions
happen to be SemVer would mislabel the ones that are not. The closed
list is the types whose ecosystem requires SemVer.

## Alternatives

- Compare every type with SemVer, and use `unknown` only when parsing
  fails. Rejected: a successful parse on a Maven version is not evidence
  that the project uses SemVer precedence.
- Use `Ord` so that build metadata has a total order. Rejected: that
  order is not precedence, and it would report a direction for a change
  that does not change which release is newer.
- Treat equal precedence as a fourth direction, `same`. Rejected: the
  contract has three values. `unknown` already means “do not treat this
  as an upgrade or a downgrade”.
- Compare rpm versions with the rpmver algorithm once epochs match.
  Rejected for v0.1: it is a second order, it needs its own tests, and
  a wrong rpm order is the same class of mistake as a wrong upgrade.
  Epoch alone is the part the purl specification calls out.

## Consequences

- A policy can allow `upgrade` and deny `downgrade` without catching a
  non-SemVer change. Those changes are `unknown`, and AP3 decides what
  an unknown direction means. This ADR does not.
- `1.10.0` sorts before `1.9.0` in the change list (ADR 0011) and is
  still an upgrade when the type is `npm` or `cargo`.
- Tests cover each pre-release step in the specification example, a
  build-metadata-only change, a parse failure, a non-SemVer type, an
  rpm epoch step, and equal rpm epochs.
