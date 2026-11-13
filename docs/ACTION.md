# GitHub Action

**Status: pre-release.** The Action is in this repository. The release
tag it downloads, `v0.1.0`, is not published yet. A foreign workflow
that uses the default pin fails at the download until that tag exists.
The trust boundary is [ADR 0017](adr/0017-github-action.md).

The Action compares the pull-request SBOM with the baseline SBOM and,
when a policy path is set, judges the diff. It writes the terminal
report to the job summary, uploads the JSON report as an artifact, and
updates one pull-request comment. It does not generate an SBOM, and it
does not run code from the pull request.

## Example

```yaml
name: bomward

on:
  pull_request:

permissions:
  contents: read
  pull-requests: write

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          fetch-depth: 0
      - uses: YaDe1804/bomward@v0.1.0
        with:
          sbom: path/to/sbom.cdx.json
          policy: path/to/policy.yaml
```

`fetch-depth: 0` is required. The Action reads the baseline and the
policy with `git show <base-sha>:<path>` and does not fetch them itself.
The new SBOM is the file at `sbom` in the tree the caller checked out.

## Inputs

| Input | Required | Default | Meaning |
| --- | --- | --- | --- |
| `sbom` | yes | — | Path of the new SBOM in the pull-request tree. Relative, and it cannot escape the repository. |
| `policy` | no | empty | Path of the policy in the base commit. Empty is report-only: exit 0, no gate. |
| `old-sbom` | no | the `sbom` path | Path of the baseline SBOM in the base commit. |
| `comment` | no | `true` | Update the pull-request comment, or create it. |
| `fail-on-deny` | no | `true` | A denial (exit 1) fails the check. `false` still writes the report and then exits 0. |
| `version` | no | `v0.1.0` | Release tag. The asset is verified against `SHA256SUMS` from that release. |
| `max-file-bytes` | no | CLI default (33554432) | Passed through only when set. |
| `max-components` | no | CLI default (100000) | Passed through only when set. |
| `max-depth` | no | CLI default (32) | Passed through only when set. |

Paths are repository-relative. An absolute path, a drive path, an empty
segment, `.`, or `..` is rejected with exit 2.

## Outputs

| Output | Meaning |
| --- | --- |
| `exit-code` | The bomward verdict: 0, 1, or 2. With `fail-on-deny: false` the step can exit 0 while this output is still 1. |
| `terminal` | Path of the terminal report file. A later step can read this path; the job summary of the Action step is not visible to the next step. |
| `markdown` | Path of the Markdown report file, the same text the comment uses. |

The terminal report is also appended to the Action step's `$GITHUB_STEP_SUMMARY`. The JSON
report is uploaded as the artifact `bomward-report-<job>`. The comment
body is the Markdown report. An existing comment is the one whose body
contains `<!-- bomward-review -->`; the Action updates that comment
instead of adding a second one.

## Exit codes

| Code | When | Check |
| --- | --- | --- |
| 0 | Pass, warning, or report-only. Also a denial when `fail-on-deny` is `false`. | green |
| 1 | The policy denies the diff and `fail-on-deny` is `true`. The comment and the summary are written first. | red |
| 2 | Usage, a missing base file, a bad release, a checksum mismatch, or bomward failing before a verdict. | red |

A checksum mismatch exits 2 before the archive is extracted and before
the binary is started.

## Trust model

From the pull request, the Action reads one file: the new SBOM, as data.
The policy and the baseline are read from the base commit named by the
`pull_request` event (`base.sha`). When that event has no base, the
Action uses `GITHUB_BASE_REF` and `GITHUB_SHA`.

The Action does not run a script, a hook, or a build from the
pull-request tree. The binary comes from the pinned release of
`YaDe1804/bomward`. AP8 publishes these assets, and `SHA256SUMS` with
`sha256sum` lines (`<hex>  <filename>`):

| Runner | Asset | Binary inside |
| --- | --- | --- |
| `Linux` | `bomward-x86_64-unknown-linux-gnu.tar.gz` | `bomward` |
| `macOS` | `bomward-aarch64-apple-darwin.tar.gz` | `bomward` |
| `Windows` | `bomward-x86_64-pc-windows-msvc.zip` | `bomward.exe` |

The bomward self-test is the one workflow that sets
`BOMWARD_USE_LOCAL=1`. That builds `cargo build --release --locked` in
this repository and skips the download. Any other caller uses the
verified release.

## Permissions and fork pull requests

| Permission | Why |
| --- | --- |
| `contents: read` | Read the checkout and the base commit. |
| `pull-requests: write` | Create or update the comment. |

On a fork pull request, `pull_request` gives the token read access only.
The comment API returns 403. The Action appends the report to the job
summary and exits with the verdict. The missing permission is not a
failure.

A comment on that fork pull request uses two workflows. `pull_request`
runs the Action (the JSON artifact is already uploaded) and does not
need write access. `workflow_run`, triggered by that workflow and
running on the default branch, has `pull-requests: write` and posts the
comment from the artifact. The untrusted workflow never receives the
write token.
