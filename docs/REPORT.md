# Report

`bomward diff` writes one review in one of three formats. The sentences
are the same in all three. Rule ids and change ids are copied as the
policy and the diff spell them. [ADR 0015](adr/0015-report-formats-and-escaping.md)
records the format rules. [STYLE.md](STYLE.md) is the word list.

```text
bomward diff OLD NEW [--policy FILE] [--format terminal|markdown|json] [--output FILE]
```

`--format` defaults to `terminal`. The report goes to stdout. `--output`
writes those same bytes to a file and leaves stdout empty. The file is
created or replaced. Parent directories are not created.

## What the header says

The first lines name the tool and its version, the result, the exit
code, and the mode.

| Mode | Result line | Exit code |
| --- | --- | --- |
| report-only (no `--policy`) | `No policy was applied.` | 0 |
| `warn` or `enforce`, nothing denied | `Pass` or `Warning` | 0 |
| `enforce`, and a unit is denied | `Deny` | 1 |

`warn` mode turns a configured denial into a warning and says so:
`2 denials reported as warnings because the policy runs in warn mode`.
The exit code stays 0. The header counts, the listed entries, and the
exit code describe that same state.

An empty diff says `No dependency changes.` and does not add empty
sections.

## Terminal

Plain text. No color, no wrapping to the width of the terminal, and
line endings are LF.

Text that came from an SBOM is encoded before it is printed. These
characters become `\u{XXXX}`: C0 controls and DEL, C1 controls
(U+0080–U+009F), and the bidirectional and line controls U+061C,
U+200E, U+200F, U+2028, U+2029, U+202A–U+202E, and U+2066–U+2069.
A value longer than 256 Unicode scalars is cut there, then an ellipsis
and the total number of scalars are added.

## Markdown

This is the text for a pull-request comment. Denied entries and
warnings stay visible. Allowed entries and the notes sit in `<details>`
blocks. Those are the only HTML tags the report writes.

A string from an SBOM is written only inside a code span. The span uses
more backticks than the value contains, so a backtick in a name cannot
end the span. The same characters as in the terminal format are encoded
first, so a newline cannot leave the span. Headings, sentences, and
rule ids are ordinary text. Rule ids match `[a-z0-9][a-z0-9-]{0,63}`.

GitHub comments stop at 65,536 characters. The Markdown report stays
under 60,000 Unicode scalars (`MARKDOWN_CHAR_BUDGET`). When it would be
longer, allowed entries are dropped first, then warnings, then the
notes, and denied entries last. The cut says how many entries of that
class were omitted, of how many, and that the JSON report is complete.

## JSON

`report_version` is `1`. The document is the complete review: no display
cut and no comment budget. It has no timestamp, no path, and no host
name. Keys stay in the order of [report.schema.json](report.schema.json).

Stable fields:

| Field | Meaning |
| --- | --- |
| `tool.name`, `tool.version` | `bomward` and the crate version. |
| `policy_applied` | `false` in report-only mode. |
| `notice` | The sentence `No policy was applied.`, or `null` when a policy was applied. |
| `gate.mode` | `report-only`, `warn`, or `enforce`. |
| `gate.configured`, `gate.effective` | `pass`, `warn`, or `deny`. Both are `null` in report-only mode. |
| `gate.weakened_denials` | How many configured denials `warn` mode reported as warnings. |
| `gate.exit_code` | `0` or `1`, the same code the process returns. |
| `counts` | Decision totals and one total per change kind. |
| `units` | Every judged change or document condition, once. |
| `root` | The project component before and after, or `null`. |
| `roles_comparable` | Whether both documents had a dependency graph. |
| `warnings.old`, `warnings.new` | Every warning from each document. |

Enumeration values are lowercase English: `allow`, `warn`, `deny`,
`upgrade`, `downgrade`, `unknown`, and the change ids (`added`,
`removed`, `version-changed`, and the rest). `message` is the sentence
a person reads. It is not a contract. Programs should use the structured
fields.

A unit's `purl` is the canonical package URL without a version. A
component that has no package URL leaves `purl` null and fills `group`
and `name`. The report does not invent a URL for that component.

The schema is [report.schema.json](report.schema.json). Every example
under [examples/reports/](../examples/reports/) validates against it.
