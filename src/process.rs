use std::ffi::OsString;
use std::net::TcpListener;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;

pub fn reserve_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub async fn run_claude(binary: &str, port: u16, args: &[OsString]) -> Result<()> {
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let status = Command::new(binary)
        .args(args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "sk-ant-codex-proxy")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("claude exited with status {status}");
    }
    Ok(())
}
