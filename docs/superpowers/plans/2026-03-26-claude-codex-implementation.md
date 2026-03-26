# Claude Codex Initial Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first working `claude-codex` wrapper with OpenAI OAuth, `~/.codex/auth.json` compatibility, an Anthropic-compatible proxy, model mapping, token counting, and `claude` process launch.

**Architecture:** Keep `src/main.rs` thin and push behavior into focused modules for CLI parsing, auth, protocol mapping, server routing, and process supervision. The implementation stays provider-oriented from day one, with one concrete OpenAI provider and pure translation code that is heavily unit-tested.

**Tech Stack:** Rust 2021, `tokio`, `axum`, `reqwest`, `serde`, `serde_json`, `clap`, `thiserror`, `tracing`, `oauth2`, `tiny_http`, `wiremock`, `tempfile`, `assert_cmd`

---

**Preflight**

- Use `superpowers:using-git-worktrees` before Task 1 execution.
- Create a fresh branch named `codex/initial-implementation`.
- Do not implement on `master`.

### Task 1: Scaffold The Crate And Parse CLI Modes

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/cli.rs`
- Create: `src/config.rs`
- Create: `src/error.rs`
- Test: `src/cli.rs`

- [ ] **Step 1: Create the minimal crate scaffold required to run Rust tests**

```toml
# Cargo.toml
[package]
name = "claude-codex"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
async-trait = "0.1"
axum = { version = "0.8", features = ["macros"] }
bytes = "1.10"
clap = { version = "4.5", features = ["derive"] }
futures-core = "0.3"
futures-util = "0.3"
oauth2 = "5.0"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tiny_http = "0.12"
tokio = { version = "1.44", features = ["macros", "process", "rt-multi-thread", "signal", "sync", "time", "net"] }
tokio-stream = "0.1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2.5"
webbrowser = "1.0"

[dev-dependencies]
assert_cmd = "2.0"
http-body-util = "0.1"
predicates = "3.1"
tempfile = "3.17"
tower = { version = "0.5", features = ["util"] }
wiremock = "0.6"
```

```rust
// src/main.rs
mod cli;
mod config;
mod error;

use crate::cli::ParsedCli;
use crate::error::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let cli = cli::parse(std::env::args_os())?;
    tracing_subscriber::fmt().with_env_filter("info").init();

    match cli {
        ParsedCli::Run { claude_args: _ } => Ok(()),
        ParsedCli::Auth { command: _ } => Ok(()),
        ParsedCli::ProxyServe => Ok(()),
    }
}
```

```rust
// src/config.rs
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            auth_file: home.join(".codex").join("auth.json"),
        }
    }
}
```

```rust
// src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
}

impl From<clap::Error> for AppError {
    fn from(value: clap::Error) -> Self {
        Self::Message(value.to_string())
    }
}
```

- [ ] **Step 2: Write the failing CLI parsing tests**

```rust
// src/cli.rs
#[cfg(test)]
mod tests {
    use super::{parse, AuthCommand, ParsedCli};

    #[test]
    fn parses_auth_login_command() {
        let parsed = parse(["claude-codex", "auth", "login"]).expect("auth login should parse");
        assert_eq!(
            parsed,
            ParsedCli::Auth {
                command: AuthCommand::Login,
            }
        );
    }

    #[test]
    fn treats_unknown_words_as_claude_arguments() {
        let parsed =
            parse(["claude-codex", "--model", "claude-3-5-sonnet-latest"]).expect("run mode");

        assert_eq!(
            parsed,
            ParsedCli::Run {
                claude_args: vec!["--model".into(), "claude-3-5-sonnet-latest".into()],
            }
        );
    }

    #[test]
    fn parses_proxy_serve_command() {
        let parsed = parse(["claude-codex", "proxy", "serve"]).expect("proxy serve should parse");
        assert_eq!(parsed, ParsedCli::ProxyServe);
    }
}
```

- [ ] **Step 3: Run the CLI tests to verify they fail for the expected reason**

Run: `cargo test cli::tests:: -- --nocapture`

Expected: FAIL or compile error because `parse`, `ParsedCli`, or `AuthCommand` are not defined yet.

- [ ] **Step 4: Implement the CLI parser with reserved management commands and passthrough run mode**

```rust
// src/cli.rs
use std::ffi::OsString;

use clap::{Parser, Subcommand};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCli {
    Run { claude_args: Vec<OsString> },
    Auth { command: AuthCommand },
    ProxyServe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCommand {
    Login,
    Status,
    Logout,
}

#[derive(Debug, Parser)]
#[command(name = "claude-codex")]
struct ManagementCli {
    #[command(subcommand)]
    command: ManagementCommand,
}

#[derive(Debug, Subcommand)]
enum ManagementCommand {
    Auth {
        #[command(subcommand)]
        command: AuthSubcommand,
    },
    Proxy {
        #[command(subcommand)]
        command: ProxySubcommand,
    },
}

#[derive(Debug, Subcommand)]
enum AuthSubcommand {
    Login,
    Status,
    Logout,
}

#[derive(Debug, Subcommand)]
enum ProxySubcommand {
    Serve,
}

pub fn parse<I, T>(args: I) -> Result<ParsedCli, AppError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let collected: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match collected.get(1).and_then(|value| value.to_str()) {
        Some("auth") | Some("proxy") => parse_management(collected),
        _ => Ok(ParsedCli::Run {
            claude_args: collected.into_iter().skip(1).collect(),
        }),
    }
}

