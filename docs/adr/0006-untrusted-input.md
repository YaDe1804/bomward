# ADR 0006 — Untrusted input

- Status: accepted
- Date: 2026-11-11

## Context

The files under review are attacker-controlled. They arrive through CI
from pull requests. bomward is a security tool: a parse failure has to
be an error, and a component name has to be data, including when a
pull-request comment renders the report as Markdown.

## Decision

bomward is static.

- No network access, no shell, no execution of foreign code.
- Open only the paths the user passed. Never build a path, URL, or
  command from a string inside a SBOM or a policy file.
- The size, component-count, and depth limits have defaults. Each one
  is overridable with a flag on `diff`:

  | Limit | Default | Flag |
  | --- | --- | --- |
  | one input file | 32 MiB (33,554,432 bytes) | `--max-file-bytes` |
  | component objects, including nested ones | 100,000 | `--max-components` |
  | nesting of `components` inside `components`, and JSON nesting while parsing | 32 | `--max-depth` |

- `--max-depth` does **not** limit the path length of the dependency
  graph. A path is bounded by the component count, because every walk
  carries a visited set and stops when it meets a component it has
  already seen.
- A cycle in the dependency graph is data. It is stored with the edges
  that declared it. It is not an input error and it has no flag.
- A limit violation is an error (exit 2). The message names the limit,
  the value that was found, and the flag that raises it:

  ```text
  file size {found} exceeds limit {limit} (--max-file-bytes)
  component count {found} exceeds limit {limit} (--max-components)
  nesting depth {found} exceeds limit {limit} (--max-depth)
  ```

- Over-limit input does not panic and does not truncate. A document that
  only contains a cycle is accepted, up to the component-count limit.
- Do not write a SBOM string into terminal or Markdown output unchanged.
  Encode U+0000–U+001F and U+007F as `\u{XXXX}` (four uppercase hex
  digits). That includes tab, newline, and ESC.
  In Markdown, also escape `` \ ` * _ { } [ ] ( ) # + - . ! | < > ``.
- JSON output uses JSON string encoding and no second escape layer.
- The terminal format emits no ANSI color sequences of its own.
- Fixtures are inert documents written for the tests. No malware
  samples and no copied malicious packages.

The defaults are the v0.1 contract. Changing a default is an ADR change.
A single run may raise a limit with the flag for that limit.

## Rationale

32 MiB and 100,000 components are enough for a large application SBOM
and small enough that a CI job can refuse a pathological file quickly.
Depth 32 covers real `components` trees and stops a nested JSON bomb.
A visited set stops a graph walk that would otherwise not terminate, so
a cycle does not have to be an error. Real lockfiles contain cycles
(workspaces that depend on each other, optional peer edges). Rejecting
them would fail a review that has nothing to do with the policy.

Checked against the CycloneDX JSON schemas on 2026-11-11. In JSON 1.4,
1.5, 1.6, and 1.7, `dependencies` is a flat adjacency list. Each item is
`{ "ref": "<bom-ref>", "dependsOn": ["<bom-ref>", ...] }`. The
`dependsOn` entries are strings. They are not nested dependency objects,
so a long chain or a cycle does not add JSON depth. The XML schema has
historically allowed nested `dependency` elements; CycloneDX
specification issue 146 records that the JSON schema and the XSD
disagree. v0.1 reads JSON only (ADR 0004), so that XML shape is out of
scope. A JSON array element inside `dependsOn` that is not a string is
an invalid document, not a depth violation and not a cycle.

Escaping, rather than stripping, keeps the review honest: the reader can
see that the name contained a control character. Stripping would hide
the payload. Refusing to follow paths and URLs from the document keeps
the tool inside the files the operator named.

## Alternatives

- No size limit, rely on memory. Rejected: one CI variable then holds
  the availability of the runner.
- Truncate long strings and keep going. Rejected: a truncated name can
  hide the difference the policy was meant to judge.
- Shell out to a CycloneDX CLI. Rejected: that is foreign code and a
  process spawn on untrusted paths.
- Allow network "only for license lookup". Rejected: the product rule
  is no network, and a lookup would leak the dependency set.

## Consequences

- Ingest enforces the limits before and during the parse, so a huge
  file is refused without holding the whole payload past the cap.
- `--max-depth` counts two things: how deep `components` is nested
  inside `components`, and how deep JSON arrays and objects are nested
  while parsing. It does not count edges in `dependencies`. Component
  nesting is compared with the flag value. JSON container nesting is
  compared with the ceiling in [ADR 0008](0008-cyclonedx-parser.md),
  which is wide enough for a component tree of that depth. Both
  failures use the nesting-depth sentence above.
- Graph walks in this crate and in later diff code use a visited set.
  They do not reject the document because a cycle exists.
- The report module owns escaping. Callers pass untrusted strings
  through that module and do not format them themselves.
- `bomward-core` must not gain a HTTP or process dependency. The
  dependency set in `deny.toml` stays on crates.io and on the license
  allow-list; a client such as `reqwest` is a new ADR, not a `cargo add`.
- The numbers above are the defaults. A real SBOM that exceeds one of
  them is passed with that run's flag. Changing the default for every
  run is an amendment of this ADR.
- The violation text is part of the contract. AP1 uses the three
  sentences above and does not invent a second shape.
