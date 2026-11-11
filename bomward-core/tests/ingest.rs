//! Ingest tests. Fixtures are inert documents written for these tests.

use std::path::PathBuf;
use std::time::Instant;

use bomward_core::{
    Component, ComponentKey, ContentHash, DependencyRole, Document, DocumentError, Error,
    LimitKind, Limits, WarningKind, read_bytes, read_path,
};

fn fixture(name: &str) -> Result<Vec<u8>, String> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(name);
    std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn read_default(bytes: &[u8]) -> Result<Document, Error> {
    read_bytes(bytes, &Limits::defaults())
}

fn read_fixture(name: &str) -> Result<Document, String> {
    read_default(&fixture(name)?).map_err(|error| error.to_string())
}

fn require_err(result: Result<Document, Error>) -> Result<Error, String> {
    match result {
        Ok(document) => Err(format!(
            "expected an error, read {} components",
            document.components.len()
        )),
        Err(error) => Ok(error),
    }
}

fn key_name(key: &ComponentKey) -> &str {
    match key {
        ComponentKey::Name { name, .. } | ComponentKey::Purl { name, .. } => name,
    }
}

fn find<'a>(
    document: &'a Document,
    name: &str,
) -> Result<(&'a ComponentKey, &'a Component), String> {
    document
        .components
        .iter()
        .find(|(key, _)| key_name(key) == name)
        .ok_or_else(|| format!("missing component {name}"))
}

fn assert_library(document: &Document) -> Result<(), String> {
    let (key, component) = find(document, "left-pad")?;
    let ComponentKey::Purl {
        ty,
        namespace,
        name,
        qualifiers,
        subpath,
    } = key
    else {
        return Err(format!("left-pad key is not a purl: {key:?}"));
    };
    if ty != "npm"
        || namespace.is_some()
        || name != "left-pad"
        || !qualifiers.is_empty()
        || subpath.is_some()
    {
        return Err(format!("unexpected key {key:?}"));
    }
    if component.role != DependencyRole::Direct {
        return Err(format!("role {:?}", component.role));
    }
    let attributes = component
        .versions
        .iter()
        .find(|(version, _)| version.version == "1.2.0" && version.epoch.is_none())
        .map(|(_, attributes)| attributes)
        .ok_or("missing version 1.2.0")?;
    for license in ["MIT", "Custom", "MIT OR Apache-2.0"] {
        if !attributes.licenses.contains(license) {
            return Err(format!("missing license {license}"));
        }
    }
    let hash = ContentHash {
        alg: "SHA-256".to_owned(),
        content: "abcd".to_owned(),
    };
    if !attributes.hashes.contains(&hash) {
        return Err("missing hash".to_owned());
    }
    if !attributes.suppliers.contains("Example")
        || !attributes.scopes.contains("required")
        || !attributes.types.contains("library")
    {
        return Err(format!("attributes {attributes:?}"));
    }
    let (_, root) = find(document, "app")?;
    if root.role != DependencyRole::Root {
        return Err(format!("root role {:?}", root.role));
    }
    Ok(())
}

#[test]
fn each_spec_version_is_read_in_full() -> Result<(), String> {
    for name in [
        "minimal-1.4.json",
        "minimal-1.5.json",
        "minimal-1.6.json",
        "minimal-1.7.json",
    ] {
        let document = read_fixture(name)?;
        assert_library(&document)?;
        let again = read_fixture(name)?;
        if document.to_canonical_json() != again.to_canonical_json() {
            return Err(format!("{name} is not stable"));
        }
    }
    Ok(())
}

#[test]
fn minimal_1_6_matches_the_golden_encoding() -> Result<(), String> {
    let document = read_fixture("minimal-1.6.json")?;
    let expected = include_str!("fixtures/golden/minimal-1.6.json");
    let actual = document.to_canonical_json();
    if actual != expected.trim_end_matches(['\r', '\n']) {
        return Err(actual);
    }
    Ok(())
}

