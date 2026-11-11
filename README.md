# bomward

bomward is an SBOM review gate for pull requests and releases. It compares
two CycloneDX SBOMs, explains the change in plain language, judges it
against a policy, and returns an exit code for CI.

It is not another generic SBOM diff. `sbomdiff` and `sbom-tools` already
report differences. bomward answers a review question: may this change
pass, and why?

**Status: pre-release, not usable yet.** The `diff` command is parsed and
then exits with code 2. It does not read SBOMs and it does not apply a
policy.

The contract is [docs/SPEC.md](docs/SPEC.md).

## License

Licensed under AGPL-3.0-only. See [LICENSE](LICENSE).

A commercial license is available from Deniz Yayvan. See
[COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md). Contributions require the
agreement in [CLA.md](CLA.md).

Copyright © 2026 Deniz Yayvan.
