// SPDX-License-Identifier: AGPL-3.0-only

//! Typed `CycloneDX` decode.
//!
//! Only the fields ADR 0010 names are materialized. Every other value is
//! skipped with [`IgnoredAny`]. Component count and nesting are checked while
//! a component object is entered, so tokens after a failed limit are not read.
//!
//! `bomFormat` and `specVersion` are accepted before a following component
//! list becomes a model. A list that appears earlier is kept as raw JSON and
//! interpreted only after the header is accepted. ADR 0008 records why.

use std::collections::BTreeSet;
use std::fmt::{self, Write};

use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;

use crate::error::{DocumentError, Error, LimitExceeded, LimitKind};
use crate::limits::Limits;
use crate::model::ContentHash;

const MARK: char = '\u{1f}';
const SPEC_VERSION_CHARS: usize = 64;

pub(super) struct Inventory {
    pub(super) metadata: Option<RawComponent>,
    pub(super) components: Vec<RawComponent>,
    pub(super) dependencies: Option<Vec<RawDependency>>,
}

pub(super) struct RawComponent {
    pub(super) name: Option<String>,
    pub(super) group: Option<String>,
    pub(super) version: Option<String>,
    pub(super) purl: PurlField,
    pub(super) bom_ref: Option<String>,
    pub(super) ty: Option<String>,
    pub(super) scope: Option<String>,
    pub(super) supplier: Option<String>,
    pub(super) licenses: BTreeSet<String>,
    pub(super) hashes: BTreeSet<ContentHash>,
    pub(super) children: Vec<RawComponent>,
}

pub(super) enum PurlField {
    Absent,
    Text(String),
    NotString,
}

pub(super) struct RawDependency {
    pub(super) source: String,
    pub(super) targets: Vec<String>,
}

pub(super) fn read_inventory(text: &str, limits: &Limits) -> Result<Inventory, Error> {
    let mut seed = TopSeed::new(limits);
    let mut deserializer = serde_json::Deserializer::from_str(text);
    (&mut seed)
        .deserialize(&mut deserializer)
        .map_err(|error| classify(&error))?;
    deserializer.end().map_err(|error| classify(&error))?;
    seed.finish()
}

struct Budget<'a> {
    limits: &'a Limits,
    count: u64,
}

impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self { limits, count: 0 }
    }

    fn admit<E: de::Error>(&mut self, depth: u32) -> Result<(), E> {
        if depth > self.limits.max_depth {
            return Err(fail_limit(
                LimitKind::NestingDepth,
                u64::from(depth),
                u64::from(self.limits.max_depth),
            ));
        }
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_components {
            return Err(fail_limit(
                LimitKind::ComponentCount,
                self.count,
                self.limits.max_components,
            ));
        }
        Ok(())
    }

    fn undo(&mut self, added: Option<u64>) {
        if let Some(added) = added {
            self.count = self.count.saturating_sub(added);
        }
    }
}

struct TopSeed<'a> {
    budget: Budget<'a>,
    bom: BomState,
    spec: SpecState,
    early_metadata: Option<Box<RawValue>>,
    early_components: Option<Box<RawValue>>,
    early_dependencies: Option<Box<RawValue>>,
    metadata: Option<RawComponent>,
    metadata_added: Option<u64>,
    components: Vec<RawComponent>,
    components_added: Option<u64>,
    dependencies: Option<Vec<RawDependency>>,
    flushed: bool,
}

