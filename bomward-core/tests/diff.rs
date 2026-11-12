//! Diff tests. Documents are built in memory or read from inert fixtures.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use bomward_core::diff::{AttributeFlag, Change, ChangeDetail, ChangeKind, Diff, Direction, diff};
use bomward_core::{
    ComponentKey, ContentHash, DependencyRole, Document, GraphNode, Limits, Version,
    VersionAttributes, VersionId, Warning, WarningKind, read_bytes,
};

fn empty_doc() -> Document {
    Document {
        components: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        warnings: Vec::new(),
    }
}

fn npm(name: &str) -> ComponentKey {
    typed("npm", name)
}

fn typed(ty: &str, name: &str) -> ComponentKey {
    ComponentKey::Purl {
        ty: ty.to_owned(),
        namespace: None,
        name: name.to_owned(),
        qualifiers: BTreeMap::new(),
        subpath: None,
    }
}

fn named(name: &str) -> ComponentKey {
    ComponentKey::Name {
        group: String::new(),
        name: name.to_owned(),
    }
}

fn ver(version: &str) -> VersionId {
    VersionId {
        version: version.to_owned(),
        epoch: None,
    }
}

fn ver_epoch(version: &str, epoch: &str) -> VersionId {
    VersionId {
        version: version.to_owned(),
        epoch: Some(epoch.to_owned()),
    }
}

fn put(document: &mut Document, key: ComponentKey, version: VersionId, role: DependencyRole) {
    put_attrs(document, key, version, role, VersionAttributes::default());
}

fn put_attrs(
    document: &mut Document,
    key: ComponentKey,
    version: VersionId,
    role: DependencyRole,
    attributes: VersionAttributes,
) {
    document
        .components
        .entry(key)
        .or_default()
        .versions
        .insert(version, Version { attributes, role });
}

fn link(document: &mut Document, from: GraphNode, to: GraphNode) {
    document.dependencies.entry(from).or_default().insert(to);
}

fn node(key: ComponentKey, version: &str) -> GraphNode {
    GraphNode {
        key,
        version: ver(version),
    }
}

fn licenses(values: &[&str]) -> VersionAttributes {
    let mut attributes = VersionAttributes::default();
    for value in values {
        attributes.licenses.insert((*value).to_owned());
    }
    attributes
}

fn kinds(diff: &Diff) -> Vec<ChangeKind> {
    diff.changes.iter().map(|change| change.kind).collect()
}

fn one(diff: &Diff, kind: ChangeKind) -> Result<&Change, String> {
    let matches = diff
        .changes
        .iter()
        .filter(|change| change.kind == kind)
        .collect::<Vec<_>>();
    match (matches.first(), matches.len()) {
        (Some(change), 1) => Ok(*change),
        _ => Err(format!("{matches:?}")),
    }
}

fn key_name(key: &ComponentKey) -> &str {
    match key {
        ComponentKey::Purl { name, .. } | ComponentKey::Name { name, .. } => name,
    }
}

#[test]
fn an_identical_document_is_an_empty_diff() -> Result<(), String> {
    let mut document = empty_doc();
    put(
        &mut document,
        npm("left"),
        ver("1.0.0"),
        DependencyRole::Direct,
    );
    document.warnings.push(Warning {
        key: None,
        kind: WarningKind::DuplicateKey,
    });
    let result = diff(&document, &document);
    if !result.is_empty() || result.summary.counts != bomward_core::diff::ChangeCounts::default() {
        return Err(format!("{result:?}"));
    }
    if result.warnings.old.len() != 1 || result.warnings.new.len() != 1 {
        return Err("warnings were not copied".to_owned());
    }
    Ok(())
}