fn parse_management(args: Vec<OsString>) -> Result<ParsedCli, AppError> {
    let parsed = ManagementCli::try_parse_from(args)?;
    let output = match parsed.command {
        ManagementCommand::Auth { command } => ParsedCli::Auth {
            command: match command {
                AuthSubcommand::Login => AuthCommand::Login,
                AuthSubcommand::Status => AuthCommand::Status,
                AuthSubcommand::Logout => AuthCommand::Logout,
            },
        },
        ManagementCommand::Proxy { command } => match command {
            ProxySubcommand::Serve => ParsedCli::ProxyServe,
        },
    };
    Ok(output)
}
```

- [ ] **Step 5: Run the CLI tests and the full suite for green**

Run: `cargo test`

Expected: PASS with 3 passing tests and no failures.

- [ ] **Step 6: Commit the scaffold and CLI parsing slice**

```bash
git add Cargo.toml src/main.rs src/cli.rs src/config.rs src/error.rs
git commit -m "feat: scaffold crate and parse cli modes"
```

### Task 2: Persist Auth Data In `~/.codex/auth.json`

**Files:**
- Create: `src/auth/mod.rs`
- Create: `src/auth/session.rs`
- Create: `src/auth/session_store.rs`
- Modify: `src/main.rs`
- Modify: `src/config.rs`
- Test: `src/auth/session_store.rs`

- [ ] **Step 1: Write the failing auth file compatibility tests**

```rust
// src/auth/session_store.rs
#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{CodexAuthFile, CodexTokens, FileSessionStore};

    #[test]
    fn loads_existing_codex_auth_json() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{
              "auth_mode":"openai",
              "tokens":{
                "id_token":"id-token",
                "access_token":"access-token",
                "refresh_token":"refresh-token",
                "account_id":"acct_123"
              },
              "last_refresh":"2026-03-26T12:00:00Z"
            }"#,
        )
        .expect("auth file should be written");

        let store = FileSessionStore::new(path.clone());
        let auth = store.load().expect("auth should load").expect("auth should exist");

        assert_eq!(auth.auth_mode.as_deref(), Some("openai"));
        assert_eq!(auth.tokens.access_token.as_deref(), Some("access-token"));
        assert_eq!(auth.tokens.refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(store.path(), path.as_path());
    }

    #[test]
    fn saves_the_same_shape_used_by_codex() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        let store = FileSessionStore::new(path.clone());

        store
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: Some("id-token".to_string()),
                    access_token: Some("access-token".to_string()),
                    refresh_token: Some("refresh-token".to_string()),
                    account_id: Some("acct_123".to_string()),
                },
                last_refresh: Some("2026-03-26T12:00:00Z".to_string()),
            })
            .expect("auth should save");

        let raw = fs::read_to_string(path).expect("saved auth");
        assert!(raw.contains("\"auth_mode\":\"openai\""));
        assert!(raw.contains("\"access_token\":\"access-token\""));
        assert!(raw.contains("\"refresh_token\":\"refresh-token\""));
    }
}
```

- [ ] **Step 2: Run the auth store tests to verify they fail**

Run: `cargo test auth::session_store::tests:: -- --nocapture`

Expected: FAIL or compile error because the `auth` module and store types do not exist yet.

- [ ] **Step 3: Implement the auth file types and atomic file store**

```rust
// src/auth/mod.rs
pub mod session;
pub mod session_store;
```

```rust
// src/auth/session.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexAuthFile {
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub tokens: CodexTokens,
    #[serde(default)]
    pub last_refresh: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexTokens {
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}
```

```rust
// src/auth/session_store.rs
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::auth::session::CodexAuthFile;

#[derive(Debug, Clone)]
pub struct FileSessionStore {
    path: PathBuf,
}

impl FileSessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<Option<CodexAuthFile>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.path)?;
        let parsed = serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(Some(parsed))
    }

    pub fn save(&self, auth: &CodexAuthFile) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp_path = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(auth)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        fs::write(&tmp_path, body)?;
        fs::rename(tmp_path, &self.path)?;
        Ok(())
    }

    pub fn clear(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
```

```rust
// src/config.rs
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
    pub callback_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            auth_file: home.join(".codex").join("auth.json"),
            callback_port: 1455,
        }
    }
}
```

```rust
// src/main.rs
mod auth;
mod cli;
mod config;
mod error;
```

- [ ] **Step 4: Run the auth store tests and the full suite**

Run: `cargo test`

Expected: PASS with the CLI tests and the 2 new auth store tests passing.

- [ ] **Step 5: Commit the auth file persistence slice**

```bash
git add Cargo.toml src/main.rs src/config.rs src/auth/mod.rs src/auth/session.rs src/auth/session_store.rs
git commit -m "feat: add codex auth file store"
```

### Task 3: Implement OpenAI OAuth Refresh And Auth Commands

**Files:**
- Modify: `src/auth/mod.rs`
- Create: `src/auth/provider.rs`
- Create: `src/auth/openai.rs`
- Modify: `src/main.rs`
- Modify: `src/error.rs`
- Test: `src/auth/openai.rs`

- [ ] **Step 1: Write the failing OpenAI auth provider tests**

```rust
// src/auth/openai.rs
#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::auth::provider::AuthProvider;
    use crate::auth::session::{CodexAuthFile, CodexTokens};
    use crate::auth::session_store::FileSessionStore;

    use super::{OpenAiAuthConfig, OpenAiAuthProvider};

    #[tokio::test]
    async fn refreshes_an_expiring_session_and_persists_new_tokens() {
        let server = MockServer::start().await;
        let dir = tempdir().expect("temp dir");
        let auth_path = dir.path().join("auth.json");
        let store = FileSessionStore::new(auth_path.clone());
        store
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: None,
                    access_token: Some("expired-access".to_string()),
                    refresh_token: Some("refresh-token".to_string()),
                    account_id: Some("acct_123".to_string()),
                },
                last_refresh: Some("2026-03-26T12:00:00Z".to_string()),
            })
            .expect("seed auth");

        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=refresh-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    r#"{"access_token":"new-access","refresh_token":"new-refresh","id_token":"new-id","expires_in":3600}"#,
                    "application/json",
                ),
            )
            .mount(&server)
            .await;

        let provider = OpenAiAuthProvider::new(
            OpenAiAuthConfig {
                client_id: "client-id".to_string(),
                auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                token_url: format!("{}/oauth/token", server.uri()),
                redirect_port: 1455,
                callback_timeout_secs: 1,
                refresh_grace_period_secs: 60,
            },
            store,
        );

        let token = provider.ensure_access_token().await.expect("token should refresh");
        assert_eq!(token, "new-access");

        let saved = provider.session_store().load().expect("load").expect("present");
        assert_eq!(saved.tokens.access_token.as_deref(), Some("new-access"));
        assert_eq!(saved.tokens.refresh_token.as_deref(), Some("new-refresh"));
    }
}
```

- [ ] **Step 2: Run the provider tests to verify they fail**

Run: `cargo test auth::openai::tests::refreshes_an_expiring_session_and_persists_new_tokens -- --nocapture`

Expected: FAIL or compile error because `AuthProvider` and `OpenAiAuthProvider` do not exist yet.

- [ ] **Step 3: Implement the auth provider trait, OpenAI OAuth provider, and auth command dispatch**

```rust
// src/auth/mod.rs
pub mod openai;
pub mod provider;
pub mod session;
pub mod session_store;
```

```rust
// src/auth/provider.rs
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    pub connected: bool,
    pub has_refresh_token: bool,
    pub auth_path: std::path::PathBuf,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn login(&self) -> anyhow::Result<()>;
    async fn ensure_access_token(&self) -> anyhow::Result<String>;
    async fn status(&self) -> anyhow::Result<AuthStatus>;
    async fn logout(&self) -> anyhow::Result<()>;
}
```

```rust
// src/auth/openai.rs
use std::time::Duration;

