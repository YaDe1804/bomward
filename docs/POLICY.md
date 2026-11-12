# Policy

A policy file decides what bomward does with a diff. It is YAML, written
by the operator. A typo, an unknown field, or a repeated key is an error.
The tool does not guess and does not warn: the run stops before a verdict
exists. That failure is exit code 2, the same bucket as a broken SBOM or
a bad flag.

The schema and the evaluation rules below are version 1.
[ADR 0013](adr/0013-policy-schema.md) records why they are shaped this way.
[ADR 0014](adr/0014-yaml-parser.md) records the parser.

Three files under `examples/policies/` are ready to copy:

| File | What it does |
| --- | --- |
| `recommended.yaml` | `mode: warn`. Downgrades, hash contradictions, dropped hash algorithms, and a newly arrived GPL or AGPL license are `deny`, which this mode reports as `warn`. A new direct dependency is `warn`. |
| `strict.yaml` | The same rules with `mode: enforce` and `default: warn`. A missing graph and a duplicate key in the new SBOM are `deny`. |
| `licenses.yaml` | License rules only, including an `unless` exception. The exception is written with `%40`, which is the canonical spelling of an npm scope. |

## File

```yaml
version: 1                 # required. The integer 1. "1" and 1.0 are errors.
mode: enforce              # required. warn or enforce.
default: allow             # required. allow, warn, or deny.
unknown_role: direct       # optional. direct (the default) or transitive.
rules:                     # required. May be empty.
  - id: no-downgrade
    description: A downgrade needs a person to look at it.
    when:
      change: [version-changed]
      direction: [downgrade]
    decision: deny
```

`version`, `mode`, `default`, and `rules` are required. `unknown_role`
defaults to `direct` when it is omitted. An empty `rules` list is valid:
every unit then takes `default`.

A rule has:

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Unique. Matches `[a-z0-9][a-z0-9-]{0,63}`. |
| `description` | no | At most 500 characters. Shown by a later report. Not matched. |
| `when` | yes | The matchers below. |
| `unless` | no | Package patterns this rule skips. Change rules only. |
| `decision` | yes | `allow`, `warn`, or `deny`. |

A rule is either a change rule or a document rule. Setting both is an
error. A `when` with no matcher is an error. Matching every change is
written `change: [any]`.

Fields in `when` combine with AND. Values inside one list combine with OR.
An empty list is an error.

## Change matchers

`change` lists one or more of:

`added`, `removed`, `version-changed`, `license-changed`,
`supplier-changed`, `hash-changed`, `hash-coverage-changed`,
`scope-changed`, `role-changed`, or the single value `any`.

`any` is not combined with another value. Omitting `change` on a change
rule matches every change kind. `direction` and `coverage` still require
the kind to be written out, so `change: [any]` plus `direction` is an error.

A rule that names `license-changed`, `supplier-changed`, `hash-changed`,
`hash-coverage-changed`, `scope-changed`, or `role-changed` also matches a
`version-changed` row that carries that flag. A license that arrives inside
an upgrade is still a license change. The other matchers then see the
version before the update and the version after it.

| Field | Values | When it is legal |
| --- | --- | --- |
| `direction` | `upgrade`, `downgrade`, `unknown` | `change` lists `version-changed` |
| `role` | `direct`, `transitive`, `unknown` | change rule |
| `license` | SPDX license ids | change rule |
| `missing` | `license`, `hash`, `supplier` | change rule |
| `coverage` | `algorithm-added`, `algorithm-removed`, `all-dropped` | `change` lists `hash-coverage-changed` |
| `scope` | `required`, `optional`, `excluded` | change rule |
| `package` | patterns, see below | change rule |

`role` is the new document's role. For `removed` it is the old role.
`root` is not a value: the root component is the document condition
`root-changed`, not a dependency change.

`unknown_role: direct` (the default) means a version whose role is
`unknown` matches both `role: [unknown]` and `role: [direct]`.
`unknown_role: transitive` matches `unknown` and `transitive`. A generated
SBOM with no graph would otherwise walk past every direct-dependency rule.

`license` matches an SPDX id inside a license string, not a substring.
Comparison ignores ASCII case. `GPL-3.0-only` does not match
`LGPL-3.0-only`. `GPL-3.0` does not match `GPL-3.0-or-later`. `OR`, `AND`,
and `WITH` still match the id: `MIT OR GPL-3.0-only` matches `GPL-3.0-only`,
and `GPL-2.0-only WITH Classpath-exception-2.0` matches `GPL-2.0-only`.
A string that is not a valid SPDX expression matches nothing.

