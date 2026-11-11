// SPDX-License-Identifier: AGPL-3.0-only

//! Read a caller-supplied `CycloneDX` JSON file into the normalized model.
//!
//! Ingest is the only module that interprets SBOM bytes. It enforces the size,
//! component-count, and depth limits. Those limits have defaults and can be
//! raised per run. It does not open paths found inside the document. Parsing
//! is not implemented in this milestone.
