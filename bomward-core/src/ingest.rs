// SPDX-License-Identifier: AGPL-3.0-only

//! Read a caller-supplied `CycloneDX` JSON document into the normalized model.
//!
//! Ingest is the only module that interprets SBOM bytes. It opens the path
//! the caller passed and no path found inside the document. Limits are the
//! defaults in [`crate::Limits`], or the values the caller supplies.
//!
//! `bomFormat` and `specVersion` are checked before any component is built.
//! Unknown JSON fields are skipped without building a tree. Duplicate object
//! keys keep the last value, which is what `serde_json` does, and the
//! document warns `duplicate-key` (ADR 0008).

mod decode;

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::{DocumentError, Error, LimitExceeded, LimitKind};
use crate::limits::Limits;
use crate::model::{
    Component, ComponentKey, DependencyRole, Document, GraphNode, Version, VersionAttributes,
    VersionId, Warning, WarningKind,
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
/// Returns [`Error::Io`] when the path cannot be read, [`Error::UnsupportedDepth`]
/// when `--max-depth` is above the supported maximum, and the same errors as
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
    limits.check_depth()?;
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
/// * [`Error::UnsupportedDepth`] when `--max-depth` is above
///   [`Limits::MAX_SUPPORTED_DEPTH`]. No bytes are parsed.
/// * [`Error::LimitExceeded`] when the byte length, the component count, or
///   the component nesting is over the limit, or when JSON containers are
///   nested past [`Limits::json_depth_limit`]. The JSON sentence does not
///   name `--max-depth`.
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
    limits.check_depth()?;
    let found = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if found > limits.max_file_bytes {
        return Err(limit(LimitKind::FileBytes, found, limits.max_file_bytes));
    }
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| DocumentError::Utf8)?;
    let duplicate_key = match scan_json(text, Limits::json_depth_limit()) {
        Ok(duplicate_key) => duplicate_key,
        Err(found_depth) => {
            return Err(limit(
                LimitKind::JsonNesting,
                found_depth,
                u64::from(Limits::json_depth_limit()),
            ));
        }
    };
    let inventory = decode::read_inventory(text, limits)?;
    assemble(inventory, duplicate_key)
}

fn limit(kind: LimitKind, found: u64, limit: u64) -> Error {
    LimitExceeded { kind, found, limit }.into()
}

/// Container depth, counting `{` and `[` outside strings. The outermost value
/// is depth 1. Returns the depth that first exceeded `limit`.
/// One pass over the text. `Ok` carries whether any object repeated a key.
/// `Err` is the container depth that crossed `limit`.
///
/// Key sets live only for objects that are still open, so the extra memory
/// stays within the file-size cap and the depth cap. A scan that loses the
/// grammar stops recording keys and keeps counting depth.
fn scan_json(text: &str, limit: u32) -> Result<bool, u64> {
    let mut scan = Scan {
        chars: text.chars().peekable(),
        depth: 0,
        limit,
        duplicate: false,
        tracking: true,
        frames: Vec::new(),
        text: Text::Code,
        key_buf: String::new(),
    };
    scan.run()?;
    Ok(scan.duplicate)
}

struct Scan<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    depth: u32,
    limit: u32,
    duplicate: bool,
    tracking: bool,
    frames: Vec<Container>,
    text: Text,
    key_buf: String,
}

struct Container {
    object: bool,
    phase: Phase,
    keys: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    KeyOrEnd,
    Colon,
    ValueOrEnd,
    CommaOrEnd,
}

enum Text {
    Code,
    Atom,
    String { capturing: bool, decode_ok: bool },
}

