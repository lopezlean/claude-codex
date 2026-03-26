use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fmt, sync::Arc};

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
const CALLBACK_HOST: &str = "127.0.0.1";

type BrowserOpener = Arc<dyn Fn(&str) -> Result<()> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_port: u16,
    pub callback_timeout_secs: u64,
    pub refresh_grace_period_secs: u64,
}

#[derive(Clone)]
pub struct OpenAiAuthProvider {
    client: Client,
    config: OpenAiAuthConfig,
    store: FileSessionStore,
    browser_opener: BrowserOpener,
}

impl OpenAiAuthProvider {
    pub fn new(config: OpenAiAuthConfig, store: FileSessionStore) -> Self {
        Self::new_with_browser_opener(config, store, Arc::new(default_browser_opener))
    }

    pub fn new_with_browser_opener(
        config: OpenAiAuthConfig,
        store: FileSessionStore,
        browser_opener: BrowserOpener,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("OpenAI auth HTTP client should build");
        Self {
            client,
            config,
            store,
            browser_opener,
        }
    }

    pub fn session_store(&self) -> &FileSessionStore {
        &self.store
    }

    fn callback_origin(&self) -> String {
        format!("http://{CALLBACK_HOST}:{}", self.config.redirect_port)
    }

    fn redirect_uri(&self) -> String {
        format!("{}/auth/callback", self.callback_origin())
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

impl fmt::Debug for OpenAiAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiAuthProvider")
            .field("config", &self.config)
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl AuthProvider for OpenAiAuthProvider {
    async fn login(&self) -> Result<()> {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let state = CsrfToken::new_random();
        let callback_listener =
            CallbackListener::bind(self.config.redirect_port, self.callback_origin())?;
        let auth_url = self.authorize_url(challenge.as_str(), state.secret())?;

        (self.browser_opener)(auth_url.as_str())?;

        let expected_state = state.secret().to_string();
        let callback_timeout_secs = self.config.callback_timeout_secs;
        let callback_task = tokio::task::spawn_blocking(move || {
            callback_listener.wait_for_code(&expected_state, callback_timeout_secs)
        });
        let code = callback_task
            .await
            .context("OpenAI callback listener task failed")??;
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

fn default_browser_opener(auth_url: &str) -> Result<()> {
    webbrowser::open(auth_url)
        .context("failed to open the OpenAI authorization page in a browser")?;
    Ok(())
}

struct CallbackListener {
    server: Server,
    callback_origin: String,
}

impl CallbackListener {
    fn bind(port: u16, callback_origin: String) -> Result<Self> {
        let server = Server::http((CALLBACK_HOST, port))
            .map_err(|error| anyhow!("failed to bind OpenAI callback server: {error}"))?;
        Ok(Self {
            server,
            callback_origin,
        })
    }

    fn wait_for_code(self, expected_state: &str, timeout_secs: u64) -> Result<String> {
        for _ in 0..timeout_secs {
            if let Ok(Some(request)) = self.server.recv_timeout(Duration::from_secs(1)) {
                let callback_url = format!("{}{}", self.callback_origin, request.url());
                let parsed = Url::parse(&callback_url).with_context(|| {
                    format!("failed to parse OAuth callback URL: {callback_url}")
                })?;

                if let Some((_, error)) = parsed.query_pairs().find(|(key, _)| key == "error") {
                    let response = failure_response("Authorization failed.");
                    let _ = request.respond(response);
                    bail!("OpenAI authorization failed: {error}");
                }

                let callback_state = parsed
                    .query_pairs()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.into_owned())
                    .ok_or_else(|| anyhow!("oauth callback missing state"))?;
                if callback_state != expected_state {
                    let response = failure_response("Authorization state mismatch.");
                    let _ = request.respond(response);
                    bail!("oauth callback state mismatch");
                }

                if let Some((_, code)) = parsed.query_pairs().find(|(key, _)| key == "code") {
                    let response = success_response();
                    let _ = request.respond(response);
                    return Ok(code.into_owned());
                }
            }
        }

        bail!("oauth callback timed out");
    }
}

fn success_response() -> Response<std::io::Cursor<Vec<u8>>> {
    html_response("<html><body><h1>claude-codex</h1><p>Authorization completed.</p></body></html>")
}

fn failure_response(message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    html_response(&format!(
        "<html><body><h1>claude-codex</h1><p>{message}</p></body></html>"
    ))
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
            .expect("static header should be valid"),
    )
}

impl fmt::Debug for CallbackListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackListener")
            .field("callback_origin", &self.callback_origin)
            .finish_non_exhaustive()
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::Mutex;

    use anyhow::{anyhow, Result};
    use tempfile::tempdir;
    use url::Url;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::auth::provider::AuthProvider;
    use crate::auth::session::{CodexAuthFile, CodexTokens};
    use crate::auth::session_store::FileSessionStore;

    use super::{OpenAiAuthConfig, OpenAiAuthProvider};

    static LOGIN_TEST_LOCK: Mutex<()> = Mutex::new(());

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

    #[test]
    fn callback_origin_and_redirect_uri_use_the_same_loopback_host() {
        let provider = test_provider(1455);

        assert_eq!(provider.callback_origin(), "http://127.0.0.1:1455");
        assert_eq!(
            provider.redirect_uri(),
            "http://127.0.0.1:1455/auth/callback"
        );
    }

    #[tokio::test]
    async fn login_accepts_an_immediate_callback_after_opening_the_browser() {
        let _guard = LOGIN_TEST_LOCK.lock().expect("lock login test mutex");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=fast-code"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    r#"{"access_token":"login-access","refresh_token":"login-refresh","id_token":"login-id"}"#,
                    "application/json",
                ),
            )
            .mount(&server)
            .await;

