// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end runs of `bomward diff`.

use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bomward() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bomward"))
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn scratch(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("bomward-{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn output_of(args: &[String]) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(bomward().args(refs).output()?)
}

#[test]
fn diff_help_matches_the_golden() -> Result<(), Box<dyn std::error::Error>> {
    let output = bomward().args(["diff", "--help"]).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(output.status.success(), "{stdout}");
    assert!(!stdout.contains('\r'));
    let path = repo("tests/fixtures/diff-help.txt");
    if std::env::var_os("BOMWARD_UPDATE_GOLDENS").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &stdout)?;
        return Ok(());
    }
    let expected = fs::read_to_string(&path)?;
    assert_eq!(stdout, expected);
    Ok(())
}

#[test]
fn every_golden_combination_matches_stdout_and_exit_code() -> Result<(), Box<dyn std::error::Error>>
{
    let cases = [
        (
            "lockward-recommended",
            Some("recommended"),
            "lockward-old.cdx.json",
        ),
        ("lockward-strict", Some("strict"), "lockward-old.cdx.json"),
        (
            "lockward-licenses",
            Some("licenses"),
            "lockward-old.cdx.json",
        ),
        ("lockward-report-only", None, "lockward-old.cdx.json"),
        (
            "lockward-old-format-recommended",
            Some("recommended"),
            "lockward-old-format.cdx.json",
        ),
        (
            "lockward-old-format-strict",
            Some("strict"),
            "lockward-old-format.cdx.json",
        ),
        (
            "lockward-old-format-licenses",
            Some("licenses"),
            "lockward-old-format.cdx.json",
        ),
        (
            "lockward-old-format-report-only",
            None,
            "lockward-old-format.cdx.json",
        ),
    ];
    for (name, policy, old) in cases {
        for extension in ["txt", "md", "json"] {
            let format = match extension {
                "txt" => "terminal",
                "md" => "markdown",
                _ => "json",
            };
            check_case(name, policy, old, format, extension)?;
        }
    }
    Ok(())
}

fn check_case(
    name: &str,
    policy: Option<&str>,
    old: &str,
    format: &str,
    extension: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let old_path = repo(&format!("../bomward-core/tests/fixtures/{old}"));
    let new_path = repo("../bomward-core/tests/fixtures/lockward-new.cdx.json");
    let mut args = vec![
        "diff".to_owned(),
        display(&old_path),
        display(&new_path),
        "--format".to_owned(),
        format.to_owned(),
    ];
    if let Some(policy) = policy {
        args.push("--policy".to_owned());
        args.push(display(&repo(&format!(
            "../examples/policies/{policy}.yaml"
        ))));
    }
    let output = output_of(&args)?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    let expected = fs::read_to_string(repo(&format!("../examples/reports/{name}.{extension}")))?;
    let code = exit_code_of(&fs::read_to_string(repo(&format!(
        "../examples/reports/{name}.json"
    )))?)?;
    assert_eq!(
        output.status.code(),
        Some(code),
        "{name} {format}: {stderr}"
    );
    assert_eq!(stdout, expected, "{name} {format}");
    assert!(!stdout.contains('\r'), "{name}");
    assert!(stderr.is_empty(), "{stderr}");
    Ok(())
}

#[test]
fn output_file_matches_stdout_and_a_second_run() -> Result<(), Box<dyn std::error::Error>> {
    let dir = scratch("output")?;
    let destination = dir.join("report.md");
    let old = repo("../bomward-core/tests/fixtures/lockward-old.cdx.json");
    let new = repo("../bomward-core/tests/fixtures/lockward-new.cdx.json");
    let policy = repo("../examples/policies/strict.yaml");
    let args = [
        "diff",
        &display(&old),
        &display(&new),
        "--policy",
        &display(&policy),
        "--format",
        "markdown",
        "--output",
        &display(&destination),
    ];
    let first = bomward().args(args).output()?;
    let stderr = String::from_utf8(first.stderr)?;
    assert_eq!(first.status.code(), Some(1), "{stderr}");
    assert!(first.stdout.is_empty(), "{stderr}");
    let written = fs::read_to_string(&destination)?;
    let expected = fs::read_to_string(repo("../examples/reports/lockward-strict.md"))?;
    assert_eq!(written, expected);
    let second = bomward().args(args).output()?;
    assert_eq!(second.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&destination)?, written);
    Ok(())
}

