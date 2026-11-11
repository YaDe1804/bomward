# ADR 0007 — Exit codes and determinism

- Status: accepted
- Date: 2026-11-11

## Context

CI needs a stable code for "block this change" that is distinct from
"the tool could not run". Reviewers also need a report they can diff.
A timestamp, a map iteration order, or a canonicalized Windows path
makes the same SBOM produce two bytes sequences.

## Decision

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Inputs are valid and the policy allows the change. |
| 1 | Inputs are valid and the policy denies the change. |
| 2 | Usage error, unreadable or rejected input, or a command that is not implemented. |

No other code is produced. The process does not panic its way to a
non-zero code.

An omitted policy file cannot be violated. A valid diff with no policy
exits 0. The foundation `diff` command is not implemented, so it exits 2
even when the arguments are present.

Determinism:

- The same SBOM bytes, policy bytes, and format flag produce
  byte-identical stdout.
- Collections are sorted by the identity key from ADR 0005, then by a
  fixed change-kind order.
- Report stdout uses LF, including on Windows.
- Stdout contains no timestamps, durations, host names, randomness, or
  local paths.
- Paths in stderr, when a diagnostic needs them, are the argument
  strings unchanged. They are not canonicalized.
- `--format terminal` emits no ANSI color.

## Rationale

Code 1 is the only "policy failed" signal, so a CI `if` can block on it
without also blocking on a missing file. Code 2 keeps broken input out
of the pass/fail policy channel. Treating "not implemented" as 2 is the
same channel: the tool did not produce a verdict.

Byte-identical stdout is what makes golden files work on Windows and
Linux. Leaving paths and timestamps out of stdout is the practical way
to get there. Echoing argument strings unchanged, rather than
`canonicalize`, keeps stderr stable for the same command line too.

## Alternatives

- Exit 1 for both policy failures and broken input. Rejected: a missing
  file would look like a policy violation and could block a release for
  the wrong reason.
- Include the input paths and a generation timestamp in the report.
  Rejected: they depend on the machine and the clock, so the same SBOM
  would not have one golden file.
- Color by default. Rejected: ANSI bytes are not stable across "no
  color" captures and they are an injection channel next to untrusted
  names. Color can be reconsidered only as an explicit opt-in that
  golden files do not use.

## Consequences

- CI should treat 0 as pass, 1 as fail-the-check, and 2 as an error in
  the job. Mapping 2 to a successful check is a misuse.
- Golden tests compare stdout, not stderr.
- AP4's renderer sorts and escapes. It does not call the clock.
- The foundation tests lock `--version`, `--help`, and the exit code of
  `diff` without arguments. They do not lock a report, because there is
  no report yet.
