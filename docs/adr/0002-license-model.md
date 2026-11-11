# ADR 0002 — License model

- Status: accepted
- Date: 2026-11-11

## Context

bomward is a security tool meant to sit in other people's CI. The public
project needs a license that keeps modifications available, including when
the tool is offered as a service. The copyright holder also needs a path
for organizations that cannot accept that obligation.

The copyright holder is Deniz Yayvan, personally, as a freelancer.
Contact: `deniz.yayvan@yadelabs.de`.

## Decision

bomward is dual-licensed, on the same pattern as Lockward:

- The public license is **AGPL-3.0-only** (`LICENSE`).
- A commercial license from Deniz Yayvan is the alternative for use that
  cannot meet the AGPL. Terms are in `COMMERCIAL-LICENSE.md`.
- Contributions are accepted only under the grant in `CLA.md`, so a
  contribution can be shipped under either license.

`Cargo.toml` records `AGPL-3.0-only`. Dependencies must use licenses
`cargo-deny` allows in `deny.toml`. That list is permissive licenses
compatible with AGPL-3.0, plus MPL-2.0, plus AGPL-3.0-only itself.
GPL-2.0-only is not allowed.

## Rationale

AGPL-3.0-only matches the sister project and covers the case where
someone runs a modified bomward as a network service and would otherwise
keep the source closed. A commercial second license is the explicit
alternative for closed-source embedding and for organizations that
cannot use the AGPL. The CLA is what makes the second license possible
without asking each contributor again.

"Only" and not "or-later" keeps a future AGPL revision from applying
automatically.

## Alternatives

- Apache-2.0 only. Rejected for this decision. It was the documented
  alternative: choosing it would drop `COMMERCIAL-LICENSE.md` and
  `CLA.md`. It does not require service-side source sharing.
- AGPL-3.0-or-later. Rejected: a later Free Software Foundation text
  would become an upstream license change without a new decision.
- No CLA. Rejected: contributions would be AGPL-only and could not be
  relicensed commercially.

## Consequences

- `COMMERCIAL-LICENSE.md` and `CLA.md` stay in the repository. Both are
  drafts for legal review, as in Lockward. They are not a substitute
  for that review.
- The commercial offer is made by Deniz Yayvan personally, not by a
  company.
- Adding a dependency is a license decision. `cargo deny check` fails
  the gate when a crate uses a license outside the allow-list.