#[test]
fn added_and_removed_are_opposites() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(&mut old, npm("gone"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut new, npm("fresh"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut old, npm("stay"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut new, npm("stay"), ver("1.0.0"), DependencyRole::Direct);
    let forward = diff(&old, &new);
    if kinds(&forward) != [ChangeKind::Added, ChangeKind::Removed] {
        return Err(format!("{:?}", kinds(&forward)));
    }
    let added = one(&forward, ChangeKind::Added)?;
    let removed = one(&forward, ChangeKind::Removed)?;
    if key_name(&added.key) != "fresh"
        || added.old_role.is_some()
        || added.new_role != Some(DependencyRole::Direct)
    {
        return Err(format!("{added:?}"));
    }
    if key_name(&removed.key) != "gone" || removed.new_role.is_some() {
        return Err(format!("{removed:?}"));
    }
    let backward = diff(&new, &old);
    let added_names = names_of(&forward, ChangeKind::Added);
    let removed_back = names_of(&backward, ChangeKind::Removed);
    if added_names != removed_back {
        return Err(format!("{added_names:?} vs {removed_back:?}"));
    }
    Ok(())
}

fn names_of(diff: &Diff, kind: ChangeKind) -> BTreeSet<String> {
    diff.changes
        .iter()
        .filter(|change| change.kind == kind)
        .map(|change| key_name(&change.key).to_owned())
        .collect()
}

#[test]
fn one_version_each_is_version_changed_with_direction() -> Result<(), String> {
    let upgrade = version_pair("npm", "1.0.0", None, "1.1.0", None)?;
    expect_direction(&upgrade, Direction::Upgrade)?;
    let downgrade = version_pair("npm", "2.0.0", None, "1.0.0", None)?;
    expect_direction(&downgrade, Direction::Downgrade)?;
    let unknown = version_pair("pypi", "1.0.0", None, "2.0.0", None)?;
    expect_direction(&unknown, Direction::Unknown)?;
    let cargo = version_pair("cargo", "1.9.0", None, "1.10.0", None)?;
    expect_direction(&cargo, Direction::Upgrade)?;
    let same = version_pair("npm", "1.0.0", None, "1.0.0", None)?;
    if kinds(&same).contains(&ChangeKind::VersionChanged) {
        return Err("equal versions were a version change".to_owned());
    }
    Ok(())
}

fn version_pair(
    ty: &str,
    old_version: &str,
    old_epoch: Option<&str>,
    new_version: &str,
    new_epoch: Option<&str>,
) -> Result<Diff, String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    let old_id = match old_epoch {
        Some(epoch) => ver_epoch(old_version, epoch),
        None => ver(old_version),
    };
    let new_id = match new_epoch {
        Some(epoch) => ver_epoch(new_version, epoch),
        None => ver(new_version),
    };
    put(&mut old, typed(ty, "pkg"), old_id, DependencyRole::Direct);
    put(&mut new, typed(ty, "pkg"), new_id, DependencyRole::Direct);
    let result = diff(&old, &new);
    if old_version == new_version && old_epoch == new_epoch {
        return Ok(result);
    }
    if kinds(&result) != [ChangeKind::VersionChanged] {
        return Err(format!("{:?}", kinds(&result)));
    }
    Ok(result)
}

fn expect_direction(diff: &Diff, direction: Direction) -> Result<(), String> {
    let change = one(diff, ChangeKind::VersionChanged)?;
    let ChangeDetail::VersionChanged {
        direction: found, ..
    } = &change.detail
    else {
        return Err(format!("{:?}", change.detail));
    };
    if *found != direction {
        return Err(format!("{found:?}"));
    }
    if diff.changes.iter().any(|change| {
        matches!(
            change.kind,
            ChangeKind::LicenseChanged
                | ChangeKind::Added
                | ChangeKind::Removed
                | ChangeKind::HashChanged
        )
    }) {
        return Err("version change also emitted another kind".to_owned());
    }
    Ok(())
}

#[test]
fn semver_precedence_covers_the_specification_examples() -> Result<(), String> {
    let steps = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];
    for pair in steps.windows(2) {
        let Some(before) = pair.first() else {
            return Err("missing step".to_owned());
        };
        let Some(after) = pair.get(1) else {
            return Err("missing step".to_owned());
        };
        let result = version_pair("npm", before, None, after, None)?;
        expect_direction(&result, Direction::Upgrade)?;
    }
    let build = version_pair("npm", "1.0.0+aaa", None, "1.0.0+bbb", None)?;
    expect_direction(&build, Direction::Unknown)?;
    let loose = version_pair("npm", "v1.2.3", None, "1.2.3", None)?;
    expect_direction(&loose, Direction::Unknown)?;
    let short = version_pair("cargo", "1.2", None, "1.2.0", None)?;
    expect_direction(&short, Direction::Unknown)?;
    let zeros = version_pair("npm", "01.0.0", None, "1.0.0", None)?;
    expect_direction(&zeros, Direction::Unknown)?;
    Ok(())
}

#[test]
fn rpm_epoch_outranks_the_version_string() -> Result<(), String> {
    let higher = version_pair("rpm", "9.0", Some("1"), "1.0", Some("2"))?;
    expect_direction(&higher, Direction::Upgrade)?;
    let equal = version_pair("rpm", "1.0.0", Some("1"), "2.0.0", Some("1"))?;
    expect_direction(&equal, Direction::Unknown)?;
    let missing = version_pair("rpm", "1.0.0", None, "2.0.0", Some("1"))?;
    expect_direction(&missing, Direction::Upgrade)?;
    Ok(())
}

