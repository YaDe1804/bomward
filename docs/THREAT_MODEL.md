# Threat model

bomward judges untrusted SBOM files and prints into CI logs and pull-request
comments. This document lists the assets, the threats that matter for v0.1,
and the mitigation that is in force. Mitigations that are limits or output
rules are decided in [ADR 0006](adr/0006-untrusted-input.md).

## Assets

- The exit code. A crafted SBOM must not turn a violation into a pass, or a
  broken file into a pass.
- The report text. A component name must not become an instruction to the
  terminal or a HTML/Markdown injection in a pull-request comment.
- The machine running the tool. Parsing a SBOM must not run package code,
  open a network connection, or read files the user did not name.
- The bomward build itself. A compromised dependency is in scope for the
  tool's own supply chain, and out of scope for the SBOMs under review.

## Assumptions

- The operator passes the two SBOM paths and the optional policy path.
- The CI trust boundary is those files. Everything inside them is hostile.
- bomward is not a vulnerability scanner. It does not know whether a
  package is malicious. It knows whether the *change* breaks the policy.

## Threats and mitigations

### Hostile document

A SBOM can be huge, deeply nested, cyclic, or full of duplicate objects.
That is a denial-of-service against the CI job, or a panic that a caller
might misread as a pass.

Mitigation: the defaults reject a file larger than 32 MiB, a document
with more than 100,000 components, or component nesting deeper than 32.
That nesting means `components` inside `components`. It does not mean
the length of a dependency chain, and it does not mean JSON container
depth. Each default has a flag (`--max-file-bytes`, `--max-components`,
`--max-depth`). A violation names the limit, the value found, and that
flag, and exits 2. `--max-depth` above 63 is a usage error: that is the
deepest component tree the JSON cap still holds. JSON container depth
is capped at 127, independent of the flag, so a nested bomb is refused
before `serde_json` builds it (ADR 0008). That error does not name
`--max-depth`. A dependency cycle is stored as data.
Every graph walk keeps a visited set, so a cycle cannot spin. Library
code does not panic.
`unsafe` is forbidden, so a parse bug cannot become a memory-safety bug
inside bomward.

### Output injection

Component names, licenses, and supplier strings are copied into Markdown
that GitHub renders, and into terminal logs that interpret ANSI escapes.

Mitigation: terminal and Markdown output escape control characters
(U+0000–U+001F and U+007F), including newline and ESC, as `\u{XXXX}`.
Markdown output also escapes `` \ ` * _ { } [ ] ( ) # + - . ! | < > ``.
JSON output uses JSON string encoding only. The terminal format emits no
ANSI color. The successful report does not include local paths.

### Path confusion

A SBOM can contain strings that look like paths (`../../etc/passwd`,
absolute Windows paths, package locations).

Mitigation: those strings are data. bomward never opens them and never
joins them onto a base directory. The only files opened are the paths
from the command line.

### Code execution and network

A component may refer to an install script, a repository URL, or a
registry. Following that reference would run or fetch untrusted content.

Mitigation: no network client, no shell, no execution of component
artifacts. The tool reads the two documents and the policy file and stops.

### Supply chain of bomward

The review gate is itself a dependency of the user's CI.

Mitigation: crates.io is the only crate source. `Cargo.lock` is committed.
`cargo deny` checks advisories, the license allow-list, and registries.
Duplicate versions are a warning. Wildcard requirements are denied.
`cargo audit` runs in CI. GitHub Actions are pinned to a commit SHA.
Dependabot tracks `cargo` and `github-actions`. The dependency set stays
small; each new crate is justified in the commit that adds it.

## Out of scope

- Whether a listed package is malware, vulnerable, or typosquatted.
- Authenticity of the SBOM (signatures, attestations). v0.1 trusts the
  operator to pass the files they mean to compare.
- Availability of GitHub, the registry, or the advisory database. bomward
  does not contact them at review time. `cargo deny` and `cargo audit` do
  contact the advisory database in CI, which is the tool's build, not the
  review.