        let dir = tempdir().expect("temp dir");
        let auth_path = dir.path().join("auth.json");
        let store = FileSessionStore::new(auth_path);
        let callback_port = reserve_loopback_port();
        let opener = Arc::new(move |auth_url: &str| -> Result<()> {
            let auth_url = Url::parse(auth_url).expect("auth url should parse");
            let redirect_uri = auth_url
                .query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned())
                .expect("redirect_uri should be present");
            let state = auth_url
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned())
                .expect("state should be present");

            std::thread::spawn(move || send_callback(&redirect_uri, "fast-code", &state));
            Ok(())
        });

        let provider = OpenAiAuthProvider::new_with_browser_opener(
            OpenAiAuthConfig {
                client_id: "client-id".to_string(),
                auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                token_url: format!("{}/oauth/token", server.uri()),
                redirect_port: callback_port,
                callback_timeout_secs: 2,
                refresh_grace_period_secs: 60,
            },
            store,
            opener,
        );

        provider.login().await.expect("login should succeed");

        let saved = provider
            .session_store()
            .load()
            .expect("load")
            .expect("present");
        assert_eq!(saved.tokens.access_token.as_deref(), Some("login-access"));
        assert_eq!(saved.tokens.refresh_token.as_deref(), Some("login-refresh"));
        assert_eq!(saved.tokens.id_token.as_deref(), Some("login-id"));
    }

    #[tokio::test]
    async fn login_releases_the_callback_port_when_opening_the_browser_fails() {
        let _guard = LOGIN_TEST_LOCK.lock().expect("lock login test mutex");
        let dir = tempdir().expect("temp dir");
        let auth_path = dir.path().join("auth.json");
        let store = FileSessionStore::new(auth_path);
        let callback_port = reserve_loopback_port();
        let opener = Arc::new(move |_auth_url: &str| -> Result<()> {
            Err(anyhow!("synthetic browser failure"))
        });

        let provider = OpenAiAuthProvider::new_with_browser_opener(
            OpenAiAuthConfig {
                client_id: "client-id".to_string(),
                auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                token_url: "https://auth.openai.com/oauth/token".to_string(),
                redirect_port: callback_port,
                callback_timeout_secs: 2,
                refresh_grace_period_secs: 60,
            },
            store,
            opener,
        );

        let error = provider.login().await.expect_err("login should fail");
        assert!(
            error.to_string().contains("synthetic browser failure"),
            "unexpected error: {error}"
        );

        TcpListener::bind(("127.0.0.1", callback_port))
            .expect("callback port should be released when login fails");
    }

    fn test_provider(port: u16) -> OpenAiAuthProvider {
        let dir = tempdir().expect("temp dir");
        OpenAiAuthProvider::new(
            OpenAiAuthConfig {
                client_id: "client-id".to_string(),
                auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                token_url: "https://auth.openai.com/oauth/token".to_string(),
                redirect_port: port,
                callback_timeout_secs: 1,
                refresh_grace_period_secs: 60,
            },
            FileSessionStore::new(dir.path().join("auth.json")),
        )
    }

    fn reserve_loopback_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        port
    }

    fn send_callback(redirect_uri: &str, code: &str, state: &str) {
        let url = Url::parse(redirect_uri).expect("redirect uri should parse");
        let host = url.host_str().expect("host should be present");
        let port = url.port_or_known_default().expect("port should be present");
        let path = format!("{}?code={code}&state={state}", url.path());

        let mut stream =
            TcpStream::connect((host, port)).expect("callback listener should be accepting");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
        )
        .expect("callback request should write");
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
    }
}
