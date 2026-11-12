# ADR 0014 — YAML parser

- Status: accepted
- Date: 2026-11-12

## Context

The policy file is YAML. [ADR 0010 of Lockward](https://github.com/YaDe1804/lockward/blob/main/docs/adr/010-yaml-dependency.md)
pins `noyalib` at `=0.0.57` for the same job. A shared product family
should use that parser unless a review of the current crate says not to.
`serde_yaml` is archived. `serde_yml` is a deprecated shim. Neither is
a candidate.

The policy is configuration, but it is still a file from disk. Anchors
and aliases can expand without bound (a billion laughs). A repeated key
must be an error, not a silent last-wins. A validation error should be
able to name a line and a column.

## Decision

bomward reads policy YAML with `noyalib` pinned exactly to **0.0.60**
(`version = "=0.0.60"`), the current crates.io release on 2026-11-12.
The call is `from_str_with_config`. Features stay at the crate default
(`std`, `fast-int`, `fast-float`, `strict-deserialise`). There is no
caret. A later bump is its own review.

This is the same parser Lockward uses and not the same patch. Lockward
stays on 0.0.57 because 0.0.58 raised that crate's MSRV from 1.86 to
1.89 and told older consumers to stay on 0.0.57. bomward's MSRV is 1.99,
so 0.0.60 is inside the floor. The newer patch is what this review
looked at.

### Review of 0.0.60

- License `MIT OR Apache-2.0`, which [deny.toml](../../deny.toml) allows.
  Edition 2024. `rust-version` 1.89.0.
- `src/lib.rs` sets `#![forbid(unsafe_code)]`. Comments that say "unsafe"
  are about YAML tags or about `memchr` inside the optional SIMD notes.
  The default build does not enable the `simd` feature. The crate does
  not contain an `unsafe` block.
- Direct dependencies of the default features: `serde_core`, `indexmap`,
  `rustc-hash`, `memchr`, `itoa`, `ryu`, `serde_ignored`, `smallvec`.
  `indexmap` pulls `hashbrown` and `equivalent`. Several of those crates
  contain `unsafe`. That `unsafe` is theirs, not copied into bomward.
  bomward does not call it.
- `build.rs` declares two `cfg` names, runs `rustc --version`, and reads
  `NOYALIB_COVERAGE`. It has no build dependencies. It does not use the
  network, read a source file, or generate code.
- Aliases are not switched off by a single flag. They are counted.
  `alias_count > max_alias_expansions` is the check, so a budget of 0
  rejects the first alias. The crate also has an alias-to-anchor ratio
  and an event budget. bomward sets `max_alias_expansions` to 0 on top
  of `ParserConfig::strict()`, whose other budgets are the crate's
  tighter profile. An alias is therefore an error. A nested alias bomb
  fails at the first alias instead of expanding.
- Duplicate keys: `ParserConfig::strict()` sets
  `DuplicateKeyPolicy::Error`. The default of the crate is last-wins,
  which this configuration does not use.
- `Error::location()` returns `Option<Location>` with `line()` and
  `column()`. Parse errors and duplicate keys carry one. bomward copies
  those numbers into `PolicyError` when they are present.

`ParserConfig::strict()` also rejects a duplicate key and tightens
depth and size budgets. bomward sets `max_document_length` to the policy
file limit (1 MiB) so the parser stops before it builds a larger tree.

## Rationale

0.0.60 is the reviewed current release, it still forbids `unsafe`, and
it can express the two behaviors the policy requires: aliases do not
expand, and a repeated key is an error. Staying on 0.0.57 would ignore
a parser this MSRV is allowed to use, including located unknown-field
errors added in 0.0.58. Taking a caret range would move a `0.0.x` crate
without a review. That is the failure Lockward's pin exists to prevent.

## Alternatives

- `noyalib =0.0.57`, byte-identical to Lockward. Rejected: its own
  changelog says consumers below MSRV 1.89 should stay there. bomward
  is not below that line. The family still shares the parser.
- `serde_yaml` 0.9. Rejected: archived, and it does not give a reliable
  alias budget this review can point at.
- `serde_yml`. Rejected: a deprecated shim over the same line.
- A hand-written YAML subset. Rejected: the policy file is still YAML
  that operators will write with ordinary tools. A second parser is a
  second set of surprises.

## Consequences

- The exact requirement is `=0.0.60` in `bomward-core`. `cargo update`
  must not move it.
- Policy load uses `DuplicateKeyPolicy::Error` and
  `max_alias_expansions = 0`.
- Line and column are part of the error when the parser has them.
  They are never taken from a SBOM.
- The next noyalib bump repeats this review. It does not ride along
  with an unrelated lockfile refresh.