impl<'a> TopSeed<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            budget: Budget::new(limits),
            bom: BomState::Missing,
            spec: SpecState::Missing,
            early_metadata: None,
            early_components: None,
            early_dependencies: None,
            metadata: None,
            metadata_added: None,
            components: Vec::new(),
            components_added: None,
            dependencies: None,
            flushed: false,
        }
    }

    fn header_accepted(&self) -> bool {
        self.bom == BomState::Ok && matches!(self.spec, SpecState::Supported)
    }

    fn finish(self) -> Result<Inventory, Error> {
        if self.bom != BomState::Ok {
            return Err(DocumentError::BomFormat.into());
        }
        match self.spec {
            SpecState::Supported => Ok(Inventory {
                metadata: self.metadata,
                components: self.components,
                dependencies: self.dependencies,
            }),
            SpecState::Missing => Err(Error::UnsupportedSpecVersion {
                found: "missing".to_owned(),
            }),
            SpecState::Rejected(found) => Err(Error::UnsupportedSpecVersion { found }),
        }
    }

    fn flush_early<E: de::Error>(&mut self) -> Result<(), E> {
        if !self.header_accepted() || self.flushed {
            return Ok(());
        }
        self.flushed = true;
        if let Some(raw) = self.early_metadata.take() {
            self.replace_metadata(|budget| metadata_from_raw(&raw, budget))?;
        }
        if let Some(raw) = self.early_components.take() {
            self.replace_components(|budget| components_from_raw(&raw, budget))?;
        }
        if let Some(raw) = self.early_dependencies.take() {
            self.dependencies =
                Some(dependencies_from_raw(&raw).map_err(|error| forward_error(&error))?);
        }
        Ok(())
    }

    fn take_metadata<'de, A: MapAccess<'de>>(&mut self, map: &mut A) -> Result<(), A::Error> {
        if self.header_accepted() {
            self.replace_metadata(|budget| map.next_value_seed(MetadataSeed { budget }))
        } else {
            self.early_metadata = Some(map.next_value()?);
            Ok(())
        }
    }

    fn take_components<'de, A: MapAccess<'de>>(&mut self, map: &mut A) -> Result<(), A::Error> {
        if self.header_accepted() {
            self.replace_components(|budget| map.next_value_seed(ListSeed { budget, depth: 1 }))
        } else {
            self.early_components = Some(map.next_value()?);
            Ok(())
        }
    }

    fn take_dependencies<'de, A: MapAccess<'de>>(&mut self, map: &mut A) -> Result<(), A::Error> {
        if self.header_accepted() {
            self.dependencies = Some(map.next_value_seed(DependencyListSeed)?);
            Ok(())
        } else {
            self.early_dependencies = Some(map.next_value()?);
            Ok(())
        }
    }

    fn replace_metadata<E: de::Error>(
        &mut self,
        load: impl FnOnce(&mut Budget<'_>) -> Result<Option<RawComponent>, E>,
    ) -> Result<(), E> {
        self.budget.undo(self.metadata_added.take());
        let before = self.budget.count;
        self.metadata = load(&mut self.budget)?;
        self.metadata_added = Some(self.budget.count.saturating_sub(before));
        Ok(())
    }

    fn replace_components<E: de::Error>(
        &mut self,
        load: impl FnOnce(&mut Budget<'_>) -> Result<Vec<RawComponent>, E>,
    ) -> Result<(), E> {
        self.budget.undo(self.components_added.take());
        let before = self.budget.count;
        self.components = load(&mut self.budget)?;
        self.components_added = Some(self.budget.count.saturating_sub(before));
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for &mut TopSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TopVisitor { seed: self })
    }
}

struct TopVisitor<'a, 'l> {
    seed: &'a mut TopSeed<'l>,
}

