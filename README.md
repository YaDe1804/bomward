# bomward

bomward is an SBOM review gate for pull requests and releases. It compares
two CycloneDX SBOMs, explains the change in plain language, judges it
against a policy, and returns an exit code for CI.

It is not another generic SBOM diff. `sbomdiff` and `sbom-tools` already
report differences. bomward answers a review question: may this change
pass, and why?

**Status: pre-release.** The formats and the exit codes below are the
v0.1 contract. The GitHub Action is in this repository. The release
binary it downloads is not published yet.

```text
bomward diff bomward-core/tests/fixtures/lockward-old.cdx.json bomward-core/tests/fixtures/lockward-new.cdx.json --policy examples/policies/strict.yaml --format markdown
```

That command exits 1. The Markdown it writes is
[examples/reports/lockward-strict.md](examples/reports/lockward-strict.md).
The opening of that report:

```markdown
# bomward review

**Result:** Deny

**Exit code:** 1

**Mode:** enforce

**Decisions:** 1 denial, 8 warnings, 0 allowed

**Changes:** 3 added, 1 removed, 2 version-changed, 1 license-changed, 1 scope-changed, 1 role-changed

## Denied

- `npm` `rolled`@`1.0.0` version-changed direct
  Version changed from `2.0.0` to `1.0.0` (downgrade).
  Rules: `no-downgrade`.
```

Omit `--policy` for a report that does not judge the change. That run
exits 0 and every format includes the sentence `No policy was applied.`

The contract is [docs/SPEC.md](docs/SPEC.md). Policy files are described
in [docs/POLICY.md](docs/POLICY.md). The three report formats are
described in [docs/REPORT.md](docs/REPORT.md). The GitHub Action is
described in [docs/ACTION.md](docs/ACTION.md).

## GitHub Action

The composite Action runs `bomward diff` as a review gate. It reads the
new SBOM from the pull request as data. The policy and the baseline SBOM
come from the base commit, so the pull request cannot change the rules
or the comparison. The binary is a pinned release asset checked against
`SHA256SUMS` before it is executed. [ADR 0017](docs/adr/0017-github-action.md)
records that boundary.

The caller needs `contents: read` and, for the comment,
`pull-requests: write`. Check out the repository with `fetch-depth: 0`
so the base commit is present. The Action does not check out another
tree and does not build the pull request.

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

`v0.1.0` is the default pin. That tag is not published yet. Until it
exists, a foreign repository cannot download the binary.

The step outputs `exit-code`, `terminal`, and `markdown`. `terminal` and
`markdown` are the paths of those report files, so a later step can read
the review. The job summary written during the Action step stays on that
step.

A fork pull request triggered by `pull_request` gets a token that cannot
write comments. The Action writes the report to the job summary, keeps
the verdict, and does not fail because the comment was skipped. A
comment on a fork pull request needs two workflows: `pull_request`
uploads the report artifact, and `workflow_run` on the default branch,
with `pull-requests: write`, posts the comment.

## License

Licensed under AGPL-3.0-only. See [LICENSE](LICENSE).

A commercial license is available from Deniz Yayvan. See
[COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md). Contributions require the
agreement in [CLA.md](CLA.md).

Copyright © 2026 Deniz Yayvan.
