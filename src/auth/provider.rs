use std::path::PathBuf;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    pub connected: bool,
    pub has_refresh_token: bool,
    pub auth_path: PathBuf,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn login(&self) -> anyhow::Result<()>;
    async fn ensure_access_token(&self) -> anyhow::Result<String>;
    async fn status(&self) -> anyhow::Result<AuthStatus>;
    async fn logout(&self) -> anyhow::Result<()>;
}