use oauth2::basic::BasicClient;
use oauth2::{AuthType, AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenUrl};
use reqwest::Client;
use serde::Deserialize;
use tiny_http::{Header, Response, Server};
use url::Url;

use crate::auth::provider::{AuthProvider, AuthStatus};
use crate::auth::session::{CodexAuthFile, CodexTokens};
use crate::auth::session_store::FileSessionStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_port: u16,
    pub callback_timeout_secs: u64,
    pub refresh_grace_period_secs: u64,
}

#[derive(Debug, Clone)]
pub struct OpenAiAuthProvider {
    client: Client,
    config: OpenAiAuthConfig,
    store: FileSessionStore,
}

impl OpenAiAuthProvider {
    pub fn new(config: OpenAiAuthConfig, store: FileSessionStore) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            config,
            store,
        }
    }

    pub fn session_store(&self) -> &FileSessionStore {
        &self.store
    }

    fn oauth_client(&self) -> anyhow::Result<BasicClient> {
        Ok(BasicClient::new(ClientId::new(self.config.client_id.clone()))
            .set_auth_uri(AuthUrl::new(self.config.auth_url.clone())?)
            .set_token_uri(TokenUrl::new(self.config.token_url.clone())?)
            .set_auth_type(AuthType::RequestBody)
            .set_redirect_uri(RedirectUrl::new(format!(
                "http://localhost:{}/auth/callback",
                self.config.redirect_port
            ))?))
    }

    async fn refresh_session(&self, refresh_token: &str) -> anyhow::Result<CodexAuthFile> {
        let response = self
            .client
            .post(&self.config.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.config.client_id.as_str()),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await?;

        let response = response.error_for_status()?;
        let token: TokenResponse = response.json().await?;
        let auth = CodexAuthFile {
            auth_mode: Some("openai".to_string()),
            tokens: CodexTokens {
                id_token: token.id_token,
                access_token: Some(token.access_token),
                refresh_token: token.refresh_token.or_else(|| Some(refresh_token.to_string())),
                account_id: None,
            },
            last_refresh: Some(chrono_like_timestamp()),
        };
        self.store.save(&auth)?;
        Ok(auth)
    }

    fn receive_callback_code(&self) -> anyhow::Result<String> {
        let server = Server::http(("0.0.0.0", self.config.redirect_port))?;
        for _ in 0..self.config.callback_timeout_secs {
            if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(1)) {
                let parsed = Url::parse(&format!(
                    "http://localhost:{}{}",
                    self.config.redirect_port,
                    request.url()
                ))?;
                if let Some((_, code)) = parsed.query_pairs().find(|(key, _)| key == "code") {
                    let response = Response::from_string(
                        "<html><body><h1>claude-codex</h1><p>Authorization completed.</p></body></html>",
                    )
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).expect("header"),
                    );
                    let _ = request.respond(response);
                    return Ok(code.into_owned());
                }
            }
        }
        anyhow::bail!("oauth callback timed out")
    }
}

#[async_trait::async_trait]
impl AuthProvider for OpenAiAuthProvider {
    async fn login(&self) -> anyhow::Result<()> {
        let oauth = self.oauth_client()?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let redirect_uri = format!("http://localhost:{}/auth/callback", self.config.redirect_port);
        let mut builder = oauth.authorize_url(CsrfToken::new_random);
        for scope in ["openid", "profile", "email", "offline_access"] {
            builder = builder.add_scope(Scope::new(scope.to_string()));
        }
        let (auth_url, _) = builder
            .set_pkce_challenge(challenge)
            .add_extra_param("codex_cli_simplified_flow", "true")
            .url();
        webbrowser::open(auth_url.as_str())?;
        let code = self.receive_callback_code()?;
        let response = self
            .client
            .post(&self.config.token_url)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("client_id", self.config.client_id.as_str()),
                ("code_verifier", verifier.secret()),
            ])
            .send()
            .await?
            .error_for_status()?;
        let token: TokenResponse = response.json().await?;
        self.store.save(&CodexAuthFile {
            auth_mode: Some("openai".to_string()),
            tokens: CodexTokens {
                id_token: token.id_token,
                access_token: Some(token.access_token),
                refresh_token: token.refresh_token,
                account_id: None,
            },
            last_refresh: Some(chrono_like_timestamp()),
        })?;
        Ok(())
    }

    async fn ensure_access_token(&self) -> anyhow::Result<String> {
        let existing = self.store.load()?.ok_or_else(|| anyhow::anyhow!("run `claude-codex auth login` first"))?;
        if let Some(refresh_token) = existing.tokens.refresh_token.as_deref() {
            let refreshed = self.refresh_session(refresh_token).await?;
            return refreshed
                .tokens
                .access_token
                .ok_or_else(|| anyhow::anyhow!("refreshed access token missing"));
        }
        existing
            .tokens
            .access_token
            .ok_or_else(|| anyhow::anyhow!("access token missing from auth file"))
    }

    async fn status(&self) -> anyhow::Result<AuthStatus> {
        let current = self.store.load()?;
        Ok(AuthStatus {
            connected: current.is_some(),
            has_refresh_token: current
                .as_ref()
                .and_then(|auth| auth.tokens.refresh_token.as_ref())
                .map(|value| !value.is_empty())
                .unwrap_or(false),
            auth_path: self.store.path().to_path_buf(),
        })
    }

    async fn logout(&self) -> anyhow::Result<()> {
        self.store.clear()?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{now}")
}
```

```rust
// src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
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
```

```rust
// src/main.rs
mod auth;
mod cli;
mod config;
mod error;

use auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use auth::provider::AuthProvider;
use auth::session_store::FileSessionStore;
use cli::{AuthCommand, ParsedCli};
use config::AppConfig;
use error::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = cli::parse(std::env::args_os())?;
    let config = AppConfig::from_env();
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

    match cli {
        ParsedCli::Run { claude_args: _ } => Ok(()),
        ParsedCli::ProxyServe => Ok(()),
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
    }
}
```

- [ ] **Step 4: Run the targeted provider test and the full suite**

Run: `cargo test`

Expected: PASS with the new refresh test passing and previous tests still green.

- [ ] **Step 5: Commit the OAuth provider slice**

```bash
git add src/main.rs src/error.rs src/auth/mod.rs src/auth/provider.rs src/auth/openai.rs
git commit -m "feat: add openai oauth provider"
```

### Task 4: Translate Anthropic Requests Into OpenAI Chat Requests

**Files:**
- Create: `src/protocol/mod.rs`
- Create: `src/protocol/anthropic.rs`
- Create: `src/protocol/openai.rs`
- Create: `src/protocol/mapper.rs`
- Modify: `src/main.rs`
- Test: `src/protocol/mapper.rs`

- [ ] **Step 1: Write the failing mapper tests for text, tools, and model aliases**

```rust
// src/protocol/mapper.rs
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{map_anthropic_to_openai, map_model_name};
    use crate::protocol::anthropic::{
        AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, ToolChoice,
    };

    #[test]
    fn maps_system_and_user_text_to_chat_completions_messages() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            system: Some("You are concise.".to_string()),
            max_tokens: Some(512),
            stream: false,
            tools: vec![],
            tool_choice: None,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::Text {
                    text: "Say hello".to_string(),
                }],
            }],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.model, "gpt-4o");
        assert_eq!(mapped.messages[0].role, "system");
        assert_eq!(mapped.messages[1].role, "user");
    }

    #[test]
    fn maps_tool_result_blocks_to_tool_messages() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-5-haiku-latest".to_string(),
            system: None,
            max_tokens: Some(256),
            stream: false,
            tools: vec![],
            tool_choice: Some(ToolChoice::Auto),
            messages: vec![
                AnthropicMessage {
                    role: "assistant".to_string(),
                    content: vec![AnthropicContentBlock::ToolUse {
                        id: "toolu_123".to_string(),
                        name: "lookup".to_string(),
                        input: json!({"city":"Madrid"}),
                    }],
                },
                AnthropicMessage {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::ToolResult {
                        tool_use_id: "toolu_123".to_string(),
                        content: "sunny".to_string(),
                    }],
                },
            ],
        };

        let mapped = map_anthropic_to_openai(&request).expect("mapping should work");
        assert_eq!(mapped.model, "gpt-4o-mini");
        assert_eq!(mapped.messages.last().expect("last").role, "tool");
    }

    #[test]
    fn falls_back_to_default_model_for_unknown_claude_alias() {
        assert_eq!(map_model_name("claude-unknown"), "gpt-4o");
    }
}
```

- [ ] **Step 2: Run the mapper tests to verify they fail**

Run: `cargo test protocol::mapper::tests:: -- --nocapture`

Expected: FAIL or compile error because the protocol types and mapper do not exist yet.

- [ ] **Step 3: Implement Anthropic/OpenAI types and pure mapping logic**

```rust
// src/protocol/mod.rs
pub mod anthropic;
pub mod mapper;
pub mod openai;
```

```rust
// src/protocol/anthropic.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Vec<AnthropicToolDefinition>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
    pub messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
}
```

```rust
// src/protocol/openai.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiChatMessage>,
    #[serde(default)]
    pub tools: Vec<OpenAiToolDefinition>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OpenAiFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAiToolFunction {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub parameters: Value,
}
```

```rust
// src/protocol/mapper.rs
use anyhow::Result;

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest};
use crate::protocol::openai::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiFunctionCall, OpenAiToolCall, OpenAiToolDefinition,
    OpenAiToolFunction,
};

pub fn map_model_name(model: &str) -> &'static str {
    if model.starts_with("claude-3-5-haiku-") {
        "gpt-4o-mini"
    } else {
        "gpt-4o"
    }
}

