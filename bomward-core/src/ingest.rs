// SPDX-License-Identifier: AGPL-3.0-only

//! Read a caller-supplied `CycloneDX` JSON document into the normalized model.
//!
//! Ingest is the only module that interprets SBOM bytes. It opens the path
//! the caller passed and no path found inside the document. Limits are the
//! defaults in [`crate::Limits`], or the values the caller supplies.
//!
//! `bomFormat` and `specVersion` are checked before any component is built.
//! Unknown JSON fields are ignored. Duplicate object keys keep the last
//! value, which is what `serde_json` does (ADR 0008).

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::fmt::Write;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::{DocumentError, Error, LimitExceeded, LimitKind};
use crate::limits::Limits;
use crate::model::{
    Component, ComponentKey, ContentHash, DependencyRole, Document, VersionAttributes, VersionId,
    Warning, WarningKind,
};
use crate::purl::{ParsedPurl, parse_purl};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Read the `CycloneDX` JSON file at `path`.
///
/// Only `path` is opened. A file that is already larger than
/// [`Limits::max_file_bytes`] is refused from its metadata. The read itself
/// stops after `limit + 1` bytes, so a file that grows in between cannot
/// hand the parser more than that.
///
/// # Errors
///
/// Returns [`Error::Io`] when the path cannot be read, and the same errors as
/// [`read_bytes`] for the contents.
///
/// # Examples
///
/// The byte entry point is the one tests and fuzzing call. Reading a path
/// uses the same parser after the size cap:
///
/// ```
/// use bomward_core::{Limits, read_bytes};
///
/// let bytes = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"left-pad","version":"1.0.0"}]}"#;
/// let document = read_bytes(bytes, &Limits::defaults())?;
/// assert_eq!(document.components.len(), 1);
/// # Ok::<(), bomward_core::Error>(())
/// ```
#[must_use = "ignoring the result drops a rejected document"]
pub fn read_path(path: &Path, limits: &Limits) -> Result<Document, Error> {
    let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > limits.max_file_bytes {
        return Err(limit(
            LimitKind::FileBytes,
            metadata.len(),
            limits.max_file_bytes,
        ));
    }
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buffer = Vec::new();
    let cap = limits.max_file_bytes.saturating_add(1);
    file.take(cap)
        .read_to_end(&mut buffer)
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let found = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
    if found > limits.max_file_bytes {
        return Err(limit(LimitKind::FileBytes, found, limits.max_file_bytes));
    }
    read_bytes(&buffer, limits)
}

/// Read a `CycloneDX` JSON document from memory.
///
/// A leading UTF-8 BOM is skipped. Invalid UTF-8 is an input error. The slice
/// is refused when it is longer than [`Limits::max_file_bytes`], before it is
/// parsed.
///
/// # Errors
///
/// * [`Error::LimitExceeded`] when the byte length, the component count, the
///   component nesting, or the JSON container depth is over the limit.
/// * [`Error::UnsupportedSpecVersion`] when `specVersion` is missing or not
///   1.4, 1.5, 1.6, or 1.7.
/// * [`Error::InvalidDocument`] when the bytes are not a `CycloneDX` JSON
///   document this version reads, including a component that needs a name
///   and does not have one.
///
/// The error text does not include names, purls, or other document strings.
/// `specVersion` is the exception, and it is sanitized.
///
/// # Examples
///
/// ```
/// use bomward_core::{Limits, read_bytes};
///
/// let bytes = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"left-pad","version":"1.0.0"}]}"#;
/// let document = read_bytes(bytes, &Limits::defaults())?;
/// assert_eq!(document.components.len(), 1);
/// # Ok::<(), bomward_core::Error>(())
/// ```
#[must_use = "ignoring the result drops a rejected document"]
pub fn read_bytes(bytes: &[u8], limits: &Limits) -> Result<Document, Error> {
    let found = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if found > limits.max_file_bytes {
        return Err(limit(LimitKind::FileBytes, found, limits.max_file_bytes));
    }
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| DocumentError::Utf8)?;
    if let Some(found_depth) = json_depth_past(text, limits.json_depth_limit()) {
        return Err(limit(
            LimitKind::NestingDepth,
            found_depth,
            u64::from(limits.json_depth_limit()),
        ));
    }
    let value = parse_json(text)?;
    check_header(&value)?;
    assemble(&value, limits)
}

fn limit(kind: LimitKind, found: u64, limit: u64) -> Error {
    LimitExceeded { kind, found, limit }.into()
}

