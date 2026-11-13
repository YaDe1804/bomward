//! CLI contract tests for the foundation binary.
//!
//! These tests spawn `bomward`. They do not read fixture SBOMs.

use std::process::Command;

fn bomward() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bomward"))
}

fn run(args: &[&str]) -> Result<std::process::Output, std::io::Error> {
    bomward().args(args).output()
}

#[test]
fn version_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let output = run(&["--version"])?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("bomward"), "{stdout}");
    assert!(stdout.contains("0.1.0"), "{stdout}");
    Ok(())
}

#[test]
fn help_shows_diff_command_and_options() -> Result<(), Box<dyn std::error::Error>> {
    let output = run(&["--help"])?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("diff"), "{stdout}");
    assert!(stdout.contains("--policy"), "{stdout}");
    assert!(stdout.contains("--format"), "{stdout}");
    assert!(stdout.contains("terminal"), "{stdout}");
    assert!(stdout.contains("markdown"), "{stdout}");
    assert!(stdout.contains("json"), "{stdout}");
    assert!(stdout.contains("--max-file-bytes"), "{stdout}");
    assert!(stdout.contains("--max-components"), "{stdout}");
    assert!(stdout.contains("--max-depth"), "{stdout}");
    assert!(stdout.contains("--output"), "{stdout}");
    assert!(stdout.contains("No policy was applied."), "{stdout}");
    Ok(())
}

#[test]
fn diff_help_shows_planned_options() -> Result<(), Box<dyn std::error::Error>> {
    let output = run(&["diff", "--help"])?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("--policy"), "{stdout}");
    assert!(stdout.contains("--format"), "{stdout}");
    assert!(stdout.contains("<OLD>"), "{stdout}");
    assert!(stdout.contains("<NEW>"), "{stdout}");
    assert!(stdout.contains("--max-file-bytes"), "{stdout}");
    assert!(stdout.contains("--max-components"), "{stdout}");
    assert!(stdout.contains("--max-depth"), "{stdout}");
    assert!(stdout.contains("--output"), "{stdout}");
    assert!(stdout.contains("No policy was applied."), "{stdout}");
    Ok(())
}

#[test]
fn diff_without_arguments_exits_2() -> Result<(), Box<dyn std::error::Error>> {
    let output = run(&["diff"])?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

#[test]
fn a_missing_old_file_names_old_and_exits_2() -> Result<(), Box<dyn std::error::Error>> {
    let path = "old.json";
    let output = run(&["diff", path, "new.json"])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "{stderr}");
    assert!(
        stderr.starts_with("error: failed to read OLD old.json:"),
        "{stderr}"
    );
    assert!(stderr.contains("os error 2"), "{stderr}");
    Ok(())
}