#[test]
fn several_versions_are_not_paired() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    for version in ["1.0.0", "2.0.0"] {
        put(&mut old, npm("multi"), ver(version), DependencyRole::Direct);
    }
    for version in ["1.0.0", "2.0.0", "3.0.0"] {
        put(&mut new, npm("multi"), ver(version), DependencyRole::Direct);
    }
    let result = diff(&old, &new);
    if kinds(&result) != [ChangeKind::Added] {
        return Err(format!("{:?}", kinds(&result)));
    }
    let added = one(&result, ChangeKind::Added)?;
    if added.version.version != "3.0.0" {
        return Err(added.version.version.clone());
    }
    Ok(())
}

#[test]
fn attribute_changes_name_what_was_added_and_removed() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put_attrs(
        &mut old,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Transitive,
        licenses(&["MIT"]),
    );
    put_attrs(
        &mut new,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Direct,
        licenses(&["Apache-2.0"]),
    );
    old.components
        .values_mut()
        .next()
        .ok_or("missing")?
        .versions
        .values_mut()
        .next()
        .ok_or("missing")?
        .attributes
        .suppliers
        .insert("Old".to_owned());
    new.components
        .values_mut()
        .next()
        .ok_or("missing")?
        .versions
        .values_mut()
        .next()
        .ok_or("missing")?
        .attributes
        .suppliers
        .insert("New".to_owned());
    old.components
        .values_mut()
        .next()
        .ok_or("missing")?
        .versions
        .values_mut()
        .next()
        .ok_or("missing")?
        .attributes
        .scopes
        .insert("excluded".to_owned());
    new.components
        .values_mut()
        .next()
        .ok_or("missing")?
        .versions
        .values_mut()
        .next()
        .ok_or("missing")?
        .attributes
        .scopes
        .insert("required".to_owned());
    let result = diff(&old, &new);
    expect_set(
        &result,
        ChangeKind::LicenseChanged,
        &["Apache-2.0"],
        &["MIT"],
    )?;
    expect_set(&result, ChangeKind::SupplierChanged, &["New"], &["Old"])?;
    expect_set(
        &result,
        ChangeKind::ScopeChanged,
        &["required"],
        &["excluded"],
    )?;
    let role = one(&result, ChangeKind::RoleChanged)?;
    if role.old_role != Some(DependencyRole::Transitive)
        || role.new_role != Some(DependencyRole::Direct)
    {
        return Err(format!("{role:?}"));
    }
    let unchanged = diff(&old, &old);
    if !unchanged.is_empty() {
        return Err("identical attributes changed".to_owned());
    }
    Ok(())
}

fn expect_set(
    diff: &Diff,
    kind: ChangeKind,
    added: &[&str],
    removed: &[&str],
) -> Result<(), String> {
    let change = one(diff, kind)?;
    let ChangeDetail::SetChanged {
        added: found_added,
        removed: found_removed,
    } = &change.detail
    else {
        return Err(format!("{:?}", change.detail));
    };
    let added = added
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    let removed = removed
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if found_added != &added || found_removed != &removed {
        return Err(format!("{found_added:?} {found_removed:?}"));
    }
    Ok(())
}

#[test]
fn unknown_role_on_one_side_is_a_role_change() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(&mut old, npm("pkg"), ver("1.0.0"), DependencyRole::Unknown);
    put(&mut new, npm("pkg"), ver("1.0.0"), DependencyRole::Direct);
    let result = diff(&old, &new);
    let role = one(&result, ChangeKind::RoleChanged)?;
    if role.old_role != Some(DependencyRole::Unknown) {
        return Err(format!("{role:?}"));
    }
    let both = diff(&old, &old);
    if !both.is_empty() {
        return Err("unknown to unknown changed".to_owned());
    }
    Ok(())
}

