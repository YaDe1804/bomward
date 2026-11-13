# ADR 0017 — GitHub Action and trust boundary

- Status: accepted
- Date: 2026-11-13

## Context

bomward has to run as a review gate in a repository it does not own.
That repository's pull request is untrusted. A workflow that checks out
the pull request and then builds it, or that reads the policy from the
pull request, lets the change under review pick the rules and the code
that judge it.

The binary is not published yet. The release work that follows this
Action publishes the assets this ADR names. Until that tag exists, the
bomward repository's own workflow may build the binary it has just
checked out. Every other caller downloads a release.

## Decision

The Action is trigger-independent. It reads the base commit and the head
commit from the `pull_request` event. When that event is absent, the base
is `GITHUB_BASE_REF` and the head is `GITHUB_SHA`.

The trust boundary is the base commit:

- The new SBOM is the only byte the Action reads from the pull-request
  tree, and it reads that file as data.
- The policy file and the baseline SBOM are read with
  `git show <base-sha>:<path>` from the base commit. A pull request
  cannot change the rules it is judged against, and it cannot change
  the baseline it is compared with.
- The Action does not run a script, a hook, or a build from the
  pull-request tree.

The binary comes from a pinned release of `YaDe1804/bomward`. The Action
downloads the asset for the runner and `SHA256SUMS` from that same
release, and it checks the asset against that file before it makes the
binary executable. A missing entry or a mismatch exits 2. The binary is
not started. The asset names AP8 must publish are:

| Runner | Asset | Binary inside |
| --- | --- | --- |
| `Linux` | `bomward-x86_64-unknown-linux-gnu.tar.gz` | `bomward` |
| `macOS` | `bomward-aarch64-apple-darwin.tar.gz` | `bomward` |
| `Windows` | `bomward-x86_64-pc-windows-msvc.zip` | `bomward.exe` |

`SHA256SUMS` uses `sha256sum` lines: the hex digest, two spaces, and the
asset file name.

A fork pull request triggered by `pull_request` receives a
`GITHUB_TOKEN` that cannot write comments. The Action still writes the
terminal report to the job summary and still finishes with the verdict.
A missing write permission is not a failure. A caller that wants the
comment on a fork pull request uses two workflows: `pull_request` uploads
the report as an artifact, and `workflow_run` on the default branch,
with `pull-requests: write`, posts the comment.

The bomward repository's self-test is the one caller that may set
`BOMWARD_USE_LOCAL=1` and run a binary it built with
`cargo build --release --locked`. Any other caller uses the verified
download.

## Rationale

The policy is the gate. If it came from the pull request, the pull
request would grade itself. The baseline has the same problem: a pull
request that can replace yesterday's SBOM can hide a change. Both files
therefore come from the commit the pull request is trying to update.

The checksum is checked before `chmod` and before any execution, so a
substituted archive never becomes a process. Exit 2 matches
[ADR 0007](0007-exit-codes-and-determinism.md): the gate did not produce
a verdict.

Fork pull requests are a normal way to contribute. Failing the check
because GitHub withheld the comment token would punish a review that
succeeded. The summary still shows the report, and the verdict still
colors the check. The two-workflow pattern is the documented way to get
a comment without granting the untrusted workflow write access.

## Alternatives

- Read the policy from the pull-request tree. Rejected: the change
  under review would choose the rules.
- Build the pull request, or run its scripts, to produce the SBOM.
  Rejected: that executes untrusted code. bomward only diffs SBOM files
  the caller already has.
- Skip the checksum until AP8 signs the release. Rejected: a download
  that is not checked is not a pinned binary. The pin and the digest
  land together.
- Fail the check when the comment cannot be written. Rejected: a fork
  pull request would be red for a permission GitHub removes on purpose.

## Consequences

- The caller checks out the repository with `fetch-depth: 0` so the base
  commit is present for `git show`. The Action does not fetch the
  pull-request head again and does not check out another tree.
- A relative path that escapes the repository (`..`, an absolute path)
  is rejected before `git show`.
- `permissions: pull-requests: write` is required for the comment.
  `contents: read` is enough for the diff.
- AP8 publishes the three assets and `SHA256SUMS` under the tag the
  Action pins. The default pin is `v0.1.0`.
- The self-test workflow is the only place that sets
  `BOMWARD_USE_LOCAL=1`.
- Callers follow [the Action guide](../ACTION.md).