#[test]
fn component_order_does_not_change_the_model() -> Result<(), String> {
    let first = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"b","version":"1","purl":"pkg:generic/b@1"},{"name":"a","version":"1","purl":"pkg:generic/a@1"}]}"#;
    let second = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"a","version":"1","purl":"pkg:generic/a@1"},{"name":"b","version":"1","purl":"pkg:generic/b@1"}]}"#;
    let left = read_default(first).map_err(|error| error.to_string())?;
    let right = read_default(second).map_err(|error| error.to_string())?;
    if left.to_canonical_json() != right.to_canonical_json() {
        return Err(left.to_canonical_json());
    }
    Ok(())
}

#[test]
fn lockward_shaped_npm_document_is_read() -> Result<(), String> {
    let bytes = fixture("lockward-npm-1.6.json")?;
    let document = read_default(&bytes).map_err(|error| error.to_string())?;
    let (scoped, component) = find(&document, "left-pad")?;
    let ComponentKey::Purl {
        ty,
        namespace,
        name,
        ..
    } = scoped
    else {
        return Err("scoped package is not a purl".to_owned());
    };
    if ty != "npm" || namespace.as_deref() != Some("@scope") || name != "left-pad" {
        return Err(format!("scoped key {scoped:?}"));
    }
    let attributes = component
        .versions
        .values()
        .next()
        .ok_or("missing scoped version")?;
    if !attributes.hashes.contains(&ContentHash {
        alg: "SHA-256".to_owned(),
        content: "abcd".to_owned(),
    }) {
        return Err("missing SRI-shaped hash".to_owned());
    }
    let (_, other) = find(&document, "ansi-regex")?;
    let scoped_scopes = component
        .versions
        .values()
        .next()
        .map(|attributes| attributes.scopes.clone())
        .ok_or("missing scoped version scopes")?;
    let other_scopes = other
        .versions
        .values()
        .next()
        .map(|attributes| attributes.scopes.clone())
        .ok_or("missing other scopes")?;
    if scoped_scopes.len() != 1 || other_scopes.len() != 1 || scoped_scopes == other_scopes {
        return Err(format!("scopes {scoped_scopes:?} and {other_scopes:?}"));
    }
    if document
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::RoleUnknown)
        && document
            .components
            .values()
            .all(|component| component.role == DependencyRole::Unknown)
    {
        return Ok(());
    }
    Err("a document without dependencies should be unknown".to_owned())
}

#[test]
fn nested_components_are_both_stored() -> Result<(), String> {
    let document = read_fixture("nested-components.json")?;
    find(&document, "parent")?;
    find(&document, "child")?;
    Ok(())
}

#[test]
fn two_versions_are_one_key() -> Result<(), String> {
    let document = read_fixture("two-versions.json")?;
    if document.components.len() != 1 {
        return Err(format!("{} keys", document.components.len()));
    }
    let component = document
        .components
        .values()
        .next()
        .ok_or("missing component")?;
    let versions = component
        .versions
        .keys()
        .map(|version| version.version.as_str())
        .collect::<Vec<_>>();
    if versions != ["1.0.0", "2.0.0"] {
        return Err(format!("{versions:?}"));
    }
    Ok(())
}

#[test]
fn a_cycle_is_data() -> Result<(), String> {
    let document = read_fixture("cycle.json")?;
    let (_, first) = find(&document, "a")?;
    let (_, second) = find(&document, "b")?;
    if first.role != DependencyRole::Direct || second.role != DependencyRole::Transitive {
        return Err(format!("roles {:?} {:?}", first.role, second.role));
    }
    if document.dependencies.len() != 3 {
        return Err(format!("{} edge sources", document.dependencies.len()));
    }
    Ok(())
}

#[test]
fn a_dependency_chain_longer_than_32_is_accepted() -> Result<(), String> {
    let document = read_default(chain(40).as_bytes()).map_err(|error| error.to_string())?;
    let (_, last) = find(&document, "n39")?;
    let (_, first) = find(&document, "n0")?;
    if first.role != DependencyRole::Direct || last.role != DependencyRole::Transitive {
        return Err(format!("roles {:?} {:?}", first.role, last.role));
    }
    if document.components.len() != 41 {
        return Err(format!("{} components", document.components.len()));
    }
    Ok(())
}

