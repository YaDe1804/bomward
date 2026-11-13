# ADR 0007 — Exit codes and determinism

- Status: accepted, amended by [ADR 0016](0016-cli-output.md)
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
| 0 | Inputs are valid, and either no policy was applied or the policy allows the change. |
| 1 | Inputs are valid and the policy denies the change. |
| 2 | Usage error, unreadable or rejected input, or a command that is not implemented. |

No other code is produced. The process does not panic its way to a
non-zero code.

Omitting `--policy` selects report-only mode. A valid diff then exits 0.
There is no policy verdict. Terminal, Markdown, and JSON each include
the exact sentence `No policy was applied.` JSON also sets
`"policy_applied": false`. When `--policy` is present, that sentence is
absent and `"policy_applied"` is `true`.

Warnings, including an invalid `purl` (ADR 0005), do not change the exit
code. They are part of the report.

The foundation build exited 2 because `diff` was not implemented.
[ADR 0016](0016-cli-output.md) is the current command: a finished report
exits 0 or 1, and exit 2 means no report was written.

Determinism:

- The same SBOM bytes, policy bytes, format flag, and limit flags produce
  byte-identical stdout.
- Warnings are sorted by identity key, then by message.
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
without also blocking on a missing file or on a report-only run. Code 0
with no policy is a successful report, and the required sentence keeps
that run from looking like a silent approval. Code 2 keeps broken input
out of the pass/fail policy channel. Treating "not implemented" as 2 is
the same channel: the tool did not produce a verdict.

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