pub fn map_anthropic_to_openai(request: &AnthropicMessagesRequest) -> Result<OpenAiChatRequest> {
    let mut messages = Vec::new();

    if let Some(system) = &request.system {
        messages.push(OpenAiChatMessage {
            role: "system".to_string(),
            content: Some(system.clone()),
            tool_call_id: None,
            tool_calls: vec![],
        });
    }

    for message in &request.messages {
        for block in &message.content {
            match block {
                AnthropicContentBlock::Text { text } => messages.push(OpenAiChatMessage {
                    role: message.role.clone(),
                    content: Some(text.clone()),
                    tool_call_id: None,
                    tool_calls: vec![],
                }),
                AnthropicContentBlock::ToolUse { id, name, input } => messages.push(OpenAiChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_call_id: None,
                    tool_calls: vec![OpenAiToolCall {
                        id: id.clone(),
                        kind: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: name.clone(),
                            arguments: serde_json::to_string(input)?,
                        },
                    }],
                }),
                AnthropicContentBlock::ToolResult { tool_use_id, content } => messages.push(OpenAiChatMessage {
                    role: "tool".to_string(),
                    content: Some(content.clone()),
                    tool_call_id: Some(tool_use_id.clone()),
                    tool_calls: vec![],
                }),
            }
        }
    }

    let tools = request
        .tools
        .iter()
        .map(|tool| OpenAiToolDefinition {
            kind: "function".to_string(),
            function: OpenAiToolFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            },
        })
        .collect();

    Ok(OpenAiChatRequest {
        model: map_model_name(&request.model).to_string(),
        messages,
        tools,
        stream: request.stream,
        max_tokens: request.max_tokens,
    })
}
```

```rust
// src/main.rs
mod auth;
mod cli;
mod config;
mod error;
mod protocol;
```

- [ ] **Step 4: Run the mapper tests and the full suite**

Run: `cargo test`

Expected: PASS with the new mapper tests green.

- [ ] **Step 5: Commit the protocol mapping slice**

```bash
git add src/main.rs src/protocol/mod.rs src/protocol/anthropic.rs src/protocol/openai.rs src/protocol/mapper.rs
git commit -m "feat: map anthropic messages to openai chat"
```

### Task 5: Add Backend Provider, Health, Count Tokens, And Non-Streaming `/v1/messages`

**Files:**
- Create: `src/backend/mod.rs`
- Create: `src/backend/provider.rs`
- Create: `src/backend/openai.rs`
- Create: `src/handlers/mod.rs`
- Create: `src/handlers/messages.rs`
- Create: `src/handlers/count_tokens.rs`
- Create: `src/handlers/health.rs`
- Create: `src/server.rs`
- Modify: `src/main.rs`
- Test: `src/server.rs`

- [ ] **Step 1: Write the failing server tests for health, token counting, and non-stream translation**

```rust
// src/server.rs
#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::auth::session::{CodexAuthFile, CodexTokens};
    use crate::auth::session_store::FileSessionStore;
    use crate::backend::openai::OpenAiBackendConfig;
    use crate::server::build_router;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let router = build_router_for_test("http://127.0.0.1:9").await;
        let response = router
            .oneshot(axum::http::Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn forwards_non_stream_messages_and_returns_anthropic_shape() {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains("\"model\":\"gpt-4o\""))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    r#"{
                      "id":"chatcmpl_123",
                      "choices":[{"message":{"role":"assistant","content":"Hello from OpenAI","tool_calls":[]}}],
                      "usage":{"prompt_tokens":10,"completion_tokens":4}
                    }"#,
                    "application/json",
                ),
            )
            .mount(&upstream)
            .await;

        let router = build_router_for_test(&upstream.uri()).await;
        let response = router
            .oneshot(
                axum::http::Request::post("/v1/messages")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                          "model":"claude-3-5-sonnet-latest",
                          "messages":[{"role":"user","content":[{"type":"text","text":"Hello"}]}],
                          "stream":false
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(body.to_vec()).unwrap();
        assert!(raw.contains("\"type\":\"text\""));
        assert!(raw.contains("Hello from OpenAI"));
    }

    #[tokio::test]
    async fn count_tokens_returns_a_local_estimate() {
        let router = build_router_for_test("http://127.0.0.1:9").await;
        let response = router
            .oneshot(
                axum::http::Request::post("/v1/messages/count_tokens")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                          "model":"claude-3-5-sonnet-latest",
                          "messages":[{"role":"user","content":[{"type":"text","text":"Hello world"}]}]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    async fn build_router_for_test(upstream_base_url: &str) -> axum::Router {
        let dir = tempfile::tempdir().expect("temp dir");
        let auth_path = dir.path().join("auth.json");
        FileSessionStore::new(auth_path)
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: None,
                    access_token: Some("access-token".to_string()),
                    refresh_token: None,
                    account_id: None,
                },
                last_refresh: Some("123".to_string()),
            })
            .expect("seed auth");

        build_router(
            crate::server::AppState::for_tests(
                FileSessionStore::new(dir.path().join("auth.json")),
                OpenAiBackendConfig {
                    base_url: upstream_base_url.to_string(),
                    chat_completions_path: "/v1/chat/completions".to_string(),
                },
            )
            .await,
        )
    }
}
```

- [ ] **Step 2: Run the server tests to verify they fail**

Run: `cargo test server::tests:: -- --nocapture`

Expected: FAIL or compile error because the backend, handlers, and router do not exist yet.

- [ ] **Step 3: Implement the backend provider, handlers, and Axum router**

```rust
// src/backend/mod.rs
pub mod openai;
pub mod provider;
```

```rust
// src/backend/provider.rs
use anyhow::Result;

use crate::protocol::anthropic::AnthropicMessagesRequest;
use crate::protocol::openai::OpenAiChatRequest;

pub struct UpstreamResponse {
    pub status: reqwest::StatusCode,
    pub body: serde_json::Value,
}

#[async_trait::async_trait]
pub trait BackendProvider: Send + Sync {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse>;
}
```

```rust
// src/backend/openai.rs
use anyhow::Result;
use reqwest::Client;

use crate::backend::provider::{BackendProvider, UpstreamResponse};
use crate::protocol::openai::OpenAiChatRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiBackendConfig {
    pub base_url: String,
    pub chat_completions_path: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiBackendProvider {
    client: Client,
    config: OpenAiBackendConfig,
}

impl OpenAiBackendProvider {
    pub fn new(config: OpenAiBackendConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl BackendProvider for OpenAiBackendProvider {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse> {
        let response = self
            .client
            .post(format!("{}{}", self.config.base_url, self.config.chat_completions_path))
            .bearer_auth(access_token)
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let body = response.json().await?;
        Ok(UpstreamResponse { status, body })
    }
}
```

```rust
// src/handlers/mod.rs
pub mod count_tokens;
pub mod health;
pub mod messages;
```

```rust
// src/handlers/health.rs
pub async fn health() -> &'static str {
    "ok"
}
```

```rust
// src/handlers/count_tokens.rs
use axum::Json;
use serde::Serialize;

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest};

#[derive(Debug, Serialize)]
pub struct CountTokensResponse {
    pub input_tokens: usize,
}

pub async fn count_tokens(Json(request): Json<AnthropicMessagesRequest>) -> Json<CountTokensResponse> {
    let mut input_tokens = 0usize;
    for message in request.messages {
        for block in message.content {
            input_tokens += match block {
                AnthropicContentBlock::Text { text } => text.split_whitespace().count(),
                AnthropicContentBlock::ToolUse { name, input, .. } => {
                    name.split_whitespace().count() + input.to_string().len() / 4
                }
                AnthropicContentBlock::ToolResult { content, .. } => content.split_whitespace().count(),
            };
        }
    }
    Json(CountTokensResponse { input_tokens })
}
```

```rust
// src/handlers/messages.rs
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest};
use crate::protocol::mapper::map_anthropic_to_openai;
use crate::server::AppState;