fn chain(length: u32) -> String {
    let mut json = String::from(
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"type":"application","bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":["#,
    );
    for index in 0..length {
        if index > 0 {
            json.push(',');
        }
        json.push_str(r#"{"type":"library","bom-ref":"n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"","name":"n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"","version":"1","purl":"pkg:generic/n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"@1"}"#);
    }
    json.push_str(r#"],"dependencies":[{"ref":"app","dependsOn":["n0"]}"#);
    for index in 0..length.saturating_sub(1) {
        let next = index + 1;
        json.push_str(r#",{"ref":"n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"","dependsOn":["n"#);
        json.push_str(&next.to_string());
        json.push_str(r#""]}"#);
    }
    json.push_str("]}");
    json
}

#[test]
fn file_size_boundary_is_exact() -> Result<(), String> {
    let mut bytes = fixture("minimal-1.6.json")?;
    let limit = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
    let limits = Limits::new(limit, 100, 32).map_err(|error| error.to_string())?;
    read_bytes(&bytes, &limits).map_err(|error| error.to_string())?;
    bytes.push(b' ');
    let error = require_err(read_bytes(&bytes, &limits))?;
    let found = limit + 1;
    if error.to_string() != format!("file size {found} exceeds limit {limit} (--max-file-bytes)") {
        return Err(error.to_string());
    }
    Ok(())
}

#[test]
fn path_read_uses_metadata_and_the_byte_cap() -> Result<(), String> {
    let bytes = fixture("minimal-1.6.json")?;
    let limit = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
    let path = std::env::temp_dir().join(format!("bomward-size-{}.json", std::process::id()));
    std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;
    let limits = Limits::new(limit, 100, 32).map_err(|error| error.to_string())?;
    let accepted = read_path(&path, &limits).map_err(|error| error.to_string());
    let mut over = bytes.clone();
    over.push(b' ');
    std::fs::write(&path, &over).map_err(|error| error.to_string())?;
    let rejected = read_path(&path, &limits);
    let removed = std::fs::remove_file(&path);
    accepted?;
    removed.map_err(|error| error.to_string())?;
    let error = require_err(rejected)?;
    let found = limit + 1;
    if error.to_string() != format!("file size {found} exceeds limit {limit} (--max-file-bytes)") {
        return Err(error.to_string());
    }
    Ok(())
}

#[test]
fn component_count_boundary_is_exact() -> Result<(), String> {
    let limits =
        Limits::new(Limits::DEFAULT_MAX_FILE_BYTES, 2, 32).map_err(|error| error.to_string())?;
    read_bytes(count_document(2).as_bytes(), &limits).map_err(|error| error.to_string())?;
    let error = require_err(read_bytes(count_document(3).as_bytes(), &limits))?;
    if error.to_string() != "component count 3 exceeds limit 2 (--max-components)" {
        return Err(error.to_string());
    }
    match error {
        Error::LimitExceeded(exceeded) if exceeded.kind == LimitKind::ComponentCount => Ok(()),
        other => Err(other.to_string()),
    }
}

fn count_document(count: u32) -> String {
    let mut json = String::from(r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":["#);
    for index in 0..count {
        if index > 0 {
            json.push(',');
        }
        json.push_str(r#"{"name":"n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"","version":"1"}"#);
    }
    json.push_str("]}");
    json
}

#[test]
fn component_nesting_boundary_is_exact() -> Result<(), String> {
    read_default(nested(32).as_bytes()).map_err(|error| error.to_string())?;
    let error = require_err(read_default(nested(33).as_bytes()))?;
    if error.to_string() != "nesting depth 33 exceeds limit 32 (--max-depth)" {
        return Err(error.to_string());
    }
    Ok(())
}

fn nested(depth: u32) -> String {
    let mut body = String::new();
    for level in (0..depth).rev() {
        let name = level.to_string();
        body = if body.is_empty() {
            format!(r#"{{"type":"library","name":"n{name}","version":"1"}}"#)
        } else {
            format!(r#"{{"type":"library","name":"n{name}","version":"1","components":[{body}]}}"#)
        };
    }
    format!(r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{body}]}}"#)
}

#[test]
fn json_nesting_boundary_is_exact() -> Result<(), String> {
    let limit = Limits::json_depth_limit();
    let accepted = padded(limit);
    read_default(accepted.as_bytes()).map_err(|error| error.to_string())?;
    let error = require_err(read_default(padded(limit + 1).as_bytes()))?;
    let found = u64::from(limit) + 1;
    let text = error.to_string();
    if text != format!("json nesting {found} exceeds limit {limit}") || text.contains("--max-depth")
    {
        return Err(text);
    }
    match error {
        Error::LimitExceeded(exceeded) if exceeded.kind == LimitKind::JsonNesting => Ok(()),
        other => Err(other.to_string()),
    }
}

#[test]
fn max_depth_above_the_json_cap_is_a_usage_error() -> Result<(), String> {
    let expected = "--max-depth 64 exceeds supported maximum 63";
    match Limits::new(1, 1, 64) {
        Ok(_) => return Err("depth 64 was accepted".to_owned()),
        Err(error) if error.to_string() == expected => {}
        Err(error) => return Err(error.to_string()),
    }
    let limits = Limits::new(
        Limits::DEFAULT_MAX_FILE_BYTES,
        1_000,
        Limits::MAX_SUPPORTED_DEPTH,
    )
    .map_err(|error| error.to_string())?;
    read_bytes(nested(Limits::MAX_SUPPORTED_DEPTH).as_bytes(), &limits)
        .map_err(|error| error.to_string())?;
    let bypassed = Limits {
        max_file_bytes: 1_024,
        max_components: 10,
        max_depth: 64,
    };
    let error = require_err(read_bytes(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[]}"#,
        &bypassed,
    ))?;
    if error.to_string() != expected {
        return Err(error.to_string());
    }
    match error {
        Error::UnsupportedDepth(depth) if depth.requested == 64 && depth.maximum == 63 => Ok(()),
        other => Err(other.to_string()),
    }
}

fn padded(depth: u32) -> String {
    let arrays = depth.saturating_sub(1);
    let mut nested = String::from("0");
    for _ in 0..arrays {
        nested = format!("[{nested}]");
    }
    format!(
        r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{{"name":"a","version":"1"}}],"pad":{nested}}}"#
    )
}

#[test]
fn rejected_headers_use_the_contract_text() -> Result<(), String> {
    let cases = [
        (
            br#"{"bomFormat":"CycloneDX","specVersion":"1.3"}"#.as_slice(),
            "unsupported specVersion 1.3; supported versions: 1.4, 1.5, 1.6, 1.7",
        ),
        (
            br#"{"bomFormat":"CycloneDX","specVersion":"1.8"}"#,
            "unsupported specVersion 1.8; supported versions: 1.4, 1.5, 1.6, 1.7",
        ),
        (
            br#"{"bomFormat":"CycloneDX"}"#,
            "unsupported specVersion missing; supported versions: 1.4, 1.5, 1.6, 1.7",
        ),
        (
            br#"{"bomFormat":"CycloneDX","specVersion":true}"#,
            "unsupported specVersion true; supported versions: 1.4, 1.5, 1.6, 1.7",
        ),
        (br#"{"specVersion":"1.6"}"#, "bomFormat must be CycloneDX"),
        (
            br#"{"bomFormat":"spdx","specVersion":"1.6"}"#,
            "bomFormat must be CycloneDX",
        ),
    ];
    for (bytes, expected) in cases {
        let error = require_err(read_default(bytes))?;
        if error.to_string() != expected {
            return Err(format!("{error} != {expected}"));
        }
        if error.to_string().contains("spdx") {
            return Err(error.to_string());
        }
    }
    Ok(())
}

#[test]
fn hostile_spec_version_is_escaped_and_names_stay_out_of_the_error() -> Result<(), String> {
    let bytes = br#"{"bomFormat":"CycloneDX","specVersion":"1.3\n","components":[{"name":"UNIQUE_HOSTILE_NAME"}]}"#;
    let error = require_err(read_default(bytes))?;
    let text = error.to_string();
    if text.contains("UNIQUE_HOSTILE_NAME") || text.contains('\n') {
        return Err(text);
    }
    if text != "unsupported specVersion 1.3\\u{000A}; supported versions: 1.4, 1.5, 1.6, 1.7" {
        return Err(text);
    }
    Ok(())
}

#[test]
fn syntax_and_encoding_failures_are_typed() -> Result<(), String> {
    let xml = require_err(read_default(b"<?xml version=\"1.0\"?><bom/>"))?;
    assert_document(&xml, DocumentErrorKind::Json)?;
    let empty = require_err(read_default(b""))?;
    assert_document(&empty, DocumentErrorKind::Json)?;
    let truncated = require_err(read_default(br#"{"bomFormat":"CycloneDX""#))?;
    assert_document(&truncated, DocumentErrorKind::Json)?;
    let utf8 = require_err(read_default(&[0xFF, 0xFE]))?;
    assert_document(&utf8, DocumentErrorKind::Utf8)?;
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend(fixture("minimal-1.6.json")?);
    read_default(&bom).map_err(|error| error.to_string())?;
    Ok(())
}

enum DocumentErrorKind {
    Json,
    Utf8,
}

fn assert_document(error: &Error, kind: DocumentErrorKind) -> Result<(), String> {
    match (error, kind) {
        (Error::InvalidDocument(DocumentError::Json { .. }), DocumentErrorKind::Json) => Ok(()),
        (Error::InvalidDocument(DocumentError::Utf8), DocumentErrorKind::Utf8) => {
            if error.to_string() == "invalid UTF-8" {
                Ok(())
            } else {
                Err(error.to_string())
            }
        }
        _ => Err(error.to_string()),
    }
}

#[test]
fn invalid_purl_falls_back_and_a_missing_purl_does_not_warn() -> Result<(), String> {
    let invalid = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"group":"g","name":"n","purl":"not a purl"}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let missing = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"group":"g","name":"n"}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let expected = ComponentKey::Name {
        group: "g".to_owned(),
        name: "n".to_owned(),
    };
    if !invalid.components.contains_key(&expected) || !missing.components.contains_key(&expected) {
        return Err("fallback key mismatch".to_owned());
    }
    if invalid.warnings.len() != 1 {
        return Err(format!("{} warnings", invalid.warnings.len()));
    }
    let warning = invalid.warnings.first().ok_or("missing warning")?;
    if warning.kind != WarningKind::InvalidPurl
        || warning.kind.message() != "invalid purl; fell back to group and name"
        || warning.kind.id() != "invalid-purl"
    {
        return Err(format!("{warning:?}"));
    }
    if !missing.warnings.is_empty() {
        return Err(format!("missing purl warned: {:?}", missing.warnings));
    }
    Ok(())
}

#[test]
fn a_fallback_without_a_name_is_rejected() -> Result<(), String> {
    let error = require_err(read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"purl":"not a purl","group":"UNIQUE_HOSTILE_NAME"}]}"#,
    ))?;
    if error.to_string() != "component is missing a name"
        || error.to_string().contains("UNIQUE_HOSTILE_NAME")
    {
        return Err(error.to_string());
    }
    Ok(())
}