impl Scan<'_> {
    fn run(&mut self) -> Result<(), u64> {
        while let Some(character) = self.chars.next() {
            if self.in_string() {
                self.string_char(character);
                continue;
            }
            if self.in_atom() {
                if atom_char(character) {
                    continue;
                }
                self.finish_atom();
            }
            self.code_char(character)?;
        }
        Ok(())
    }

    fn in_string(&self) -> bool {
        matches!(self.text, Text::String { .. })
    }

    fn in_atom(&self) -> bool {
        matches!(self.text, Text::Atom)
    }

    fn code_char(&mut self, character: char) -> Result<(), u64> {
        match character {
            ' ' | '\n' | '\r' | '\t' => {}
            '{' => self.open(true)?,
            '[' => self.open(false)?,
            '}' => self.close(true),
            ']' => self.close(false),
            '"' => self.start_string(),
            ':' => self.colon(),
            ',' => self.comma(),
            other if self.tracking && atom_char(other) && self.value_expected() => {
                self.text = Text::Atom;
            }
            _ => self.lose(),
        }
        Ok(())
    }

    fn open(&mut self, object: bool) -> Result<(), u64> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > self.limit {
            return Err(u64::from(self.depth));
        }
        if self.tracking && self.value_expected() {
            self.frames.push(Container {
                object,
                phase: if object {
                    Phase::KeyOrEnd
                } else {
                    Phase::ValueOrEnd
                },
                keys: BTreeSet::new(),
            });
        } else {
            self.lose();
        }
        Ok(())
    }

    fn close(&mut self, object: bool) {
        self.depth = self.depth.saturating_sub(1);
        if !self.tracking {
            return;
        }
        let Some(frame) = self.frames.pop() else {
            self.lose();
            return;
        };
        let empty = (object && frame.phase == Phase::KeyOrEnd)
            || (!object && frame.phase == Phase::ValueOrEnd);
        if frame.object != object || !(empty || frame.phase == Phase::CommaOrEnd) {
            self.lose();
            return;
        }
        if let Some(parent) = self.frames.last_mut() {
            if parent.phase == Phase::ValueOrEnd {
                parent.phase = Phase::CommaOrEnd;
            } else {
                self.lose();
            }
        }
    }

    fn start_string(&mut self) {
        let capturing = self.tracking
            && self
                .frames
                .last()
                .is_some_and(|frame| frame.object && frame.phase == Phase::KeyOrEnd);
        if self.tracking && !capturing && !self.value_expected() {
            self.lose();
        }
        self.key_buf.clear();
        self.text = Text::String {
            capturing: capturing && self.tracking,
            decode_ok: true,
        };
    }

    fn colon(&mut self) {
        if !self.tracking {
            return;
        }
        if let Some(frame) = self.frames.last_mut()
            && frame.phase == Phase::Colon
        {
            frame.phase = Phase::ValueOrEnd;
        } else {
            self.lose();
        }
    }

    fn comma(&mut self) {
        if !self.tracking {
            return;
        }
        let Some(frame) = self.frames.last_mut() else {
            self.lose();
            return;
        };
        if frame.phase != Phase::CommaOrEnd {
            self.lose();
            return;
        }
        frame.phase = if frame.object {
            Phase::KeyOrEnd
        } else {
            Phase::ValueOrEnd
        };
    }

    fn finish_atom(&mut self) {
        self.text = Text::Code;
        if !self.tracking {
            return;
        }
        if let Some(frame) = self.frames.last_mut()
            && frame.phase == Phase::ValueOrEnd
        {
            frame.phase = Phase::CommaOrEnd;
        } else if !self.frames.is_empty() {
            self.lose();
        }
    }

    fn value_expected(&self) -> bool {
        match self.frames.last() {
            None => true,
            Some(frame) => frame.phase == Phase::ValueOrEnd,
        }
    }

    fn lose(&mut self) {
        self.tracking = false;
        self.frames.clear();
    }

    fn string_char(&mut self, character: char) {
        let Text::String {
            capturing,
            decode_ok,
        } = self.text
        else {
            return;
        };
        if character == '\\' {
            let Some(escaped) = self.chars.next() else {
                return;
            };
            if capturing && decode_ok {
                let still_ok = push_escape(escaped, &mut self.chars, &mut self.key_buf);
                self.text = Text::String {
                    capturing,
                    decode_ok: still_ok,
                };
            }
            return;
        }
        if character == '"' {
            self.finish_string(capturing, decode_ok);
            return;
        }
        if capturing && decode_ok {
            self.key_buf.push(character);
        }
    }

    fn finish_string(&mut self, capturing: bool, decode_ok: bool) {
        self.text = Text::Code;
        if !self.tracking {
            return;
        }
        if capturing {
            if !decode_ok {
                self.lose();
                return;
            }
            let key = std::mem::take(&mut self.key_buf);
            if let Some(frame) = self.frames.last_mut()
                && !frame.keys.insert(key)
            {
                self.duplicate = true;
            }
            if let Some(frame) = self.frames.last_mut() {
                frame.phase = Phase::Colon;
            }
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            if frame.phase == Phase::ValueOrEnd {
                frame.phase = Phase::CommaOrEnd;
            } else {
                self.lose();
            }
        }
    }
}

