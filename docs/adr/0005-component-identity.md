# ADR 0005 — Component identity

- Status: accepted
- Date: 2026-11-11

## Context

A diff has to know when a component in the new SBOM is the same
component as one in the old SBOM. `bom-ref` looks like an identifier and
is not one across documents: tools regenerate it, and two generators do
not agree. Using it would report every rebuild as a remove plus an add.

## Decision

The primary key is the Package URL (`purl`) **without its version**.

- Parse `purl` as a Package URL.
- Remove the version component.
- Keep type, namespace, name, qualifiers, and subpath.
- Compare qualifiers independent of their order.
- If `purl` is present but not a valid Package URL, reject the document
  (exit 2). Do not fall back.
- If `purl` is absent, the key is `group` plus `name`. A missing `group`
  is the empty string. A missing `name` rejects the document.
- `bom-ref` is not a key. It may be used only to walk the dependency
  graph **inside the document that declared it**.

Version, hashes, licenses, and supplier are attributes. A version change
is a diff event on one identity, not a second component.

Two components in the same document with the same key are an input
error (exit 2). The foundation does not pick a silent merge.

## Rationale

`purl` is the identifier that survives a rebuild and is shared across
generators. The version has to stay out of the key or a bump would look
like a removal and an addition, which is the wrong review event.

The fallback exists because many SBOMs still omit `purl` and only have
`group` and `name`. Falling back on a *broken* `purl` would hide a
corrupt document and could merge two components. That failure is an
input error instead.

`bom-ref` is specified as a document-local reference for the dependency
graph. It is stable within one file and not across two files, so it is
the wrong join key and the right graph handle.

## Alternatives

- `bom-ref` as the key. Rejected: it is not stable across two SBOMs.
- `purl` including the version. Rejected: version changes would not be
  visible as changes of one component.
- `name` only, ignoring `group` and `purl`. Rejected: unrelated
  components share names across ecosystems and publishers.
- Drop qualifiers and subpath from the key. Rejected: the brief says to
  drop the version, not the rest of the identity. Qualifiers such as
  architecture distinguish components a review should not merge.

## Consequences

- Ingest owns purl parsing, qualifier ordering, and duplicate detection.
- The diff joins on this key and never on `bom-ref`.
- Direct versus transitive still uses each document's own graph, via
  that document's `bom-ref` values, and does not compare those strings
  across documents.
- A SBOM that repeats one `purl` at two versions in a single file is
  rejected until a later ADR defines a merge.