#[test]
fn hash_content_is_separate_from_coverage_and_case() -> Result<(), String> {
    let conflict = hash_pair(&[("SHA-256", "aaaa")], &[("SHA-256", "bbbb")]);
    expect_hash_conflict(&conflict, "SHA-256", &["aaaa"], &["bbbb"])?;
    if kinds(&conflict).contains(&ChangeKind::HashCoverageChanged) {
        return Err("content conflict was also coverage".to_owned());
    }
    let added_alg = hash_pair(
        &[("SHA-256", "aaaa")],
        &[("SHA-256", "aaaa"), ("SHA-512", "eeee")],
    );
    if kinds(&added_alg) != [ChangeKind::HashCoverageChanged] {
        return Err(format!("{:?}", kinds(&added_alg)));
    }
    let dropped = hash_pair(&[("SHA-256", "aaaa")], &[]);
    let coverage = one(&dropped, ChangeKind::HashCoverageChanged)?;
    let ChangeDetail::HashCoverageChanged {
        all_dropped,
        removed,
        ..
    } = &coverage.detail
    else {
        return Err(format!("{:?}", coverage.detail));
    };
    if !*all_dropped || !removed.contains("SHA-256") {
        return Err(format!("{coverage:?}"));
    }
    let case = hash_pair(&[("SHA-256", "ABCD")], &[("SHA-256", "abcd")]);
    if !case.is_empty() {
        return Err(format!("case differed: {case:?}"));
    }
    let both = hash_pair(
        &[("SHA-256", "aaaa")],
        &[("SHA-256", "bbbb"), ("SHA-512", "eeee")],
    );
    if !kinds(&both).contains(&ChangeKind::HashChanged)
        || !kinds(&both).contains(&ChangeKind::HashCoverageChanged)
    {
        return Err(format!("{:?}", kinds(&both)));
    }
    Ok(())
}

fn hash_pair(old_hashes: &[(&str, &str)], new_hashes: &[(&str, &str)]) -> Diff {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put_attrs(
        &mut old,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Direct,
        hashes(old_hashes),
    );
    put_attrs(
        &mut new,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Direct,
        hashes(new_hashes),
    );
    diff(&old, &new)
}

fn hashes(values: &[(&str, &str)]) -> VersionAttributes {
    let mut attributes = VersionAttributes::default();
    for (alg, content) in values {
        attributes.hashes.insert(ContentHash {
            alg: (*alg).to_owned(),
            content: (*content).to_owned(),
        });
    }
    attributes
}

fn expect_hash_conflict(
    diff: &Diff,
    algorithm: &str,
    old_contents: &[&str],
    new_contents: &[&str],
) -> Result<(), String> {
    let change = one(diff, ChangeKind::HashChanged)?;
    let ChangeDetail::HashChanged { algorithms } = &change.detail else {
        return Err(format!("{:?}", change.detail));
    };
    let found = algorithms
        .iter()
        .find(|conflict| conflict.algorithm == algorithm)
        .ok_or("missing algorithm")?;
    let old_contents = old_contents
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    let new_contents = new_contents
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if found.old_contents != old_contents || found.new_contents != new_contents {
        return Err(format!("{found:?}"));
    }
    Ok(())
}

#[test]
fn a_version_change_keeps_attribute_differences_as_flags() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put_attrs(
        &mut old,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Direct,
        licenses(&["MIT"]),
    );
    put_attrs(
        &mut new,
        npm("pkg"),
        ver("2.0.0"),
        DependencyRole::Transitive,
        licenses(&["Apache-2.0"]),
    );
    let result = diff(&old, &new);
    let change = one(&result, ChangeKind::VersionChanged)?;
    let ChangeDetail::VersionChanged { flags, .. } = &change.detail else {
        return Err(format!("{:?}", change.detail));
    };
    if !flags.contains(&AttributeFlag::LicenseChanged)
        || !flags.contains(&AttributeFlag::RoleChanged)
    {
        return Err(format!("{flags:?}"));
    }
    if kinds(&result).len() != 1 {
        return Err(format!("{:?}", kinds(&result)));
    }
    Ok(())
}

#[test]
fn a_lost_purl_is_removed_plus_added() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(
        &mut old,
        npm("left-pad"),
        ver("1.0.0"),
        DependencyRole::Direct,
    );
    put(
        &mut new,
        named("left-pad"),
        ver("1.0.0"),
        DependencyRole::Direct,
    );
    let result = diff(&old, &new);
    if kinds(&result) != [ChangeKind::Removed, ChangeKind::Added] {
        return Err(format!("{:?}", kinds(&result)));
    }
    Ok(())
}

#[test]
fn a_root_version_change_is_not_a_dependency_change() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(
        &mut old,
        typed("generic", "app"),
        ver("1.0.0"),
        DependencyRole::Root,
    );
    put(
        &mut new,
        typed("generic", "app"),
        ver("2.0.0"),
        DependencyRole::Root,
    );
    put(&mut old, npm("stay"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut new, npm("stay"), ver("1.0.0"), DependencyRole::Direct);
    let result = diff(&old, &new);
    if !result.changes.is_empty() {
        return Err(format!("{:?}", result.changes));
    }
    let root = result.root.as_ref().ok_or("missing root change")?;
    let old_root = root.old.as_ref().ok_or("missing old root")?;
    let new_root = root.new.as_ref().ok_or("missing new root")?;
    if old_root.version.version != "1.0.0" || new_root.version.version != "2.0.0" {
        return Err(format!("{root:?}"));
    }
    Ok(())
}