fn atom_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
}

fn push_escape(
    escaped: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    buf: &mut String,
) -> bool {
    match escaped {
        '"' | '\\' | '/' => buf.push(escaped),
        'b' => buf.push('\u{0008}'),
        'f' => buf.push('\u{000c}'),
        'n' => buf.push('\n'),
        'r' => buf.push('\r'),
        't' => buf.push('\t'),
        'u' => return push_unicode(chars, buf),
        _ => return false,
    }
    true
}

fn push_unicode(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, buf: &mut String) -> bool {
    let Some(unit) = take_hex4(chars) else {
        return false;
    };
    let code = if (0xD800..=0xDBFF).contains(&unit) {
        if !peek_unicode_escape(chars) {
            return false;
        }
        let Some(low) = take_hex4(chars) else {
            return false;
        };
        if !(0xDC00..=0xDFFF).contains(&low) {
            return false;
        }
        0x1_0000 + (((unit - 0xD800) << 10) | (low - 0xDC00))
    } else if (0xDC00..=0xDFFF).contains(&unit) {
        return false;
    } else {
        unit
    };
    match char::from_u32(code) {
        Some(character) => {
            buf.push(character);
            true
        }
        None => false,
    }
}

fn peek_unicode_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut look = chars.clone();
    if look.next() == Some('\\') && look.next() == Some('u') {
        chars.next();
        chars.next();
        true
    } else {
        false
    }
}

