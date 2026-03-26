# Claude Codex Model Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add backend-aware model selection so `claude-codex` defaults to `gpt-5.4`, validates requested models against a Codex catalog, and exposes `claude-codex models list`.

**Architecture:** Introduce a dedicated `src/models.rs` registry for backend detection and model catalogs, resolve the active backend from the access token before launching `claude`, and keep launcher environment injection unchanged except for the validated backend model value. Extend the CLI with a `models list` path and cover the new behavior with unit and integration tests.

**Tech Stack:** Rust 2021, tokio, axum, reqwest, serde, assert_cmd

---

### Task 1: Add failing tests for the model registry

**Files:**
- Create: `src/models.rs`

- [x] **Step 1: Write the failing test**

Add tests for:
- `backend_kind_for_token("ey...") == Codex`
- `backend_kind_for_token("sk-...") == ChatCompletions`
- Codex default model is `gpt-5.4`
- Codex catalog contains the approved list
- unsupported Codex model is rejected

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test models::tests -- --nocapture`
Expected: FAIL because `src/models.rs` does not exist yet.

- [x] **Step 3: Write minimal implementation**

Implement `BackendKind`, backend detection, default model lookup, available models lookup, and support validation.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test models::tests -- --nocapture`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/models.rs src/main.rs
git commit -m "feat: add backend model registry"
```

### Task 2: Add failing tests for CLI parsing and launcher behavior

**Files:**
- Modify: `src/cli.rs`
- Modify: `tests/cli_wrapper.rs`

- [x] **Step 1: Write the failing test**

Add tests for:
- parsing `claude-codex models list`
- default launcher model becomes `gpt-5.4`
- explicit `--model gpt-5.4-mini` still works
- invalid model fails before child launch
- `models list` prints the Codex catalog for an OAuth/JWT auth session

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test cli::tests::parses_models_list_command -- --nocapture`
Run: `cargo test --test cli_wrapper -- --nocapture`
Expected: FAIL due to missing command and old launcher default.

- [x] **Step 3: Write minimal implementation**

Extend the CLI enum and parsing logic, then update runtime behavior to support `models list` and validated model selection.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test cli::tests::parses_models_list_command -- --nocapture`
Run: `cargo test --test cli_wrapper -- --nocapture`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/cli.rs src/process.rs src/main.rs tests/cli_wrapper.rs
git commit -m "feat: validate and list backend models"
```

### Task 3: Update docs to reflect implemented behavior

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`

- [x] **Step 1: Write the failing test**

No automated docs test is required, but review the current wording that still says the default model is `gpt-5-codex-mini` and that the catalog is not implemented yet.

- [x] **Step 2: Run verification to confirm mismatch**

Run: `rg -n "gpt-5-codex-mini|not implemented yet|model catalog" README.md AGENTS.md`
Expected: matches showing stale docs.

- [x] **Step 3: Write minimal implementation**

Update the docs so they describe:
- default `gpt-5.4`
- `claude-codex models list`
- backend-aware model validation

- [x] **Step 4: Run verification to verify docs are updated**

Run: `rg -n "gpt-5.4|models list" README.md AGENTS.md`
Expected: matches for the new behavior only.

- [x] **Step 5: Commit**

```bash
git add README.md AGENTS.md
git commit -m "docs: describe model selection behavior"
```

### Task 4: Final verification

**Files:**
- Modify: none

- [x] **Step 1: Run formatting**

Run: `cargo fmt --check`
Expected: PASS

- [x] **Step 2: Run full test suite**

Run: `cargo test`
Expected: PASS

- [x] **Step 3: Inspect repo state**

Run: `git status --short`
Expected: clean working tree