#[test]
fn provenance_covers_direct_transitive_cyclic_and_unreachable() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    let root = typed("generic", "app");
    put(&mut old, root.clone(), ver("1.0.0"), DependencyRole::Root);
    put(&mut new, root.clone(), ver("1.0.0"), DependencyRole::Root);
    put(&mut old, npm("mid"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut new, npm("mid"), ver("1.0.0"), DependencyRole::Direct);
    put(
        &mut new,
        npm("direct"),
        ver("1.0.0"),
        DependencyRole::Direct,
    );
    put(
        &mut new,
        npm("later"),
        ver("1.0.0"),
        DependencyRole::Transitive,
    );
    put(&mut old, npm("loop"), ver("1.0.0"), DependencyRole::Direct);
    put(&mut new, npm("loop"), ver("1.0.0"), DependencyRole::Direct);
    put(
        &mut new,
        npm("cycle"),
        ver("1.0.0"),
        DependencyRole::Transitive,
    );
    put(&mut new, npm("lost"), ver("1.0.0"), DependencyRole::Unknown);
    link(
        &mut new,
        node(root.clone(), "1.0.0"),
        node(npm("direct"), "1.0.0"),
    );
    link(
        &mut new,
        node(root.clone(), "1.0.0"),
        node(npm("mid"), "1.0.0"),
    );
    link(
        &mut new,
        node(npm("mid"), "1.0.0"),
        node(npm("later"), "1.0.0"),
    );
    link(
        &mut new,
        node(root.clone(), "1.0.0"),
        node(npm("loop"), "1.0.0"),
    );
    link(
        &mut new,
        node(npm("loop"), "1.0.0"),
        node(npm("cycle"), "1.0.0"),
    );
    link(
        &mut new,
        node(npm("cycle"), "1.0.0"),
        node(npm("loop"), "1.0.0"),
    );
    let result = diff(&old, &new);
    expect_path(&result, "direct", &["app", "direct"])?;
    expect_introduced(&result, "direct", &["app"])?;
    expect_path(&result, "later", &["app", "mid", "later"])?;
    expect_introduced(&result, "later", &["mid"])?;
    expect_path(&result, "cycle", &["app", "loop", "cycle"])?;
    let lost = find(&result, "lost")?;
    let ChangeDetail::Added {
        path_from_root,
        introduced_by,
        ..
    } = &lost.detail
    else {
        return Err(format!("{:?}", lost.detail));
    };
    if path_from_root.is_some() || introduced_by.total != 0 {
        return Err(format!("{lost:?}"));
    }
    Ok(())
}

fn find<'a>(diff: &'a Diff, name: &str) -> Result<&'a Change, String> {
    diff.changes
        .iter()
        .find(|change| key_name(&change.key) == name)
        .ok_or_else(|| format!("missing {name}"))
}

fn expect_path(diff: &Diff, name: &str, names: &[&str]) -> Result<(), String> {
    let change = find(diff, name)?;
    let ChangeDetail::Added { path_from_root, .. } = &change.detail else {
        return Err(format!("{:?}", change.detail));
    };
    let path = path_from_root.as_ref().ok_or("missing path")?;
    let found = path
        .nodes
        .iter()
        .map(|node| key_name(&node.key))
        .collect::<Vec<_>>();
    if found != names || path.total != names.len() {
        return Err(format!("{found:?} total {}", path.total));
    }
    Ok(())
}

fn expect_introduced(diff: &Diff, name: &str, names: &[&str]) -> Result<(), String> {
    let change = find(diff, name)?;
    let ChangeDetail::Added { introduced_by, .. } = &change.detail else {
        return Err(format!("{:?}", change.detail));
    };
    let found = introduced_by
        .nodes
        .iter()
        .map(|node| key_name(&node.key))
        .collect::<Vec<_>>();
    if found != names || introduced_by.total != names.len() {
        return Err(format!("{found:?}"));
    }
    Ok(())
}

