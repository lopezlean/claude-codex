# claude-codex

`claude-codex` is a Rust wrapper for Claude Code that starts a local Anthropic-compatible proxy, injects the environment Claude Code expects, and translates requests to OpenAI-compatible backends.

It is designed to give Claude Code users a local launch experience while keeping the backend pluggable for future AI providers.

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

When `claude-codex` launches Claude Code, it starts the local proxy, waits until it is ready, and then sets the environment Claude Code expects:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_API_KEY`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_ATTRIBUTION_HEADER`
- model tier defaults for Claude Code subagents

## Current Model Behavior

Model handling is intentionally simple today.

- If you do not pass `--model`, the launcher currently defaults to `gpt-5-codex-mini`.
- If you do pass `--model`, that value is forwarded to Claude Code and used for the backend model environment variables.
- The planned model catalog and backend-specific chooser are documented separately, but they are not implemented yet.

This means the launcher behaves like a thin wrapper today rather than a model picker.

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

The repository also includes design notes under `docs/superpowers/specs/` for the proxy, auth, docs, and model-selection work.

## Limitations

- The proxy is local only.
- Model selection is not catalog-driven yet.
- `count_tokens` is a local estimate, not a provider tokenizer call.
- The project currently focuses on OpenAI-compatible backends and does not claim full Anthropic feature parity.