pub async fn create_message(
    State(state): State<AppState>,
    Json(request): Json<AnthropicMessagesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let access_token = state
        .auth
        .ensure_access_token()
        .await
        .map_err(internal_error)?;
    let mapped = map_anthropic_to_openai(&request).map_err(internal_error)?;
    let upstream = state
        .backend
        .send_chat(&access_token, &mapped)
        .await
        .map_err(internal_error)?;
    if !upstream.status.is_success() {
        return Err((StatusCode::BAD_GATEWAY, upstream.body.to_string()));
    }

    let assistant_text = upstream
        .body
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    Ok(Json(json!({
        "id": "msg_codex_proxy",
        "type": "message",
        "role": "assistant",
        "model": request.model,
        "content": [
            {
                "type": "text",
                "text": assistant_text
            }
        ],
        "stop_reason": "end_turn"
    })))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
```

```rust
// src/server.rs
use std::sync::Arc;

use axum::{routing::{get, post}, Router};

use crate::auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use crate::auth::provider::AuthProvider;
use crate::auth::session_store::FileSessionStore;
use crate::backend::openai::{OpenAiBackendConfig, OpenAiBackendProvider};
use crate::backend::provider::BackendProvider;
use crate::handlers::{count_tokens::count_tokens, health::health, messages::create_message};

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<dyn AuthProvider>,
    pub backend: Arc<dyn BackendProvider>,
}

impl AppState {
    pub async fn for_tests(store: FileSessionStore, backend: OpenAiBackendConfig) -> Self {
        Self {
            auth: Arc::new(OpenAiAuthProvider::new(
                OpenAiAuthConfig {
                    client_id: "client-id".to_string(),
                    auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                    token_url: "https://auth.openai.com/oauth/token".to_string(),
                    redirect_port: 1455,
                    callback_timeout_secs: 1,
                    refresh_grace_period_secs: 60,
                },
                store,
            )),
            backend: Arc::new(OpenAiBackendProvider::new(backend)),
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/messages", post(create_message))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .with_state(state)
}
```

```rust
// src/main.rs
mod auth;
mod backend;
mod cli;
mod config;
mod error;
mod handlers;
mod protocol;
mod server;
```

- [ ] **Step 4: Run the server tests and then the full suite**

Run: `cargo test`

Expected: PASS with the 3 new server tests green.

- [ ] **Step 5: Commit the server and non-stream proxy slice**

```bash
git add src/main.rs src/backend/mod.rs src/backend/provider.rs src/backend/openai.rs src/handlers/mod.rs src/handlers/messages.rs src/handlers/count_tokens.rs src/handlers/health.rs src/server.rs
git commit -m "feat: add non-stream proxy endpoints"
```

### Task 6: Re-Encode OpenAI Streaming Chunks As Anthropic SSE

**Files:**
- Create: `src/protocol/stream.rs`
- Modify: `src/protocol/mod.rs`
- Modify: `src/backend/provider.rs`
- Modify: `src/backend/openai.rs`
- Modify: `src/handlers/messages.rs`
- Test: `src/protocol/stream.rs`

- [ ] **Step 1: Write the failing stream translation tests**

```rust
// src/protocol/stream.rs
#[cfg(test)]
mod tests {
    use super::translate_openai_sse_frame;

    #[test]
    fn converts_openai_content_delta_to_anthropic_events() {
        let frame = r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#;
        let translated = translate_openai_sse_frame(frame).expect("translation should work");
        assert!(translated.contains("event: message_start"));
        assert!(translated.contains("event: content_block_start"));
        assert!(translated.contains("event: content_block_delta"));
        assert!(translated.contains("\"text\":\"Hel\""));
    }

    #[test]
    fn converts_done_marker_to_message_stop() {
        let translated = translate_openai_sse_frame("data: [DONE]").expect("done marker");
        assert!(translated.contains("event: content_block_stop"));
        assert!(translated.contains("event: message_stop"));
    }
}
```

- [ ] **Step 2: Run the stream tests to verify they fail**

Run: `cargo test protocol::stream::tests:: -- --nocapture`

Expected: FAIL because the stream translator does not exist yet.

- [ ] **Step 3: Implement the SSE translator and wire it into the message handler**

```rust
// src/protocol/mod.rs
pub mod anthropic;
pub mod mapper;
pub mod openai;
pub mod stream;
```

```rust
// src/protocol/stream.rs
use anyhow::Result;
use serde_json::Value;

pub fn translate_openai_sse_frame(frame: &str) -> Result<String> {
    let payload = frame.trim().strip_prefix("data: ").unwrap_or(frame.trim());
    if payload == "[DONE]" {
        return Ok(
            "event: content_block_stop\ndata: {\"index\":0}\n\nevent: message_stop\ndata: {}\n\n"
                .to_string(),
        );
    }

    let parsed: Value = serde_json::from_str(payload)?;
    let delta = parsed
        .pointer("/choices/0/delta/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    Ok(format!(
        "event: message_start\ndata: {{\"type\":\"message\"}}\n\nevent: content_block_start\ndata: {{\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\nevent: content_block_delta\ndata: {{\"type\":\"text_delta\",\"text\":\"{}\"}}\n\n",
        delta
    ))
}
```

```rust
// src/backend/provider.rs
use anyhow::Result;
use futures_core::Stream;

use crate::protocol::openai::OpenAiChatRequest;

pub struct UpstreamResponse {
    pub status: reqwest::StatusCode,
    pub body: serde_json::Value,
}

pub type UpstreamStream = std::pin::Pin<Box<dyn Stream<Item = Result<bytes::Bytes>> + Send>>;

#[async_trait::async_trait]
pub trait BackendProvider: Send + Sync {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse>;