On a `license-changed` row, and on a `version-changed` row whose licenses
differ, only the license strings that were added are scanned. A license
that was only removed does not match. On every other row the licenses of
the version under review are scanned. That version is the new one, and for
`removed` the old one.

`missing` is true when that version has no license, no hash, or no
supplier. The list is OR: any one of the named gaps is enough.

`scope` is the scope set after the change, with the same default the diff
uses. An empty scope is `required`. The list matches when any named scope
is in that set.

`coverage` looks at algorithms that appeared, disappeared, or were all
dropped. On a version update the two versions are compared. `all-dropped`
means the new version has no hashes and the old version had some.

## Package patterns

Patterns are compared to the canonical package URL of the component, without
the version. The spelling is the one ADR 0009 produces, so an npm scope is
percent-encoded:

```text
pkg:npm/%40acme/tool
```

`pkg:npm/@acme/*` does not match that URL. Write `%40`.

| Pattern | Matches |
| --- | --- |
| `*` | any run of characters other than `/` |
| `**` | any run, including `/` |
| anything else | that character, literally |

A pattern must start with `pkg:`, except the exact pattern `**`.
`***` is rejected. No other character is special: `?` and `[` are literal.

A component that has no package URL, only `group` and `name`, matches the
exact pattern `**` and nothing else. `pkg:**` does not name it.

`unless` uses the same patterns. It skips that rule for the matching
packages. It does not cancel a different rule. A `deny` from another rule
still stands.

## Document conditions

`document` is a list. Any one value matching is enough. The ids are:

| Id | When the unit exists |
| --- | --- |
| `root-changed` | `metadata.component` differs |
| `graph-unavailable` | At least one document has no dependency graph |
| a warning id | That warning is present on the **new** document |

Warning ids, in the order they are judged: `invalid-purl`, `orphan-ref`,
`duplicate-bom-ref`, `role-unknown`, `conflicting-attributes`,
`version-mismatch`, `duplicate-key`, `unreachable`, `no-components`,
`unknown-hash-algorithm`.

One unit is produced per warning id, not per component. Warnings from the
old document are copied onto the verdict so a report can list them. No
rule can name them in this version.

`unless` and the change matchers are not valid on a document rule.

## Decisions

Each unit gets one decision.

1. Every rule that matches is collected. The ids are stored sorted.
2. The strictest matching decision wins: `deny`, then `warn`, then `allow`.
3. An `allow` never cancels a `deny`. The only exception is `unless` on
   the deny rule itself.
4. If no rule matches, `default` is used and the unit says so.

`mode: enforce` keeps that decision. `mode: warn` turns a configured
`deny` into an effective `warn`. The unit stores both. A warning-mode
policy can be tightened later by changing one word, without editing rules.

The verdict's overall result is the strictest effective decision:

| Strictest effective decision | Overall | Exit code |
| --- | --- | --- |
| `allow`, or no units | `pass` | 0 |
| `warn` | `warn` | 0 |
| `deny` | `deny` | 1 |

The same fold of the configured decisions is stored beside it. Exit code 1
is only the effective `deny`. A file in `mode: warn` does not fail the run.
Exit code 2 is a file that could not be read or was not valid. The CLI
applies these codes in a later work package. The library exposes
`Verdict::exit_code` for that.

Rule order in the file is not an input. Reordering rules does not change
the verdict. Document units come first (`root-changed`, then
`graph-unavailable`, then warning ids), then the dependency changes in
diff order.

## Limits

These are the defaults. A caller of the library can set lower ones.

| Limit | Default |
| --- | --- |
| File size | 1 MiB |
| Rules | 1000 |
| Entries in one list | 256 |
| `description` | 500 characters |
| Package pattern | 256 characters |

A file past the limit is rejected with the limit and the value that was
found. It is not truncated. The size is checked before the file is parsed.

YAML aliases are rejected, including a nested alias bomb. Anchors that are
never referenced are harmless. A repeated key is an error. Unknown fields
are errors. The parser's error names a line and a column when it has them.
Checks that run after parsing name the rule id and the field. Text taken
from the file is cut at 64 characters, and control characters are written
as `\u{XXXX}`.

Indentation follows YAML 1.2 as the strict parser reads it: indent in
multiples of two spaces.

## What a verdict contains

`bomward_core::policy::evaluate` takes a loaded policy and a diff. It
cannot fail and it does not read files. The verdict carries the mode, the
configured and effective overall results, one unit per document condition
and per change, the matching rule ids, whether `default` applied, counts
of each decision, and the old document's warnings.

`Verdict::to_canonical_json` is a test encoding. It is not a stable
document format. The report, in a later work package, is what people read.