/// Container depth, counting `{` and `[` outside strings. The outermost value
/// is depth 1. Returns the depth that first exceeded `limit`.
fn json_depth_past(text: &str, limit: u32) -> Option<u64> {
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' | '[' => {
                depth = depth.saturating_add(1);
                if depth > limit {
                    return Some(u64::from(depth));
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    None
}

fn parse_json(text: &str) -> Result<Value, Error> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = Value::deserialize(&mut deserializer).map_err(|error| json_error(&error))?;
    deserializer.end().map_err(|error| json_error(&error))?;
    Ok(value)
}

fn json_error(error: &serde_json::Error) -> Error {
    DocumentError::Json {
        line: error.line(),
        column: error.column(),
    }
    .into()
}

fn check_header(value: &Value) -> Result<(), Error> {
    let Some(object) = value.as_object() else {
        return Err(DocumentError::BomFormat.into());
    };
    match object.get("bomFormat") {
        Some(Value::String(text)) if text == "CycloneDX" => {}
        _ => return Err(DocumentError::BomFormat.into()),
    }
    let spec = object.get("specVersion");
    if !spec.is_some_and(supported_spec) {
        return Err(Error::UnsupportedSpecVersion {
            found: spec_version_found(spec),
        });
    }
    Ok(())
}

fn supported_spec(value: &Value) -> bool {
    matches!(
        value,
        Value::String(text) if matches!(text.as_str(), "1.4" | "1.5" | "1.6" | "1.7")
    )
}

fn spec_version_found(value: Option<&Value>) -> String {
    match value {
        None => "missing".to_owned(),
        Some(Value::String(text)) => sanitize_spec_version(text),
        Some(Value::Number(number)) => sanitize_spec_version(&number.to_string()),
        Some(Value::Bool(true)) => "true".to_owned(),
        Some(Value::Bool(false)) => "false".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::Array(_) | Value::Object(_)) => "invalid".to_owned(),
    }
}

const SPEC_VERSION_CHARS: usize = 64;

fn sanitize_spec_version(raw: &str) -> String {
    let mut escaped = String::new();
    for character in raw.chars() {
        if character <= '\u{001F}' || character == '\u{007F}' {
            let code = u32::from(character);
            let _ = write!(escaped, "\\u{{{code:04X}}}");
        } else {
            escaped.push(character);
        }
    }
    if escaped.chars().count() <= SPEC_VERSION_CHARS {
        escaped
    } else {
        escaped.chars().take(SPEC_VERSION_CHARS).collect()
    }
}

struct ParsedComponent {
    key: ComponentKey,
    version: VersionId,
    attributes: VersionAttributes,
    bom_ref: Option<String>,
    invalid_purl: bool,
}

struct Assembler<'a> {
    limits: &'a Limits,
    count: u64,
    components: BTreeMap<ComponentKey, Component>,
    warnings: BTreeSet<Warning>,
    refs: BTreeMap<String, ComponentKey>,
    root_key: Option<ComponentKey>,
    dependencies: BTreeMap<ComponentKey, BTreeSet<ComponentKey>>,
}

fn assemble(value: &Value, limits: &Limits) -> Result<Document, Error> {
    let Some(object) = value.as_object() else {
        return Err(DocumentError::Shape.into());
    };
    let mut assembler = Assembler {
        limits,
        count: 0,
        components: BTreeMap::new(),
        warnings: BTreeSet::new(),
        refs: BTreeMap::new(),
        root_key: None,
        dependencies: BTreeMap::new(),
    };
    assembler.read_inventory(object)?;
    let dependencies_present = assembler.read_dependencies(object)?;
    assembler.classify(dependencies_present);
    Ok(Document {
        components: assembler.components,
        dependencies: assembler.dependencies,
        warnings: assembler.warnings.into_iter().collect(),
    })
}