    async fn send_chat_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamStream>;
}
```

```rust
// src/backend/openai.rs
use futures_util::StreamExt;

use crate::backend::provider::{BackendProvider, UpstreamResponse, UpstreamStream};
use crate::protocol::openai::OpenAiChatRequest;

#[async_trait::async_trait]
impl BackendProvider for OpenAiBackendProvider {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse> {
        let response = self
            .client
            .post(format!("{}{}", self.config.base_url, self.config.chat_completions_path))
            .bearer_auth(access_token)
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let body = response.json().await?;
        Ok(UpstreamResponse { status, body })
    }

    async fn send_chat_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamStream> {
        let response = self
            .client
            .post(format!("{}{}", self.config.base_url, self.config.chat_completions_path))
            .bearer_auth(access_token)
            .json(request)
            .send()
            .await?
            .error_for_status()?;
        Ok(Box::pin(response.bytes_stream().map(|chunk| chunk.map_err(anyhow::Error::from))))
    }
}
```

```rust
// src/handlers/messages.rs
use axum::body::Body;
use axum::{extract::State, Json};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt;

use crate::protocol::anthropic::AnthropicMessagesRequest;
use crate::protocol::mapper::map_anthropic_to_openai;
use crate::protocol::stream::translate_openai_sse_frame;
use crate::server::AppState;

pub async fn create_message(
    State(state): State<AppState>,
    Json(request): Json<AnthropicMessagesRequest>,
) -> Result<Response, (StatusCode, String)> {
    let access_token = state.auth.ensure_access_token().await.map_err(internal_error)?;
    let mapped = map_anthropic_to_openai(&request).map_err(internal_error)?;

    if request.stream {
        let stream = state
            .backend
            .send_chat_stream(&access_token, &mapped)
            .await
            .map_err(internal_error)?
            .map(|chunk| match chunk {
                Ok(bytes) => {
                    let raw = String::from_utf8_lossy(&bytes);
                    translate_openai_sse_frame(&raw)
                        .map(Bytes::from)
                        .map_err(internal_error)
                }
                Err(error) => Err(internal_error(error)),
            });

        let body = Body::from_stream(stream);
        let response = (
            [(header::CONTENT_TYPE, "text/event-stream")],
            body,
        )
            .into_response();
        return Ok(response);
    }

    let upstream = state
        .backend
        .send_chat(&access_token, &mapped)
        .await
        .map_err(internal_error)?;
    if !upstream.status.is_success() {
        return Err((StatusCode::BAD_GATEWAY, upstream.body.to_string()));
    }

    let assistant_text = upstream
        .body
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let body = serde_json::json!({
        "id": "msg_codex_proxy",
        "type": "message",
        "role": "assistant",
        "model": request.model,
        "content": [
            {
                "type": "text",
                "text": assistant_text
            }
        ],
        "stop_reason": "end_turn"
    });

    Ok(axum::Json(body).into_response())
}
```

- [ ] **Step 4: Run the stream tests and the full suite**

Run: `cargo test`

Expected: PASS with the 2 stream tests green and the previous server tests still green.

- [ ] **Step 5: Commit the streaming translation slice**

```bash
git add src/protocol/mod.rs src/protocol/stream.rs src/backend/provider.rs src/backend/openai.rs src/handlers/messages.rs
git commit -m "feat: translate openai streams to anthropic sse"
```

### Task 7: Launch `claude`, Inject Environment, And Supervise Proxy Lifetime

**Files:**
- Create: `src/process.rs`
- Modify: `src/config.rs`
- Modify: `src/server.rs`
- Modify: `src/main.rs`
- Test: `tests/cli_wrapper.rs`

- [ ] **Step 1: Write the failing wrapper integration test using a fake `claude` executable**

```rust
// tests/cli_wrapper.rs
use std::fs;

use assert_cmd::Command;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn run_mode_launches_claude_with_proxy_environment() {
    let dir = tempdir().expect("temp dir");
    let bin_dir = dir.path().join("bin");
    let home_dir = dir.path().join("home");
    let capture_path = dir.path().join("capture.txt");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
    fs::write(
        home_dir.join(".codex").join("auth.json"),
        json!({
            "auth_mode": "openai",
            "tokens": {
                "access_token": "access-token"
            },
            "last_refresh": "123"
        })
        .to_string(),
    )
    .expect("auth file");

    let script = format!(
        "#!/bin/sh\nprintf 'BASE=%s\\nKEY=%s\\nARGS=%s\\n' \"$ANTHROPIC_BASE_URL\" \"$ANTHROPIC_API_KEY\" \"$*\" > \"{}\"\n",
        capture_path.display()
    );
    let claude_path = bin_dir.join("claude");
    fs::write(&claude_path, script).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&claude_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&claude_path, perms).unwrap();
    }

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env("PATH", format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()))
        .arg("--print")
        .arg("hello")
        .assert()
        .success();

    let captured = fs::read_to_string(capture_path).expect("capture");
    assert!(captured.contains("BASE=http://127.0.0.1:"));
    assert!(captured.contains("KEY=sk-ant-codex-proxy"));
    assert!(captured.contains("ARGS=--print hello"));
}
```

- [ ] **Step 2: Run the wrapper test to verify it fails**

Run: `cargo test --test cli_wrapper -- --nocapture`

Expected: FAIL because the run mode does not start the proxy or spawn the `claude` process yet.

- [ ] **Step 3: Implement process launch, random free port allocation, and graceful shutdown**

```rust
// src/config.rs
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
    pub callback_port: u16,
    pub claude_binary: String,
    pub upstream_base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            auth_file: home.join(".codex").join("auth.json"),
            callback_port: 1455,
            claude_binary: "claude".to_string(),
            upstream_base_url: "https://api.openai.com".to_string(),
        }
    }
}
```

```rust
// src/process.rs
use std::net::TcpListener;
use std::process::Stdio;

