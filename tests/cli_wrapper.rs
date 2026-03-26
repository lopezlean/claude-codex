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
                "access_token": "access-token"
            },
            "last_refresh": "123"
        })
        .to_string(),
    )
    .expect("auth file");

    let script = format!(
        "#!/bin/sh\nprintf 'BASE=%s\\nKEY=%s\\nARGS=%s\\n' \"$ANTHROPIC_BASE_URL\" \"$ANTHROPIC_API_KEY\" \"$*\" > \"{}\"\n",
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
        .arg("--print")
        .arg("hello")
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert!(captured.contains("BASE=http://127.0.0.1:"));
    assert!(captured.contains("KEY=sk-ant-codex-proxy"));
    assert!(captured.contains("ARGS=--print hello"));
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
                "access_token": "access-token"
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
