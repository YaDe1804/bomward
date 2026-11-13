//! Deterministic smoke fuzz for the decoder and the policy parser.
//!
//! The seed is fixed, so a failure replays the same case. Each loop is
//! bounded so the suite stays fast on stable, including Windows.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bomward_core::diff::diff;
use bomward_core::policy::{self, PolicyLimits};
use bomward_core::report::{Format, render};
use bomward_core::{Document, Limits, read_bytes};

/// Fixed seed. A failing case is named by this value and its index.
const SEED: u64 = 0xB01D_0007;

/// Cases per loop. The same count covers documents, raw bytes, and policies.
const ROUNDS: u32 = 2_000;

const HOSTILE: &[&str] = &[
    "",
    "\0",
    "\u{202E}epoN",
    "\u{200B}",
    "\u{FEFF}",
    "name\u{0007}",
    "\"quoted\"",
    "\\",
    "pkg:generic/%0a@1",
    "<img src=x>",
];

struct Lcg {
    state: u64,
}

impl Lcg {
    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        u32::try_from(self.state >> 32).unwrap_or(0)
    }

    fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }

    fn byte(&mut self) -> u8 {
        u8::try_from(self.next_u32() % 256).unwrap_or(0)
    }
}

fn empty_document() -> Document {
    Document {
        components: BTreeMap::default(),
        dependencies: BTreeMap::default(),
        warnings: Vec::new(),
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write_hex(&mut out, u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn write_hex(out: &mut String, code: u32) -> Result<(), std::fmt::Error> {
    use std::fmt::Write as _;
    write!(out, "\\u{code:04x}")
}

fn hostile<'a>(rng: &mut Lcg) -> &'a str {
    HOSTILE
        .get(usize::try_from(rng.below(u32::try_from(HOSTILE.len()).unwrap_or(1))).unwrap_or(0))
        .copied()
        .unwrap_or("")
}

fn long_text(rng: &mut Lcg) -> String {
    let length = 64 + rng.below(2_048);
    let mut text = String::new();
    text.reserve(usize::try_from(length).unwrap_or(0));
    let piece = hostile(rng);
    while u32::try_from(text.len()).unwrap_or(u32::MAX) < length {
        text.push_str(piece);
        text.push('\u{202E}');
    }
    text
}

#[test]
fn structured_documents_never_panic() -> Result<(), String> {
    let mut rng = Lcg { state: SEED };
    let empty = empty_document();
    let limits = Limits::default();
    for index in 0..ROUNDS {
        let bytes = structured_document(&mut rng, index);
        exercise(&bytes, &limits, &empty)
            .map_err(|error| format!("seed {SEED} case {index}: {error}"))?;
    }
    Ok(())
}

#[test]
fn random_bytes_never_panic() -> Result<(), String> {
    let mut rng = Lcg {
        state: SEED ^ 0xA5A5_A5A5_A5A5_A5A5,
    };
    let empty = empty_document();
    let limits = Limits::default();
    for index in 0..ROUNDS {
        let length = rng.below(1_024);
        let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
        for _ in 0..length {
            bytes.push(rng.byte());
        }
        exercise(&bytes, &limits, &empty)
            .map_err(|error| format!("seed {SEED} bytes {index}: {error}"))?;
    }
    Ok(())
}

#[test]
fn policy_inputs_never_panic() {
    let mut rng = Lcg {
        state: SEED ^ 0x0F0F_0F0F_0F0F_0F0F,
    };
    let limits = PolicyLimits::default();
    for index in 0..ROUNDS {
        let bytes = policy_input(&mut rng, index);
        let _ = policy::read_bytes(&bytes, &limits);
    }
}

fn exercise(bytes: &[u8], limits: &Limits, empty: &Document) -> Result<(), String> {
    let Ok(document) = read_bytes(bytes, limits) else {
        return Ok(());
    };
    let difference = diff(empty, &document);
    for format in [Format::Terminal, Format::Markdown, Format::Json] {
        let report = render(&difference, None, format);
        if !report.ends_with('\n') || report.contains('\r') {
            return Err("report was not a single LF text".to_owned());
        }
    }
    Ok(())
}

fn structured_document(rng: &mut Lcg, index: u32) -> Vec<u8> {
    let body = match index % 8 {
        0 => {
            let name = hostile(rng).to_owned();
            component_object(rng, &name, 0)
        }
        1 => {
            let name = long_text(rng);
            component_object(rng, &name, 0)
        }
        2 => wrong_types(rng),
        3 => boundary_numbers(),
        4 => {
            let depth = if rng.below(5) == 0 {
                40
            } else {
                1 + rng.below(8)
            };
            nested_components(rng, depth)
        }
        5 => cycle_document(rng),
        6 => self_dependency(rng),
        _ => empty_fields(rng),
    };
    body.into_bytes()
}

fn component_object(rng: &mut Lcg, name: &str, depth: u32) -> String {
    let version = hostile(rng);
    let purl_type = hostile(rng);
    let purl_version = hostile(rng);
    let purl = format!("pkg:generic/{purl_type}@{purl_version}");
    let ty = json_string(hostile(rng));
    let name_json = json_string(name);
    let version_json = json_string(version);
    let purl_json = json_string(&purl);
    let reference = json_string(&format!("r{depth}"));
    let scope = json_string(hostile(rng));
    let license = json_string(hostile(rng));
    let algorithm = json_string(hostile(rng));
    let content = json_string(hostile(rng));
    let mut object = format!(
        r#"{{"type":{ty},"name":{name_json},"version":{version_json},"purl":{purl_json},"bom-ref":{reference},"scope":{scope},"licenses":[{{"license":{{"id":{license}}}}}],"hashes":[{{"alg":{algorithm},"content":{content}}}]"#
    );
    if depth < 3 && rng.below(2) == 0 {
        object.push_str(",\"components\":[");
        object.push_str(&component_object(rng, name, depth + 1));
        object.push(']');
    }
    object.push('}');
    if depth == 0 {
        format!(
            r#"{{"bomFormat":"CycloneDX","specVersion":{},"components":[{object}]}}"#,
            json_string(spec_version(rng)),
        )
    } else {
        object
    }
}

fn spec_version(rng: &mut Lcg) -> &'static str {
    ["1.4", "1.5", "1.6", "1.7", "1.3", ""]
        .get(usize::try_from(rng.below(6)).unwrap_or(0))
        .copied()
        .unwrap_or("1.6")
}

fn wrong_types(rng: &mut Lcg) -> String {
    format!(
        r#"{{"bomFormat":{bom},"specVersion":true,"metadata":[],"components":{{"name":1}},"dependencies":"no","extra":null}}"#,
        bom = rng.below(3),
    )
}

fn boundary_numbers() -> String {
    r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"n","version":18446744073709551615,"hashes":[{"alg":-9223372036854775808,"content":1e309}],"licenses":[9223372036854775807]}]}"#.to_owned()
}

