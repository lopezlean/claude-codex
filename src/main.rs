mod auth;
mod cli;
mod config;
mod error;

use crate::cli::ParsedCli;
use crate::error::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let _config = config::AppConfig::from_env();

    match cli::parse(std::env::args_os())? {
        ParsedCli::Run { .. } => Ok(()),
        ParsedCli::Auth { .. } => Ok(()),
        ParsedCli::ProxyServe => Ok(()),
    }
}
