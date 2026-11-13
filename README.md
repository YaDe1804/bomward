# bomward

bomward is an SBOM review gate for pull requests and releases. It compares
two CycloneDX SBOMs, explains the change in plain language, judges it
against a policy, and returns an exit code for CI.

It is not another generic SBOM diff. `sbomdiff` and `sbom-tools` already
report differences. bomward answers a review question: may this change
pass, and why?

**Status: pre-release.** The formats and the exit codes below are the
v0.1 contract. There is no GitHub Action and no published binary yet.

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
described in [docs/REPORT.md](docs/REPORT.md).

## License

Licensed under AGPL-3.0-only. See [LICENSE](LICENSE).

A commercial license is available from Deniz Yayvan. See
[COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md). Contributions require the
agreement in [CLA.md](CLA.md).

Copyright © 2026 Deniz Yayvan.
