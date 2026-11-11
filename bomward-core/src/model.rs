// SPDX-License-Identifier: AGPL-3.0-only

//! Normalized components and their identities.
//!
//! Identity is the Package URL without its version and without
//! version-dependent qualifiers. Each key holds a set of versions. A missing
//! or invalid Package URL uses `group` plus `name`; an invalid one also
//! warns. `bom-ref` is not an identity. The model types arrive with ingest.