impl Assembler<'_> {
    fn read_inventory(&mut self, object: &Map<String, Value>) -> Result<(), Error> {
        if let Some(metadata) = object.get("metadata") {
            let metadata = metadata.as_object().ok_or(DocumentError::Shape)?;
            if let Some(component) = metadata.get("component") {
                self.read_component(component, 1, true)?;
            }
        }
        if let Some(components) = object.get("components") {
            let components = components.as_array().ok_or(DocumentError::Shape)?;
            for component in components {
                self.read_component(component, 1, false)?;
            }
        }
        Ok(())
    }

    fn read_component(&mut self, value: &Value, depth: u32, is_root: bool) -> Result<(), Error> {
        if depth > self.limits.max_depth {
            return Err(limit(
                LimitKind::NestingDepth,
                u64::from(depth),
                u64::from(self.limits.max_depth),
            ));
        }
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_components {
            return Err(limit(
                LimitKind::ComponentCount,
                self.count,
                self.limits.max_components,
            ));
        }
        let object = value.as_object().ok_or(DocumentError::Shape)?;
        let parsed = component_fields(object)?;
        if parsed.invalid_purl {
            self.warnings.insert(Warning {
                key: Some(parsed.key.clone()),
                kind: WarningKind::InvalidPurl,
            });
        }
        if let Some(bom_ref) = &parsed.bom_ref {
            if self.refs.contains_key(bom_ref) {
                self.warnings.insert(Warning {
                    key: Some(parsed.key.clone()),
                    kind: WarningKind::DuplicateBomRef,
                });
            } else {
                self.refs.insert(bom_ref.clone(), parsed.key.clone());
            }
        }
        if is_root {
            self.root_key = Some(parsed.key.clone());
        }
        self.merge(parsed.key, parsed.version, parsed.attributes);
        if let Some(children) = object.get("components") {
            let children = children.as_array().ok_or(DocumentError::Shape)?;
            let child_depth = depth.saturating_add(1);
            for child in children {
                self.read_component(child, child_depth, false)?;
            }
        }
        Ok(())
    }

    fn merge(&mut self, key: ComponentKey, version: VersionId, attributes: VersionAttributes) {
        let component = self.components.entry(key.clone()).or_default();
        match component.versions.entry(version) {
            Entry::Vacant(slot) => {
                slot.insert(attributes);
            }
            Entry::Occupied(mut slot) => {
                if slot.get_mut().absorb(&attributes) {
                    self.warnings.insert(Warning {
                        key: Some(key),
                        kind: WarningKind::ConflictingAttributes,
                    });
                }
            }
        }
    }

    fn read_dependencies(&mut self, object: &Map<String, Value>) -> Result<bool, Error> {
        let Some(dependencies) = object.get("dependencies") else {
            return Ok(false);
        };
        let dependencies = dependencies.as_array().ok_or(DocumentError::Shape)?;
        for entry in dependencies {
            let entry = entry.as_object().ok_or(DocumentError::Shape)?;
            let Some(Value::String(source)) = entry.get("ref") else {
                return Err(DocumentError::Shape.into());
            };
            let targets = depends_on(entry)?;
            let Some(source_key) = self.refs.get(source).cloned() else {
                self.warnings.insert(Warning {
                    key: None,
                    kind: WarningKind::OrphanRef,
                });
                continue;
            };
            let mut resolved = BTreeSet::new();
            let mut dangling = false;
            for target in targets {
                match self.refs.get(&target) {
                    Some(key) => {
                        resolved.insert(key.clone());
                    }
                    None => dangling = true,
                }
            }
            if dangling {
                self.warnings.insert(Warning {
                    key: Some(source_key.clone()),
                    kind: WarningKind::OrphanRef,
                });
            }
            if !resolved.is_empty() {
                self.dependencies
                    .entry(source_key)
                    .or_default()
                    .extend(resolved);
            }
        }
        Ok(true)
    }

    fn classify(&mut self, dependencies_present: bool) {
        let Some(root_key) = self.root_key.clone() else {
            self.warnings.insert(Warning {
                key: None,
                kind: WarningKind::ScopeUnknown,
            });
            return;
        };
        if !dependencies_present {
            self.warnings.insert(Warning {
                key: None,
                kind: WarningKind::ScopeUnknown,
            });
            return;
        }
        if let Some(component) = self.components.get_mut(&root_key) {
            component.role = DependencyRole::Root;
        }
        let directs = self
            .dependencies
            .get(&root_key)
            .cloned()
            .unwrap_or_default();
        for target in &directs {
            set_role(&mut self.components, target, DependencyRole::Direct);
        }
        // The visited set is what stops a cycle. Path length is not a limit.
        let mut stack: Vec<ComponentKey> = directs.into_iter().collect();
        let mut visited = BTreeSet::new();
        visited.insert(root_key);
        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            let Some(next) = self.dependencies.get(&node).cloned() else {
                continue;
            };
            for child in next {
                set_role(&mut self.components, &child, DependencyRole::Transitive);
                if !visited.contains(&child) {
                    stack.push(child);
                }
            }
        }
    }
}

fn set_role(
    components: &mut BTreeMap<ComponentKey, Component>,
    key: &ComponentKey,
    role: DependencyRole,
) {
    let Some(component) = components.get_mut(key) else {
        return;
    };
    if component.role == DependencyRole::Unknown {
        component.role = role;
    }
}

