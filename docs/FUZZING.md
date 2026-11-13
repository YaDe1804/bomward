# Fuzzing

The decoder and the policy parser are fuzzed. The stable smoke fuzz is
part of the test gate. libFuzzer runs in a separate CI workflow.
[ADR 0018](adr/0018-fuzzing-and-attribute-amplification.md) records why
there is no extra limit on attributes per component.

## Stable smoke fuzz

`bomward-core/tests/fuzz_smoke.rs` runs on the stable toolchain,
including Windows. A fixed seed (`0xB01D_0007`) drives three loops of
2,000 cases:

- structured CycloneDX-like documents, including hostile strings,
  boundary numbers, wrong types, nested components, a dependency cycle,
  and a self-edge
- raw bytes
- policy-shaped bytes

Each document is passed to `bomward_core::read_bytes`. A document that
parses is diffed against an empty document and rendered as terminal
text, Markdown, and JSON. A policy input is passed to
`policy::read_bytes`. A panic fails the test. The failure names the
seed and the case index.

`bomward-core/tests/limits_boundary.rs` locks the boundaries that run
already checked: component depth 32 accepted and 33 rejected, JSON
container depth 127 accepted and 128 rejected, a cycle and a self-edge
that both finish, and 100,000 hashes accepted under the file-size limit.

```text
cargo nextest run --locked --test fuzz_smoke --test limits_boundary
```

## libFuzzer

The harness is its own workspace in `fuzz/`, so the main workspace does
not depend on nightly. Two targets take raw bytes:

| Target | Call |
| --- | --- |
| `decode` | `bomward_core::read_bytes` with `Limits::default()` |
| `policy` | `policy::read_bytes` with `PolicyLimits::default()` |

Seeds are committed:

- `fuzz/corpus/decode/` — the CycloneDX fixtures under
  `bomward-core/tests/fixtures/`
- `fuzz/corpus/policy/` — `examples/policies/`

`fuzz/target/`, `fuzz/artifacts/`, and `fuzz/coverage/` are gitignored.
The corpus is not.

libFuzzer needs nightly, a C++ compiler, and a Unix host. It does not
run on Windows. From `fuzz/`:

```text
cargo +nightly install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fuzz run decode -- -max_total_time=60
cargo +nightly fuzz run policy -- -max_total_time=60
```

`fuzz/rust-toolchain.toml` selects nightly for commands run in that
directory. The repository root stays on 1.100.0.

## CI

`.github/workflows/fuzz.yml` is not part of the blocking `ci.yml` job.

- On `pull_request`, each target runs for 60 seconds, starting from the
  committed corpus.
- On the daily `schedule`, each target runs for 600 seconds.
- A crash fails the job and uploads `fuzz/artifacts`.

## Reproducing a crash

CI stores the crashing input under `fuzz/artifacts/<target>/`. Copy that
file into a local checkout and run only that input, from `fuzz/`:

```text
cargo +nightly fuzz run decode artifacts/decode/<crash-file>
```

Replace `decode` with `policy` for a policy crash. The same command
without a file path fuzzes again. `cargo +nightly fuzz tmin decode
artifacts/decode/<crash-file>` shrinks the input.

## Out of scope

Fuzzing looks for panics, hangs, and memory blow-ups in the decoder and
the policy parser. Homoglyphs and other visible confusable characters
are not part of that search. The report encodes invisible characters and
does not try to tell lookalike letters apart.
