mod auth;
mod backend;
mod cli;
mod config;
mod error;
mod handlers;
mod process;
mod protocol;
mod server;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use crate::auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use crate::auth::provider::AuthProvider;
use crate::auth::session_store::FileSessionStore;
use crate::backend::openai::{OpenAiBackendConfig, OpenAiBackendProvider};
use crate::cli::{AuthCommand, ParsedCli};
use crate::config::AppConfig;
use crate::error::AppError;
use crate::process::{reserve_local_port, run_claude};
use crate::server::{serve, AppState};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), AppError> {
    let cli = cli::parse(std::env::args_os())?;
    let config = AppConfig::from_env()?;
    let store = FileSessionStore::new(config.auth_file.clone());
    let auth = Arc::new(OpenAiAuthProvider::new(
        OpenAiAuthConfig {
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_string(),
            auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            redirect_port: config.callback_port,
            callback_timeout_secs: 120,
            refresh_grace_period_secs: 60,
        },
        store,
    ));
    let backend = Arc::new(OpenAiBackendProvider::new(OpenAiBackendConfig {
        base_url: config.upstream_base_url.clone(),
        chat_completions_path: "/v1/chat/completions".to_string(),
    }));

    match cli {
        ParsedCli::Run { claude_args } => {
            auth.ensure_access_token().await?;
            let port = reserve_local_port()?;
            let state = AppState { auth, backend };
            let server_task: JoinHandle<anyhow::Result<()>> = tokio::spawn(serve(state, port));
            let run_result = run_claude(&config.claude_binary, port, &claude_args).await;
            server_task.abort();
            run_result?;
            Ok(())
        }
        ParsedCli::Auth { command } => {
            match command {
                AuthCommand::Login => auth.login().await?,
                AuthCommand::Status => {
                    let status = auth.status().await?;
                    println!(
                        "provider=openai connected={} has_refresh_token={} auth_path={}",
                        status.connected,
                        status.has_refresh_token,
                        status.auth_path.display()
                    );
                }
                AuthCommand::Logout => auth.logout().await?,
            }
            Ok(())
        }
        ParsedCli::ProxyServe => {
            let port = reserve_local_port()?;
            let state = AppState { auth, backend };
            serve(state, port).await?;
            Ok(())
        }
    }
}