fn depends_on(entry: &Map<String, Value>) -> Result<Vec<String>, Error> {
    let Some(value) = entry.get("dependsOn") else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or(DocumentError::Shape)?;
    let mut targets = Vec::new();
    for item in items {
        match item {
            Value::String(reference) => targets.push(reference.clone()),
            _ => return Err(DocumentError::Shape.into()),
        }
    }
    Ok(targets)
}

fn component_fields(object: &Map<String, Value>) -> Result<ParsedComponent, Error> {
    let parsed_purl = match object.get("purl") {
        Some(Value::String(text)) => parse_purl(text),
        _ => None,
    };
    let invalid_purl = object.contains_key("purl") && parsed_purl.is_none();
    let key = match &parsed_purl {
        Some(purl) => purl.key.clone(),
        None => name_key(object)?,
    };
    let version = version_id(object, parsed_purl.as_ref());
    let mut attributes = attributes_of(object);
    if let Some(purl) = &parsed_purl {
        copy_purl_attributes(&mut attributes, purl);
    }
    Ok(ParsedComponent {
        key,
        version,
        attributes,
        bom_ref: object
            .get("bom-ref")
            .and_then(Value::as_str)
            .map(str::to_owned),
        invalid_purl,
    })
}

fn name_key(object: &Map<String, Value>) -> Result<ComponentKey, Error> {
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or(DocumentError::MissingName)?;
    let group = object
        .get("group")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Ok(ComponentKey::Name {
        group,
        name: name.to_owned(),
    })
}

fn version_id(object: &Map<String, Value>, purl: Option<&ParsedPurl>) -> VersionId {
    let component_version = object.get("version").and_then(Value::as_str).unwrap_or("");
    match purl {
        Some(purl) => VersionId {
            version: purl
                .version
                .clone()
                .unwrap_or_else(|| component_version.to_owned()),
            epoch: purl.epoch.clone(),
        },
        None => VersionId {
            version: component_version.to_owned(),
            epoch: None,
        },
    }
}

fn attributes_of(object: &Map<String, Value>) -> VersionAttributes {
    let mut attributes = VersionAttributes::default();
    if let Some(ty) = object.get("type").and_then(Value::as_str) {
        attributes.types.insert(ty.to_owned());
    }
    if let Some(scope) = object.get("scope").and_then(Value::as_str) {
        attributes.scopes.insert(scope.to_owned());
    }
    if let Some(name) = object
        .get("supplier")
        .and_then(Value::as_object)
        .and_then(|supplier| supplier.get("name"))
        .and_then(Value::as_str)
    {
        attributes.suppliers.insert(name.to_owned());
    }
    if let Some(licenses) = object.get("licenses") {
        collect_licenses(&mut attributes.licenses, licenses);
    }
    if let Some(hashes) = object.get("hashes") {
        collect_hashes(&mut attributes.hashes, hashes);
    }
    attributes
}

fn copy_purl_attributes(attributes: &mut VersionAttributes, purl: &ParsedPurl) {
    attributes.checksums.clone_from(&purl.checksums);
    attributes.download_urls.clone_from(&purl.download_urls);
    attributes.vcs_urls.clone_from(&purl.vcs_urls);
    attributes.file_names.clone_from(&purl.file_names);
    attributes.vers.clone_from(&purl.vers);
}

fn collect_licenses(target: &mut BTreeSet<String>, value: &Value) {
    let Some(items) = value.as_array() else {
        return;
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        if let Some(expression) = object.get("expression").and_then(Value::as_str) {
            target.insert(expression.to_owned());
        }
        if let Some(license) = object.get("license").and_then(Value::as_object) {
            if let Some(id) = license.get("id").and_then(Value::as_str) {
                target.insert(id.to_owned());
            }
            if let Some(name) = license.get("name").and_then(Value::as_str) {
                target.insert(name.to_owned());
            }
        }
    }
}

fn collect_hashes(target: &mut BTreeSet<ContentHash>, value: &Value) {
    let Some(items) = value.as_array() else {
        return;
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let (Some(alg), Some(content)) = (
            object.get("alg").and_then(Value::as_str),
            object.get("content").and_then(Value::as_str),
        ) else {
            continue;
        };
        target.insert(ContentHash {
            alg: alg.to_owned(),
            content: content.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::json_depth_past;

    #[test]
    fn braces_inside_strings_do_not_add_depth() {
        let text = r#"{"name":"a{b[c]d}","ok":true}"#;
        assert!(json_depth_past(text, 1).is_none());
        assert_eq!(json_depth_past(text, 0), Some(1));
    }
}
