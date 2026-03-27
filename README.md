# claude-codex

```text
      ▟████████▙
     ▟█▛▀    ▀▀▀
    ▟█▛    ▐▛███▜▌
    ██▌    ▝▜█████▛▘
    ▜█▙     ▘▘ ▝▝
     ▜█▙▄    ▄▄▄
      ▜████████▛

`claude-codex` is a Rust wrapper for Claude Code that starts a local Anthropic-compatible proxy, injects the environment Claude Code expects, and translates requests to OpenAI-compatible backends.

It is designed to give Claude Code users a local launch experience while keeping the backend pluggable for future AI providers.

## Why claude-codex?

Claude Code is an incredible tool, but it's locked into a single provider. Many of us use a mix of models (like OpenAI) and want to keep using the superior Claude Code interface without being forced to switch contexts or tools.

claude-codex was born to bridge that gap: keep the UI you love, use the models you need.

## Current Status

The current implementation supports:

- A `claude` launcher that starts a local proxy on a free loopback port
- OpenAI OAuth session storage in `~/.codex/auth.json`
- `POST /v1/messages`, `POST /v1/messages/count_tokens`, and `GET /healthz`
- Translation between Anthropic message shapes and OpenAI-compatible chat requests
- Streaming translation for Claude Code requests that use `stream=true`
- A Codex Responses API path for OAuth/JWT-style OpenAI tokens
- A Chat Completions path for `sk-*` API keys

## Requirements

- Rust 2021
- `cargo`
- Claude Code installed as `claude` in `PATH`, or available at `~/.claude/local/claude`
- A valid OpenAI OAuth session in `~/.codex/auth.json` if you want to use the OAuth flow

## Build

```bash
cargo build
```

Run the full test suite:

```bash
cargo test
```

Or use the helper script:

```bash
./run.sh test
```

## Authentication

The auth file lives at `~/.codex/auth.json` and is reused if it already exists.

Available auth commands:

```bash
cargo run -- auth login
cargo run -- auth status
cargo run -- auth logout
```

`auth login` opens the browser, completes the OAuth flow, and stores the session locally. `auth status` reports whether the session is connected, whether a refresh token exists, and where the auth file lives.

## Usage

Run Claude Code through the wrapper:

```bash
cargo run -- --print hello
```

Choose a Codex reasoning effort explicitly:

```bash
cargo run -- --effort low --print hello
```

Start only the local proxy:

```bash
cargo run -- proxy serve
```

Use the helper script for the common paths:

```bash
./run.sh run --print hello
./run.sh auth-status
./run.sh proxy
```

List the models available for the active backend:

```bash
cargo run -- models list
```

When `claude-codex` launches Claude Code, it starts the local proxy, waits until it is ready, and then sets the environment Claude Code expects:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_API_KEY`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_ATTRIBUTION_HEADER`
- model tier defaults for Claude Code subagents

## Current Model Behavior

Model handling is backend-aware.

- For Codex-backed OAuth/JWT sessions, the launcher defaults to `gpt-5.4`.
- For Chat Completions API-key sessions, the launcher defaults to `gpt-4o`.
- If you pass `--model`, `claude-codex` validates it against the active backend catalog before launching `claude`.
- If you pass `--effort`, `claude-codex` validates it against the active backend before launching `claude`.
- `cargo run -- models list` prints the supported models for the active backend and marks the default.

Current effort behavior:

- `--effort` is supported only for Codex-backed OAuth/JWT sessions.
- Supported values are `low`, `medium`, and `high`.
- If omitted, Codex requests default to `medium`.
- Chat Completions sessions do not emulate effort levels and fail early if `--effort` is provided.

Current Codex optimization behavior:

- Codex requests default to `text.verbosity = low`.
- Codex request history is trimmed automatically before the Responses API call.
- The newest 8 non-system messages are preserved unchanged.
- Older text messages are capped to 1,200 characters.
- Older tool-result messages are capped to 600 characters.
- If the estimated prompt still exceeds the budget, the oldest non-system messages are dropped.
- Chat Completions requests do not use this optimization pass.
- Codex responses include optimization metrics in response headers:
  - `x-claude-codex-prompt-tokens-before`
  - `x-claude-codex-prompt-tokens-after`
  - `x-claude-codex-trimmed-messages`
  - `x-claude-codex-dropped-messages`
  - `x-claude-codex-trimmed-text-messages`
  - `x-claude-codex-trimmed-tool-results`

Current Codex catalog:

- `gpt-5.4`
- `gpt-5.4-mini`
- `gpt-5.3-codex`
- `gpt-5.2-codex`
- `gpt-5.2`
- `gpt-5.1-codex-max`
- `gpt-5.1-codex-mini`

## Architecture

The codebase is split into a few small pieces:

- `src/main.rs` wires auth, backend routing, the proxy, and the Claude Code launcher together.
- `src/auth/` handles the OpenAI OAuth session and the `~/.codex/auth.json` store.
- `src/backend/` decides whether the current token should use Chat Completions or the Codex Responses API.
- `src/protocol/` contains the request/response translation logic and SSE bridging.
- `src/handlers/` exposes the local HTTP endpoints.
- `src/process.rs` finds and launches `claude`, injects environment variables, and supervises the child process.

## Development

Useful files and commands:

```bash
./run.sh test
cargo fmt --check
cargo test
```

The repository also includes design notes under `docs/specs/` for the core proxy/auth work and model-selection behavior.

## Limitations

- The proxy is local only.
- `count_tokens` is a local estimate, not a provider tokenizer call.
- The project currently focuses on OpenAI-compatible backends and does not claim full Anthropic feature parity.