impl<'de> Visitor<'de> for TopVisitor<'_, '_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a CycloneDX object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<TopKey>()? {
            match key {
                TopKey::BomFormat => {
                    self.seed.bom = map.next_value()?;
                    self.seed.flush_early()?;
                }
                TopKey::SpecVersion => {
                    self.seed.spec = map.next_value()?;
                    self.seed.flush_early()?;
                }
                TopKey::Metadata => self.seed.take_metadata(&mut map)?,
                TopKey::Components => self.seed.take_components(&mut map)?,
                TopKey::Dependencies => self.seed.take_dependencies(&mut map)?,
                TopKey::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Err(fail_bom())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<(), E> {
        Err(fail_bom())
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Err(fail_bom())
    }
}

fn metadata_from_raw<E: de::Error>(
    raw: &RawValue,
    budget: &mut Budget<'_>,
) -> Result<Option<RawComponent>, E> {
    let mut deserializer = serde_json::Deserializer::from_str(raw.get());
    let parsed = MetadataSeed { budget }
        .deserialize(&mut deserializer)
        .map_err(|error| forward_error(&error))?;
    deserializer.end().map_err(|error| forward_error(&error))?;
    Ok(parsed)
}

fn components_from_raw<E: de::Error>(
    raw: &RawValue,
    budget: &mut Budget<'_>,
) -> Result<Vec<RawComponent>, E> {
    let mut deserializer = serde_json::Deserializer::from_str(raw.get());
    let parsed = ListSeed { budget, depth: 1 }
        .deserialize(&mut deserializer)
        .map_err(|error| forward_error(&error))?;
    deserializer.end().map_err(|error| forward_error(&error))?;
    Ok(parsed)
}

fn dependencies_from_raw(raw: &RawValue) -> Result<Vec<RawDependency>, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(raw.get());
    let parsed = DependencyListSeed.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(parsed)
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "camelCase")]
enum TopKey {
    #[serde(rename = "bomFormat")]
    BomFormat,
    #[serde(rename = "specVersion")]
    SpecVersion,
    Metadata,
    Components,
    Dependencies,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BomState {
    Missing,
    Ok,
    Bad,
}

impl<'de> Deserialize<'de> for BomState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BomVisitor)
    }
}

struct BomVisitor;

