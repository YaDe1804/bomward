# ADR 0018 — Fuzzing and the attribute-amplification bound

- Status: accepted
- Date: 2026-11-13

## Context

The decoder and the policy parser accept attacker-controlled bytes.
An independent fuzz run of 8,000 structured documents, 2,000 raw-byte
inputs, and 3,000 policy inputs found no crash, no timeout, and no
memory amplification. That result has to stay true as the code changes.

One place amplification could still hide is attributes on a single
component. A file can hold a very large `hashes` array. A per-attribute
cap would be a new limit, a new error sentence, and a new flag.

## Decision

The decoder and the policy parser are fuzzed in two places:

- A deterministic smoke fuzz runs in the stable test suite, on Windows,
  Linux, and macOS. It feeds structured documents, raw bytes, and
  policy-shaped bytes to `read_bytes` and `policy::read_bytes`. A
  successful document is also diffed against an empty document and
  rendered in all three report formats. The seed is fixed.
- `cargo fuzz` targets `decode` and `policy` live in `fuzz/`, a
  workspace of their own, so the main workspace stays on the stable
  toolchain. CI runs them: 60 seconds on a pull request, 10 minutes on
  the daily schedule. A crash uploads `fuzz/artifacts` and fails that
  job. The smoke fuzz is the check that blocks the merge.

No new per-attribute limit is added. The file-size limit from
[ADR 0006](0006-untrusted-input.md) already bounds the work. The
measurement behind that choice: a 31 MiB input carrying 200,000 hashes
used about 35 MiB of resident memory. The regression test keeps 100,000
hashes on one component accepted, and it rejects the same bytes once
they are padded past `--max-file-bytes`.

## Rationale

The file-size check runs before parsing. Bytes that never enter the
process cannot become a hash set. The measurement shows the resident
set stays in the same order of magnitude as the input, so a separate
hash cap would not buy a tighter bound. It would add a limit callers
have to learn, and an error that can hide the file-size error.

The smoke fuzz is on stable because not every developer has nightly.
libFuzzer stays in CI, where a nightly toolchain and a Linux runner
are available. The two layers look for the same class of bug: a panic,
a hang, or a blow-up. Neither layer changes what the decoder accepts.

## Alternatives

- Cap the number of hashes, licenses, or other attributes per
  component. Rejected: the 31 MiB / 200,000-hash measurement does not
  show amplification that the file-size limit fails to catch, and the
  extra limit is more API for the same bound.
- Put libFuzzer in the blocking CI job. Rejected: it needs nightly and
  a Unix runner, and a long run would hold the merge. The stable smoke
  fuzz is the gate. The libFuzzer job is visible beside it.
- Fuzz homoglyphs and other visible confusable characters. Rejected:
  that is a report-policy question, not a parser crash. Visible
  lookalikes stay out of scope, as [ADR 0015](0015-report-formats-and-escaping.md)
  already says.

## Consequences

- `bomward-core/tests/fuzz_smoke.rs` and
  `bomward-core/tests/limits_boundary.rs` are part of `cargo nextest`.
- `fuzz/` is not a member of the stable workspace. Its toolchain file
  asks for nightly. The root toolchain stays 1.100.0.
- A future attribute cap needs a new ADR. This one is the reason the
  cap was not added.
- How to run the targets, and how to replay a crash, is
  [FUZZING.md](../FUZZING.md).
