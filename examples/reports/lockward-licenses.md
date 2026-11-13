# bomward review

**Result:** Warning

**Exit code:** 0

**Mode:** enforce

**Decisions:** 0 denials, 1 warning, 8 allowed

**Changes:** 3 added, 1 removed, 2 version-changed, 1 license-changed, 1 scope-changed, 1 role-changed

## Warnings

- `npm` `licensed`@`1.0.0` license-changed direct
  Licenses added: `Apache-2.0`. Removed: `MIT`.
  Rules: `notice-apache`.

<details>
<summary>Allowed (8)</summary>

- `npm` `bumped`@`1.1.0` version-changed direct
  Version changed from `1.0.0` to `1.1.0` (upgrade).
  Decision from default.
- `npm` `devkit`@`1.0.0` scope-changed direct
  Scopes added: `required`. Removed: `excluded`.
  Decision from default.
- `npm` `fresh`@`1.0.0` added direct
  Added. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `gone`@`1.0.0` removed direct
  Removed. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `multi`@`3.0.0` added transitive
  Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `multi`@`3.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `nested-new`@`1.0.0` added transitive
  Added. Introduced by `npm` `fresh`@`1.0.0`. Path from the root: `npm` `review-app`@`1.0.0`, `npm` `fresh`@`1.0.0`, `npm` `nested-new`@`1.0.0`. Licenses: `MIT`. Suppliers: none. Scopes: `required`. Hash algorithms: `SHA-256`.
  Decision from default.
- `npm` `promoted`@`1.0.0` role-changed direct
  Role changed from transitive to direct.
  Decision from default.
- `npm` `rolled`@`1.0.0` version-changed direct
  Version changed from `2.0.0` to `1.0.0` (downgrade).
  Decision from default.

</details>

bomward 0.1.0
