use std::ffi::OsString;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use tokio::process::{Child, Command};

use crate::error::AppError;

pub fn reserve_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn spawn_claude(binary: &str, port: u16, args: &[OsString]) -> Result<Child, AppError> {
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let claude_path = resolve_claude_binary(binary)?;
    let model = selected_model(args).unwrap_or_default();

    Command::new(claude_path)
        .args(args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "")
        .env("ANTHROPIC_AUTH_TOKEN", "claude-codex-proxy")
        .env("CLAUDE_CODE_ATTRIBUTION_HEADER", "0")
        .env("ANTHROPIC_DEFAULT_OPUS_MODEL", &model)
        .env("ANTHROPIC_DEFAULT_SONNET_MODEL", &model)
        .env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &model)
        .env("CLAUDE_CODE_SUBAGENT_MODEL", &model)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::MissingClaudeBinary,
            _ => AppError::Proxy(error.to_string()),
        })
}

fn selected_model(args: &[OsString]) -> Option<String> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let Some(raw) = arg.to_str() else {
            continue;
        };

        if let Some(model) = raw.strip_prefix("--model=") {
            return Some(model.to_string());
        }

        if raw == "--model" {
            return args
                .next()
                .map(|value| value.to_string_lossy().into_owned());
        }
    }

    None
}

fn resolve_claude_binary(binary: &str) -> Result<OsString, AppError> {
    if has_explicit_path(binary) {
        return Ok(binary.into());
    }

    if let Some(path) = find_in_path(binary) {
        return Ok(path.into_os_string());
    }

    if binary == "claude" {
        if let Some(path) = local_claude_fallback() {
            return Ok(path.into_os_string());
        }
    }

    Err(AppError::MissingClaudeBinary)
}

fn has_explicit_path(binary: &str) -> bool {
    Path::new(binary).components().count() > 1
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for candidate in binary_candidates(binary) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

fn local_claude_fallback() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let candidate = PathBuf::from(home)
        .join(".claude")
        .join("local")
        .join(default_claude_binary_name());
    candidate.is_file().then_some(candidate)
}

fn binary_candidates(binary: &str) -> Vec<String> {
    let candidates = vec![binary.to_string()];
    #[cfg(windows)]
    if !binary.ends_with(".exe") {
        let mut candidates = candidates;
        candidates.push(format!("{binary}.exe"));
        return candidates;
    }
    candidates
}

fn default_claude_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "claude.exe"
    }

    #[cfg(not(windows))]
    {
        "claude"
    }
}

pub async fn wait_for_claude(child: &mut Child) -> Result<(), AppError> {
    let status = child
        .wait()
        .await
        .map_err(|error| AppError::Proxy(error.to_string()))?;
    if !status.success() {
        return Err(AppError::Proxy(format!(
            "claude exited with status {status}"
        )));
    }
    Ok(())
}

pub async fn terminate_claude(child: &mut Child) -> Result<(), AppError> {
    match child.start_kill() {
        Ok(()) => {}
        Err(error) if matches!(error.kind(), ErrorKind::InvalidInput | ErrorKind::NotFound) => {
            return Ok(());
        }
        Err(error) => return Err(AppError::Proxy(error.to_string())),
    }

    let _ = child.wait().await;
    Ok(())
}