#[test]
fn provenance_lists_keep_the_total_when_the_cap_is_hit() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    let root = typed("generic", "app");
    put(&mut old, root.clone(), ver("1"), DependencyRole::Root);
    put(&mut new, root.clone(), ver("1"), DependencyRole::Root);
    put(&mut new, npm("leaf"), ver("1"), DependencyRole::Transitive);
    for index in 0..33 {
        let name = format!("p{index:02}");
        put(&mut old, npm(&name), ver("1"), DependencyRole::Direct);
        put(&mut new, npm(&name), ver("1"), DependencyRole::Direct);
        link(&mut new, node(npm(&name), "1"), node(npm("leaf"), "1"));
    }
    let mut cursor = root.clone();
    for index in 0..32 {
        let name = format!("c{index:02}");
        let role = if index == 31 {
            DependencyRole::Transitive
        } else {
            DependencyRole::Direct
        };
        if index < 31 {
            put(&mut old, npm(&name), ver("1"), role);
        }
        put(&mut new, npm(&name), ver("1"), role);
        link(&mut new, node(cursor, "1"), node(npm(&name), "1"));
        cursor = npm(&name);
    }
    let result = diff(&old, &new);
    let leaf = find(&result, "leaf")?;
    let ChangeDetail::Added { introduced_by, .. } = &leaf.detail else {
        return Err(format!("{:?}", leaf.detail));
    };
    if introduced_by.total != 33 || introduced_by.nodes.len() != 32 {
        return Err(format!(
            "introduced {} of {}",
            introduced_by.nodes.len(),
            introduced_by.total
        ));
    }
    let chain = find(&result, "c31")?;
    let ChangeDetail::Added { path_from_root, .. } = &chain.detail else {
        return Err(format!("{:?}", chain.detail));
    };
    let path = path_from_root.as_ref().ok_or("missing chain")?;
    if path.total != 33 || path.nodes.len() != 32 {
        return Err(format!("path {} of {}", path.nodes.len(), path.total));
    }
    let first = path.nodes.first().ok_or("empty path")?;
    if key_name(&first.key) != "app" {
        return Err(key_name(&first.key).to_owned());
    }
    Ok(())
}

#[test]
fn hostile_strings_are_copied_unchanged() -> Result<(), String> {
    let hostile = "a\n\u{001b}*_`|\u{007f}";
    let long = "n".repeat(8_000);
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(&mut old, npm("pkg"), ver("1.0.0"), DependencyRole::Direct);
    put_attrs(
        &mut new,
        npm("pkg"),
        ver("1.0.0"),
        DependencyRole::Direct,
        licenses(&[hostile, long.as_str()]),
    );
    let result = diff(&old, &new);
    let change = one(&result, ChangeKind::LicenseChanged)?;
    let ChangeDetail::SetChanged { added, .. } = &change.detail else {
        return Err(format!("{:?}", change.detail));
    };
    if !added.contains(hostile) || !added.contains(&long) {
        return Err("hostile license was rewritten".to_owned());
    }
    Ok(())
}

#[test]
fn generated_documents_keep_symmetry_and_lose_no_version() -> Result<(), String> {
    for size in [0_u32, 1, 2, 5, 8] {
        let old = generated(size, false);
        let new = generated(size, true);
        if !diff(&old, &old).is_empty() {
            return Err(format!("diff(a, a) for {size}"));
        }
        let forward = diff(&old, &new);
        let backward = diff(&new, &old);
        if names_of(&forward, ChangeKind::Added) != names_of(&backward, ChangeKind::Removed) {
            return Err(format!("symmetry added {size}"));
        }
        if names_of(&forward, ChangeKind::Removed) != names_of(&backward, ChangeKind::Added) {
            return Err(format!("symmetry removed {size}"));
        }
        for change in &forward.changes {
            if change.kind != ChangeKind::VersionChanged {
                continue;
            }
            let ChangeDetail::VersionChanged { direction, .. } = &change.detail else {
                continue;
            };
            let reverse = backward
                .changes
                .iter()
                .find(|other| other.key == change.key && other.kind == ChangeKind::VersionChanged)
                .ok_or("missing reverse version change")?;
            let ChangeDetail::VersionChanged {
                direction: flipped, ..
            } = &reverse.detail
            else {
                return Err("reverse was not a version change".to_owned());
            };
            let expected = match direction {
                Direction::Upgrade => Direction::Downgrade,
                Direction::Downgrade => Direction::Upgrade,
                Direction::Unknown => Direction::Unknown,
            };
            if *flipped != expected {
                return Err(format!("{direction:?} flipped to {flipped:?}"));
            }
        }
        account(&old, &new, &forward)?;
    }
    Ok(())
}

fn generated(size: u32, changed: bool) -> Document {
    let mut document = empty_doc();
    put(
        &mut document,
        typed("generic", "app"),
        ver("1.0.0"),
        DependencyRole::Root,
    );
    for index in 0..size {
        let name = format!("p{index}");
        let version = if changed && index % 4 == 0 {
            "2.0.0"
        } else {
            "1.0.0"
        };
        let role = if index % 2 == 0 {
            DependencyRole::Direct
        } else {
            DependencyRole::Transitive
        };
        let attributes = if changed && index % 4 == 1 {
            licenses(&["Apache-2.0"])
        } else if index % 3 == 0 {
            licenses(&["MIT"])
        } else {
            VersionAttributes::default()
        };
        put_attrs(&mut document, npm(&name), ver(version), role, attributes);
    }
    document
}

