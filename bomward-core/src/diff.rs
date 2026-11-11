// SPDX-License-Identifier: AGPL-3.0-only

//! Compare two normalized models.
//!
//! The diff reports components that were added or removed, and changes of
//! version, license, hash, or supplier. It joins on component identity and
//! does not compare `bom-ref` across the two documents. The comparison is not
//! implemented in this milestone.
