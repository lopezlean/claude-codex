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

pub fn spawn_claude(
    binary: &str,
    port: u16,
    extra_args: &[OsString],
    backend_model: &str,
) -> Result<Child, AppError> {
    let base_url = format!("http://127.0.0.1:{port}");
    let claude_path = resolve_claude_binary(binary)?;
    let child_args = build_claude_args(backend_model, extra_args);

    Command::new(claude_path)
        .args(&child_args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "")
        .env("ANTHROPIC_AUTH_TOKEN", "claude-codex-proxy")
        .env("CLAUDE_CODE_ATTRIBUTION_HEADER", "0")
        .env("ANTHROPIC_DEFAULT_OPUS_MODEL", &backend_model)
        .env("ANTHROPIC_DEFAULT_SONNET_MODEL", &backend_model)
        .env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &backend_model)
        .env("CLAUDE_CODE_SUBAGENT_MODEL", &backend_model)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::MissingClaudeBinary,
            _ => AppError::Proxy(error.to_string()),
        })
}

pub fn split_wrapper_args(args: &[OsString]) -> (Vec<OsString>, Option<String>, Option<String>) {
    let mut forwarded_args = Vec::new();
    let mut selected_model = None;
    let mut selected_effort = None;
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

        if let Some(effort) = raw.strip_prefix("--effort=") {
            selected_effort = Some(effort.to_string());
            index += 1;
            continue;
        }

        if raw == "--effort" {
            selected_effort = args
                .get(index + 1)
                .map(|value| value.to_string_lossy().into_owned());
            index += usize::from(args.get(index + 1).is_some()) + 1;
            continue;
        }

        forwarded_args.push(args[index].clone());
        index += 1;
    }

    (forwarded_args, selected_model, selected_effort)
}

fn build_claude_args(model: &str, extra_args: &[OsString]) -> Vec<OsString> {
    let mut args = Vec::with_capacity(extra_args.len() + 2);
    if !model.is_empty() {
        args.push("--model".into());
        args.push(model.into());
    }
    args.extend(extra_args.iter().cloned());
    args
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::split_wrapper_args;

    #[test]
    fn split_wrapper_args_extracts_model_and_effort_flags() {
        let args = vec![
            OsString::from("--model"),
            OsString::from("gpt-5.4-mini"),
            OsString::from("--effort"),
            OsString::from("low"),
            OsString::from("--print"),
            OsString::from("hello"),
        ];

        let (forwarded, model, effort) = split_wrapper_args(&args);

        assert_eq!(
            forwarded,
            vec![OsString::from("--print"), OsString::from("hello")]
        );
        assert_eq!(model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(effort.as_deref(), Some("low"));
    }

    #[test]
    fn split_wrapper_args_extracts_inline_effort_flag() {
        let args = vec![
            OsString::from("--effort=high"),
            OsString::from("--print"),
            OsString::from("hello"),
        ];

        let (forwarded, model, effort) = split_wrapper_args(&args);

        assert_eq!(
            forwarded,
            vec![OsString::from("--print"), OsString::from("hello")]
        );
        assert_eq!(model, None);
        assert_eq!(effort.as_deref(), Some("high"));
    }
}
