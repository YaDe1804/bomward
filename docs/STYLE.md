# Output style

English is the only language. Write the sentence a reviewer can check, then stop.

## Sentences

- Plain and specific. No exclamation marks. No slang.
- Sentence case in headings: "Denied", not "DENIED".
- One term for one thing. Use the glossary below. Do not alternate synonyms.
- A change entry names the package, then what changed, then the rule ids.
  Markdown writes that as one list item. The section heading already
  says Denied, Warnings, or Allowed, so the item does not repeat it.
  The terminal still starts a judged line with `Deny`, `Warning`, or
  `Allow`.

```
- npm left-pad@1.1.0 version-changed direct
  Version changed from 1.0.0 to 1.1.0 (upgrade).
  Rules: no-downgrade.
```

The changes line lists only kinds with a count above zero, and it does
not repeat the word "change": `3 added, 1 removed, 1 version-changed`.
No changes is the sentence `No dependency changes.`

Hash contents are JSON only. Markdown and the terminal name the
algorithms. `Introduced by` and `Path from the root` are written only
for a transitive package. A notes line that says `none` is omitted.

## Glossary

| Term | Use it for |
| --- | --- |
| change | One dependency difference: `added`, `removed`, `version-changed`, `license-changed`, `supplier-changed`, `hash-changed`, `hash-coverage-changed`, `scope-changed`, or `role-changed`. Not "finding" and not "violation". |
| role | Where a version sits in the graph: `root`, `direct`, `transitive`, or `unknown`. Not "scope". Scope is the CycloneDX field. |
| direction | `upgrade`, `downgrade`, or `direction unknown` on a `version-changed` entry. Not "bump". |
| verdict | The policy judgment of one diff: mode, decisions, and rule ids. |
| unit | One judged subject. Either one change or one document condition. |
| decision | `allow`, `warn`, or `deny` on a unit. The display labels are `Allow`, `Warning`, and `Deny`. |
| result | The strictest effective decision of the run: `Pass`, `Warning`, or `Deny`. |
| report-only | A run with no policy file. The sentence is `No policy was applied.` |
| default | The policy's `default` decision, used when no rule matched. Say "Decision from default." |
| project component | `metadata.component`. Not "the root dependency". |
| package | One component key and one version. The readable form is the ecosystem, the decoded name, and the version. |

## Labels and identifiers

Display labels and JSON identifiers are different.

| Value | JSON identifier | Display label |
| --- | --- | --- |
| Result pass | `pass` | `Pass` |
| Result warn | `warn` | `Warning` |
| Result deny | `deny` | `Deny` |
| Mode | `enforce`, `warn`, `report-only` | the same words |
| Decision | `allow`, `warn`, `deny` | `Allow`, `Warning`, `Deny` |
| Change kind | the id, such as `version-changed` | the same id |
| Role | `root`, `direct`, `transitive`, `unknown` | the same words |

`message` is an English sentence from the report catalog. It is not a stable contract. Rule ids and change ids do not change.

## Plurals

A count of zero or of two or more uses the plural. A count of one uses the singular. Write both forms. Do not emit "1 denials".

```
0 denials
1 denial
2 denials

0 warnings
1 warning
2 warnings

0 allowed
1 allowed
2 allowed
```

The same rule applies to omitted entries ("1 allowed entry omitted", "2 allowed entries omitted") and to listed packages.

## What not to add

No localization catalog. No `--lang`. No second copy of a sentence. No ANSI color.