fn account(old: &Document, new: &Document, result: &Diff) -> Result<(), String> {
    for (document, side) in [(old, "old"), (new, "new")] {
        for (key, component) in &document.components {
            for (version, stored) in &component.versions {
                if stored.role == DependencyRole::Root {
                    continue;
                }
                if version_unchanged(old, new, key, version) {
                    continue;
                }
                let captured = result.changes.iter().any(|change| {
                    &change.key == key
                        && (change.old_version.as_ref() == Some(version)
                            || change.new_version.as_ref() == Some(version))
                });
                if !captured {
                    return Err(format!("lost {side} {}", key_name(key)));
                }
            }
        }
    }
    Ok(())
}

fn version_unchanged(
    old: &Document,
    new: &Document,
    key: &ComponentKey,
    version: &VersionId,
) -> bool {
    let Some(old_version) = old
        .components
        .get(key)
        .and_then(|component| component.versions.get(version))
    else {
        return false;
    };
    let Some(new_version) = new
        .components
        .get(key)
        .and_then(|component| component.versions.get(version))
    else {
        return false;
    };
    old_version == new_version
}

#[test]
fn component_order_does_not_change_the_diff() -> Result<(), String> {
    let first = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"bom-ref":"b","name":"b","version":"1","purl":"pkg:npm/b@1"},{"bom-ref":"a","name":"a","version":"1","purl":"pkg:npm/a@2"}],"dependencies":[{"ref":"app","dependsOn":["b"]},{"ref":"b","dependsOn":["a"]}]}"#;
    let second = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"bom-ref":"a","name":"a","version":"1","purl":"pkg:npm/a@2"},{"bom-ref":"b","name":"b","version":"1","purl":"pkg:npm/b@1"}],"dependencies":[{"ref":"b","dependsOn":["a"]},{"ref":"app","dependsOn":["b"]}]}"#;
    let base = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"bom-ref":"b","name":"b","version":"1","purl":"pkg:npm/b@1"}],"dependencies":[{"ref":"app","dependsOn":["b"]}]}"#;
    let limits = Limits::defaults();
    let left = read_bytes(first, &limits).map_err(|error| error.to_string())?;
    let right = read_bytes(second, &limits).map_err(|error| error.to_string())?;
    let baseline = read_bytes(base, &limits).map_err(|error| error.to_string())?;
    let one = diff(&baseline, &left).to_canonical_json();
    let other = diff(&baseline, &right).to_canonical_json();
    if one != other {
        return Err(one);
    }
    Ok(())
}

#[test]
fn spec_version_alone_is_not_a_change() -> Result<(), String> {
    let limits = Limits::defaults();
    let old = read_bytes(include_bytes!("fixtures/minimal-1.4.json"), &limits)
        .map_err(|error| error.to_string())?;
    let new = read_bytes(include_bytes!("fixtures/minimal-1.7.json"), &limits)
        .map_err(|error| error.to_string())?;
    let result = diff(&old, &new);
    if !result.is_empty() {
        return Err(result.to_canonical_json());
    }
    Ok(())
}

