use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("authentication is required; run `claude-codex auth login`")]
    MissingAuth,
    #[error("could not find `claude` in PATH")]
    MissingClaudeBinary,
    #[error("proxy request failed: {0}")]
    Proxy(String),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("{0}")]
    Message(String),
}

impl From<clap::Error> for AppError {
    fn from(value: clap::Error) -> Self {
        Self::Message(value.to_string())
    }
}
