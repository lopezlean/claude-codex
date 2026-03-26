# Claude Codex Documentation Design

## Goal

Add two top-level documentation files for the repository:

- `README.md` for human users
- `AGENTS.md` for generic AI coding agents

Both files must be written in English and reflect the actual current behavior of the project.

## Scope

Included:

- A user-facing `README.md`
- A generic repository-level `AGENTS.md`
- Documentation aligned with the current `claude-codex` implementation

Excluded:

- Provider-specific tutorials beyond the current OpenAI OAuth and Codex flow
- Deep protocol documentation for every internal type
- Auto-generated docs

## README Audience and Purpose

The `README.md` is for humans who want to understand, build, run, and troubleshoot `claude-codex`.

It should answer these questions quickly:

- What is `claude-codex`?
- What problem does it solve?
- How do I build and run it?
- How do I authenticate?
- How do I launch Claude Code through it?
- How do I choose a model?
- How do I run tests?
- What are the current limitations?

## README Structure

Recommended sections:

1. Title and short summary
2. What it does
3. Current status
4. Requirements
5. Build
6. Authentication
7. Usage
8. Model selection
9. Development
10. Testing
11. Limitations

### README Content Notes

- Keep the opening short and concrete.
- Document the main commands:
  - `cargo run -- auth login`
  - `cargo run -- auth status`
  - `cargo run --`
  - `cargo run -- proxy serve`
  - `./run.sh test`
- Explain that the tool reads `~/.codex/auth.json`.
- Explain that it starts a local proxy and launches `claude` with injected Anthropic-compatible environment variables.
- Mention the current default backend model behavior and model-selection support in terms consistent with the current implementation.
- Keep architecture coverage short and high-level.
- Call out important current limitations without overselling parity.

## AGENTS Audience and Purpose

The `AGENTS.md` file is for generic AI agents working in this repository.

It should provide enough operational context that an agent can make safe and relevant changes without reading the whole codebase first.

The file must not assume a specific assistant product. It should be useful for any coding agent.

## AGENTS Structure

Recommended sections:

1. Repository mission
2. Key architecture
3. Important entry points
4. Working conventions
5. Verification expectations
6. Common change map
7. Safety notes

### AGENTS Content Notes

The file should cover:

- The repository purpose: wrapper + local proxy translating Claude Code traffic
- Main modules and what each one owns
- Where to edit for:
  - launcher changes
  - auth changes
  - protocol mapping changes
  - streaming changes
  - backend routing changes
- Preferred verification commands:
  - `cargo fmt --check`
  - `cargo test`
- Repository expectations:
  - keep documentation, comments, and code in English
  - preserve the `~/.codex/auth.json` compatibility contract
  - avoid breaking the launcher environment contract with `claude`
  - keep Anthropic <-> backend mapping logic covered by tests
- Advice to prefer small targeted changes over broad refactors

## Tone and Style

Both files should be:

- English-only
- concise but useful
- practical
- accurate to the current repository state

The README should be slightly more explanatory.
The AGENTS file should be more operational and directive.

## Accuracy Constraints

The documentation must match the code as it exists when written.

Important examples:

- If the launcher defaults to a specific backend model, document that exact default.
- If model selection behavior is still evolving, describe current behavior without documenting future planned behavior as already implemented.
- Do not claim complete Anthropic parity if it does not exist.

## Validation

Before considering the documentation task complete:

- confirm both files exist at repository root
- review them for English-only wording
- ensure commands match the current CLI
- ensure descriptions match the implemented auth and backend behavior

## Spec Self-Review

Checked:

- No placeholders remain.
- Scope is limited to `README.md` and `AGENTS.md`.
- The audience split between humans and generic AI agents is explicit.
- The content requirements align with the current project contract and implementation state.
