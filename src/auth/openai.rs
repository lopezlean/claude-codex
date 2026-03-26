use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use oauth2::{CsrfToken, PkceCodeChallenge};
use reqwest::Client;
use serde::Deserialize;
use tiny_http::{Header, Response, Server};
use url::Url;

use crate::auth::provider::{AuthProvider, AuthStatus};
use crate::auth::session::{CodexAuthFile, CodexTokens};
use crate::auth::session_store::FileSessionStore;

const ACCESS_TOKEN_LIFETIME_SECS: u64 = 3600;

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
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("OpenAI auth HTTP client should build");
        Self {
            client,
            config,
            store,
        }
    }

    pub fn session_store(&self) -> &FileSessionStore {
        &self.store
    }

    fn redirect_uri(&self) -> String {
        format!(
            "http://localhost:{}/auth/callback",
            self.config.redirect_port
        )
    }

    fn authorize_url(&self, challenge: &str, state: &str) -> Result<String> {
        let mut url = Url::parse(&self.config.auth_url)
            .with_context(|| format!("invalid OpenAI auth URL: {}", self.config.auth_url))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("redirect_uri", &self.redirect_uri());
            query.append_pair("scope", "openid profile email offline_access");
            query.append_pair("code_challenge", challenge);
            query.append_pair("code_challenge_method", "S256");
            query.append_pair("state", state);
            query.append_pair("codex_cli_simplified_flow", "true");
        }
        Ok(url.into())
    }

    async fn refresh_session(
        &self,
        existing: &CodexAuthFile,
        refresh_token: &str,
    ) -> Result<CodexAuthFile> {
        let response = self
            .client
            .post(&self.config.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.config.client_id.as_str()),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("failed to send refresh-token request to OpenAI")?
            .error_for_status()
            .context("OpenAI rejected the refresh-token request")?;

        let token: TokenResponse = response
            .json()
            .await
            .context("failed to decode OpenAI refresh-token response")?;
        let auth = CodexAuthFile {
            auth_mode: Some("openai".to_string()),
            tokens: CodexTokens {
                id_token: token.id_token,
                access_token: Some(token.access_token),
                refresh_token: token
                    .refresh_token
                    .or_else(|| Some(refresh_token.to_string())),
                account_id: existing.tokens.account_id.clone(),
            },
            last_refresh: Some(current_timestamp_secs().to_string()),
        };
        self.store
            .save(&auth)
            .context("failed to persist refreshed OpenAI auth session")?;
        Ok(auth)
    }

    fn receive_callback_code(&self, expected_state: &str) -> Result<String> {
        let server = Server::http(("127.0.0.1", self.config.redirect_port))
            .map_err(|error| anyhow!("failed to bind OpenAI callback server: {error}"))?;

        for _ in 0..self.config.callback_timeout_secs {
            if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(1)) {
                let callback_url = format!(
                    "http://localhost:{}{}",
                    self.config.redirect_port,
                    request.url()
                );
                let parsed = Url::parse(&callback_url).with_context(|| {
                    format!("failed to parse OAuth callback URL: {callback_url}")
                })?;

                if let Some((_, error)) = parsed.query_pairs().find(|(key, _)| key == "error") {
                    let response = Response::from_string(
                        "<html><body><h1>claude-codex</h1><p>Authorization failed.</p></body></html>",
                    )
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                            .expect("static header should be valid"),
                    );
                    let _ = request.respond(response);
                    bail!("OpenAI authorization failed: {error}");
                }

                let callback_state = parsed
                    .query_pairs()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.into_owned())
                    .ok_or_else(|| anyhow!("oauth callback missing state"))?;
                if callback_state != expected_state {
                    let response = Response::from_string(
                        "<html><body><h1>claude-codex</h1><p>Authorization state mismatch.</p></body></html>",
                    )
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                            .expect("static header should be valid"),
                    );
                    let _ = request.respond(response);
                    bail!("oauth callback state mismatch");
                }

                if let Some((_, code)) = parsed.query_pairs().find(|(key, _)| key == "code") {
                    let response = Response::from_string(
                        "<html><body><h1>claude-codex</h1><p>Authorization completed.</p></body></html>",
                    )
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                            .expect("static header should be valid"),
                    );
                    let _ = request.respond(response);
                    return Ok(code.into_owned());
                }
            }
        }

        bail!("oauth callback timed out");
    }

    fn should_refresh(&self, session: &CodexAuthFile) -> bool {
        let refresh_token = match session.tokens.refresh_token.as_deref() {
            Some(value) if !value.is_empty() => value,
            _ => return false,
        };
        if refresh_token.is_empty() {
            return false;
        }

        match session.tokens.access_token.as_deref() {
            Some(value) if !value.is_empty() => {}
            _ => return true,
        }

        let Some(last_refresh) = session.last_refresh.as_deref() else {
            return true;
        };

        // Older auth files may carry non-epoch timestamps; refresh once to normalize them.
        let Ok(last_refresh_secs) = last_refresh.parse::<u64>() else {
            return true;
        };

        let refresh_window = ACCESS_TOKEN_LIFETIME_SECS.saturating_sub(
            self.config
                .refresh_grace_period_secs
                .min(ACCESS_TOKEN_LIFETIME_SECS),
        );
        current_timestamp_secs() >= last_refresh_secs.saturating_add(refresh_window)
    }
}

#[async_trait]
impl AuthProvider for OpenAiAuthProvider {
    async fn login(&self) -> Result<()> {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let state = CsrfToken::new_random();
        let auth_url = self.authorize_url(challenge.as_str(), state.secret())?;

        webbrowser::open(auth_url.as_str())
            .context("failed to open the OpenAI authorization page in a browser")?;

        let code = self.receive_callback_code(state.secret())?;
        let redirect_uri = self.redirect_uri();
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
            .await
            .context("failed to exchange the OpenAI authorization code")?
            .error_for_status()
            .context("OpenAI rejected the authorization-code exchange")?;

        let token: TokenResponse = response
            .json()
            .await
            .context("failed to decode OpenAI authorization response")?;
        self.store
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: token.id_token,
                    access_token: Some(token.access_token),
                    refresh_token: token.refresh_token,
                    account_id: None,
                },
                last_refresh: Some(current_timestamp_secs().to_string()),
            })
            .context("failed to persist OpenAI auth session")?;
        Ok(())
    }

    async fn ensure_access_token(&self) -> Result<String> {
        let existing = self
            .store
            .load()
            .context("failed to load the OpenAI auth session")?
            .ok_or_else(|| anyhow!("run `claude-codex auth login` first"))?;

        if self.should_refresh(&existing) {
            let refresh_token = existing
                .tokens
                .refresh_token
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("refresh token missing from auth file"))?;
            let refreshed = self.refresh_session(&existing, refresh_token).await?;
            return refreshed
                .tokens
                .access_token
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("refreshed access token missing"));
        }

        existing
            .tokens
            .access_token
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("access token missing from auth file"))
    }

    async fn status(&self) -> Result<AuthStatus> {
        let current = self
            .store
            .load()
            .context("failed to load the OpenAI auth session")?;
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

    async fn logout(&self) -> Result<()> {
        self.store
            .clear()
            .context("failed to remove the OpenAI auth session")?;
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

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

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

        let token = provider
            .ensure_access_token()
            .await
            .expect("token should refresh");
        assert_eq!(token, "new-access");

        let saved = provider
            .session_store()
            .load()
            .expect("load")
            .expect("present");
        assert_eq!(saved.tokens.access_token.as_deref(), Some("new-access"));
        assert_eq!(saved.tokens.refresh_token.as_deref(), Some("new-refresh"));
    }
}
