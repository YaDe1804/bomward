// SPDX-License-Identifier: AGPL-3.0-only

//! Version direction for `npm` and `cargo`, plus rpm epoch.
//!
//! The rules are ADR 0012. A direction is never guessed.

use std::cmp::Ordering;

use semver::Version;

use crate::model::{ComponentKey, VersionId};

use super::Direction;

pub(super) fn direction(key: &ComponentKey, old: &VersionId, new: &VersionId) -> Direction {
    let ty = purl_type(key);
    match epoch_order(ty, old, new) {
        EpochOrder::Upgrade => Direction::Upgrade,
        EpochOrder::Downgrade => Direction::Downgrade,
        EpochOrder::Unknown => Direction::Unknown,
        EpochOrder::Equal => version_order(ty, old, new),
    }
}

fn purl_type(key: &ComponentKey) -> Option<&str> {
    match key {
        ComponentKey::Purl { ty, .. } => Some(ty.as_str()),
        ComponentKey::Name { .. } => None,
    }
}

enum EpochOrder {
    Upgrade,
    Downgrade,
    Equal,
    Unknown,
}

fn epoch_order(ty: Option<&str>, old: &VersionId, new: &VersionId) -> EpochOrder {
    let Ok(old_epoch) = epoch_number(ty, old.epoch.as_ref()) else {
        return EpochOrder::Unknown;
    };
    let Ok(new_epoch) = epoch_number(ty, new.epoch.as_ref()) else {
        return EpochOrder::Unknown;
    };
    match (old_epoch, new_epoch) {
        (None, None) => EpochOrder::Equal,
        (Some(old_value), Some(new_value)) => match new_value.cmp(&old_value) {
            Ordering::Greater => EpochOrder::Upgrade,
            Ordering::Less => EpochOrder::Downgrade,
            Ordering::Equal => EpochOrder::Equal,
        },
        (None, Some(_)) | (Some(_), None) => EpochOrder::Unknown,
    }
}

/// `Ok(None)` means the epoch is absent and this type does not default it.
/// `Err(())` means a present epoch is not a `u64`.
fn epoch_number(ty: Option<&str>, epoch: Option<&String>) -> Result<Option<u64>, ()> {
    match epoch {
        None if ty == Some("rpm") => Ok(Some(0)),
        None => Ok(None),
        Some(text) => text.parse::<u64>().map(Some).map_err(|_| ()),
    }
}

fn version_order(ty: Option<&str>, old: &VersionId, new: &VersionId) -> Direction {
    if ty != Some("npm") && ty != Some("cargo") {
        return Direction::Unknown;
    }
    let Ok(old_version) = Version::parse(&old.version) else {
        return Direction::Unknown;
    };
    let Ok(new_version) = Version::parse(&new.version) else {
        return Direction::Unknown;
    };
    match old_version.cmp_precedence(&new_version) {
        Ordering::Less => Direction::Upgrade,
        Ordering::Greater => Direction::Downgrade,
        Ordering::Equal => Direction::Unknown,
    }
}
