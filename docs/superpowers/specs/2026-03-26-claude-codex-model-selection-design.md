# Claude Codex Model Selection Design

## Goal

Allow users to choose the backend model used by `claude-codex` instead of relying on a fixed hardcoded default.

For the first iteration:

- The user can keep using `--model <name>` in normal run mode.
- If the user does not pass `--model`, the default backend model must be `gpt-5.4`.
- `claude-codex` must validate the selected model against the active backend catalog before launching `claude`.
- `claude-codex models list` must print the models supported by the active backend.

This change is focused on model selection only. It does not add interactive prompts, persistent preferences, or backend override flags.

## Scope

Included:

- A model catalog abstraction in the Rust codebase.
- Codex model catalog as the first concrete catalog.
- Validation of `--model` during launch.
- A `models list` CLI subcommand.
- Clear error messages when the user selects an unsupported model.

Excluded:

- Interactive model picker.
- Persisting a preferred model to disk.
- User-configurable catalogs.
- Automatic online discovery of available models.

## User Experience

Normal usage:

- `claude-codex`
  - Launches Claude Code using backend model `gpt-5.4`.
- `claude-codex --model gpt-5.4-mini`
  - Launches Claude Code using `gpt-5.4-mini`.
- `claude-codex models list`
  - Prints the models available for the active backend.

If the user passes an unsupported model, `claude-codex` must fail before launching `claude` and show a message in this shape:

`unsupported model '<name>' for codex backend. Available models: ...`

## Architecture

Add a dedicated model registry module:

- `src/models.rs`

Responsibilities:

- Identify the backend kind relevant for model validation.
- Expose the default model for that backend.
- Expose the list of available models for that backend.
- Validate whether a model is supported.

Initial types:

- `BackendKind`
- `ModelCatalog`

Suggested interface:

- `backend_kind_for_token(access_token: &str) -> BackendKind`
- `default_model_for(backend: BackendKind) -> &'static str`
- `available_models_for(backend: BackendKind) -> &'static [&'static str]`
- `is_supported_model(backend: BackendKind, model: &str) -> bool`

For the first iteration, backend detection will follow the same rule already used by the backend router:

- OAuth/JWT token starting with `ey` -> `BackendKind::Codex`
- API key starting with `sk-` -> `BackendKind::ChatCompletions`

## Initial Catalogs

### Codex

The Codex catalog starts with these values:

- `gpt-5.4`
- `gpt-5.4-mini`
- `gpt-5.3-codex`
- `gpt-5.2-codex`
- `gpt-5.2`
- `gpt-5.1-codex-max`
- `gpt-5.1-codex-mini`

Default:

- `gpt-5.4`

### Chat Completions

The architecture must allow a separate Chat Completions catalog, but the first implementation may keep it minimal if current runtime behavior only needs Codex validation for the active OAuth path.

If a Chat Completions catalog is introduced immediately, it should be kept clearly separate from the Codex list.

## Launcher Flow

Run mode will change as follows:

1. Resolve the active access token through the auth provider.
2. Determine the active backend kind from that token.
3. Resolve the selected model:
   - `--model <name>` if present
   - otherwise `default_model_for(active_backend)`
4. Validate the model against the active backend catalog.
5. If valid, launch `claude` with that backend model in:
   - `--model`
   - `ANTHROPIC_DEFAULT_OPUS_MODEL`
   - `ANTHROPIC_DEFAULT_SONNET_MODEL`
   - `ANTHROPIC_DEFAULT_HAIKU_MODEL`
   - `CLAUDE_CODE_SUBAGENT_MODEL`
6. If invalid, return a descriptive error and do not launch `claude`.

This keeps the launcher behavior aligned with the existing Ollama-style environment injection while removing the hardcoded fixed model default.

## CLI Contract

Add a new command:

- `claude-codex models list`

Behavior:

- Resolve the active backend from the available auth token.
- Print one model per line for the active backend.
- Optionally annotate the default model, for example:
  - `gpt-5.4 (default)`

This command is informational only and does not mutate config or auth state.

## Error Handling

Failure cases:

- No valid auth session exists:
  - Same auth error path already used by run mode.
- Unsupported model:
  - Clear validation error listing supported models.
- Unknown token shape:
  - Return a descriptive backend detection error instead of guessing.

Errors must happen before launching the child `claude` process.

## Testing

### Unit Tests

- backend kind detection from token prefixes
- Codex default model is `gpt-5.4`
- Codex catalog contains the approved list
- model validation accepts supported models and rejects unsupported ones

### CLI Tests

- run mode without `--model` launches with `gpt-5.4`
- run mode with `--model gpt-5.4-mini` launches with that model
- invalid model fails before child launch
- `models list` prints the active backend catalog

## Implementation Notes

- Keep model lists in one module instead of scattering literals across launcher and backend code.
- Reuse the existing `--model` parsing in the launcher rather than introducing a second flag.
- Keep the first version deterministic and local. Do not fetch available models from the network.

## Spec Self-Review

Checked:

- No placeholders remain.
- The scope is limited to model selection and listing.
- The backend detection rule matches the current OAuth versus API-key routing logic.
- The default model is explicitly defined as `gpt-5.4` for the Codex backend.
