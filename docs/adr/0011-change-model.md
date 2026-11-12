# ADR 0011 — Change model

- Status: accepted
- Date: 2026-11-12

## Context

Policy (AP3) and the report (AP4) must describe a review without opening
the two documents again. The diff is the only input they get.

[ADR 0005](0005-component-identity.md) already fixes identity: a
component key, plus a set of versions. [ADR 0010](0010-document-mapping.md)
stores the graph role on each version and keeps the CycloneDX `scope`
field as an attribute. A diff that compared only keys, or that treated
`scope` as the role, would hide the changes the policy has to judge.

The kinds below are the policy contract. Their identifiers are stable.

## Decision

### Levels

The comparison walks component keys, then the version set of each key,
then the attributes and the role of a version that exists on both sides.
It does not compare `bom-ref`, and it does not compare document order.

### Change kinds

Identifiers are kebab-case. They sort in the order listed.

| Id | When |
| --- | --- |
| `added` | The version exists only in the new document. |
| `removed` | The version exists only in the old document. |
| `version-changed` | Pairing, defined below. |
| `license-changed` | The license set differs for a version that is in both documents. The detail lists the strings that were added and the strings that were removed. |
| `supplier-changed` | Same, for the supplier set. |
| `hash-changed` | For a version in both documents, the content of an algorithm that exists on **both** sides differs. This is the strongest signal in the diff. It is never mixed with an algorithm that only appeared or only disappeared. |
| `hash-coverage-changed` | An algorithm was added or removed, and no shared algorithm contradicts. The detail separates “every hash disappeared” from every other coverage change. |
| `scope-changed` | The CycloneDX `scope` set differs. Example: `excluded` to `required`. |
| `role-changed` | The graph role differs. Example: `transitive` to `direct`. `unknown` on one side is a real change. |

`type` and the purl leftovers (`checksum`, `download_url`, `vcs_url`,
`file_name`, `vers`) are not change kinds in v0.1.

Hash content is compared with ASCII case folding, so `ABCD` and `abcd`
are the same content. The algorithm name is compared exactly, after
ingest has stored the CycloneDX spelling ([ADR 0010](0010-document-mapping.md)).
`SHA-256` and `sha-256` are therefore one algorithm: different content
is `hash-changed`, and the same content is not a change. When one
version carries two contents for one algorithm, the contents are a set:
the sets differ, or they do not. A shared algorithm whose content sets
differ is `hash-changed`, even if one side has an extra content for that
same algorithm. An algorithm name present on only one side is
`hash-coverage-changed`. If both things happen, the diff carries both
entries. `hash-changed` lists only the contradicting algorithms.
`hash-coverage-changed` lists only the algorithms that were added or
removed, plus a flag that is true when the new side has no hashes and
the old side had some.

License, supplier, and scope details use the same added/removed split.
A set that is equal produces no entry.

CycloneDX defines `scope` as `required`, `optional`, or `excluded`. The
JSON schema for 1.4, 1.5, 1.6, and 1.7 says, on the component `scope`
field: "If scope is not specified, 'required' scope SHOULD be assumed
by the consumer of the BOM." The field's default is `required`. The
model still stores an absent field as an empty set, so it shows what
the document contained. The diff is the consumer that
assumes the default: an empty scope set compares as `required`.
`required` against a missing scope is not `scope-changed`. `excluded`
against a missing scope is `scope-changed`, and the detail names
`required` for the side that omitted the field.

### Pairing a version change

A key produces one `version-changed` when the old document has **exactly
one** version of that key, the new document has **exactly one**, and the
two version ids differ.

Every other shape stays a set change. Added versions are `added`.
Removed versions are `removed`. The diff does not choose which old
version was replaced by which new one. In particular, two versions on
one side and three on the other are only `added` and `removed`.

The entry sorts by the **new** version id. `added` sorts by the new
version, `removed` by the old version, and an attribute or role change
by the shared version. Kind order is the table above. The version order
is the order of `VersionId`, which is lexicographic on the version
string and then the epoch. It is not a SemVer order. Direction is a
separate field ([ADR 0012](0012-version-ordering.md)).

### Attributes on `version-changed`

The entry carries both attribute sets, so a license change that arrives
with the update is not dropped. It does **not** also emit
`license-changed`, `supplier-changed`, `hash-changed`,
`hash-coverage-changed`, `scope-changed`, or `role-changed` for that
pair.

The same entry carries one flag per those kinds, set when that
comparison would have produced an entry. AP3 can write a rule for
“update that also changes the license” without a second row and without
double-counting the summary.

### Roles

Roles are compared only when both documents have a graph. A document
has a graph when it has a root and a `dependencies` field. Ingest
records the opposite with one `role-unknown` warning
([ADR 0010](0010-document-mapping.md)): no `metadata.component`, or no
`dependencies` field. An empty `dependencies` array is a graph.