impl<'de> Visitor<'de> for BomVisitor {
    type Value = BomState;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bomFormat")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<BomState, E> {
        if value == "CycloneDX" {
            Ok(BomState::Ok)
        } else {
            Ok(BomState::Bad)
        }
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<BomState, E> {
        self.visit_str(&value)
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<BomState, E> {
        Ok(BomState::Bad)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<BomState, E> {
        Ok(BomState::Bad)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<BomState, E> {
        Ok(BomState::Bad)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<BomState, E> {
        Ok(BomState::Bad)
    }

    fn visit_unit<E: de::Error>(self) -> Result<BomState, E> {
        Ok(BomState::Bad)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<BomState, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(BomState::Bad)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<BomState, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(BomState::Bad)
    }
}

enum SpecState {
    Missing,
    Supported,
    Rejected(String),
}

impl<'de> Deserialize<'de> for SpecState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SpecVisitor)
    }
}

struct SpecVisitor;

impl<'de> Visitor<'de> for SpecVisitor {
    type Value = SpecState;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("specVersion")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<SpecState, E> {
        Ok(spec_from_text(value))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<SpecState, E> {
        self.visit_str(&value)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<SpecState, E> {
        Ok(SpecState::Rejected(
            if value { "true" } else { "false" }.to_owned(),
        ))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<SpecState, E> {
        Ok(spec_from_text(&value.to_string()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<SpecState, E> {
        Ok(spec_from_text(&value.to_string()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<SpecState, E> {
        Ok(spec_from_text(&value.to_string()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<SpecState, E> {
        Ok(SpecState::Rejected("null".to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<SpecState, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(SpecState::Rejected("invalid".to_owned()))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<SpecState, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(SpecState::Rejected("invalid".to_owned()))
    }
}

fn spec_from_text(value: &str) -> SpecState {
    if matches!(value, "1.4" | "1.5" | "1.6" | "1.7") {
        SpecState::Supported
    } else {
        SpecState::Rejected(sanitize_spec_version(value))
    }
}

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

struct MetadataSeed<'a, 'l> {
    budget: &'a mut Budget<'l>,
}

impl<'de> DeserializeSeed<'de> for MetadataSeed<'_, '_> {
    type Value = Option<RawComponent>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(MetadataVisitor {
            budget: self.budget,
        })
    }
}

struct MetadataVisitor<'a, 'l> {
    budget: &'a mut Budget<'l>,
}

impl<'de> Visitor<'de> for MetadataVisitor<'_, '_> {
    type Value = Option<RawComponent>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("metadata")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut component = None;
        let mut added = None;
        while let Some(key) = map.next_key::<OneField>()? {
            match key {
                OneField::Known => {
                    self.budget.undo(added.take());
                    let before = self.budget.count;
                    component = Some(map.next_value_seed(ComponentSeed {
                        budget: self.budget,
                        depth: 1,
                    })?);
                    added = Some(self.budget.count.saturating_sub(before));
                }
                OneField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(component)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Err(fail_shape())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(fail_shape())
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum OneField {
    #[serde(rename = "component")]
    Known,
    #[serde(other)]
    Other,
}

struct ListSeed<'a, 'l> {
    budget: &'a mut Budget<'l>,
    depth: u32,
}

impl<'de> DeserializeSeed<'de> for ListSeed<'_, '_> {
    type Value = Vec<RawComponent>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ListVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}

struct ListVisitor<'a, 'l> {
    budget: &'a mut Budget<'l>,
    depth: u32,
}

impl<'de> Visitor<'de> for ListVisitor<'_, '_> {
    type Value = Vec<RawComponent>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("components")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut components = Vec::new();
        loop {
            let Some(component) = seq.next_element_seed(ComponentSeed {
                budget: self.budget,
                depth: self.depth,
            })?
            else {
                break;
            };
            components.push(component);
        }
        Ok(components)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Err(fail_shape())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(fail_shape())
    }
}

struct ComponentSeed<'a, 'l> {
    budget: &'a mut Budget<'l>,
    depth: u32,
}

impl<'de> DeserializeSeed<'de> for ComponentSeed<'_, '_> {
    type Value = RawComponent;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ComponentVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}

struct ComponentVisitor<'a, 'l> {
    budget: &'a mut Budget<'l>,
    depth: u32,
}

impl<'de> Visitor<'de> for ComponentVisitor<'_, '_> {
    type Value = RawComponent;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("component")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.budget.admit(self.depth)?;
        let mut raw = RawComponent {
            name: None,
            group: None,
            version: None,
            purl: PurlField::Absent,
            bom_ref: None,
            ty: None,
            scope: None,
            supplier: None,
            licenses: BTreeSet::new(),
            hashes: BTreeSet::new(),
            children: Vec::new(),
        };
        let mut children_added = None;
        while let Some(key) = map.next_key::<ComponentField>()? {
            match key {
                ComponentField::Name => raw.name = map.next_value::<MaybeString>()?.text(),
                ComponentField::Group => raw.group = map.next_value::<MaybeString>()?.text(),
                ComponentField::Version => raw.version = map.next_value::<MaybeString>()?.text(),
                ComponentField::Purl => raw.purl = map.next_value()?,
                ComponentField::BomRef => raw.bom_ref = map.next_value::<MaybeString>()?.text(),
                ComponentField::Type => raw.ty = map.next_value::<MaybeString>()?.text(),
                ComponentField::Scope => raw.scope = map.next_value::<MaybeString>()?.text(),
                ComponentField::Supplier => raw.supplier = map.next_value_seed(SupplierSeed)?,
                ComponentField::Licenses => raw.licenses = map.next_value_seed(LicenseSeed)?,
                ComponentField::Hashes => raw.hashes = map.next_value_seed(HashSeed)?,
                ComponentField::Components => {
                    self.budget.undo(children_added.take());
                    let before = self.budget.count;
                    raw.children = map.next_value_seed(ListSeed {
                        budget: self.budget,
                        depth: self.depth.saturating_add(1),
                    })?;
                    children_added = Some(self.budget.count.saturating_sub(before));
                }
                ComponentField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(raw)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        self.reject()
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        self.reject()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.reject()
    }
}

impl ComponentVisitor<'_, '_> {
    fn reject<E: de::Error>(self) -> Result<RawComponent, E> {
        self.budget.admit(self.depth)?;
        Err(fail_shape())
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum ComponentField {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "group")]
    Group,
    #[serde(rename = "version")]
    Version,
    #[serde(rename = "purl")]
    Purl,
    #[serde(rename = "bom-ref")]
    BomRef,
    #[serde(rename = "type")]
    Type,
    #[serde(rename = "scope")]
    Scope,
    #[serde(rename = "supplier")]
    Supplier,
    #[serde(rename = "licenses")]
    Licenses,
    #[serde(rename = "hashes")]
    Hashes,
    #[serde(rename = "components")]
    Components,
    #[serde(other)]
    Other,
}

enum MaybeString {
    Text(String),
    Other,
}

impl MaybeString {
    fn text(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Other => None,
        }
    }
}

impl<'de> Deserialize<'de> for MaybeString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(MaybeStringVisitor)
    }
}

struct MaybeStringVisitor;

impl<'de> Visitor<'de> for MaybeStringVisitor {
    type Value = MaybeString;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<MaybeString, E> {
        Ok(MaybeString::Text(value.to_owned()))
    }

    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<MaybeString, E> {
        self.visit_str(value)
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<MaybeString, E> {
        Ok(MaybeString::Text(value))
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<MaybeString, E> {
        Ok(MaybeString::Other)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<MaybeString, E> {
        Ok(MaybeString::Other)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<MaybeString, E> {
        Ok(MaybeString::Other)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<MaybeString, E> {
        Ok(MaybeString::Other)
    }

    fn visit_unit<E: de::Error>(self) -> Result<MaybeString, E> {
        Ok(MaybeString::Other)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<MaybeString, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(MaybeString::Other)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MaybeString, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(MaybeString::Other)
    }
}

impl<'de> Deserialize<'de> for PurlField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match MaybeString::deserialize(deserializer)? {
            MaybeString::Text(text) => Self::Text(text),
            MaybeString::Other => Self::NotString,
        })
    }
}

struct SupplierSeed;

impl<'de> DeserializeSeed<'de> for SupplierSeed {
    type Value = Option<String>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SupplierVisitor)
    }
}

struct SupplierVisitor;

impl<'de> Visitor<'de> for SupplierVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("supplier")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name = None;
        while let Some(key) = map.next_key::<NameField>()? {
            match key {
                NameField::Name => name = map.next_value::<MaybeString>()?.text(),
                NameField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(name)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum NameField {
    #[serde(rename = "name")]
    Name,
    #[serde(other)]
    Other,
}

struct LicenseSeed;

impl<'de> DeserializeSeed<'de> for LicenseSeed {
    type Value = BTreeSet<String>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LicenseVisitor)
    }
}

struct LicenseVisitor;

impl<'de> Visitor<'de> for LicenseVisitor {
    type Value = BTreeSet<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("licenses")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut licenses = BTreeSet::new();
        while let Some(item) = seq.next_element_seed(LicenseItemSeed)? {
            licenses.extend(item);
        }
        Ok(licenses)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(BTreeSet::new())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }
}

struct LicenseItemSeed;

impl<'de> DeserializeSeed<'de> for LicenseItemSeed {
    type Value = BTreeSet<String>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LicenseItemVisitor)
    }
}

struct LicenseItemVisitor;

impl<'de> Visitor<'de> for LicenseItemVisitor {
    type Value = BTreeSet<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("license choice")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut licenses = BTreeSet::new();
        while let Some(key) = map.next_key::<LicenseField>()? {
            match key {
                LicenseField::Expression => {
                    if let Some(text) = map.next_value::<MaybeString>()?.text() {
                        licenses.insert(text);
                    }
                }
                LicenseField::License => {
                    let body = map.next_value::<LicenseBody>()?;
                    if let Some(id) = body.id {
                        licenses.insert(id);
                    }
                    if let Some(name) = body.name {
                        licenses.insert(name);
                    }
                }
                LicenseField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(licenses)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(BTreeSet::new())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum LicenseField {
    #[serde(rename = "expression")]
    Expression,
    #[serde(rename = "license")]
    License,
    #[serde(other)]
    Other,
}

struct LicenseBody {
    id: Option<String>,
    name: Option<String>,
}

impl<'de> Deserialize<'de> for LicenseBody {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LicenseBodyVisitor)
    }
}

struct LicenseBodyVisitor;

impl<'de> Visitor<'de> for LicenseBodyVisitor {
    type Value = LicenseBody;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("license")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut body = LicenseBody {
            id: None,
            name: None,
        };
        while let Some(key) = map.next_key::<LicenseBodyField>()? {
            match key {
                LicenseBodyField::Id => body.id = map.next_value::<MaybeString>()?.text(),
                LicenseBodyField::Name => body.name = map.next_value::<MaybeString>()?.text(),
                LicenseBodyField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(body)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(LicenseBody {
            id: None,
            name: None,
        })
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum LicenseBodyField {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "name")]
    Name,
    #[serde(other)]
    Other,
}

struct HashSeed;

impl<'de> DeserializeSeed<'de> for HashSeed {
    type Value = BTreeSet<ContentHash>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HashVisitor)
    }
}

struct HashVisitor;

impl<'de> Visitor<'de> for HashVisitor {
    type Value = BTreeSet<ContentHash>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hashes")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut hashes = BTreeSet::new();
        while let Some(hash) = seq.next_element_seed(HashItemSeed)? {
            if let Some(hash) = hash {
                hashes.insert(hash);
            }
        }
        Ok(hashes)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(BTreeSet::new())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(BTreeSet::new())
    }
}

struct HashItemSeed;

impl<'de> DeserializeSeed<'de> for HashItemSeed {
    type Value = Option<ContentHash>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HashItemVisitor)
    }
}

struct HashItemVisitor;

impl<'de> Visitor<'de> for HashItemVisitor {
    type Value = Option<ContentHash>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hash")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut alg = None;
        let mut content = None;
        while let Some(key) = map.next_key::<HashField>()? {
            match key {
                HashField::Alg => alg = map.next_value::<MaybeString>()?.text(),
                HashField::Content => content = map.next_value::<MaybeString>()?.text(),
                HashField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(match (alg, content) {
            (Some(alg), Some(content)) => Some(ContentHash { alg, content }),
            _ => None,
        })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum HashField {
    #[serde(rename = "alg")]
    Alg,
    #[serde(rename = "content")]
    Content,
    #[serde(other)]
    Other,
}

struct DependencyListSeed;

impl<'de> DeserializeSeed<'de> for DependencyListSeed {
    type Value = Vec<RawDependency>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DependencyListVisitor)
    }
}

struct DependencyListVisitor;

impl<'de> Visitor<'de> for DependencyListVisitor {
    type Value = Vec<RawDependency>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dependencies")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut entries = Vec::new();
        while let Some(entry) = seq.next_element_seed(DependencySeed)? {
            entries.push(entry);
        }
        Ok(entries)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Err(fail_shape())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(fail_shape())
    }
}

struct DependencySeed;

impl<'de> DeserializeSeed<'de> for DependencySeed {
    type Value = RawDependency;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DependencyVisitor)
    }
}

struct DependencyVisitor;

impl<'de> Visitor<'de> for DependencyVisitor {
    type Value = RawDependency;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dependency")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut source = None;
        let mut targets = Vec::new();
        while let Some(key) = map.next_key::<DependencyField>()? {
            match key {
                DependencyField::Ref => {
                    source = Some(require_string(map.next_value()?)?);
                }
                DependencyField::DependsOn => {
                    targets = map.next_value_seed(TargetListSeed)?;
                }
                DependencyField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        let Some(source) = source else {
            return Err(fail_shape());
        };
        Ok(RawDependency { source, targets })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Err(fail_shape())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(fail_shape())
    }
}

fn require_string<E: de::Error>(value: MaybeString) -> Result<String, E> {
    match value {
        MaybeString::Text(text) => Ok(text),
        MaybeString::Other => Err(fail_shape()),
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum DependencyField {
    #[serde(rename = "ref")]
    Ref,
    #[serde(rename = "dependsOn")]
    DependsOn,
    #[serde(other)]
    Other,
}

struct TargetListSeed;

impl<'de> DeserializeSeed<'de> for TargetListSeed {
    type Value = Vec<String>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TargetListVisitor)
    }
}

struct TargetListVisitor;

impl<'de> Visitor<'de> for TargetListVisitor {
    type Value = Vec<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dependsOn")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut targets = Vec::new();
        while let Some(item) = seq.next_element::<MaybeString>()? {
            targets.push(require_string(item)?);
        }
        Ok(targets)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Err(fail_shape())
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Err(fail_shape())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Err(fail_shape())
    }
}

fn fail_limit<E: de::Error>(kind: LimitKind, found: u64, limit: u64) -> E {
    let tag = match kind {
        LimitKind::ComponentCount => "components",
        LimitKind::NestingDepth => "depth",
        LimitKind::FileBytes | LimitKind::JsonNesting => "unused",
    };
    E::custom(format!(
        "bw1{MARK}limit{MARK}{tag}{MARK}{found}{MARK}{limit}"
    ))
}

fn fail_shape<E: de::Error>() -> E {
    E::custom(format!("bw1{MARK}doc{MARK}shape"))
}

fn fail_bom<E: de::Error>() -> E {
    E::custom(format!("bw1{MARK}doc{MARK}bom"))
}

fn forward_error<E: de::Error>(error: &serde_json::Error) -> E {
    let message = message_of(error);
    if message.starts_with("bw1\u{1f}") {
        E::custom(message)
    } else {
        E::custom(format!(
            "bw1{MARK}json{MARK}{}{MARK}{}",
            error.line(),
            error.column()
        ))
    }
}

fn classify(error: &serde_json::Error) -> Error {
    let message = message_of(error);
    decode_mark(&message).unwrap_or_else(|| {
        DocumentError::Json {
            line: error.line(),
            column: error.column(),
        }
        .into()
    })
}

fn message_of(error: &serde_json::Error) -> String {
    let text = error.to_string();
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    text.strip_suffix(&suffix).unwrap_or(&text).to_owned()
}

fn decode_mark(message: &str) -> Option<Error> {
    let rest = message.strip_prefix("bw1\u{1f}")?;
    let (kind, rest) = rest.split_once('\u{1f}')?;
    match kind {
        "limit" => decode_limit(rest),
        "doc" => match rest {
            "shape" => Some(DocumentError::Shape.into()),
            "bom" => Some(DocumentError::BomFormat.into()),
            _ => None,
        },
        "json" => {
            let (line, column) = rest.split_once('\u{1f}')?;
            Some(
                DocumentError::Json {
                    line: line.parse().ok()?,
                    column: column.parse().ok()?,
                }
                .into(),
            )
        }
        _ => None,
    }
}

fn decode_limit(rest: &str) -> Option<Error> {
    let mut parts = rest.split('\u{1f}');
    let tag = parts.next()?;
    let found = parts.next()?.parse().ok()?;
    let limit = parts.next()?.parse().ok()?;
    let kind = match tag {
        "components" => LimitKind::ComponentCount,
        "depth" => LimitKind::NestingDepth,
        _ => return None,
    };
    Some(LimitExceeded { kind, found, limit }.into())
}
