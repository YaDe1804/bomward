# ADR 0016 — CLI output

- Status: accepted
- Date: 2026-11-13

## Context

`bomward diff` is the only command. It has to print a report, return the
exit code from [ADR 0007](0007-exit-codes-and-determinism.md), and stay
quiet when a pipe closes. The library decides. The binary only reads
files, calls the library, and writes bytes.

## Decision

```text
bomward diff <OLD> <NEW> [--policy <FILE>] [--format <terminal|markdown|json>] [--output <FILE>]
             [--max-file-bytes N] [--max-components N] [--max-depth N]
```

The default format is `terminal`. The report is written to stdout. With
`--output`, those same bytes are written to that file and stdout is
empty. The file is created or truncated. Parent directories are not
created. A directory path is an error. An `--output` path that is one
of the two SBOM files is rejected before either file is opened for
writing: the process exits 2, names `OLD` or `NEW`, and leaves that
file unchanged. The check compares the files, so a different spelling
of the same path is the same refusal.

Diagnostics go to stderr, prefixed with `error: `. The report itself is
never written to stderr.

Exit codes stay those of ADR 0007:

| Code | When |
| --- | --- |
| 0 | The report was written and the effective result is not `deny`. Report-only is 0. |
| 1 | The report was written and the effective result is `deny`. The report is complete. |
| 2 | Usage, a file that cannot be read, a rejected SBOM, a limit, a rejected policy, an output path that is one of the two inputs, or a write error. No report is written. |

The message names the input. `OLD` and `NEW` are the two SBOM arguments.
`policy` is the policy argument. The path is the argument string, not a
canonicalized path. A `--max-depth` above 63 names the flag and does not
name a file, because no file is opened.

A closed stdout pipe (`bomward diff … | head -1`) writes nothing further
and prints no message. The exit code stays the verdict: 0 or 1. It is
not an error and it does not panic. A denial must not become a pass
because the reader stopped.

stdout uses LF, including on Windows. The report bytes do not contain
CR.

## Rationale

Code 1 has to be a finished verdict. Writing the report before exiting
1 lets a pull-request comment show the denial that caused the failure.
Code 2 means there is no verdict, so a half-written report would look
like one. A closed pipe is the reader stopping. The bytes already
follow the verdict, so the process keeps that code and stays quiet.
Printing `Broken pipe`, or forcing code 0, would hide a denial from a
script that uses `pipefail`.

## Alternatives

- Exit 2 on a closed pipe. Rejected: `head` would then report a failure
  for a report that was produced.
- Create missing parent directories for `--output`. Rejected: a typo
  would write a file into a new tree the operator did not ask for.
- Put the usage error on stdout. Rejected: a JSON consumer would parse
  the error as a report.

## Consequences

- `bomward-cli` contains no review rules. It calls ingest, diff, policy,
  and report.
- Tests spawn the binary. They cover every error class, both output
  targets, a second identical run, and a closed pipe.
- `--help` is a golden file. The command sets a fixed terminal width so
  the file does not depend on the console.
