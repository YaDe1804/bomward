// SPDX-License-Identifier: AGPL-3.0-only

//! Library behind the bomward SBOM review gate.
//!
//! Data flows from [`ingest`] through [`model`], [`diff`], and [`policy`] to
//! [`report`]. This crate does not access the network and does not execute
//! untrusted code.
//!
//! Ingest, the model, and the diff are implemented. Policy and report still
//! return [`Error::NotImplemented`].

#![forbid(unsafe_code)]

pub mod diff;
pub mod error;
pub mod ingest;
pub mod limits;
pub mod model;
pub mod policy;
pub mod report;

mod purl;

pub use error::{DocumentError, Error, LimitExceeded, LimitKind, UnsupportedDepth};
pub use ingest::{read_bytes, read_path};
pub use limits::Limits;
pub use model::{
    Component, ComponentKey, ContentHash, DependencyRole, Document, GraphNode, Version,
    VersionAttributes, VersionId, Warning, WarningKind,
};
