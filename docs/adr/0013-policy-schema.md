# ADR 0013 — Policy schema and evaluation

- Status: accepted
- Date: 2026-11-12

## Context

AP2 produces a typed diff. AP3 has to judge that diff against a file the
operator wrote. The foundation only sketched the file. A sketch that says
`deny` outranks `allow` and then stops is not enough to implement, and it
is not enough for a later report to explain a verdict.

The policy is configuration. A typo, an unknown field, or a repeated key
must not become a quiet allow. The order of rules in the file must not
change the verdict. A license or hash change that arrives inside a version
update must not slip past a rule written for that kind of change.

## Decision

Schema version 1 is YAML. The fields, the matchers, and the evaluation
rules are the contract in [POLICY.md](../POLICY.md). This ADR records why
they are shaped that way. The user-facing page does not require this ADR.

### File

`version` is required and is the integer 1. `mode` is required and is
`warn` or `enforce`. `default` is required and is `allow`, `warn`, or
`deny`. `unknown_role` is optional and is `direct` (the default) or
`transitive`. `rules` is required and may be empty.

A rule has a unique `id` matching `[a-z0-9][a-z0-9-]{0,63}`, an optional
`description`, a required `decision` (`allow`, `warn`, or `deny`), and
either change matchers or `document`, never both. A rule with no matcher
is an error. Matching every change is written `change: [any]`.

Change matchers are AND across fields and OR within a list. `direction`
is valid only when `change` lists `version-changed`. `coverage` is valid
only when `change` lists `hash-coverage-changed`. `any` is not combined
with another change kind, and it does not unlock `direction` or
`coverage`. `unless` is optional package patterns and is valid only on a
change rule.

`document` lists `root-changed`, `graph-unavailable`, and any warning id
from [ADR 0010](0010-document-mapping.md). Those warning ids are read
from the **new** document only.

### Evaluation

Each dependency change and each document condition that applies is one
unit. Each unit gets one decision.

A rule whose `change` list names `license-changed`, `supplier-changed`,
`hash-changed`, `hash-coverage-changed`, `scope-changed`, or
`role-changed` also matches a `version-changed` that carries that flag.
The other matchers then see the attributes before and after the update.

The role is the new document's role, and for `removed` the old one. With
`unknown_role: direct`, a version whose role is `unknown` matches both
`role: [unknown]` and `role: [direct]`. With `transitive`, it matches
`unknown` and `transitive`. A missing graph must not make a direct-dependency
rule go quiet.

A license id matches when `spdx` parses the license string and that id is
one of the expression's license tokens, compared without regard to case.
`GPL-3.0-only` does not match `LGPL-3.0-only`. `GPL-3.0` does not match
`GPL-3.0-or-later`. The version after the change is scanned. For `removed`,
that is the old version. For `license-changed`, and for a `version-changed`
that carries the license flag, only the license strings that were added
are scanned. `MIT OR GPL-3.0-only` matches `GPL-3.0-only`. A choice in the
expression does not hide an id. A string that is not a valid SPDX
expression matches nothing.

Package patterns are compared with the canonical purl of the key, without
version, as [ADR 0009](0009-purl-normalization.md) spells it
(`pkg:npm/%40acme/tool`). `*` is any run of characters other than `/`.
`**` is any run, including `/`. No other character is special. A component
whose key is `group` plus `name` matches only a pattern that is exactly
`**`. A pattern that does not start with `pkg:` is an error, except `**`.
The pattern is literal against that canonical text, so `@` is not `%40`.

`scope` is the scope set after the change. An empty set is `required`,
the same default the diff uses. `missing` means the version after the
change has no license, no hash, or no supplier.

Among matching rules the strictest decision wins: `deny`, then `warn`,
then `allow`. No `allow` cancels a `deny`. An exception is `unless`.
The unit records every matching rule id, sorted. If none match, `default`
applies and the unit says so.

`mode: warn` turns each configured `deny` into an effective `warn`.
`mode: enforce` keeps the configured decision. The unit stores both.
The verdict's overall result is the strictest effective decision, named
`pass` when every unit is `allow`, otherwise `warn` or `deny`. The same
fold is stored for the configured decisions. The exit code is 1 exactly
when the effective overall result is `deny`, and 0 otherwise
([ADR 0007](0007-exit-codes-and-determinism.md)). The CLI applies that
code in a later work package.

Document conditions are ordered `root-changed`, then `graph-unavailable`,
then the warning ids in the order of [ADR 0010](0010-document-mapping.md).
Change units follow in the diff's order. Warnings from the old document
are copied onto the verdict so a report can list them. No rule can name
them in v0.1.

Rule order in the file is not an input. Reordering rules does not change
the verdict bytes.

### Suite alignment

Lockward's decisions are `allow | warn | review | fail`, and its exit
codes are `0 ok, 1 warn, 2 fail`. bomward's decisions are
`allow | warn | deny`, and its exit codes are `0 pass, 1 deny, 2 error`.
This ADR records the difference and does not close it. Aligning the two
tools is a later decision for a shared suite.

## Rationale

First-match, as a firewall does it, was the obvious alternative and it
fails the second principle: the verdict would depend on who edited the
file last. Strictest-wins plus `unless` keeps exceptions on the rule
they belong to.

Treating an unknown role as direct by default is the stricter reading.
A generated SBOM with no graph would otherwise sail through every
`role: [direct]` rule.

Scanning every license id in an `OR` expression is the stricter reading.
A dual-licensed package that may be taken under GPL still brought GPL
into the tree. The alternative, matching only when every choice is
denied, would hide that.

`**` as the only pattern for a name key avoids a guess. `pkg:npm/*` does
not name that component, and pretending it does would deny or allow a
package the pattern did not mention.

## Alternatives

- First matching rule wins. Rejected: the verdict depends on file order.
- Let `allow` override an earlier `deny`. Rejected: an exception belongs
  on the rule, as `unless`, not on a later rule that happens to sort
  first or last.
- Match a license id as a substring. Rejected: `GPL-3.0-only` would match
  `LGPL-3.0-only`.
- Treat `MIT OR GPL-3.0-only` as not GPL because the user might choose
  MIT. Rejected: the file still offers GPL. The stricter reading is the
  one this gate is for.
- Compare package patterns to the raw purl text from the SBOM. Rejected:
  [ADR 0009](0009-purl-normalization.md) already decided the canonical
  form. Patterns follow it, including `%40`.

## Consequences

- [POLICY.md](../POLICY.md) is the page operators read.
- The evaluator cannot fail and does no I/O. A bad file fails at load,
  as exit 2, before a verdict exists.
- Old-document warnings are visible and not matchable until a later ADR
  says otherwise.
- The suite alignment above stays open.
