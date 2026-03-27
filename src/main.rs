mod auth;
mod backend;
mod cli;
mod config;
mod error;
mod handlers;
mod models;
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
use crate::cli::{AuthCommand, ModelsCommand, ParsedCli};
use crate::config::AppConfig;
use crate::error::AppError;
use crate::models::{
    available_models_for, backend_kind_for_token, default_effort, default_model_for,
    resolve_effort, resolve_model,
};
use crate::process::{
    reserve_local_port, spawn_claude, split_wrapper_args, terminate_claude, wait_for_claude,
};
use crate::server::{serve, wait_until_ready, AppState};
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
        codex_responses_url: "https://chatgpt.com/backend-api/codex/responses".to_string(),
    }));

    match cli {
        ParsedCli::Run { claude_args } => {
            let access_token = auth.ensure_access_token().await?;
            let backend_kind = backend_kind_for_token(&access_token)?;
            let (extra_args, requested_model, requested_effort) = split_wrapper_args(&claude_args);
            let backend_model = resolve_model(backend_kind, requested_model.as_deref())?;
            let effort = resolve_effort(backend_kind, requested_effort.as_deref())?;
            let port = reserve_local_port()?;
            let state = AppState {
                auth,
                backend,
                effort,
            };
            let server_task: JoinHandle<anyhow::Result<()>> = tokio::spawn(serve(state, port));
            if let Err(error) = wait_until_ready(port).await {
                server_task.abort();
                return Err(AppError::Anyhow(error));
            }
            let mut child = spawn_claude(&config.claude_binary, port, &extra_args, &backend_model)?;
            let run_result = tokio::select! {
                result = wait_for_claude(&mut child) => result,
                signal = tokio::signal::ctrl_c() => {
                    signal.map_err(anyhow::Error::from)?;
                    terminate_claude(&mut child).await?;
                    Err(AppError::Message("received interrupt signal".to_string()))
                }
            };
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
        ParsedCli::Models { command } => {
            match command {
                ModelsCommand::List => {
                    let access_token = auth.ensure_access_token().await?;
                    let backend_kind = backend_kind_for_token(&access_token)?;
                    let default = default_model_for(backend_kind);
                    for model in available_models_for(backend_kind) {
                        if *model == default {
                            println!("{model} (default)");
                        } else {
                            println!("{model}");
                        }
                    }
                }
            }
            Ok(())
        }
        ParsedCli::ProxyServe => {
            let port = reserve_local_port()?;
            let state = AppState {
                auth,
                backend,
                effort: default_effort(),
            };
            serve(state, port).await?;
            Ok(())
        }
    }
}
