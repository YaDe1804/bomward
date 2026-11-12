# Lockward fixture pair

These two lockfiles were written for bomward. They are inert. They do not
describe real packages and they are not fetched from a registry.

Lockward 0.2.0 turned each file into a CycloneDX 1.6 document:

```text
lockward scan bomward-core/tests/fixtures/lockward/old --offline --cache-dir <temp> --format cyclonedx --scanned-at 2026-01-01T00:00:00Z --output bomward-core/tests/fixtures/lockward-old.cdx.json
lockward scan bomward-core/tests/fixtures/lockward/new --offline --cache-dir <temp> --format cyclonedx --scanned-at 2026-01-01T00:00:00Z --output bomward-core/tests/fixtures/lockward-new.cdx.json
```

The binary was `lockward-cli` 0.2.0, built from `../lockward` with
`cargo run -p lockward-cli --locked`. Offline mode warns about advisory
coverage and exits 1. The SBOM is still written. `--scanned-at` is fixed
so the timestamp does not depend on the clock.

`lockward-old-format.cdx.json` was not produced by Lockward. It was
derived by hand from `lockward-old.cdx.json` by deleting the
`dependencies` array and every `bom-ref`. That is the shape an older
Lockward wrote: the same components, with no graph and no `bom-ref`.

What the pair is built to show, old then new:

| Package | Old | New | Intended diff |
| --- | --- | --- | --- |
| `fresh` | absent | direct | `added` |
| `nested-new` | absent | under `fresh` | `added`, transitive |
| `gone` | direct | absent | `removed` |
| `bumped` | 1.0.0 | 1.1.0 | `version-changed`, upgrade |
| `rolled` | 2.0.0 | 1.0.0 | `version-changed`, downgrade |
| `licensed` | MIT | Apache-2.0 | `license-changed` |
| `devkit` | dev, scope `excluded` | runtime, scope `required` | `scope-changed` |
| `promoted` | only via `carrier` | also a direct dependency | `role-changed` |
| `multi` | 1.0.0 and 2.0.0 | 1.0.0, 2.0.0, and 3.0.0 | `added` for 3.0.0, no `version-changed` |
| `cycle-a`, `cycle-b` | each depends on the other | unchanged | a cycle, no change |

`@scope/left-pad` and `ansi-regex` are unchanged. One is `required`, the
other is `excluded`. Lockward does not emit a supplier, so this pair has
no `supplier-changed`. Hashes are identical, so it has no `hash-changed`
and no `hash-coverage-changed`.
