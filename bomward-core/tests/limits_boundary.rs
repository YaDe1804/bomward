//! Regression locks for the limits an external fuzz run already exercised.
//!
//! Depth 32, JSON container depth 127, dependency cycles, and a large hash
//! set are accepted. One step past a limit is a typed error. There is no
//! separate cap on hashes: the file-size limit is what stops the work.

use std::fmt::Write as _;

use bomward_core::{ComponentKey, DependencyRole, Document, Error, LimitKind, Limits, read_bytes};

#[test]
fn component_depth_32_is_accepted_and_33_is_the_depth_limit() -> Result<(), String> {
    read_bytes(nested(32).as_bytes(), &Limits::default()).map_err(|error| error.to_string())?;
    let error = require_err(read_bytes(nested(33).as_bytes(), &Limits::default()))?;
    if error.to_string() != "nesting depth 33 exceeds limit 32 (--max-depth)" {
        return Err(error.to_string());
    }
    match error {
        Error::LimitExceeded(exceeded) if exceeded.kind == LimitKind::NestingDepth => Ok(()),
        other => Err(other.to_string()),
    }
}

#[test]
fn json_container_127_is_accepted_and_128_is_the_json_limit() -> Result<(), String> {
    let limit = Limits::json_depth_limit();
    if limit != 127 {
        return Err(format!("json cap is {limit}"));
    }
    read_bytes(padded(limit).as_bytes(), &Limits::default()).map_err(|error| error.to_string())?;
    let error = require_err(read_bytes(padded(limit + 1).as_bytes(), &Limits::default()))?;
    let text = error.to_string();
    if text != "json nesting 128 exceeds limit 127" || text.contains("--max-depth") {
        return Err(text);
    }
    match error {
        Error::LimitExceeded(exceeded) if exceeded.kind == LimitKind::JsonNesting => Ok(()),
        other => Err(other.to_string()),
    }
}

#[test]
fn a_dependency_cycle_terminates() -> Result<(), String> {
    let document =
        read_bytes(CYCLE.as_bytes(), &Limits::default()).map_err(|error| error.to_string())?;
    if role_of(&document, "a")? != DependencyRole::Direct
        || role_of(&document, "b")? != DependencyRole::Transitive
    {
        return Err("the cycle walk did not finish".to_owned());
    }
    if !depends_on(&document, "a", "b") || !depends_on(&document, "b", "a") {
        return Err("the cycle edges were dropped".to_owned());
    }
    Ok(())
}

#[test]
fn a_self_dependency_terminates() -> Result<(), String> {
    let document =
        read_bytes(SELF.as_bytes(), &Limits::default()).map_err(|error| error.to_string())?;
    if role_of(&document, "a")? != DependencyRole::Direct {
        return Err("the self-edge walk did not finish".to_owned());
    }
    if !depends_on(&document, "a", "a") {
        return Err("the self-edge was dropped".to_owned());
    }
    Ok(())
}

#[test]
fn one_hundred_thousand_hashes_are_bounded_by_the_file_size() -> Result<(), String> {
    let body = many_hashes(100_000);
    let size = u64::try_from(body.len()).map_err(|error| error.to_string())?;
    if size > Limits::DEFAULT_MAX_FILE_BYTES {
        return Err(format!("100000 hashes already occupy {size} bytes"));
    }
    let document =
        read_bytes(body.as_bytes(), &Limits::default()).map_err(|error| error.to_string())?;
    let stored = hash_count(&document)?;
    if stored != 100_000 {
        return Err(format!("{stored} hashes were stored"));
    }
    let mut over = body.into_bytes();
    let limit =
        usize::try_from(Limits::DEFAULT_MAX_FILE_BYTES).map_err(|error| error.to_string())?;
    over.resize(limit.saturating_add(1), b' ');
    let error = require_err(read_bytes(&over, &Limits::default()))?;
    let found = Limits::DEFAULT_MAX_FILE_BYTES + 1;
    let expected = format!(
        "file size {found} exceeds limit {} (--max-file-bytes)",
        Limits::DEFAULT_MAX_FILE_BYTES
    );
    if error.to_string() != expected {
        return Err(error.to_string());
    }
    match error {
        Error::LimitExceeded(exceeded) if exceeded.kind == LimitKind::FileBytes => Ok(()),
        other => Err(other.to_string()),
    }
}

const CYCLE: &str = r#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"type":"application","bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"type":"library","bom-ref":"a","name":"a","version":"1","purl":"pkg:generic/a@1"},{"type":"library","bom-ref":"b","name":"b","version":"1","purl":"pkg:generic/b@1"}],"dependencies":[{"ref":"app","dependsOn":["a"]},{"ref":"a","dependsOn":["b"]},{"ref":"b","dependsOn":["a"]}]}"#;

const SELF: &str = r#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"type":"application","bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}},"components":[{"type":"library","bom-ref":"a","name":"a","version":"1","purl":"pkg:generic/a@1"}],"dependencies":[{"ref":"app","dependsOn":["a"]},{"ref":"a","dependsOn":["a"]}]}"#;

fn require_err(result: Result<Document, Error>) -> Result<Error, String> {
    match result {
        Ok(_) => Err("the over-limit input was accepted".to_owned()),
        Err(error) => Ok(error),
    }
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

fn many_hashes(count: u32) -> String {
    let mut json = String::from(
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"a","version":"1","hashes":["#,
    );
    for index in 0..count {
        if index > 0 {
            json.push(',');
        }
        let _ = write!(json, r#"{{"alg":"SHA-256","content":"{index:08x}"}}"#);
    }
    json.push_str("]}]}");
    json
}

fn hash_count(document: &Document) -> Result<usize, String> {
    let component = document
        .components
        .values()
        .next()
        .ok_or("missing component")?;
    let version = component
        .versions
        .values()
        .next()
        .ok_or("missing version")?;
    Ok(version.attributes.hashes.len())
}

fn role_of(document: &Document, name: &str) -> Result<DependencyRole, String> {
    let component = document
        .components
        .iter()
        .find(|(key, _)| key_name(key) == name)
        .map(|(_, component)| component)
        .ok_or_else(|| format!("missing {name}"))?;
    component
        .versions
        .values()
        .next()
        .map(|version| version.role)
        .ok_or_else(|| format!("missing version of {name}"))
}

fn depends_on(document: &Document, from: &str, to: &str) -> bool {
    document.dependencies.iter().any(|(source, targets)| {
        key_name(&source.key) == from && targets.iter().any(|target| key_name(&target.key) == to)
    })
}

fn key_name(key: &ComponentKey) -> &str {
    match key {
        ComponentKey::Purl { name, .. } | ComponentKey::Name { name, .. } => name,
    }
}
