use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[test]
fn dry_run_prints_markdown_and_does_not_write_outputs() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("server.js"),
        r#"
// listUsers returns all visible users.
app.get("/users", function listUsers(req, res) { res.json([]) })
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("agent-forge").unwrap();
    cmd.arg(dir.path()).arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(contains("## Core Purpose"))
        .stdout(contains("listUsers"));

    assert!(!dir.path().join("SKILL.md").exists());
    assert!(!dir.path().join("mcp-config.json").exists());
}

#[test]
fn help_exposes_interactive_and_serve_modes() {
    let mut cmd = Command::cargo_bin("agent-forge").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(contains("--interactive"))
        .stdout(contains("serve"));
}