#[test]
fn output_refuses_to_replace_an_input() -> Result<(), Box<dyn std::error::Error>> {
    let dir = scratch("output-guard")?;
    let bytes = fs::read(repo("../bomward-core/tests/fixtures/minimal-1.6.json"))?;
    fs::write(dir.join("old.json"), &bytes)?;
    fs::write(dir.join("new.json"), &bytes)?;
    for (target, which) in [
        ("old.json", "OLD"),
        ("new.json", "NEW"),
        ("./old.json", "OLD"),
    ] {
        let output = bomward()
            .current_dir(&dir)
            .args(["diff", "old.json", "new.json", "--output", target])
            .output()?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(output.status.code(), Some(2), "{target}: {stderr}");
        assert!(output.stdout.is_empty(), "{stderr}");
        assert!(
            stderr.starts_with(&format!(
                "error: refusing to write the report over {which} {target}"
            )),
            "{stderr}"
        );
    }
    assert_eq!(fs::read(dir.join("old.json"))?, bytes);
    assert_eq!(fs::read(dir.join("new.json"))?, bytes);
    Ok(())
}

#[test]
fn error_classes_exit_2_without_a_report() -> Result<(), Box<dyn std::error::Error>> {
    let dir = scratch("errors")?;
    let missing = dir.join("missing.json");
    let missing_text = display(&missing);
    let output = bomward()
        .args(["diff", &missing_text, &missing_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(
        stderr.starts_with(&format!("error: failed to read OLD {missing_text}:")),
        "{stderr}"
    );

    let directory = display(&dir);
    let output = bomward().args(["diff", &directory, &directory]).output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.starts_with(&format!("error: failed to read OLD {directory}:")),
        "{stderr}"
    );

    let bad_json = dir.join("bad.json");
    fs::write(&bad_json, b"{")?;
    let bad_json_text = display(&bad_json);
    let output = bomward()
        .args(["diff", &bad_json_text, &bad_json_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr.contains(&format!("OLD {bad_json_text}: invalid JSON at line")),
        "{stderr}"
    );

    let wrong = dir.join("wrong.json");
    fs::write(&wrong, br#"{"bomFormat":"SPDX","specVersion":"1.6"}"#)?;
    let wrong_text = display(&wrong);
    let output = bomward()
        .args(["diff", &wrong_text, &wrong_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains(&format!("OLD {wrong_text}: bomFormat must be CycloneDX")),
        "{stderr}"
    );

    let future = dir.join("future.json");
    fs::write(&future, br#"{"bomFormat":"CycloneDX","specVersion":"9.9"}"#)?;
    let future_text = display(&future);
    let output = bomward()
        .args(["diff", &future_text, &future_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("unsupported specVersion 9.9; supported versions: 1.4, 1.5, 1.6, 1.7"),
        "{stderr}"
    );

    let minimal = repo("../bomward-core/tests/fixtures/minimal-1.6.json");
    let minimal_text = display(&minimal);
    let output = bomward()
        .args([
            "diff",
            &minimal_text,
            &minimal_text,
            "--max-file-bytes",
            "1",
        ])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("file size") && stderr.contains("--max-file-bytes"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("OLD {minimal_text}:")), "{stderr}");
    limit_policy_and_write(&dir, &minimal_text)
}

fn limit_policy_and_write(
    dir: &Path,
    minimal_text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = bomward()
        .args(["diff", minimal_text, minimal_text, "--max-components", "0"])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("component count") && stderr.contains("--max-components"),
        "{stderr}"
    );

    let output = bomward()
        .args(["diff", minimal_text, minimal_text, "--max-depth", "0"])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("nesting depth") && stderr.contains("--max-depth"),
        "{stderr}"
    );

    let output = bomward()
        .args([
            "diff",
            "does-not-exist-old",
            "does-not-exist-new",
            "--max-depth",
            "64",
        ])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr.trim(),
        "error: --max-depth 64 exceeds supported maximum 63"
    );
    assert!(!stderr.contains("does-not-exist"));

    let policy = dir.join("bad-policy.yaml");
    fs::write(
        &policy,
        b"version: 2\nmode: enforce\ndefault: allow\nrules: []\n",
    )?;
    let policy_text = display(&policy);
    let output = bomward()
        .args(["diff", minimal_text, minimal_text, "--policy", &policy_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!("policy {policy_text}:")) && stderr.contains("expected 1"),
        "{stderr}"
    );

    let nested = dir.join("missing-parent").join("out.txt");
    let nested_text = display(&nested);
    let output = bomward()
        .args(["diff", minimal_text, minimal_text, "--output", &nested_text])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.starts_with(&format!("error: failed to write {nested_text}:")),
        "{stderr}"
    );
    assert!(!nested.exists());
    assert_eq!(output.stdout, b"");
    Ok(())
}

#[test]
fn a_bad_policy_is_reported_before_the_documents() -> Result<(), Box<dyn std::error::Error>> {
    let output = bomward()
        .args([
            "diff",
            "missing-old.json",
            "missing-new.json",
            "--policy",
            "missing-policy.yaml",
        ])
        .output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.starts_with("error: failed to read policy missing-policy.yaml:"),
        "{stderr}"
    );
    assert!(!stderr.contains("missing-old.json"), "{stderr}");
    assert_eq!(output.stdout, b"");
    Ok(())
}