#[test]
fn dangling_refs_warn() -> Result<(), String> {
    let document = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"bom-ref":"a","name":"a","version":"1","purl":"pkg:generic/a@1"}],"dependencies":[{"ref":"missing"},{"ref":"app","dependsOn":["gone"]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let orphans = document
        .warnings
        .iter()
        .filter(|warning| warning.kind == WarningKind::OrphanRef)
        .count();
    if orphans != 2 {
        return Err(format!(
            "{orphans} orphan warnings, {:?}",
            document.warnings
        ));
    }
    Ok(())
}

#[test]
fn duplicate_json_keys_keep_the_last_value() -> Result<(), String> {
    let document = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1"}},"components":[{"name":"first","name":"second","version":"1"}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    find(&document, "second")?;
    if document.components.contains_key(&ComponentKey::Name {
        group: String::new(),
        name: "first".to_owned(),
    }) {
        return Err("first name survived".to_owned());
    }
    if document.warnings.len() != 1 {
        return Err(format!("{:?}", document.warnings));
    }
    let warning = document.warnings.first().ok_or("missing warning")?;
    if warning.kind != WarningKind::DuplicateKey
        || warning.kind.id() != "duplicate-key"
        || warning.key.is_some()
    {
        return Err(format!("{warning:?}"));
    }
    let nested = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1"}},"components":[{"name":"a","version":"1","components":[{"name":"b","version":"1"}]}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    if !nested.warnings.is_empty() {
        return Err(format!("{:?}", nested.warnings));
    }
    Ok(())
}

