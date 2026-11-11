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

bomward uses its own serde visitors. They name only the fields ADR 0010
reads. Unknown fields are skipped with `serde::de::IgnoredAny`, which
walks the tokens and does not build a value. The ingest path does not
construct a `serde_json::Value`.

The visitors are not a CycloneDX implementation. They are the subset the
review gate needs.

### Where the limits are checked

1. **File size, before the parse.** `read_path` compares the file
   metadata with `--max-file-bytes`. It then reads at most `limit + 1`
   bytes. A file that grows after the metadata check still cannot pass
   more than that many bytes into the parser. `{found}` is the metadata
   length when that already exceeds the limit, otherwise the number of
   bytes read. `read_bytes` compares `bytes.len()` and does not parse a
   slice that is already over the limit.
2. **JSON container depth, before serde builds a model.** A linear scan
   counts nested arrays and objects outside strings. The outermost value
   has depth 1. Each nested array or object adds 1. The scan rejects a
   document whose depth is greater than 127. `{found}` is the observed
   depth. `{limit}` is 127. The message is the JSON sentence from
   ADR 0006. It does not name `--max-depth`. The cap does not move when
   the flag changes.
3. **Component count and component nesting, while deserializing.** A
   component in `metadata.component` or in the top-level `components`
   array has depth 1. A component inside a nested `components` array has
   the parent's depth plus 1. The visitor checks the depth and increments
   the count when it enters the component object, before it reads that
   object's fields. Depth equal to `--max-depth`, and a count equal to
   `--max-components`, are accepted. One more is an error. `{found}` and
   `{limit}` for component nesting are the component depth and the flag
   value. The visitor returns that error, serde stops, and the partial
   model is dropped. A syntax error after the failing component is not
   read, so the limit error wins over it.

The ceiling is the parser we hand the bytes to, not a function of the
flag. `serde_json` 1.0.154 starts its recursion budget at 128 and
rejects the 128th container. `IgnoredAny` still recurses, so a deep
ignored subtree can overflow the stack the same way a deep value would.
v0.1 does not enable the `unbounded_depth` feature. The cap of 127 keeps
the scan and `serde_json` on the same side of that line.

A component at depth `D` sits at container depth `2 * D + 1` (the root
object, one array and one object per level). `2 * 63 + 1 = 127`, so 63
is the largest `--max-depth` the cap still carries. `Limits::new`
returns a typed usage error for a larger value, and every read checks
again so a struct literal cannot bypass it. The default 32 stays
inside that bound: a pure component chain of depth 32 has container
depth 65, so it fails the component check, not the JSON check. Nested
fields inside the deepest component (licenses, hashes) can still fill
the remaining JSON budget and hit the JSON sentence. A container bomb
past 127 is rejected by the scan and is never handed to serde.

### Header before components

`bomFormat` and `specVersion` have to be accepted before any component
is interpreted, and a document may put `components` first. One serde
derive of the whole object would build those components in file order,
so a bad `specVersion` could lose to `component is missing a name`.

The top-level visitor reads the object once:

- `bomFormat` and `specVersion` are scalars. A later duplicate replaces
  the earlier one. The last value is the one that counts.
- `metadata`, `components`, and `dependencies` that appear before both
  header fields have been accepted are kept as a raw JSON slice
  (`RawValue`). That slice is the text, not a tree, and it is not
  interpreted.
- Once both fields are accepted, a later component list is deserialized
  by the counting visitor. Crossing the count or the depth returns the
  limit error immediately.
- Slices captured before the header are interpreted only after the
  header is accepted, and then in model order: the metadata component,
  then the components array, then `dependencies`. A rejected header
  drops those slices without interpreting them.

A component list that itself contains a syntax error cannot be captured
as a slice, and the limit check has not started if the header is still
unread. That document fails as JSON. The documents this gate sees put
the header first. After the header is accepted, a syntax error that
follows one component too many is not read, and the failure is the
limit.

### What is retained

The read holds the input buffer once. That buffer is already capped by
`--max-file-bytes`.

Ignored objects are not retained. Measured before this decoder: 32 MiB
of `{"a":0}` objects in an unknown field became a `serde_json::Value`
tree of about 2.8 GiB and the document was accepted. `IgnoredAny` walks
those tokens and drops them. What remains is the input buffer plus the
model of the fields ADR 0010 names.

Known fields are owned strings and sets. A 32 MiB `name` is stored,
because the model keeps it. That cost follows the bytes of the field,
plus ordinary allocator overhead. It is not a map per ignored object.
The component counter caps how many components are kept. It does not
cap one component's license list; that list is bounded by the file.

A component list that appears before the header is copied once as raw
JSON until the header is accepted, then parsed. The copy is bounded by
the file. Header, then components, which is the usual order, does not
take that copy.

The duplicate-key scan holds the keys of objects that are still open.
That set is bounded by the file size and by depth 127.

This is not a claim that memory is smaller than the file, or constant.
It is the claim that ignored subtrees are not kept, and that the
retained data is the ADR 0010 model. The 2.8 GiB figure was the overhead
of a `Value` map for objects the review does not use.

### Duplicate object keys

`serde_json` keeps the last value of a duplicated key. The JSON RFC
leaves this to the parser. bomward uses that behavior and does not
reject the document. Generators do emit duplicate keys, and rejecting
them would fail a review for a reason the policy did not ask about.

The same depth scan records the duplicate. It is not a second parse.
Each open object keeps the keys it has already seen, and drops that set
when the object ends. The bytes were already capped by
`--max-file-bytes`, and the scan stops at depth 127, so the sets cannot
grow past the input that was accepted. One warning is enough: kind
`duplicate-key`, with no component key and with no copy of the repeated
name. A key written as `\uXXXX` is decoded before the comparison, so it
matches the key `serde_json` keeps. The same key in a nested object is
not a duplicate. If the scan loses track of the grammar it stops
recording keys and still counts depth; a document that then fails to
parse reports the JSON error, not the warning.

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

Visitors count components and depth while they read, and they refuse the
file at the failing component. The dependency set stays `serde`,
`serde_json`, and the purl crate from ADR 0009.

## Alternatives

- `cyclonedx-bom` 0.9.0. Rejected for v0.1: the limit checks would run
  only after the full document tree exists, which breaks criterion 2.
  Revisit if a later release can abort on count and depth during parse.
- Deserialize the whole document into `serde_json::Value`, then walk it.
  That was the first ingest. It checks limits only after the tree
  exists, so an unknown field of small objects is retained and expanded.
  Rejected once that cost was measured.
- A pull parser that never buffers the file. Rejected: the size check
  already reads the capped bytes, and a component list that precedes the
  header has to be held until the header is known. Holding that list as
  raw JSON is enough. A second full tree is not.

## Consequences

- Ingest structs stay small. Adding a CycloneDX field to the model is a
  change to this ADR's sibling, ADR 0010, not a crate upgrade.
- Unknown JSON fields cannot be reported. They are dropped, and they
  are not retained as a tree.
- The limit sentences stay exactly as in ADR 0006. Callers match on the
  limit kind, the found value, and the limit.
- `bomFormat` and `specVersion` are accepted before components are
  interpreted, including when `components` is the first field. A
  rejected header does not build a model. After the header is accepted,
  a component limit stops the parse before a later syntax error.
