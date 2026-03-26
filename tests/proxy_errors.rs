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
                "access_token": "ey.test.token"
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

#[cfg(unix)]
#[test]
fn run_mode_falls_back_to_local_claude_install_when_path_is_missing() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("temp dir");
    let home_dir = dir.path().join("home");
    let path_dir = dir.path().join("empty-path");
    let capture_path = dir.path().join("capture.txt");
    let fallback_dir = home_dir.join(".claude").join("local");

    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
    fs::create_dir_all(&path_dir).expect("path dir");
    fs::create_dir_all(&fallback_dir).expect("fallback dir");
    fs::write(
        home_dir.join(".codex").join("auth.json"),
        json!({
            "auth_mode": "openai",
            "tokens": {
                "access_token": "ey.test.token"
            },
            "last_refresh": "123"
        })
        .to_string(),
    )
    .expect("auth file");

    let claude_path = fallback_dir.join("claude");
    fs::write(
        &claude_path,
        format!(
            "#!/bin/sh\nprintf 'fallback-ok' > \"{}\"\n",
            capture_path.display()
        ),
    )
    .expect("fallback script");
    let mut perms = fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&claude_path, perms).unwrap();

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env("PATH", &path_dir)
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert_eq!(captured, "fallback-ok");
}