#[test]
fn conflicting_attributes_are_united_and_warned() -> Result<(), String> {
    let document = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"name":"a","version":"1","purl":"pkg:generic/a@1","licenses":[{"license":{"id":"MIT"}}]},{"name":"a","version":"1","purl":"pkg:generic/a@1","licenses":[{"license":{"id":"Apache-2.0"}}]}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let component = document.components.values().next().ok_or("missing")?;
    let attributes = component
        .versions
        .values()
        .next()
        .ok_or("missing version")?;
    if !attributes.licenses.contains("MIT") || !attributes.licenses.contains("Apache-2.0") {
        return Err(format!("{:?}", attributes.licenses));
    }
    if document.warnings.len() != 1
        || document.warnings.first().map(|warning| warning.kind)
            != Some(WarningKind::ConflictingAttributes)
    {
        return Err(format!("{:?}", document.warnings));
    }
    Ok(())
}

#[test]
fn a_component_version_that_differs_from_the_purl_warns() -> Result<(), String> {
    let mismatched = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"name":"left-pad","version":"9.9.9","purl":"pkg:npm/left-pad@1.2.0"}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let (_, component) = find(&mismatched, "left-pad")?;
    let stored = component
        .versions
        .keys()
        .next()
        .map(|version| version.version.as_str())
        .ok_or("missing version")?;
    if stored != "1.2.0" {
        return Err(stored.to_owned());
    }
    if mismatched.warnings.len() != 1 {
        return Err(format!("{:?}", mismatched.warnings));
    }
    let warning = mismatched.warnings.first().ok_or("missing warning")?;
    if warning.kind != WarningKind::VersionMismatch
        || warning.kind.id() != "version-mismatch"
        || warning.key.as_ref().map(key_name) != Some("left-pad")
    {
        return Err(format!("{warning:?}"));
    }
    let matched = read_default(
        br#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"name":"left-pad","version":"1.2.0","purl":"pkg:npm/left-pad@1.2.0"}],"dependencies":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    if !matched.warnings.is_empty() {
        return Err(format!("{:?}", matched.warnings));
    }
    Ok(())
}

