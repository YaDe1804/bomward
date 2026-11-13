# bomward review

**Result:** Warning

1 denial reported as a warning because the policy runs in warn mode

**Exit code:** 0

**Mode:** warn

**Decisions:** 0 denials, 3 warnings, 6 allowed

**Changes:** 3 added, 1 removed, 2 version-changed, 1 license-changed, 1 scope-changed

## Warnings

- `npm` `devkit`@`1.0.0` scope-changed direct
  Scopes added: `required`. Removed: `excluded`.
  Rules: `scope-becomes-required`.
- `npm` `fresh`@`1.0.0` added direct
  Added. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Rules: `new-direct`.
- `npm` `rolled`@`1.0.0` version-changed direct
  Version changed from `2.0.0` to `1.0.0` (downgrade).
  Rules: `no-downgrade`.

<details>
<summary>Allowed (6)</summary>

-  graph-unavailable
  Document condition graph-unavailable.
  Decision from default.
- `npm` `bumped`@`1.1.0` version-changed direct
  Version changed from `1.0.0` to `1.1.0` (upgrade).
  Decision from default.
- `npm` `gone`@`1.0.0` removed unknown
  Removed. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `licensed`@`1.0.0` license-changed direct
  Licenses added: `Apache-2.0`. Removed: `MIT`.
  Decision from default.
- `npm` `multi`@`3.0.0` added transitive
  Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `multi`@`3.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `nested-new`@`1.0.0` added transitive
  Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `nested-new`@`1.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.

</details>

<details>
<summary>Notes</summary>

Roles were not compared because at least one document has no dependency graph.

Warnings in the old document: 1 role-unknown warning (no package).

</details>

bomward 0.1.0
