// SPDX-License-Identifier: AGPL-3.0-only

//! Judge a diff against the review policy.
//!
//! Policy decides whether a change may pass. It does not format the report
//! and it does not read SBOM files. The rule schema is not parsed in this
//! milestone.
