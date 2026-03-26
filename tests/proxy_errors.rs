use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn run_mode_reports_missing_claude_binary_cleanly() {
    let dir = tempdir().expect("temp dir");
    let home_dir = dir.path().join("home");
    let path_dir = dir.path().join("empty-path");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
    fs::create_dir_all(&path_dir).expect("path dir");
    fs::write(
        home_dir.join(".codex").join("auth.json"),
        json!({
            "auth_mode": "openai",
            "tokens": {
                "access_token": "access-token"
            },
            "last_refresh": "123"
        })
        .to_string(),
    )
    .expect("auth file");

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env("PATH", &path_dir)
        .arg("--print")
        .arg("hello")
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not find `claude` in PATH"));
}