use tokio::process::Command;

pub fn reserve_local_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub async fn run_claude(binary: &str, port: u16, args: &[std::ffi::OsString]) -> anyhow::Result<()> {
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
```

```rust
// src/server.rs
pub async fn serve(state: AppState, port: u16) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
```

```rust
// src/main.rs
mod auth;
mod backend;
mod cli;
mod config;
mod error;
mod handlers;
mod process;
mod protocol;
mod server;

use tokio::task::JoinHandle;

use auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use auth::provider::AuthProvider;
use auth::session_store::FileSessionStore;
use backend::openai::{OpenAiBackendConfig, OpenAiBackendProvider};
use cli::{AuthCommand, ParsedCli};
use config::AppConfig;
use error::AppError;
use process::{reserve_local_port, run_claude};
use server::{serve, AppState};

#[tokio::main]
async fn main() -> Result<(), AppError> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = cli::parse(std::env::args_os())?;
    let config = AppConfig::from_env();
    let store = FileSessionStore::new(config.auth_file.clone());
    let auth = std::sync::Arc::new(OpenAiAuthProvider::new(
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
    let backend = std::sync::Arc::new(OpenAiBackendProvider::new(OpenAiBackendConfig {
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
        ParsedCli::ProxyServe => {
            let port = reserve_local_port()?;
            let state = AppState { auth, backend };
            serve(state, port).await?;
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
    }
}
```

- [ ] **Step 4: Run the wrapper integration test and the full suite**

Run: `cargo test`

Expected: PASS with the wrapper test green and earlier unit/integration tests still green.

- [ ] **Step 5: Commit the wrapper launch slice**

```bash
git add src/config.rs src/process.rs src/server.rs src/main.rs tests/cli_wrapper.rs
git commit -m "feat: launch claude through local proxy"
```

### Task 8: Tighten Error Paths, Streaming Coverage, And Final Verification

**Files:**
- Modify: `src/error.rs`
- Modify: `src/process.rs`
- Modify: `src/handlers/messages.rs`
- Create: `tests/proxy_errors.rs`
- Modify: `src/protocol/stream.rs`
- Test: `tests/proxy_errors.rs`
- Test: `src/protocol/stream.rs`

- [ ] **Step 1: Write the failing regression tests for upstream errors and streaming stop events**

```rust
// tests/proxy_errors.rs
use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn run_mode_reports_missing_claude_binary_cleanly() {
    let dir = tempdir().expect("temp dir");
    let home_dir = dir.path().join("home");
    let path_dir = dir.path().join("empty-path");
    fs::create_dir_all(home_dir.join(".codex")).expect("auth dir");
    fs::create_dir_all(&path_dir).expect("path dir");
    fs::write(
        home_dir.join(".codex").join("auth.json"),
        json!({
            "auth_mode": "openai",
            "tokens": {
                "access_token": "access-token"
            },
            "last_refresh": "123"
        })
        .to_string(),
    )
    .expect("auth file");

    Command::cargo_bin("claude-codex")
        .expect("binary")
        .env("HOME", &home_dir)
        .env("PATH", &path_dir)
        .arg("--print")
        .arg("hello")
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not find `claude` in PATH"));
}
```

```rust
// src/protocol/stream.rs
#[cfg(test)]
mod regression_tests {
    use super::translate_openai_sse_frame;

    #[test]
    fn done_marker_produces_message_stop_event() {
        let payload = translate_openai_sse_frame("data: [DONE]").expect("translation");
        assert!(payload.contains("event: message_stop"));
    }
}
```

- [ ] **Step 2: Run the new regression tests to verify they fail or expose missing behavior**

Run: `cargo test --test proxy_errors protocol::stream::regression_tests::done_marker_produces_message_stop_event -- --nocapture`

Expected: FAIL until the error reporting and streaming edge handling are fully wired.

- [ ] **Step 3: Implement final error shaping and regression fixes**

```rust
// src/error.rs
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
```

```rust
// src/process.rs
use std::io::ErrorKind;

use crate::error::AppError;

pub async fn run_claude(binary: &str, port: u16, args: &[std::ffi::OsString]) -> Result<(), AppError> {
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let status = tokio::process::Command::new(binary)
        .args(args)
        .env("ANTHROPIC_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", "sk-ant-codex-proxy")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::MissingClaudeBinary,
            _ => AppError::Proxy(error.to_string()),
        })?;

    if !status.success() {
        return Err(AppError::Proxy(format!("claude exited with status {status}")));
    }
    Ok(())
}
```

```rust
// src/handlers/messages.rs
fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("proxy request failed: {error}");
    (StatusCode::BAD_GATEWAY, error.to_string())
}
```

- [ ] **Step 4: Run the full verification commands**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo test`
Expected: PASS with all unit, server, and integration tests green

Run: `cargo run -- auth status`
Expected: PASS with a readable status line even when the auth file is missing

- [ ] **Step 5: Commit the hardening slice**

```bash
git add src/error.rs src/process.rs src/handlers/messages.rs src/protocol/stream.rs tests/proxy_errors.rs
git commit -m "test: cover proxy error and streaming regressions"
```

## Self-Review Checklist

- Spec coverage:
  - CLI wrapper behavior is covered by Tasks 1 and 7.
  - `~/.codex/auth.json` compatibility is covered by Tasks 2 and 3.
  - OpenAI OAuth reuse and refresh behavior is covered by Task 3.
  - Anthropic-to-OpenAI translation is covered by Tasks 4, 5, and 6.
  - `count_tokens` and `healthz` are covered by Task 5.
  - Streaming SSE translation is covered by Tasks 6 and 8.
  - Error handling and verification are covered by Tasks 7 and 8.
- Placeholder scan:
  - No deferred implementation markers remain.
- Type consistency:
  - The plan uses `AppConfig`, `OpenAiAuthProvider`, `OpenAiBackendProvider`, `AppState`, `ParsedCli`, `AuthCommand`, `CodexAuthFile`, and `FileSessionStore` consistently across tasks.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-03-26-claude-codex-implementation.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
