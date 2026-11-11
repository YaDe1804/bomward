// SPDX-License-Identifier: AGPL-3.0-only

//! Library behind the bomward SBOM review gate.
//!
//! Data flows from [`ingest`] through [`model`], [`diff`], and [`policy`] to
//! [`report`]. This crate does not access the network and does not execute
//! untrusted code.
//!
//! The modules are seams. This foundation milestone does not implement them.

#![forbid(unsafe_code)]

pub mod diff;
pub mod error;
pub mod ingest;
pub mod model;
pub mod policy;
pub mod report;

pub use error::Error;
