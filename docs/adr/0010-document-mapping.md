# ADR 0010 — Document to model

- Status: accepted
- Date: 2026-11-11

## Context

ADR 0005 defines the key and the version set. It does not say which
CycloneDX objects become components, which attributes survive a repeated
version, or how a document with no graph is classified. The diff (AP2)
consumes this model. It does not read JSON.

## Decision

### Where components come from

v0.1 reads:

- `metadata.component`, the root;
- every object in the top-level `components` array;
- every object in a nested `components` array, at the depth in ADR 0008.

These are not components in v0.1, and they are not stored:
`services`, `vulnerabilities`, `compositions`, `formulation`, and the
rest of the document (`annotations`, `declarations`, `definitions`,
`citations`, signatures, external references on the BOM). A `bom-ref`
that points at a service is a dangling reference, not a component.

### Identity and version

A valid purl uses ADR 0005 and ADR 0009. The version pair is
`(purl version, epoch qualifier)`. A purl with no version uses the
component's `version` string, or the empty string when that field is
absent. When both are present and they differ, the purl version wins.
The component `version` is not stored as a second version. The document
gets one warning on that component key, kind `version-mismatch`.

An invalid purl, or a missing purl, uses `group` plus `name`. A missing
`group` is the empty string. A missing `name` on that fallback rejects
the document. The error text is `component is missing a name` and does
not include the component. A valid purl does not need `name`.

The same version pair twice is one set element (ADR 0005).

### Attributes of one version

Each value is a sorted set. Order in the file does not matter.

| Attribute | Source | Set element |
| --- | --- | --- |
| licenses | `licenses[]` | `license.id`, `license.name`, and `expression`, each as its own string |
| hashes | `hashes[]` | the pair `(alg, content)` when both are strings |
| supplier | `supplier.name` | the name string |
| scope | `scope` | the scope string (`required`, `optional`, `excluded`, or whatever the file says) |
| type | `type` | the type string |
| purl leftovers | qualifiers removed from the key | `checksum`, `download_url`, `vcs_url`, `file_name`, `vers` |

A field that is absent, or that has a JSON type this table does not
name, contributes nothing. It is not an error. Names, license strings,
and hash strings are stored as they appear, including control characters
and Markdown. They are not truncated.

### The same key and version with different attributes

The stored attribute sets are the union of every occurrence. If a later
occurrence adds an element the set did not already have, the document
gets one warning, kind `conflicting-attributes`, on that component key.
Identical repeats do not warn. A difference is never dropped.

### The dependency graph

The graph is the `dependencies` array, stored as an adjacency list from
`(component key, version)` to `(component key, version)`. `bom-ref` is
resolved to that pair while reading this one document. It is not a field
of the public model, and no API that takes two documents accepts a
`bom-ref`.

`dependencies` is the flat list from ADR 0006. Each `dependsOn` entry
must be a string. A non-string is an invalid document.

A `ref` or a `dependsOn` string that matches no component is a warning,
kind `orphan-ref`, and produces no edge. Several dangling strings on the
same source collapse to one warning. A duplicated `bom-ref` keeps the
first `(key, version)` for resolution; the document gets one warning,
kind `duplicate-bom-ref`. The later component is still stored under its
own key and version.

Edges are a set. A repeated pair is one edge. A cycle, including a
self-loop, is stored as those edges.

Classification of a component, for a later diff:

The role is a field of the version, not of the key. Two versions of one
package are two nodes.

| Role | Meaning |
| --- | --- |
| `root` | The version of the `metadata.component` object. |
| `direct` | Its `bom-ref` is in the `dependsOn` list of the dependency entry whose `ref` is the root version's `bom-ref`. |
| `transitive` | Reachable from a direct version. The walk keeps a visited set of `(key, version)` and stops on a node it has already seen. |
| `unknown` | Not the root version and not reachable. |

When `metadata.component` is absent, or the `dependencies` field is
absent, every version's role is `unknown` and the document gets one
warning, kind `role-unknown`. An empty `dependencies` array is present:
the root's role is `root`, and nothing is direct. How AP2 prints these
roles is not decided here.

`scope` is only the CycloneDX field (`required`, `optional`,
`excluded`, or whatever string the file uses). It is an attribute of a
version. It is not the graph role.

### Warnings

Warnings are a sorted set of `(component key, kind)`. A warning with no
key sorts first. Kinds sort by the order below. The same pair once is
one warning.

| Kind id | When |
| --- | --- |
| `invalid-purl` | A purl was present and not valid. Text: `invalid purl; fell back to group and name`. |
| `orphan-ref` | A `ref` or `dependsOn` matches no component. |
| `duplicate-bom-ref` | Two components share a `bom-ref`. |
| `role-unknown` | No root component, or no `dependencies` field. The role is `unknown`. |
| `conflicting-attributes` | The same key and version carried different attributes. |
| `version-mismatch` | The component `version` and the purl version differ. The purl version is stored. |
| `duplicate-key` | A JSON object repeated a key. The last value is kept. The warning has no component key. |

Warnings do not change the error returned for a valid document. They do
not change the exit code (ADR 0005).

### Header

`bomFormat` must be the string `CycloneDX`. Anything else, including a
missing field, is an invalid document. The text is
`bomFormat must be CycloneDX` and does not echo the field.

`specVersion` is checked immediately after that, before components.
The text is the sentence in ADR 0004. A missing field uses `missing`.
A JSON string is escaped as in ADR 0006 (U+0000–U+001F and U+007F as
`\u{XXXX}`) and then cut at 64 characters. A JSON number, boolean, or
`null` is rendered with its JSON literal, also cut at 64 characters.
An array or object is reported as `invalid`, so a hostile header cannot
fill the error.

## Rationale

The review compares packages. Services, vulnerabilities, and formulation
describe other questions, and reading them would widen the untrusted
surface for no verdict in v0.1.

Union plus a warning is the rule that cannot hide a license or a hash.
Picking the first occurrence, or the last, would drop the other side
without a trace. ADR 0005 already made the repeated version one set
element; this ADR only says what happens to the attributes.

`unknown` is a role, not a failure. Lockfiles and generated SBOMs often
omit the graph. The warning `role-unknown` tells the later report that
the role was not knowable. The diff still has the version sets. The
CycloneDX `scope` field stays an attribute and is not that warning.

## Alternatives

- Treat nested `components` as graph edges. Rejected: the spec uses them
  as an inventory tree. Edges are `dependencies`.
- Fail the document when attributes conflict. Rejected: the union is
  still reviewable, and a hard failure hides every other change.
- Keep `bom-ref` on the public component. Rejected: AP2 would be able
  to compare it. ADR 0005 forbids that across documents.
- Classify a missing graph as transitive. Rejected: that would tell the
  policy a directness the file did not state.

## Consequences

- AP2 diffs version sets and reads `root`, `direct`, `transitive`, and
  `unknown`. It does not re-read `bom-ref`.
- A service-only BOM has an empty component map. That is a valid read.
- Attribute conflicts are visible as warnings. The report (AP4) lists
  them. Ingest only records them.