#[test]
fn hostile_names_are_stored_unchanged() -> Result<(), String> {
    let hostile = "a\n\u{001b}*_`|\u{007f}";
    let long = "n".repeat(8_000);
    let mut json = String::from(
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"a\n\u001b*_`|\u007f","version":"1"},{"name":""#,
    );
    json.push_str(&long);
    json.push_str(r#"","version":"1"}]}"#);
    let document = read_default(json.as_bytes()).map_err(|error| error.to_string())?;
    let (_, short) = find(&document, hostile)?;
    if short
        .versions
        .keys()
        .next()
        .map(|version| version.version.as_str())
        != Some("1")
    {
        return Err("hostile component was not stored".to_owned());
    }
    find(&document, &long)?;
    Ok(())
}

#[test]
fn arbitrary_bytes_do_not_panic() -> Result<(), String> {
    for byte in 0..=255 {
        match read_default(&[byte]) {
            Ok(_) | Err(_) => {}
        }
    }
    let mut state = 0x1234_5678_u64;
    for _ in 0..200 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let len = usize::try_from(state % 80).map_err(|error| error.to_string())?;
        let bytes = (0..len)
            .map(|index| u8::try_from((state >> (index % 8)) & 0xFF).unwrap_or(0))
            .collect::<Vec<_>>();
        match read_default(&bytes) {
            Ok(_) | Err(_) => {}
        }
    }
    Ok(())
}

#[test]
fn one_hundred_thousand_components_are_read() -> Result<(), String> {
    let mut json = String::from(r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":["#);
    json.reserve(8_000_000);
    for index in 0..100_000 {
        if index > 0 {
            json.push(',');
        }
        json.push_str(r#"{"type":"library","name":"n"#);
        json.push_str(&index.to_string());
        json.push_str(r#"","version":"1"}"#);
    }
    json.push_str("]}");
    let started = Instant::now();
    let document = read_default(json.as_bytes()).map_err(|error| error.to_string())?;
    let elapsed = started.elapsed();
    eprintln!("100k components: {elapsed:?}");
    if document.components.len() != 100_000 {
        return Err(format!("{} components", document.components.len()));
    }
    if elapsed.as_secs() > 60 {
        return Err(format!("too slow: {elapsed:?}"));
    }
    Ok(())
}
