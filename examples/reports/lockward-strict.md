# bomward 0.1.0

**Result:** Deny

**Exit code:** 1

**Mode:** enforce

**Decisions:** 1 denial, 8 warnings, 0 allowed

**Changes:** 3 added changes, 1 removed change, 2 version-changed changes, 1 license-changed change, 0 supplier-changed changes, 0 hash-changed changes, 0 hash-coverage-changed changes, 1 scope-changed change, 1 role-changed change

## Denied

Deny `npm` `rolled`@`1.0.0` version-changed direct

Version changed from `2.0.0` to `1.0.0` (downgrade).

Rules: `no-downgrade`.

## Warnings

Warning `npm` `bumped`@`1.1.0` version-changed direct

Version changed from `1.0.0` to `1.1.0` (upgrade).

Decision from default.

Warning `npm` `devkit`@`1.0.0` scope-changed direct

Scopes added: `required`. Removed: `excluded`.

Rules: `scope-becomes-required`.

Warning `npm` `fresh`@`1.0.0` added direct

Added. Introduced by `npm` `review-app`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`. Hash contents: `abcd`.

Rules: `new-direct`.

Warning `npm` `gone`@`1.0.0` removed direct

Removed. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`. Hash contents: `abcd`.

Decision from default.

Warning `npm` `licensed`@`1.0.0` license-changed direct

Licenses added: `Apache-2.0`. Removed: `MIT`.

Decision from default.

Warning `npm` `multi`@`3.0.0` added transitive

Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `multi`@`3.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`. Hash contents: `abcd`.

Decision from default.

Warning `npm` `nested-new`@`1.0.0` added transitive

Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `nested-new`@`1.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`. Hash contents: `abcd`.

Decision from default.

Warning `npm` `promoted`@`1.0.0` role-changed direct

Role changed from transitive to direct.

Decision from default.

<details>
<summary>Notes</summary>

Warnings in the new document: none.

Warnings in the old document: none.

</details>