#[test]
fn a_closed_stdout_pipe_keeps_a_deny() -> Result<(), Box<dyn std::error::Error>> {
    let dir = scratch("pipe-deny")?;
    let count = 12_000;
    let old = dir.join("old.json");
    let new = dir.join("new.json");
    let policy = dir.join("deny.yaml");
    fs::write(&old, document(0))?;
    fs::write(&new, document(count))?;
    fs::write(
        &policy,
        b"version: 1\nmode: enforce\ndefault: deny\nrules: []\n",
    )?;
    let mut child = bomward()
        .args([
            "diff",
            &display(&old),
            &display(&new),
            "--policy",
            &display(&policy),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("stdout was not piped")?;
    let mut byte = [0u8; 1];
    loop {
        let read = stdout.read(&mut byte)?;
        if read == 0 || byte[0] == b'\n' {
            break;
        }
    }
    drop(stdout);
    let status = child.wait()?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    fs::remove_dir_all(&dir)?;
    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    Ok(())
}

fn document(components: usize) -> String {
    let mut body = String::from(
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","metadata":{"component":{"type":"application","name":"app","version":"1"}},"components":["#,
    );
    for index in 0..components {
        if index > 0 {
            body.push(',');
        }
        let _ = write!(
            body,
            r#"{{"type":"library","name":"pkg{index}","version":"1.0.0"}}"#
        );
    }
    body.push_str("]}");
    body
}

#[test]
fn a_closed_stdout_pipe_exits_0_without_a_message() -> Result<(), Box<dyn std::error::Error>> {
    let (read, write) = std::io::pipe()?;
    drop(read);
    let old = repo("../bomward-core/tests/fixtures/lockward-old.cdx.json");
    let new = repo("../bomward-core/tests/fixtures/lockward-new.cdx.json");
    let mut child = bomward()
        .args(["diff", &display(&old), &display(&new)])
        .stdout(Stdio::from(write))
        .stderr(Stdio::piped())
        .spawn()?;
    let status = child.wait()?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    assert_eq!(status.code(), Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    Ok(())
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn exit_code_of(json: &str) -> Result<i32, Box<dyn std::error::Error>> {
    let key = "\"exit_code\": ";
    let start = json.find(key).ok_or("exit_code is missing")?;
    let rest = json
        .get(start + key.len()..)
        .ok_or("exit_code has no value")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Ok(digits.parse()?)
}
