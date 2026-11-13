# ADR 0015 — Report formats and escaping

- Status: accepted
- Date: 2026-11-13

## Context

The report is the review. It is rendered three times from one diff and
an optional verdict: terminal text, a GitHub pull-request comment, and
JSON for another program. [ADR 0006](0006-untrusted-input.md) told
Markdown to backslash-escape a punctuation list. GitHub Flavored
Markdown still autolinks a bare URL, turns `@name` into a notification,
and links `#123` and a 40-character commit hash. None of those characters
are in that list. A code span is the construct that suppresses all three.

The same SBOM string can also move a terminal cursor. C1 controls
(U+0080–U+009F) and bidirectional controls do that even after C0 and
DEL are encoded.

## Decision

One catalog builds every sentence. Terminal, Markdown, and JSON render
that catalog. They do not invent a second wording. Rule ids and change
ids are copied as the policy and the diff spell them.

Untrusted text is every string that came from a SBOM: names, groups,
versions, purl parts, licenses, suppliers, hash algorithms, and hash
contents. A policy file is foreign text as well. The report shows rule
ids from that file and no other policy prose (a description is not
rendered). Those ids are values, written the same way as SBOM strings.

### Terminal

Plain text, no ANSI sequences, no wrapping to a terminal width, LF only.
Encode these characters as `\u{XXXX}` (four uppercase hex digits):

- U+0000–U+001F and U+007F
- U+0080–U+009F (C1)
- U+061C, U+200E, U+200F
- U+2028, U+2029, U+202A–U+202E
- U+2066–U+2069

A value longer than 256 Unicode scalars is cut at 256, then an ellipsis
and the total number of scalars are added. JSON is not cut.

### Markdown

Untrusted text is written only inside a code span. The span uses enough
backticks to exceed the longest backtick run in the value (CommonMark).
A value that starts or ends with a backtick or a space is padded with
one space inside the fence. CommonMark otherwise joins the fence to a
backtick, or drops one leading and one trailing space. A value that is
only spaces is not padded: CommonMark keeps those spaces. The characters
listed for the terminal are encoded before the span is built, so a
newline cannot leave the span.

A value with no characters is not a code span. Two backticks are an
opening fence, and the next span can close the wrong fence, which puts
a later value in the prose. Markdown and the terminal write the word
`(empty)` instead. That word is report prose. JSON keeps `""`.

Report prose (headings and sentences the report wrote) is ordinary text.
The only HTML the report emits is `<details>` and `<summary>` around allowed
entries and around the notes. Denied entries and warnings stay visible.

`MARKDOWN_CHAR_BUDGET` is 60,000 Unicode scalars, under GitHub's 65,536
character comment limit. When the comment would be longer, allowed
entries are dropped first, then warnings, then denied entries. The cut
says how many entries of that class are missing and that the JSON report
is complete. Denied entries are never the first class to be cut.

### JSON

`report_version` is `1`. Enumeration values are lowercase English.
`message` is a sentence from the catalog and is not a contract. The
document is complete: no display cut, no comment budget. It has no
timestamp, no path, and no host name. Object keys stay in the order of
the schema in `docs/report.schema.json`.

Field names shared with Lockward, where the concept is the same:

| Field | Why |
| --- | --- |
| `report_version` | The document's own version, not the tool version. |
| `gate` | The mode, the result, and `exit_code`. |
| `configured` and `effective` | The decision before and after `mode`. Lockward calls these `configured_action` and `effective_action`. The `_action` suffix is omitted: bomward stores a decision (`allow`, `warn`, `deny`), not a Lockward action (`review`, `fail`). |
| `rules` | The ids that matched. |

Differences, on purpose:

- `gate` has both `configured` and `effective` (`pass`, `warn`, `deny`).
  Lockward stores one `outcome`. Warn mode can turn `deny` into `warn`,
  so one word would hide the configured result. `weakened_denials` counts
  those units.
- `gate.mode` is `enforce`, `warn`, or `report-only`.
- `policy_applied` and `notice`. Report-only sets `policy_applied` to
  `false` and `notice` to the exact sentence `No policy was applied.`
- `units` is the list. Lockward's `violations` is a different product.
- A purl key is the canonical purl without a version. A key that has no
  purl uses `group` and `name` instead of a fabricated purl.
- `root`, `roles_comparable`, and `warnings.old` / `warnings.new` have no
  Lockward equivalent.

### Counts and order

Decision counts are the effective decisions of the units. Change counts
are the change units, by kind. Both match the units that were built,
before a Markdown budget drops any of them. The dropped lines are not a
second copy of the change.

Units are ordered by effective decision: `deny`, then `warn`, then
`allow`. Inside a decision, document conditions come first, in verdict
order, then changes in diff order. Report-only has no decision. Its
units follow the diff, with a root change before the dependency changes.

An empty diff (no dependency changes and no root change) says
`No dependency changes.` and does not open an empty section.

## Rationale

A code span is the only GFM construct that turns off autolinks,
mentions, and issue references at once. Backslash escapes do not.
Encoding C1 and bidirectional controls costs a few characters and closes
the terminal channel those code points exist to operate. Cutting the
Markdown comment, and never cutting JSON, keeps the pull-request
comment inside GitHub's limit without hiding a denial from a program.

Sharing Lockward's `report_version` and `gate` names makes a reader of
both tools recognize the block. Copying `configured_action` would imply
Lockward's action vocabulary, which bomward does not use.

## Alternatives

- Keep ADR 0006's backslash list and add `@` and `:`. Rejected: GitHub
  autolinks `www.evil.example` and a bare hash with no extra punctuation
  to escape, and a new character would start another amendment.
- HTML-escape the whole comment. Rejected: the comment would be harder
  to read, and GitHub would still autolink text that is not escaped as
  HTML.
- Omit allowed entries from Markdown entirely. Rejected: the reviewer
  can open them, and the JSON file is not what GitHub shows.

## Consequences

- [ADR 0006](0006-untrusted-input.md) is amended by this ADR. The
  backslash list is no longer the Markdown rule.
- The report module owns escaping. Callers pass SBOM strings through it.
- A golden Markdown file is parsed with `comrak` (MIT or Apache-2.0,
  dev-only). The test enables GFM autolink and requires no link, image,
  or autolink node, and no HTML other than the report's own
  `<details>` and `<summary>`.
- A golden JSON file is checked with the `jsonschema` crate (MIT,
  dev-only) against `docs/report.schema.json`.
- Display length 256 and the 60,000 scalar budget are named constants.
  Changing either is an amendment of this ADR.
