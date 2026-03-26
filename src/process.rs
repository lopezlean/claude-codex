use std::ffi::OsString;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use tokio::process::{Child, Command};

use crate::error::AppError;

const DEFAULT_ACTIVE_MODEL: &str = "sonnet";
const DEFAULT_OPUS_MODEL: &str = "claude-opus-4-1-20250805";
const DEFAULT_SONNET_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_HAIKU_MODEL: &str = "claude-3-5-haiku-20241022";

pub fn reserve_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn spawn_claude(binary: &str, port: u16, args: &[OsString]) -> Result<Child, AppError> {
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let claude_path = resolve_claude_binary(binary)?;
    let (forwarded_args, requested_model) = extract_selected_model(args);
    let active_model =
        normalize_selected_model(requested_model.as_deref().unwrap_or(DEFAULT_ACTIVE_MODEL));
    let subagent_model = resolve_subagent_model(&active_model);

    Command::new(claude_path)
        .args(&forwarded_args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "")
        .env("ANTHROPIC_AUTH_TOKEN", "claude-codex-proxy")
        .env("CLAUDE_CODE_ATTRIBUTION_HEADER", "0")
        .env("ANTHROPIC_MODEL", &active_model)
        .env("ANTHROPIC_DEFAULT_OPUS_MODEL", DEFAULT_OPUS_MODEL)
        .env("ANTHROPIC_DEFAULT_SONNET_MODEL", DEFAULT_SONNET_MODEL)
        .env("ANTHROPIC_DEFAULT_HAIKU_MODEL", DEFAULT_HAIKU_MODEL)
        .env("CLAUDE_CODE_SUBAGENT_MODEL", subagent_model)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::MissingClaudeBinary,
            _ => AppError::Proxy(error.to_string()),
        })
}

fn extract_selected_model(args: &[OsString]) -> (Vec<OsString>, Option<String>) {
    let mut forwarded_args = Vec::new();
    let mut selected_model = None;
    let mut index = 0;

    while index < args.len() {
        let Some(raw) = args[index].to_str() else {
            forwarded_args.push(args[index].clone());
            index += 1;
            continue;
        };

        if let Some(model) = raw.strip_prefix("--model=") {
            selected_model = Some(model.to_string());
            index += 1;
            continue;
        }

        if raw == "--model" || raw == "-m" {
            selected_model = args
                .get(index + 1)
                .map(|value| value.to_string_lossy().into_owned());
            index += usize::from(args.get(index + 1).is_some()) + 1;
            continue;
        }

        forwarded_args.push(args[index].clone());
        index += 1;
    }

    (forwarded_args, selected_model)
}

fn normalize_selected_model(model: &str) -> String {
    let normalized = model.trim().to_ascii_lowercase();

    if normalized.is_empty() || normalized == "default" {
        return DEFAULT_ACTIVE_MODEL.to_string();
    }

    if normalized.contains("haiku") {
        return "haiku".to_string();
    }

    if normalized.contains("opus") {
        return "opus".to_string();
    }

    if normalized.contains("sonnet") {
        return "sonnet".to_string();
    }

    model.to_string()
}

fn resolve_subagent_model(active_model: &str) -> String {
    match active_model {
        "haiku" => DEFAULT_HAIKU_MODEL.to_string(),
        "opus" => DEFAULT_OPUS_MODEL.to_string(),
        "sonnet" => DEFAULT_SONNET_MODEL.to_string(),
        other => other.to_string(),
    }
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
