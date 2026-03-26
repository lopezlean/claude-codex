export const site = {
  brand: "claude-codex",
  repoUrl: "https://github.com/lopezlean/claude-codex",
  docsUrl: "https://github.com/lopezlean/claude-codex/blob/main/docs/specs/claude-codex.md",
  modelSpecUrl:
    "https://github.com/lopezlean/claude-codex/blob/main/docs/specs/claude-codex-model-selection.md",
  hero: {
    eyebrow: "Rust wrapper for Claude Code",
    title: "Keep Claude Code. Route the backend where you need it.",
    description:
      "claude-codex starts a local Anthropic-compatible proxy, reuses auth from ~/.codex/auth.json, validates backend-aware models, and launches Claude Code against OpenAI-compatible APIs."
  },
  navLinks: [
    { label: "Features", href: "#features" },
    { label: "Build", href: "#build" },
    { label: "Models", href: "#models" },
    { label: "Architecture", href: "#architecture" }
  ],
  heroTerminal: [
    "$ cargo run -- auth status",
    "provider=openai connected=true has_refresh_token=true auth_path=~/.codex/auth.json",
    "$ cargo run -- models list",
    "gpt-5.4 (default)",
    "gpt-5.4-mini",
    "gpt-5.3-codex",
    "$ cargo run -- --print hello",
    "ANTHROPIC_BASE_URL=http://127.0.0.1:43127"
  ],
  features: [
    {
      title: "Local proxy bridge",
      body:
        "Starts a loopback proxy before Claude Code launches, waits for /healthz, and points Claude traffic at the local bridge."
    },
    {
      title: "Session reuse",
      body:
        "Keeps compatibility with ~/.codex/auth.json and exposes auth login, status, and logout instead of inventing a new session store."
    },
    {
      title: "Protocol translation",
      body:
        "Maps Anthropic-style requests and streaming responses toward OpenAI-compatible endpoints while keeping the Claude Code UX intact."
    },
    {
      title: "Backend-aware defaults",
      body:
        "JWT-like Codex sessions default to gpt-5.4, while sk-* API keys use Chat Completions and default to gpt-4o."
    }
  ],
  buildSteps: [
    { label: "Run", command: "./run.sh" },
    { label: "Build", command: "cargo build" },
    { label: "Run tests", command: "cargo test" },
    { label: "Wrapper test helper", command: "./run.sh test" }
  ],
  usageCommands: [
    "cargo run -- auth login",
    "cargo run -- auth status",
    "cargo run -- auth logout",
    "cargo run -- models list",
    "cargo run --",
    "cargo run -- proxy serve"
  ],
  modelGroups: [
    {
      name: "Codex sessions",
      defaultModel: "gpt-5.4",
      detail: "Selected when the access token looks like an OAuth/JWT-style Codex session.",
      models: ["gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex", "gpt-5.2-codex", "gpt-5.2"]
    },
    {
      name: "Chat Completions",
      defaultModel: "gpt-4o",
      detail: "Selected for sk-* API keys using the chat completions path.",
      models: ["gpt-4o", "gpt-4o-mini"]
    }
  ],
  architecture: [
    {
      path: "src/auth/",
      title: "Auth state and OAuth flow",
      body:
        "Loads, refreshes, stores, and reports OpenAI session state from the Codex-compatible auth file."
    },
    {
      path: "src/backend/",
      title: "Backend routing",
      body:
        "Chooses the upstream request path and dispatches requests to Chat Completions or Codex Responses."
    },
    {
      path: "src/protocol/",
      title: "Translation boundary",
      body:
        "Converts Anthropic-shaped requests and streaming events into the OpenAI-compatible forms the upstream expects."
    },
    {
      path: "src/handlers/",
      title: "HTTP surface",
      body:
        "Serves /v1/messages, /v1/messages/count_tokens, and /healthz for the local Claude Code proxy."
    },
    {
      path: "src/process.rs",
      title: "Claude launcher",
      body:
        "Finds the claude binary, injects the required environment variables, and supervises child lifecycle."
    },
    {
      path: "src/main.rs",
      title: "Application orchestration",
      body:
        "Wires CLI parsing, auth, model resolution, proxy readiness checks, and command dispatch together."
    }
  ],
  footerLinks: [
    { label: "GitHub", href: "https://github.com/lopezlean/claude-codex" },
    {
      label: "Core spec",
      href: "https://github.com/lopezlean/claude-codex/blob/main/docs/specs/claude-codex.md"
    },
    {
      label: "Model selection",
      href: "https://github.com/lopezlean/claude-codex/blob/main/docs/specs/claude-codex-model-selection.md"
    }
  ]
} as const;
