# AGENTS.md

## Mission

`claude-codex` is a Rust CLI wrapper for Claude Code that starts a local Anthropic-compatible proxy, injects the required environment variables, and forwards Claude Code traffic to an OpenAI-compatible backend.

The repository currently supports:

- `claude-codex` run mode
- `claude-codex auth login`
- `claude-codex auth status`
- `claude-codex auth logout`
- `claude-codex proxy serve`

The implementation uses `~/.codex/auth.json` for session storage and supports an OpenAI OAuth flow plus OpenAI-compatible API-key flows.

## Architecture

The codebase is a single Rust binary with a few clear layers:

- `src/main.rs` wires CLI parsing, auth, backend selection, proxy startup, and child process supervision.
- `src/cli.rs` defines the command contract.
- `src/config.rs` resolves local paths and runtime defaults.
- `src/auth/` owns auth state, persistence, and OAuth handling.
- `src/backend/` owns upstream request dispatch and backend routing.
- `src/protocol/` owns Anthropic/OpenAI translation and SSE bridging.
- `src/handlers/` exposes the HTTP endpoints used by Claude Code.
- `src/process.rs` launches `claude`, injects environment variables, and supervises the child process.
- `src/server.rs` builds the Axum router and readiness checks.
- `tests/` contains integration coverage for launcher behavior, proxy behavior, and run-script behavior.

## Important Entry Points

- `cargo run -- auth login`
- `cargo run -- auth status`
- `cargo run -- auth logout`
- `cargo run -- models list`
- `cargo run --`
- `cargo run -- proxy serve`
- `./run.sh test`

When the wrapper runs normally, it reserves a free loopback port, waits for the proxy to become healthy, then launches `claude` with:

- `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>`
- `ANTHROPIC_API_KEY=`
- `ANTHROPIC_AUTH_TOKEN=claude-codex-proxy`
- `CLAUDE_CODE_ATTRIBUTION_HEADER=0`

Model selection is backend-aware:

- Codex sessions default to `gpt-5.4`
- Chat Completions sessions default to `gpt-4o`
- `cargo run -- models list` prints the active backend catalog
- unsupported models must fail before `claude` is launched

## Working Conventions

- Keep code, comments, and documentation in English.
- Prefer small, targeted changes over broad refactors.
- Preserve the `~/.codex/auth.json` compatibility contract.
- Keep launcher environment behavior stable unless the change is explicitly about the wrapper contract.
- Treat `src/protocol/` as the translation boundary; keep request mapping logic there rather than scattering it through handlers or the launcher.
- Avoid changing unrelated files when working on a focused task.

## Where To Edit

- Launcher or process changes: `src/process.rs` and `src/main.rs`
- Model registry or backend-aware defaults: `src/models.rs`
- Auth changes: `src/auth/`
- Proxy routing or HTTP behavior: `src/server.rs` and `src/handlers/`
- Anthropic/OpenAI request mapping: `src/protocol/mapper.rs`
- Streaming behavior: `src/protocol/stream.rs` and `src/protocol/codex.rs`
- Backend routing and upstream requests: `src/backend/openai.rs`
- CLI parsing: `src/cli.rs`

## Verification Expectations

Before considering a change done, run the relevant checks:

- `cargo fmt --check`
- `cargo test`

For launcher or run-script work, `./run.sh test` is a convenient wrapper and should also stay green.

If a change touches the child process launcher, verify the proxy still starts, the child still receives the expected environment, and `claude` still launches cleanly.

## Safety Notes

- Do not assume planned model-selection work is already implemented unless it exists in the current code.
- Do not change the auth file format without preserving backward compatibility.
- Do not weaken the proxy readiness check unless there is a strong reason and a test to support it.
- Do not add fallback behavior between backend families unless the task explicitly asks for it.
- If a change affects Anthropic or OpenAI protocol shapes, add or update tests alongside the code.

## Current Behavior To Preserve

- OAuth/JWT credentials are routed through the Codex Responses API.
- `sk-` API keys continue to use Chat Completions.
- The proxy serves `POST /v1/messages`, `POST /v1/messages/count_tokens`, and `GET /healthz`.
- The wrapper stops the child process and proxy when interrupted.
- `auth status` reports the current session state from `~/.codex/auth.json`.
