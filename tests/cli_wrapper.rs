use std::fs;

use assert_cmd::Command;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn run_mode_launches_claude_with_proxy_environment() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    let capture_path = dir.path().join("capture.txt");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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

    let script = format!(
        "#!/bin/sh\nprintf 'BASE=%s\\nTOKEN=%s\\nKEY=%s\\nATTR=%s\\nARGS=%s\\n' \"$ANTHROPIC_BASE_URL\" \"$ANTHROPIC_AUTH_TOKEN\" \"$ANTHROPIC_API_KEY\" \"$CLAUDE_CODE_ATTRIBUTION_HEADER\" \"$*\" > \"{}\"\n",
        capture_path.display()
    );
    let claude_path = bin_dir.join("claude");
    fs::write(&claude_path, script).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&claude_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&claude_path, perms).unwrap();
    }

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .env("ANTHROPIC_API_KEY", "should-be-removed")
        .arg("--print")
        .arg("hello")
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert!(captured.contains("BASE=http://127.0.0.1:"));
    assert!(!captured.contains("/v1"));
    assert!(captured.contains("TOKEN=claude-codex-proxy"));
    assert!(captured.contains("KEY="));
    assert!(captured.contains("ATTR=0"));
    assert!(captured.contains("ARGS=--model gpt-5.4 --print hello"));
}

#[test]
fn run_mode_uses_selected_backend_model_for_args_and_model_tiers() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    let capture_path = dir.path().join("capture.txt");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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

    let script = format!(
        "#!/bin/sh\nprintf 'OPUS=%s\\nSONNET=%s\\nHAIKU=%s\\nSUBAGENT=%s\\nARGS=%s\\n' \"$ANTHROPIC_DEFAULT_OPUS_MODEL\" \"$ANTHROPIC_DEFAULT_SONNET_MODEL\" \"$ANTHROPIC_DEFAULT_HAIKU_MODEL\" \"$CLAUDE_CODE_SUBAGENT_MODEL\" \"$*\" > \"{}\"\n",
        capture_path.display()
    );
    let claude_path = bin_dir.join("claude");
    fs::write(&claude_path, script).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&claude_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&claude_path, perms).unwrap();
    }

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .arg("--model")
        .arg("gpt-5.4-mini")
        .arg("--print")
        .arg("hello")
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert!(captured.contains("OPUS=gpt-5.4-mini"));
    assert!(captured.contains("SONNET=gpt-5.4-mini"));
    assert!(captured.contains("HAIKU=gpt-5.4-mini"));
    assert!(captured.contains("SUBAGENT=gpt-5.4-mini"));
    assert!(captured.contains("ARGS=--model gpt-5.4-mini --print hello"));
}

#[test]
fn run_mode_defaults_all_model_tiers_to_default_backend_model() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    let capture_path = dir.path().join("capture.txt");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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

    let script = format!(
        "#!/bin/sh\nprintf 'OPUS=%s\\nSONNET=%s\\nHAIKU=%s\\nSUBAGENT=%s\\nARGS=%s\\n' \"$ANTHROPIC_DEFAULT_OPUS_MODEL\" \"$ANTHROPIC_DEFAULT_SONNET_MODEL\" \"$ANTHROPIC_DEFAULT_HAIKU_MODEL\" \"$CLAUDE_CODE_SUBAGENT_MODEL\" \"$*\" > \"{}\"\n",
        capture_path.display()
    );
    let claude_path = bin_dir.join("claude");
    fs::write(&claude_path, script).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&claude_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&claude_path, perms).unwrap();
    }

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert!(captured.contains("OPUS=gpt-5.4"));
    assert!(captured.contains("SONNET=gpt-5.4"));
    assert!(captured.contains("HAIKU=gpt-5.4"));
    assert!(captured.contains("SUBAGENT=gpt-5.4"));
    assert!(captured.contains("ARGS=--model gpt-5.4"));
}

#[cfg(unix)]
#[test]
fn run_mode_stops_the_child_when_the_wrapper_is_interrupted() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command as StdCommand, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    let pid_path = dir.path().join("claude.pid");
    let ready_path = dir.path().join("claude.ready");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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

    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$$\" > \"{}\"\n: > \"{}\"\nwhile true; do sleep 1; done\n",
        pid_path.display(),
        ready_path.display()
    );
    let claude_path = bin_dir.join("claude");
    fs::write(&claude_path, script).expect("script");
    let mut perms = fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&claude_path, perms).unwrap();

    let mut wrapper = StdCommand::new(assert_cmd::cargo::cargo_bin("claude-codex"))
        .env("HOME", &home_dir)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .arg("--print")
        .arg("hello")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn wrapper");

    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready_path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ready_path.exists(), "claude child never became ready");

    let claude_pid: u32 = fs::read_to_string(&pid_path)
        .expect("child pid")
        .trim()
        .parse()
        .expect("pid should parse");

    let signal_status = StdCommand::new("/bin/kill")
        .arg("-INT")
        .arg(wrapper.id().to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("send signal");
    assert!(signal_status.success(), "failed to signal wrapper");

    let status = wrapper.wait().expect("wrapper status");
    assert!(!status.success(), "wrapper unexpectedly succeeded");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let probe = StdCommand::new("/bin/kill")
            .arg("-0")
            .arg(claude_pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("probe child");
        if !probe.success() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "claude child still running after wrapper interrupt"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn run_mode_rejects_models_outside_the_active_backend_catalog() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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
        .env("PATH", &bin_dir)
        .arg("--model")
        .arg("gpt-4o")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "unsupported model 'gpt-4o' for codex backend",
        ));
}

#[test]
fn models_list_prints_the_active_backend_catalog() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
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
        .env("PATH", &bin_dir)
        .arg("models")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("gpt-5.4 (default)"))
        .stdout(predicates::str::contains("gpt-5.4-mini"))
        .stdout(predicates::str::contains("gpt-5.3-codex"));
}
