mod auth;
mod backend;
mod cli;
mod config;
mod error;
mod handlers;
mod protocol;
mod server;
#[cfg(test)]
mod test_support;

use crate::auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use crate::auth::provider::AuthProvider;
use crate::auth::session_store::FileSessionStore;
use crate::cli::{AuthCommand, ParsedCli};
use crate::config::AppConfig;
use crate::error::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = AppConfig::from_env()?;
    let auth = OpenAiAuthProvider::new(
        OpenAiAuthConfig {
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_string(),
            auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            redirect_port: config.callback_port,
            callback_timeout_secs: 120,
            refresh_grace_period_secs: 60,
        },
        FileSessionStore::new(config.auth_file.clone()),
    );

    match cli::parse(std::env::args_os())? {
        ParsedCli::Run { .. } => Ok(()),
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
        ParsedCli::ProxyServe => Ok(()),
    }
}