#[test]
fn the_lockward_pair_matches_the_intended_changes() -> Result<(), String> {
    let limits = Limits::defaults();
    let old = read_bytes(include_bytes!("fixtures/lockward-old.cdx.json"), &limits)
        .map_err(|error| error.to_string())?;
    let new = read_bytes(include_bytes!("fixtures/lockward-new.cdx.json"), &limits)
        .map_err(|error| error.to_string())?;
    let result = diff(&old, &new);
    if result.root.is_some() {
        return Err("root changed".to_owned());
    }
    expect_direction_named(&result, "bumped", Direction::Upgrade)?;
    expect_direction_named(&result, "rolled", Direction::Downgrade)?;
    expect_set_named(
        &result,
        "licensed",
        ChangeKind::LicenseChanged,
        &["Apache-2.0"],
        &["MIT"],
    )?;
    expect_set_named(
        &result,
        "devkit",
        ChangeKind::ScopeChanged,
        &["required"],
        &["excluded"],
    )?;
    let promoted = named_kind(&result, "promoted", ChangeKind::RoleChanged)?;
    if promoted.old_role != Some(DependencyRole::Transitive)
        || promoted.new_role != Some(DependencyRole::Direct)
    {
        return Err(format!("{promoted:?}"));
    }
    if names_of(&result, ChangeKind::Removed) != BTreeSet::from(["gone".to_owned()]) {
        return Err(format!("{:?}", names_of(&result, ChangeKind::Removed)));
    }
    let added = names_of(&result, ChangeKind::Added);
    let expected_added = BTreeSet::from([
        "fresh".to_owned(),
        "nested-new".to_owned(),
        "multi".to_owned(),
    ]);
    if added != expected_added {
        return Err(format!("{added:?}"));
    }
    let multi = result
        .changes
        .iter()
        .find(|change| key_name(&change.key) == "multi")
        .ok_or("missing multi")?;
    if multi.kind != ChangeKind::Added || multi.version.version != "3.0.0" {
        return Err(format!("{multi:?}"));
    }
    if result
        .changes
        .iter()
        .any(|change| change.kind == ChangeKind::VersionChanged && key_name(&change.key) == "multi")
    {
        return Err("multi was paired".to_owned());
    }
    expect_introduced(&result, "fresh", &["review-app"])?;
    expect_path(
        &result,
        "nested-new",
        &["review-app", "fresh", "nested-new"],
    )?;
    if result.changes.iter().any(|change| {
        matches!(
            change.kind,
            ChangeKind::HashChanged | ChangeKind::HashCoverageChanged | ChangeKind::SupplierChanged
        )
    }) {
        return Err(format!("{:?}", kinds(&result)));
    }
    let expected = include_str!("fixtures/golden/lockward-pair.json");
    let actual = result.to_canonical_json();
    if actual != expected.trim_end_matches(['\r', '\n']) {
        return Err(actual);
    }
    Ok(())
}

fn expect_direction_named(diff: &Diff, name: &str, direction: Direction) -> Result<(), String> {
    let change = named_kind(diff, name, ChangeKind::VersionChanged)?;
    let ChangeDetail::VersionChanged {
        direction: found, ..
    } = &change.detail
    else {
        return Err(format!("{:?}", change.detail));
    };
    if *found != direction {
        return Err(format!("{name} {found:?}"));
    }
    Ok(())
}

fn expect_set_named(
    diff: &Diff,
    name: &str,
    kind: ChangeKind,
    added: &[&str],
    removed: &[&str],
) -> Result<(), String> {
    let change = named_kind(diff, name, kind)?;
    let ChangeDetail::SetChanged {
        added: found_added,
        removed: found_removed,
    } = &change.detail
    else {
        return Err(format!("{:?}", change.detail));
    };
    let added = added
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    let removed = removed
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if found_added != &added || found_removed != &removed {
        return Err(format!("{name} {found_added:?} {found_removed:?}"));
    }
    Ok(())
}

fn named_kind<'a>(diff: &'a Diff, name: &str, kind: ChangeKind) -> Result<&'a Change, String> {
    diff.changes
        .iter()
        .find(|change| key_name(&change.key) == name && change.kind == kind)
        .ok_or_else(|| format!("missing {name} {kind:?}"))
}

#[test]
fn one_hundred_thousand_components_are_diffed() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    for index in 0..100_000 {
        let key = named(&format!("n{index}"));
        put(&mut old, key.clone(), ver("1"), DependencyRole::Unknown);
        let version = if index % 10 == 0 { "2" } else { "1" };
        put(&mut new, key, ver(version), DependencyRole::Unknown);
    }
    let started = Instant::now();
    let result = diff(&old, &new);
    let elapsed = started.elapsed();
    eprintln!("100k diff: {elapsed:?}");
    if result.summary.counts.version_changed != 10_000 {
        return Err(format!("{}", result.summary.counts.version_changed));
    }
    if elapsed.as_secs() > 60 {
        return Err(format!("too slow: {elapsed:?}"));
    }
    Ok(())
}

#[test]
fn summary_counts_removed_under_the_old_role() -> Result<(), String> {
    let mut old = empty_doc();
    let mut new = empty_doc();
    put(
        &mut old,
        npm("gone"),
        ver("1.0.0"),
        DependencyRole::Transitive,
    );
    put(&mut new, npm("fresh"), ver("1.0.0"), DependencyRole::Direct);
    let result = diff(&old, &new);
    if result.summary.counts.added != 1 || result.summary.counts.removed != 1 {
        return Err(format!("{:?}", result.summary.counts));
    }
    let direct = result
        .summary
        .by_role
        .iter()
        .find(|count| count.role == DependencyRole::Direct)
        .ok_or("missing direct")?;
    let transitive = result
        .summary
        .by_role
        .iter()
        .find(|count| count.role == DependencyRole::Transitive)
        .ok_or("missing transitive")?;
    if direct.counts.added != 1 || transitive.counts.removed != 1 {
        return Err(format!("{:?}", result.summary.by_role));
    }
    Ok(())
}