The diff sets `roles_comparable` from that fact. When it is false, the
diff emits no `role-changed` and sets no role flag on `version-changed`.
`old_role` and `new_role` are still copied, so a report can show them.
When both documents have a graph, a change from or to `unknown` on one
component is `role-changed`.

Every dependency change carries `old_role` and `new_role`. Either may
be absent. `added` has no old role. `removed` has no new role. The
others have both.

A later policy uses the role in the new document. For `removed` it uses
the role in the old document. The summary counts each kind once under
that role. The root role is not a bucket in those counts, because a
root version is not a dependency change.

### Root component

`metadata.component` is the project, not a dependency. The diff stores
the old root and the new root when those snapshots differ, including
when one side has no root. A snapshot is the key, the version id, the
role, and the attributes of the version whose role is `root`.

That version is left out of the dependency comparison on the side where
it is root. A root version change is therefore only a root entry. Another
version of the same key, with a role other than `root`, is still compared
as a dependency.

### Known limit

If a component loses or gains its purl, its key changes from a purl key
to `group` plus `name`, or the other way around ([ADR 0005](0005-component-identity.md)).
The diff reports `removed` plus `added`. It does not guess that the two
keys are the same component.

### Provenance of an added version

An `added` entry also says why the version is in the new document.
Policy and the report must not walk the graph themselves.

- `introduced_by`: the nodes in the new document that directly depend on
  the new version, in `GraphNode` order.
- `path_from_root`: one shortest path from the root version to the new
  version. The search is breadth-first and keeps a visited set, so a
  cycle ends the walk. When several nodes could extend a shortest path,
  the smaller `GraphNode` is taken. The path includes the root and the
  new version. It is absent when the version is not reachable or the
  document has no root.

Both lists are capped by a named constant of 32, the same number as the
default component-nesting depth. The entry also stores the real total.
Hitting the cap does not drop that number. `version-changed` does not
carry provenance: the version was already in the old document under the
same key.

### What else the diff carries

- The warnings of each document, copied, separated into old and new.
  They are not re-sorted and not rewritten.
- A summary: the count of each kind, and the same counts split by the
  policy role above. A root entry is not counted.
- No timestamps, paths, or input order.

Two documents with the same components, the same graph roles, and the
same root produce no changes and a zero summary. Warnings are still
copied. `diff(a, a)` is that empty change list.

The diff cannot fail. It does not read files and it does not panic.
Its memory is bounded by the two documents plus the two caps of 32.

## Rationale

The one-version pairing rule covers the update a reviewer actually
sees: one release replaced by one release. Pairing inside a larger set
would invent a correspondence the documents do not state. Reporting
that invention as `upgrade` or `downgrade` is worse than two set
changes the reviewer can read.

Keeping attribute flags on `version-changed`, instead of extra rows,
leaves one event as one summary count. The flags still let a policy
distinguish a plain update from an update that changes the license or
the hash.

Separating `hash-changed` from `hash-coverage-changed` keeps “same
version, different bytes” from “the SBOM started or stopped recording
an algorithm”. The first is the strongest signal in the review. Mixing
them would let a missing hash look like a tampered package.

The root is the project under review, not a dependency of itself.
Putting it in the dependency list would make a version bump of the
project look like a new dependency.

## Alternatives

- Always emit `added` and `removed`, and never `version-changed`.
  Rejected: the common one-to-one update would have no direction and no
  single row for AP3 to allow or deny.
- Pair versions by nearest SemVer distance whenever the set sizes
  differ, including “two removed, one added”. Rejected: the nearest
  string is still a guess, and it is a false direction for every type
  that has no SemVer order.
- Emit `license-changed` in addition to `version-changed` when the
  license set differs. Rejected: the summary would count one update
  twice, and a rule on `license-changed` would match updates and
  in-place edits without a way to tell them apart. The flag on the
  update is that distinction.
- Store a missing `scope` as `required` during ingest. Rejected: the
  model would hide that the field was absent. The diff applies the
  default the spec assigns to the consumer.
- Compare roles when one document has no graph. Rejected: every
  component would change from or to `unknown`, which the file did not
  state. `roles_comparable` tells the policy to skip that comparison.
- Treat a lost purl as the same component via `group` and `name`.
  Rejected: [ADR 0005](0005-component-identity.md) made the purl the
  key on purpose. A heuristic repair would undo that for the one case
  where the purl is the fact that changed.

## Consequences

- AP3 matches the identifiers in the table. A rule sees `old_role` and
  `new_role`, and for `version-changed` the attribute flags and the
  direction from ADR 0012.
- A dependency change never has the role `root`. How a policy names the
  separate root entry is left to AP3.
- `version-changed` has no `introduced_by` and no path. AP3 does not
  get a path for an update unless a later ADR adds one.
- The canonical JSON of a `Diff` is a test encoding. It is not a public
  contract, same as the model encoding.
