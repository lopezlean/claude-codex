# Claude Codex Design

Date: 2026-03-26
Status: Draft approved in conversation

## 1. Summary

`claude-codex` is a Rust binary that wraps the `claude` CLI, starts a local Anthropic-compatible proxy, injects the required environment variables, and translates Claude Code traffic to an OpenAI-compatible backend authenticated through OAuth.

The first implementation will support OpenAI OAuth and OpenAI-compatible chat endpoints, while keeping the project open to additional AI providers in later iterations.

## 2. Goals

- Provide a seamless local wrapper for `claude`.
- Start a local proxy on a random free loopback port.
- Inject `ANTHROPIC_BASE_URL` and `ANTHROPIC_API_KEY` for the child `claude` process.
- Translate Anthropic `/v1/messages` requests to OpenAI-compatible chat requests.
- Translate OpenAI-compatible responses back to Anthropic response shapes.
- Support streaming translation from OpenAI chunks to Anthropic SSE events.
- Reuse `~/.codex/auth.json` if it exists, and create it with the same structure if it does not exist.
- Implement OAuth in a provider-oriented way so new backends can be added later without rewriting the core.

## 3. Non-Goals

- Full parity with every Anthropic API feature in v1.
- A remote multi-user proxy service.
- Support for every Claude model alias in the first release.
- Provider-specific configuration UIs.
- Exact token accounting through an upstream tokenizer API for `/v1/messages/count_tokens`.

## 4. Recommended Approach

The project will use a provider-agnostic core with one initial concrete provider implementation:

- `AuthProvider`: login, refresh, status, logout, and access token retrieval.
- `BackendProvider`: model mapping, upstream base URL resolution, and request execution capabilities.
- `OpenAiAuthProvider`: first OAuth provider, adapted from the existing PKCE flow used in `ghost`.
- `OpenAiBackendProvider`: first backend provider for OpenAI-compatible chat endpoints.

This keeps the first version practical while preventing the codebase from becoming OpenAI-specific at the core.

## 5. Crate Shape

The repository starts as a single binary crate.

```text
src/
  main.rs
  cli.rs
  config.rs
  error.rs
  server.rs
  process.rs
  handlers/
    messages.rs
    count_tokens.rs
    health.rs
  protocol/
    anthropic.rs
    openai.rs
    mapper.rs
    stream.rs
  auth/
    mod.rs
    provider.rs
    session.rs
    session_store.rs
    openai.rs
  backend/
    mod.rs
    provider.rs
    openai.rs
tests/
```

Responsibilities:

- `main.rs`: top-level entrypoint and error reporting.
- `cli.rs`: `clap` command definitions and argument parsing.
- `config.rs`: runtime configuration, provider selection, model defaults, file paths, and callback settings.
- `server.rs`: Axum router construction and shared application state.
- `process.rs`: launching and supervising the `claude` child process.
- `handlers/`: HTTP endpoints.
- `protocol/`: request and response types plus mapping logic.
- `auth/`: auth file model, session persistence, provider auth trait, and OpenAI PKCE implementation.
- `backend/`: provider-facing backend abstraction and OpenAI backend adapter.

## 6. Command Contract

Primary mode:

- `claude-codex [claude args...]`

Support commands:

- `claude-codex auth login`
- `claude-codex auth status`
- `claude-codex auth logout`
- `claude-codex proxy serve`

Behavior of the primary mode:

1. Load configuration and resolve the active provider. OpenAI is the default in v1.
2. Ensure a valid access token exists.
3. Bind a local listener on `127.0.0.1:0` and capture the assigned free port.
4. Start the Axum proxy on that port.
5. Launch `claude` with:
   - `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>/v1`
   - `ANTHROPIC_API_KEY=sk-ant-codex-proxy`
6. Forward stdin, stdout, and stderr through the child process.
7. Stop the proxy when the child exits or the wrapper receives an interrupt signal.

`proxy serve` starts the server without launching `claude`. This is primarily for tests and debugging.

## 7. Authentication Design

### 7.1 Auth File Compatibility

The default auth file path is:

- `~/.codex/auth.json`

If the file already exists, `claude-codex` will read and reuse it. If it does not exist, `claude-codex auth login` will create it using the same structure.

Observed compatible shape:

```json
{
  "auth_mode": "string",
  "tokens": {
    "id_token": "string",
    "access_token": "string",
    "refresh_token": "string",
    "account_id": "string"
  },
  "last_refresh": "string"
}
```

The implementation will model this format explicitly:

- `CodexAuthFile`
- `CodexAuthTokens`

The reader should tolerate missing optional fields and only fail when required data for the requested operation is absent.

### 7.2 Auth Abstractions

- `AuthProvider`: provider-facing interface for login, refresh, status, logout, and token retrieval.
- `SessionStore`: atomic loading and saving of the auth file.
- `OpenAiAuthProvider`: first implementation using PKCE OAuth, local callback capture, persisted refresh token, and access-token refresh.

### 7.3 OpenAI OAuth Flow

The initial provider adapts the existing pattern from `ghost`:

- PKCE challenge and verifier generation.
- Browser open to the OpenAI authorization URL.
- Local callback server on a configurable port.
- Authorization code exchange for access and refresh tokens.
- Refresh when the token is expiring soon.

The callback port is configurable. The proxy port is always random and free.

## 8. HTTP Proxy Contract