fn nested_components(rng: &mut Lcg, depth: u32) -> String {
    let mut body = String::new();
    for level in (0..depth).rev() {
        let name = json_string(&format!("n{level}-{}", hostile(rng)));
        body = if body.is_empty() {
            format!(r#"{{"type":"library","name":{name},"version":"1"}}"#)
        } else {
            format!(r#"{{"type":"library","name":{name},"version":"1","components":[{body}]}}"#)
        };
    }
    format!(r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{body}]}}"#)
}

fn cycle_document(rng: &mut Lcg) -> String {
    let left = json_string(hostile(rng));
    let right = json_string(hostile(rng));
    format!(
        r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{{"component":{{"bom-ref":"app","name":"app","version":"1","purl":"pkg:generic/app@1"}}}},"components":[{{"bom-ref":"a","name":{left},"version":"1","purl":"pkg:generic/a@1"}},{{"bom-ref":"b","name":{right},"version":"1","purl":"pkg:generic/b@1"}}],"dependencies":[{{"ref":"app","dependsOn":["a"]}},{{"ref":"a","dependsOn":["b"]}},{{"ref":"b","dependsOn":["a"]}}]}}"#
    )
}

fn self_dependency(rng: &mut Lcg) -> String {
    let name = json_string(hostile(rng));
    format!(
        r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{{"component":{{"bom-ref":"app","name":"app","version":"1"}}}},"components":[{{"bom-ref":"a","name":{name},"version":"1"}}],"dependencies":[{{"ref":"app","dependsOn":["a"]}},{{"ref":"a","dependsOn":["a"]}}]}}"#
    )
}

fn empty_fields(rng: &mut Lcg) -> String {
    format!(
        r#"{{"bomFormat":"","specVersion":"","components":[{{"name":"","version":"","purl":"","group":"","hashes":[{{"alg":"","content":""}}]}}],"dependencies":[{{"ref":"","dependsOn":[""]}}],"pad":{}}}"#,
        json_string(&long_text(rng)),
    )
}

fn policy_input(rng: &mut Lcg, index: u32) -> Vec<u8> {
    match index % 6 {
        0 => {
            let length = rng.below(512);
            let mut bytes = Vec::new();
            for _ in 0..length {
                bytes.push(rng.byte());
            }
            bytes
        }
        1 => format!(
            "version: 1\nmode: enforce\ndefault: allow\nrules: []\ndescription: {}\n",
            hostile(rng)
        )
        .into_bytes(),
        2 => b"version: [1]\nmode: {broken\nrules: [\n".to_vec(),
        3 => format!(
            "version: 1\nversion: 2\nmode: \0\nrules:\n  - id: r{index}\n    id: r{index}\n"
        )
        .into_bytes(),
        4 => {
            let mut text = String::from("version: 1\nmode: enforce\ndefault: deny\nrules:\n");
            let entries = 8 + rng.below(16);
            for rule in 0..entries {
                let description = hostile(rng);
                let _ = write!(
                    text,
                    "  - id: r{rule}\n    description: {description}\n    when:\n      change: [added]\n    decision: deny\n"
                );
            }
            text.into_bytes()
        }
        _ => format!(
            "version: {}\nmode: {}\ndefault: {}\nrules: {}\n",
            rng.below(4),
            hostile(rng),
            hostile(rng),
            rng.below(3),
        )
        .into_bytes(),
    }
}
