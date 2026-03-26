use std::ffi::OsString;
use std::io::ErrorKind;
use std::net::TcpListener;
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
    Command::new(binary)
        .args(args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_AUTH_TOKEN", "claude-codex-proxy")
        .env_remove("ANTHROPIC_API_KEY")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::MissingClaudeBinary,
            _ => AppError::Proxy(error.to_string()),
        })
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
