# ADR 0008 — CycloneDX parser

- Status: accepted
- Date: 2026-11-11

## Context

ADR 0004 accepts CycloneDX JSON 1.4, 1.5, 1.6, and 1.7, and leaves the
parser open. ADR 0006 requires the size, component-count, and depth
limits to fail the document without truncating it and without first
holding an unbounded tree. Unknown fields (a later spec, or a generator's
own extension) must not reject a review.

Checked on 2026-11-11:

- `cyclonedx-bom` 0.9.0, published 2026-11-06, Apache-2.0, MSRV 1.86.
  Repository: <https://github.com/CycloneDX/cyclonedx-rust-cargo>, tag
  `cyclonedx-bom-0.9.0`.
- Direct dependencies of that crate: `base64` 0.22.1, `fluent-uri` 0.4.1,
  `indexmap` 2.2.2, `once_cell` 1.18.0, `ordered-float` 5 (no default
  features), `purl` 0.1.3 (no default features), `regex` 1.9.3, `serde`,
  `serde_json`, `spdx` 0.13, `thiserror` 2, `time` 0.3.29 (`formatting`
  and `parsing`), `uuid` 1.6.1 (`v4`), `xml-rs` 0.8.16, `strum` 0.28
  (`derive`), and `cyclonedx-bom-macros` 0.1. No HTTP client and no
  process crate in that list. Apache-2.0 is on the `deny.toml` allow-list.

## Decision

bomward uses its own serde model. The input structs name only the fields
ADR 0010 reads. They do not use `deny_unknown_fields`. Unknown fields are
ignored.

The model is not a CycloneDX implementation. It is the subset the review
gate needs.

### Where the limits are checked

1. **File size, before the parse.** `read_path` compares the file
   metadata with `--max-file-bytes`. It then reads at most `limit + 1`
   bytes. A file that grows after the metadata check still cannot pass
   more than that many bytes into the parser. `{found}` is the metadata
   length when that already exceeds the limit, otherwise the number of
   bytes read. `read_bytes` compares `bytes.len()` and does not parse a
   slice that is already over the limit.
2. **JSON container depth, before `serde_json` builds a tree.** A linear
   scan counts nested arrays and objects outside strings. The outermost
   value has depth 1. Each nested array or object adds 1. The scan
   rejects a document whose depth is greater than 127. `{found}` is the
   observed depth. `{limit}` is 127. The message is the JSON sentence
   from ADR 0006. It does not name `--max-depth`. The cap does not move
   when the flag changes.
3. **Component count and component nesting, while walking the value.**
   A component in `metadata.component` or in the top-level `components`
   array has depth 1. A component inside a nested `components` array has
   the parent's depth plus 1. The walk increments the count and the
   depth before it inserts the component and before it walks that
   component's children. Depth equal to `--max-depth`, and a count equal
   to `--max-components`, are accepted. One more is an error.
   `{found}` and `{limit}` for component nesting are the component depth
   and the flag value. The function returns the error and drops the
   partial map. It does not return a shortened document.

The ceiling is the parser we hand the bytes to, not a function of the
flag. `serde_json` 1.0.154 starts its recursion budget at 128 and
rejects the 128th container, so the deepest `Value` it will build is
127. v0.1 does not enable the `unbounded_depth` feature: a `Value` past
that depth is dropped recursively and can overflow the stack. The cap
of 127 keeps the scan and `serde_json` on the same side of that line.

A component at depth `D` sits at container depth `2 * D + 1` (the root
object, one array and one object per level). `2 * 63 + 1 = 127`, so 63
is the largest `--max-depth` the cap still carries. `Limits::new`
returns a typed usage error for a larger value, and every read checks
again so a struct literal cannot bypass it. The default 32 stays
inside that bound: a pure component chain of depth 32 has container
depth 65, so it fails the component check, not the JSON check. Nested
fields inside the deepest component (licenses, hashes) can still fill
the remaining JSON budget and hit the JSON sentence. A container bomb
past 127 is rejected by the scan and never becomes a
`serde_json::Value`.

### Duplicate object keys

`serde_json` keeps the last value of a duplicated key. The JSON RFC
leaves this to the parser. bomward uses that behavior, tests it, and
does not reject the document. Generators do emit duplicate keys, and
rejecting them would fail a review for a reason the policy did not ask
about.

## Rationale

`cyclonedx-bom` 0.9.0 meets the version criterion. `Bom::parse_from_json`
reads the reader into a `serde_json::Value` and then into the full model
(services, vulnerabilities, formulation, signatures).
`parse_from_json_with_version` deserializes straight into the versioned
BOM struct. Neither path can stop at component N, and neither accepts a
depth limit below the library default. The spec structs inspected on the
0.9.0 tag (`specs/common/bom.rs`, `component.rs`, `dependency.rs`) do
not set `deny_unknown_fields`, so unknown fields are ignored there too.
The crate still allocates every field this review does not use, and it
pulls an XML parser, SPDX parsing, and `uuid`/`v4` to do it.

An owned model can count components and depth in the walk that builds
the review model, and it can refuse the file before that walk. The
dependency set stays `serde`, `serde_json`, and the purl crate from
ADR 0009.

## Alternatives

- `cyclonedx-bom` 0.9.0. Rejected for v0.1: the limit checks would run
  only after the full document tree exists, which breaks criterion 2.
  Revisit if a later release can abort on count and depth during parse.
- A streaming parser with no intermediate `Value`. Rejected for v0.1:
  the component walk needs the header first and random access to
  `dependencies`. The size cap and the depth scan already bound the
  bytes and the container depth before the `Value` exists.

## Consequences

- Ingest structs stay small. Adding a CycloneDX field to the model is a
  change to this ADR's sibling, ADR 0010, not a crate upgrade.
- Unknown JSON fields cannot be reported. They are dropped.
- The three limit sentences stay exactly as in ADR 0006. Callers match
  on the limit kind, the found value, and the limit.
- `bomFormat` and `specVersion` are read from the parsed value before
  any component is interpreted. A failure there does not build a model.