### 8.1 Endpoints

- `POST /v1/messages`
- `POST /v1/messages/count_tokens`
- `GET /healthz`

### 8.2 `POST /v1/messages`

The proxy accepts Anthropic-style request payloads and converts them into OpenAI-compatible chat requests.

Flow:

1. Parse Anthropic request.
2. Map Claude model names to provider models.
3. Convert system, user, assistant, tool use, and tool result content into OpenAI-compatible request messages.
4. Attach the OAuth access token as bearer authentication.
5. Forward the request upstream.
6. Convert the upstream response back into an Anthropic-compatible response body.

### 8.3 `POST /v1/messages/count_tokens`

The first version uses a deterministic local estimator instead of an upstream tokenizer endpoint. This keeps the wrapper simple and avoids a hard dependency on provider-specific token counting APIs.

The estimator will:

- Walk the Anthropic message structure.
- Count textual content and tool metadata in a stable way.
- Return a consistent estimate suitable for Claude Code compatibility.

This endpoint can later be upgraded to a provider-specific implementation without changing the CLI contract.

### 8.4 `GET /healthz`

Returns `200 OK` with a small success body for local health checks and integration tests.

## 9. Model Mapping

Initial default mapping:

- `claude-3-5-sonnet-*` -> `gpt-4o`
- `claude-3-5-haiku-*` -> `gpt-4o-mini`

The mapping layer must support future overrides from configuration so a Codex-specific model or another compatible model can replace the defaults without changing source code.

Unknown Claude model identifiers fall back to the provider default model and emit a warning through structured logs.

## 10. Protocol Mapping

### 10.1 Anthropic to OpenAI

The mapper will support these content concepts:

- Anthropic `system` -> OpenAI `system`
- Anthropic text blocks -> standard text content
- Anthropic assistant `tool_use` blocks -> OpenAI assistant `tool_calls`
- Anthropic user `tool_result` blocks -> OpenAI `tool` messages tied to the matching tool call identifier

The mapper must be implemented as pure translation logic in `protocol/mapper.rs` so it can be unit-tested independently of Axum and the network stack.

### 10.2 OpenAI to Anthropic

The response mapper will convert:

- assistant text output into Anthropic text content blocks
- tool calls into Anthropic `tool_use` blocks
- metadata into Anthropic-compatible top-level fields where practical for v1

The first implementation prioritizes preserving content and tool behavior over reproducing every optional Anthropic metadata field.

## 11. Streaming Design

Streaming is required for `POST /v1/messages` when the request sets `stream=true`.

The upstream OpenAI-compatible stream will be consumed chunk by chunk and re-encoded as Anthropic-style SSE events. The synthesized event set will include:

- `message_start`
- `content_block_start`
- `content_block_delta`
- `content_block_stop`
- `message_delta`
- `message_stop`

The v1 design prioritizes:

- no content loss
- no tool call loss
- correct ordering
- valid SSE framing

Exact chunk granularity parity with native Anthropic streaming is not required in the first release.

## 12. Error Handling

The codebase will use `thiserror` or `anyhow` for descriptive failures, with explicit translation at layer boundaries.

Important error cases:

- `claude` binary missing from `PATH`
- OAuth browser open failure
- callback timeout or user cancellation
- callback port already in use
- missing or invalid auth file
- failed token refresh
- upstream HTTP failure
- invalid upstream JSON or malformed stream chunks
- proxy startup failure

Errors exposed to the CLI should be actionable. Errors returned through the HTTP proxy should preserve relevant upstream status information without leaking internal implementation detail unnecessarily.

## 13. Testing Strategy

The project follows strict TDD during implementation.

### 13.1 Unit Tests

- Anthropic request to OpenAI request mapping
- OpenAI response to Anthropic response mapping
- model alias resolution
- auth file parsing and persistence behavior
- local token estimation for `/v1/messages/count_tokens`

### 13.2 Integration Tests

Use `wiremock` to simulate the upstream OpenAI-compatible backend:

- non-stream request forwarding
- non-stream response translation
- upstream error propagation
- expired token refresh before request forwarding
- proxy startup and health endpoint behavior

### 13.3 Streaming Tests

- SSE framing is valid
- chunk order is preserved
- text deltas are preserved
- tool call data is preserved
- stream termination emits the expected stop events

## 14. Initial Dependencies

- `axum`
- `tokio`
- `reqwest`
- `serde`
- `serde_json`
- `clap`
- `thiserror` or `anyhow`
- `tracing`
- `tracing-subscriber`
- `oauth2`
- `url`
- `wiremock`

The OpenAI OAuth implementation may also reuse a minimal local callback HTTP server dependency if it remains the most practical option.

## 15. Implementation Constraints

- All code, comments, and documentation must be written in English.
- The implementation must avoid Python dependencies such as LiteLLM.
- The provider architecture must remain open to additional AI backends.
- The auth file contract must remain compatible with `~/.codex/auth.json`.
- The default proxy bind address is loopback only.

## 16. Open Questions Resolved in This Design

- Proxy port selection: random free local port.
- Auth persistence: use `~/.codex/auth.json` if present, otherwise create it.
- OAuth source pattern: adapt the existing OpenAI PKCE flow from `ghost`.
- Extensibility: design the core around provider abstractions from day one.

## 17. Next Step

After review and approval of this design document, the next step is to create a written implementation plan and only then begin TDD-driven implementation.