fn take_hex4(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<u32> {
    let mut value = 0_u32;
    for _ in 0..4 {
        let digit = chars.peek().copied().and_then(hex_digit)?;
        chars.next();
        value = value * 16 + digit;
    }
    Some(value)
}

fn hex_digit(character: char) -> Option<u32> {
    match character {
        '0'..='9' => Some(u32::from(character) - u32::from('0')),
        'a'..='f' => Some(u32::from(character) - u32::from('a') + 10),
        'A'..='F' => Some(u32::from(character) - u32::from('A') + 10),
        _ => None,
    }
}

struct ParsedComponent {
    key: ComponentKey,
    version: VersionId,
    attributes: VersionAttributes,
    bom_ref: Option<String>,
    invalid_purl: bool,
    version_mismatch: bool,
}

struct Assembler {
    components: BTreeMap<ComponentKey, Component>,
    warnings: BTreeSet<Warning>,
    refs: BTreeMap<String, GraphNode>,
    root: Option<GraphNode>,
    dependencies: BTreeMap<GraphNode, BTreeSet<GraphNode>>,
}

fn assemble(inventory: decode::Inventory, duplicate_key: bool) -> Result<Document, Error> {
    let mut assembler = Assembler {
        components: BTreeMap::new(),
        warnings: BTreeSet::new(),
        refs: BTreeMap::new(),
        root: None,
        dependencies: BTreeMap::new(),
    };
    if let Some(root) = inventory.metadata {
        assembler.insert_tree(root, true)?;
    }
    for component in inventory.components {
        assembler.insert_tree(component, false)?;
    }
    let dependencies_present = match inventory.dependencies {
        Some(entries) => {
            assembler.read_dependencies(entries);
            true
        }
        None => false,
    };
    assembler.classify(dependencies_present);
    if assembler.components.is_empty() {
        assembler.warnings.insert(Warning {
            key: None,
            kind: WarningKind::NoComponents,
        });
    }
    if duplicate_key {
        assembler.warnings.insert(Warning {
            key: None,
            kind: WarningKind::DuplicateKey,
        });
    }
    Ok(Document {
        components: assembler.components,
        dependencies: assembler.dependencies,
        warnings: assembler.warnings.into_iter().collect(),
    })
}

impl Assembler {
    fn insert_tree(&mut self, raw: decode::RawComponent, is_root: bool) -> Result<(), Error> {
        let (parsed, children) = component_fields(raw)?;
        if parsed.invalid_purl {
            self.warnings.insert(Warning {
                key: Some(parsed.key.clone()),
                kind: WarningKind::InvalidPurl,
            });
        }
        if parsed.version_mismatch {
            self.warnings.insert(Warning {
                key: Some(parsed.key.clone()),
                kind: WarningKind::VersionMismatch,
            });
        }
        let node = GraphNode {
            key: parsed.key.clone(),
            version: parsed.version.clone(),
        };
        if let Some(bom_ref) = &parsed.bom_ref {
            if self.refs.contains_key(bom_ref) {
                self.warnings.insert(Warning {
                    key: Some(parsed.key.clone()),
                    kind: WarningKind::DuplicateBomRef,
                });
            } else {
                self.refs.insert(bom_ref.clone(), node.clone());
            }
        }
        if is_root {
            self.root = Some(node);
        }
        self.merge(parsed.key, parsed.version, parsed.attributes);
        for child in children {
            self.insert_tree(child, false)?;
        }
        Ok(())
    }

    fn merge(&mut self, key: ComponentKey, version: VersionId, attributes: VersionAttributes) {
        let component = self.components.entry(key.clone()).or_default();
        match component.versions.entry(version) {
            Entry::Vacant(slot) => {
                slot.insert(Version {
                    attributes,
                    role: DependencyRole::Unknown,
                });
            }
            Entry::Occupied(mut slot) => {
                if slot.get_mut().attributes.absorb(&attributes) {
                    self.warnings.insert(Warning {
                        key: Some(key),
                        kind: WarningKind::ConflictingAttributes,
                    });
                }
            }
        }
    }

    fn read_dependencies(&mut self, entries: Vec<decode::RawDependency>) {
        for entry in entries {
            let Some(source) = self.refs.get(&entry.source).cloned() else {
                self.warnings.insert(Warning {
                    key: None,
                    kind: WarningKind::OrphanRef,
                });
                continue;
            };
            let mut resolved = BTreeSet::new();
            let mut dangling = false;
            for target in entry.targets {
                match self.refs.get(&target) {
                    Some(node) => {
                        resolved.insert(node.clone());
                    }
                    None => dangling = true,
                }
            }
            if dangling {
                self.warnings.insert(Warning {
                    key: Some(source.key.clone()),
                    kind: WarningKind::OrphanRef,
                });
            }
            if !resolved.is_empty() {
                self.dependencies
                    .entry(source)
                    .or_default()
                    .extend(resolved);
            }
        }
    }

    fn classify(&mut self, dependencies_present: bool) {
        let Some(root) = self.root.clone() else {
            self.warnings.insert(Warning {
                key: None,
                kind: WarningKind::RoleUnknown,
            });
            return;
        };
        if !dependencies_present {
            self.warnings.insert(Warning {
                key: None,
                kind: WarningKind::RoleUnknown,
            });
            return;
        }
        set_role(&mut self.components, &root, DependencyRole::Root);
        let directs = self.dependencies.get(&root).cloned().unwrap_or_default();
        for target in &directs {
            set_role(&mut self.components, target, DependencyRole::Direct);
        }
        // The visited set is what stops a cycle. Path length is not a limit.
        let mut stack: Vec<GraphNode> = directs.into_iter().collect();
        let mut visited = BTreeSet::new();
        visited.insert(root);
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
        warn_unreachable(self);
    }
}

fn warn_unreachable(assembler: &mut Assembler) {
    let mut keys = BTreeSet::new();
    for (key, component) in &assembler.components {
        if component
            .versions
            .values()
            .any(|stored| stored.role == DependencyRole::Unknown)
        {
            keys.insert(key.clone());
        }
    }
    for key in keys {
        assembler.warnings.insert(Warning {
            key: Some(key),
            kind: WarningKind::Unreachable,
        });
    }
}

fn set_role(
    components: &mut BTreeMap<ComponentKey, Component>,
    node: &GraphNode,
    role: DependencyRole,
) {
    let Some(component) = components.get_mut(&node.key) else {
        return;
    };
    let Some(version) = component.versions.get_mut(&node.version) else {
        return;
    };
    if version.role == DependencyRole::Unknown || role == DependencyRole::Root {
        version.role = role;
    }
}

fn component_fields(
    raw: decode::RawComponent,
) -> Result<(ParsedComponent, Vec<decode::RawComponent>), Error> {
    let decode::RawComponent {
        name,
        group,
        version,
        purl,
        bom_ref,
        ty,
        scope,
        supplier,
        licenses,
        hashes,
        children,
    } = raw;
    let parsed_purl = match &purl {
        decode::PurlField::Text(text) => parse_purl(text),
        decode::PurlField::Absent | decode::PurlField::NotString => None,
    };
    let invalid_purl = !matches!(purl, decode::PurlField::Absent) && parsed_purl.is_none();
    let version_mismatch = parsed_purl.as_ref().is_some_and(|parsed| {
        match (parsed.version.as_deref(), version.as_deref()) {
            (Some(purl_version), Some(component_version)) => purl_version != component_version,
            _ => false,
        }
    });
    let key = match &parsed_purl {
        Some(parsed) => parsed.key.clone(),
        None => name_key(name, group)?,
    };
    let version = version_id(version, parsed_purl.as_ref());
    let mut attributes = VersionAttributes {
        licenses,
        hashes,
        suppliers: supplier.into_iter().collect(),
        scopes: scope.into_iter().collect(),
        types: ty.into_iter().collect(),
        ..VersionAttributes::default()
    };
    if let Some(parsed) = &parsed_purl {
        copy_purl_attributes(&mut attributes, parsed);
    }
    Ok((
        ParsedComponent {
            key,
            version,
            attributes,
            bom_ref,
            invalid_purl,
            version_mismatch,
        },
        children,
    ))
}

fn name_key(name: Option<String>, group: Option<String>) -> Result<ComponentKey, Error> {
    let name = name.ok_or(DocumentError::MissingName)?;
    Ok(ComponentKey::Name {
        group: group.unwrap_or_default(),
        name,
    })
}

fn version_id(component_version: Option<String>, purl: Option<&ParsedPurl>) -> VersionId {
    match purl {
        Some(purl) => VersionId {
            version: purl
                .version
                .clone()
                .unwrap_or_else(|| component_version.unwrap_or_default()),
            epoch: purl.epoch.clone(),
        },
        None => VersionId {
            version: component_version.unwrap_or_default(),
            epoch: None,
        },
    }
}

fn copy_purl_attributes(attributes: &mut VersionAttributes, purl: &ParsedPurl) {
    attributes.checksums.clone_from(&purl.checksums);
    attributes.download_urls.clone_from(&purl.download_urls);
    attributes.vcs_urls.clone_from(&purl.vcs_urls);
    attributes.file_names.clone_from(&purl.file_names);
    attributes.vers.clone_from(&purl.vers);
}

#[cfg(test)]
mod tests {
    use super::scan_json;

    #[test]
    fn braces_inside_strings_do_not_add_depth() {
        let text = r#"{"name":"a{b[c]d}","ok":true}"#;
        assert!(scan_json(text, 1).is_ok());
        assert_eq!(scan_json(text, 0), Err(1));
    }

    #[test]
    fn duplicate_keys_are_flagged_without_confusing_nested_objects() -> Result<(), String> {
        let cases = [
            (r#"{"a":1,"a":2}"#, true),
            (r#"{"a":{"a":1}}"#, false),
            (r#"{"a":"a"}"#, false),
            (r#"{"a":1,"\u0061":2}"#, true),
            (r#"{"/":1,"\/":2}"#, true),
            (r#"[{"a":1},{"a":2}]"#, false),
            (r#"{ "a" : 1 , "b" : 2 }"#, false),
            (r#"{"a":"a{a}"}"#, false),
        ];
        for (text, duplicate) in cases {
            match scan_json(text, 127) {
                Ok(found) if found == duplicate => {}
                Ok(found) => return Err(format!("{text} -> {found}")),
                Err(depth) => return Err(format!("{text} depth {depth}")),
            }
        }
        Ok(())
    }
}
