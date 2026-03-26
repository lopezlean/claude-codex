# Claude Codex landing design

## Goal
Create a public GitHub Pages landing site for `claude-codex` using Astro, with source files under `pages/`, visual direction inspired by the provided mockup and the Beam site setup, while keeping all technical copy aligned with the real repository.

## Constraints
- Use Astro for the site.
- Keep the site in `pages/`, not `docs/`.
- Reuse the deployment approach from Beam’s GitHub Pages workflow.
- Keep the site as a landing page only, not a docs portal.
- Use semistatic content stored in site data files, not Rust source parsing at build time.
- Correct any inaccuracies from the original mockup so public content matches the current repo.

## User-approved direction
Recommended approach: build a small Astro app in `pages/` with a single page entrypoint, a handful of focused presentation components, and a shared data module for commands, models, architecture cards, links, and copy.

Why this approach:
- keeps the public site clearly separated from internal specs
- makes GitHub Pages deployment straightforward
- preserves the visual design while avoiding content drift from hard-coded mockup text embedded across components
- stays small enough to maintain without introducing a custom content pipeline

## Information the landing must reflect
### Product summary
`claude-codex` is a Rust CLI wrapper for Claude Code that:
- starts a local Anthropic-compatible proxy
- launches the `claude` binary with environment variables pointed at that proxy
- translates request/streaming traffic toward an OpenAI-compatible backend
- reuses auth state from `~/.codex/auth.json`

### Public behaviors to highlight
- local proxy startup before launching Claude Code
- auth reuse through `auth login`, `auth status`, and `auth logout`
- backend-aware model selection
- Codex-style sessions defaulting to `gpt-5.4`
- Chat Completions sessions defaulting to `gpt-4o`
- `models list` showing the active backend catalog

### CLI commands to show
Landing examples must use real commands from the repo:
- `cargo run -- auth login`
- `cargo run -- auth status`
- `cargo run -- auth logout`
- `cargo run -- models list`
- `cargo run --`
- `cargo run -- proxy serve`

### Architecture blocks to show
Use current repo structure, not placeholder paths:
- `src/auth/`
- `src/backend/`
- `src/protocol/`
- `src/handlers/`
- `src/process.rs`
- `src/main.rs`

### Model groups to show
Represent current families semistatically from the repo state:
- Codex family including `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.3-codex`
- Chat Completions family including `gpt-4o`, `gpt-4o-mini`

## Corrections required from the mockup
The implementation must not repeat these inaccurate details:
- `src/stream/` → use `src/protocol/stream.rs` or the broader `src/protocol/`
- `src/backends/` → use `src/backend/`
- fake or stale model IDs → replace with current models from the repo
- unsupported provider claims such as OpenAI/Ollama/LocalAI/Groq router messaging → replace with current OpenAI-compatible backend wording only
- invented build/runtime contract details → replace with actual CLI examples and launcher behavior

## Site structure
### Files
Create a minimal Astro app under `pages/` with:
- `pages/package.json`
- `pages/astro.config.mjs`
- `pages/tsconfig.json`
- `pages/src/env.d.ts`
- `pages/src/pages/index.astro`
- `pages/src/components/*`
- `pages/src/data/site.ts`

### Components
The page should be split into small presentational sections:
- `TopNav.astro`
- `HeroSection.astro`
- `FeaturesSection.astro`
- `BuildSection.astro`
- `ModelsSection.astro`
- `ArchitectureSection.astro`
- `FooterSection.astro`

A shared layout file is optional; if used, keep it minimal.

### Data source
Store semistatic content in `pages/src/data/site.ts`, including:
- repository links
- nav links
- hero text
- feature cards
- command examples
- model groups
- architecture cards
- footer links

This keeps copy centralized and easy to update when the repo changes.

## Content design
### Top navigation
Include:
- project brand
- anchor links to major sections
- primary GitHub CTA

### Hero
Communicate the real value proposition:
- Rust wrapper for Claude Code
- local Anthropic-compatible proxy
- OpenAI-compatible backend bridge
- auth + model routing behavior

Include a terminal-style panel using real commands and realistic outputs, not fictional provider matrices.

### Features
Highlight four core capabilities:
- local proxy bridge
- session reuse
- protocol translation
- backend-aware defaults

### Build and usage
Show realistic getting-started and usage commands. Keep this section grounded in the repo’s current CLI contract rather than environment variable exposition.

### Models
Show the supported model families and the default behavior by backend type.

### Architecture
Show repo areas and short descriptions tied to actual responsibilities in the current codebase.

### Footer CTA
Provide:
- GitHub link
- docs/specs link if useful
- license/community style links as appropriate for the repo

## Styling direction
- Follow the overall dark, terminal-inspired aesthetic from the provided mockup.
- Reuse the lightweight Beam Astro structure as a setup reference, not as a literal content copy.
- Prefer self-contained Astro component markup and CSS over extra frontend tooling.
- Keep the site static and dependency-light.

## Deployment
Create `.github/workflows/deploy-pages.yml` adapted from Beam so it:
- builds the Astro site from `./pages`
- uploads the Astro build artifact
- deploys to GitHub Pages on the main branch

The workflow must target the `pages/` app path rather than `docs/`.

## Verification
Before claiming completion:
- install the site dependencies in `pages/`
- run the Astro build successfully
- confirm section content matches the repo’s current CLI/models/architecture
- confirm the workflow points at `pages/`

## Out of scope
- auto-generating site content by parsing Rust files
- adding a docs portal, blog, or search
- changing the Rust wrapper behavior itself
- adding backend claims not supported by the current implementation


# DESIGN CODE TO EXTRACT INTO ASTRO FILES:
<!DOCTYPE html>

<html class="dark" lang="en"><head>
<meta charset="utf-8"/>
<meta content="width=device-width, initial-scale=1.0" name="viewport"/>
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@300..700&amp;family=Inter:wght@300..700&amp;family=JetBrains+Mono:wght@400;700&amp;display=swap" rel="stylesheet"/>
<link href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:wght@100..700,0..1&amp;display=swap" rel="stylesheet"/>
<link href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:wght,FILL@100..700,0..1&amp;display=swap" rel="stylesheet"/>
<script src="https://cdn.tailwindcss.com?plugins=forms,container-queries"></script>
<script id="tailwind-config">
    tailwind.config = {
      darkMode: "class",
      theme: {
        extend: {
          colors: {
            "inverse-surface": "#dfe2eb",
            "outline-variant": "#5c4038",
            "surface": "#10141a",
            "secondary": "#a2c9ff",
            "on-tertiary-fixed-variant": "#004689",
            "primary-container": "#ff5717",
            "on-background": "#dfe2eb",
            "tertiary": "#a8c8ff",
            "surface-dim": "#10141a",
            "secondary-fixed": "#d3e4ff",
            "surface-container-high": "#262a31",
            "tertiary-fixed-dim": "#a8c8ff",
            "on-secondary-container": "#f0f4ff",
            "error": "#ffb4ab",
            "on-surface-variant": "#e5beb2",
            "surface-container-highest": "#31353c",
            "on-error": "#690005",
            "inverse-primary": "#ad3300",
            "on-primary-fixed-variant": "#842500",
            "primary-fixed-dim": "#ffb59e",
            "surface-container-lowest": "#0a0e14",
            "secondary-container": "#0071c7",
            "inverse-on-surface": "#2d3137",
            "on-tertiary-fixed": "#001b3c",
            "surface-tint": "#ffb59e",
            "outline": "#ac897e",
            "on-primary-container": "#521300",
            "surface-variant": "#31353c",
            "on-primary": "#5e1700",
            "error-container": "#93000a",
            "primary": "#ffb59e",
            "on-surface": "#dfe2eb",
            "on-secondary-fixed-variant": "#004882",
            "on-secondary-fixed": "#001c38",
            "tertiary-container": "#3491ff",
            "on-primary-fixed": "#3a0b00",
            "surface-container-low": "#181c22",
            "on-secondary": "#00315c",
            "surface-container": "#1c2026",
            "primary-fixed": "#ffdbd0",
            "on-error-container": "#ffdad6",
            "tertiary-fixed": "#d5e3ff",
            "secondary-fixed-dim": "#a2c9ff",
            "on-tertiary-container": "#002955",
            "surface-bright": "#353940",
            "background": "#10141a",
            "on-tertiary": "#003061"
          },
          fontFamily: {
            "headline": ["Space Grotesk", "sans-serif"],
            "body": ["Inter", "sans-serif"],
            "label": ["Inter", "sans-serif"],
            "mono": ["JetBrains Mono", "monospace"]
          },
          borderRadius: {"DEFAULT": "0.125rem", "lg": "0.25rem", "xl": "0.5rem", "full": "0.75rem"},
        },
      },
    }
  </script>
<style>
    .kinetic-grid {
      background-image: linear-gradient(to right, rgba(92, 64, 56, 0.05) 1px, transparent 1px),
                        linear-gradient(to bottom, rgba(92, 64, 56, 0.05) 1px, transparent 1px);
      background-size: 24px 24px;
    }
    .terminal-glow {
      box-shadow: 0 0 20px rgba(255, 87, 23, 0.1);
    }
    .glow-accent {
      filter: drop-shadow(0 0 8px rgba(255, 181, 158, 0.3));
    }
  </style>
</head>
<body class="bg-surface text-on-surface font-body selection:bg-primary-container selection:text-on-primary-container">
<!-- Shared TopNavBar -->
<div class="relative flex h-auto min-h-screen w-full flex-col bg-surface overflow-x-hidden">
<div class="layout-container flex h-full grow flex-col">
<div class="px-4 md:px-20 lg:px-40 flex flex-1 justify-center py-5 kinetic-grid">
<div class="layout-content-container flex flex-col max-w-[1200px] flex-1">
<!-- Header -->
<header class="flex items-center justify-between whitespace-nowrap border-b border-solid border-outline-variant/15 px-4 md:px-10 py-6 backdrop-blur-md sticky top-0 z-50">
<div class="flex items-center gap-4 text-primary">
<div class="size-8 glow-accent">
<svg fill="none" viewbox="0 0 48 48" xmlns="http://www.w3.org/2000/svg">
<path d="M36.7273 44C33.9891 44 31.6043 39.8386 30.3636 33.69C29.123 39.8386 26.7382 44 24 44C21.2618 44 18.877 39.8386 17.6364 33.69C16.3957 39.8386 14.0109 44 11.2727 44C7.25611 44 4 35.0457 4 24C4 12.9543 7.25611 4 11.2727 4C14.0109 4 16.3957 8.16144 17.6364 14.31C18.877 8.16144 21.2618 4 24 4C26.7382 4 29.123 8.16144 30.3636 14.31C31.6043 8.16144 33.9891 4 36.7273 4C40.7439 4 44 12.9543 44 24C44 35.0457 40.7439 44 36.7273 44Z" fill="currentColor"></path>
</svg>
</div>
<h2 class="text-on-surface text-xl font-headline font-bold leading-tight tracking-[-0.015em]">claude-codex</h2>
</div>
<div class="hidden md:flex flex-1 justify-end gap-8">
<nav class="flex items-center gap-9">
<a class="text-on-surface-variant hover:text-primary transition-colors text-sm font-medium leading-normal" href="#features">Features</a>
<a class="text-on-surface-variant hover:text-primary transition-colors text-sm font-medium leading-normal" href="#build">Build</a>
<a class="text-on-surface-variant hover:text-primary transition-colors text-sm font-medium leading-normal" href="#models">Models</a>
<a class="text-on-surface-variant hover:text-primary transition-colors text-sm font-medium leading-normal" href="#arch">Architecture</a>
</nav>
<button class="flex min-w-[100px] cursor-pointer items-center justify-center overflow-hidden rounded-lg h-10 px-5 bg-primary-container text-on-primary-container text-sm font-bold leading-normal tracking-[0.015em] hover:brightness-110 transition-all active:scale-95 shadow-[0_0_15px_rgba(255,87,23,0.3)]">
<span class="truncate">GitHub</span>
</button>
</div>
</header>
<!-- Hero Section -->
<section class="py-16 md:py-24 grid grid-cols-1 lg:grid-cols-2 gap-12 items-center px-4">
<div class="flex flex-col gap-8">
<div class="flex flex-col gap-4">
<h1 class="text-on-surface text-5xl md:text-7xl font-headline font-bold leading-tight tracking-[-0.033em]">
                  Claude Code, <span class="text-primary-container">Your Way.</span>
</h1>
<p class="text-on-surface-variant text-lg md:text-xl font-normal leading-relaxed max-w-[540px]">
                  A high-performance Rust wrapper to bridge Claude Code with OpenAI-compatible backends. Unlocking speed and versatility for local workflows.
                </p>
</div>
<div class="flex gap-4">
<button class="flex min-w-[140px] cursor-pointer items-center justify-center rounded-lg h-14 px-8 bg-primary-container text-on-primary-container text-base font-bold leading-normal hover:shadow-[0_0_20px_rgba(255,87,23,0.4)] transition-all">
                  Get Started
                </button>
<button class="flex min-w-[140px] cursor-pointer items-center justify-center rounded-lg h-14 px-8 border border-outline-variant/30 text-on-surface text-base font-bold leading-normal hover:bg-surface-container-high transition-all">
                  Documentation
                </button>
</div>
</div>
<!-- Terminal Component -->
<div class="w-full h-80 bg-surface-container-lowest rounded-xl border border-outline-variant/20 overflow-hidden terminal-glow flex flex-col font-mono text-sm relative">
<div class="h-10 bg-surface-container-low flex items-center px-4 gap-2 border-b border-outline-variant/10">
<div class="size-3 rounded-full bg-error/40"></div>
<div class="size-3 rounded-full bg-primary/40"></div>
<div class="size-3 rounded-full bg-secondary/40"></div>
<span class="ml-4 text-on-surface-variant/50 text-xs">~/projects/claude-codex</span>
</div>
<div class="p-6 flex flex-col gap-2 overflow-y-auto">
<div class="flex gap-2">
<span class="text-primary-container">$</span>
<span class="text-on-surface">cargo run -- --print hello</span>
</div>
<div class="text-on-surface-variant/60 animate-pulse">... Building dependencies</div>
<div class="text-secondary">Finished dev [unoptimized + debuginfo] target(s) in 0.24s</div>
<div class="text-on-surface">Running `target/debug/claude-codex --print hello`</div>
<div class="text-on-surface-variant mt-2 border-l-2 border-primary-container pl-4 italic">
                  "Hello, Codex. Bridge established. Ready to proxy 403 Forbidden requests to local-llm:8080."
                </div>
<div class="flex gap-2 mt-4">
<span class="text-primary-container">$</span>
<span class="w-2 h-5 bg-primary animate-bounce"></span>
</div>
</div>
</div>
</section>
<!-- Why Section -->
<section class="py-20 border-t border-outline-variant/10 px-4" id="features">
<div class="mb-16">
<h2 class="text-primary text-sm font-mono tracking-widest uppercase mb-4">Core Philosophy</h2>
<h3 class="text-on-surface text-4xl font-headline font-bold mb-6 max-w-2xl">
                Keep the UI you love, use the models you need.
              </h3>
<p class="text-on-surface-variant text-lg max-w-2xl">
                We believe in tool freedom. Claude Code is a masterful interface; we provide the piping to connect it to any OpenAI-compatible infrastructure without compromising performance.
              </p>
</div>
<!-- Feature Grid -->
<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
<div class="bg-surface-container-low p-8 rounded-xl border-t-2 border-primary-container/20 hover:border-primary-container transition-all group">
<div class="text-primary mb-6 group-hover:scale-110 transition-transform duration-300">
<span class="material-symbols-outlined text-4xl" data-icon="terminal">terminal</span>
</div>
<h4 class="text-on-surface text-xl font-bold mb-3">Local Proxy</h4>
<p class="text-on-surface-variant text-sm leading-relaxed">
                  Ultra low-latency Rust bridge running on your machine. Minimal overhead, maximum throughput.
                </p>
</div>
<div class="bg-surface-container-low p-8 rounded-xl border-t-2 border-secondary/20 hover:border-secondary transition-all group">
<div class="text-secondary mb-6 group-hover:scale-110 transition-transform duration-300">
<span class="material-symbols-outlined text-4xl" data-icon="key">key</span>
</div>
<h4 class="text-on-surface text-xl font-bold mb-3">OAuth/JWT Support</h4>
<p class="text-on-surface-variant text-sm leading-relaxed">
                  Secure authentication layers integrated directly into the proxy pipeline. Enterprise ready.
                </p>
</div>
<div class="bg-surface-container-low p-8 rounded-xl border-t-2 border-primary-container/20 hover:border-primary-container transition-all group">
<div class="text-primary mb-6 group-hover:scale-110 transition-transform duration-300">
<span class="material-symbols-outlined text-4xl" data-icon="stream">stream</span>
</div>
<h4 class="text-on-surface text-xl font-bold mb-3">SSE Translation</h4>
<p class="text-on-surface-variant text-sm leading-relaxed">
                  Real-time Server-Sent Events translation from various backends into the Claude Code protocol.
                </p>
</div>
<div class="bg-surface-container-low p-8 rounded-xl border-t-2 border-secondary/20 hover:border-secondary transition-all group">
<div class="text-secondary mb-6 group-hover:scale-110 transition-transform duration-300">
<span class="material-symbols-outlined text-4xl" data-icon="account_tree">account_tree</span>
</div>
<h4 class="text-on-surface text-xl font-bold mb-3">Modular Arch</h4>
<p class="text-on-surface-variant text-sm leading-relaxed">
                  Clean separation of handlers, protocol translation, and backends for easy community contribution.
                </p>
</div>
</div>
</section>
<!-- Build Section -->
<section class="py-20 px-4 flex flex-col lg:flex-row gap-16" id="build">
<div class="flex-1">
<h3 class="text-on-surface text-3xl font-headline font-bold mb-8">Build &amp; Usage</h3>
<div class="space-y-6">
<div class="flex flex-col gap-3">
<span class="text-on-surface-variant font-mono text-xs uppercase tracking-tighter">01. Installation</span>
<div class="bg-surface-container-lowest p-5 rounded-lg border border-outline-variant/15 font-mono text-sm text-secondary">
                    git clone https://github.com/codex/claude-codex.git<br/>
                    cd claude-codex &amp;&amp; cargo build --release
                  </div>
</div>
<div class="flex flex-col gap-3">
<span class="text-on-surface-variant font-mono text-xs uppercase tracking-tighter">02. Configure Environment</span>
<div class="bg-surface-container-lowest p-5 rounded-lg border border-outline-variant/15 font-mono text-sm text-primary">
                    export CLAUDE_CODEX_BACKEND="http://localhost:11434/v1"<br/>
                    export CLAUDE_CODEX_AUTH_TOKEN="sk-your-secret-key"
                  </div>
</div>
<div class="flex flex-col gap-3">
<span class="text-on-surface-variant font-mono text-xs uppercase tracking-tighter">03. Run Proxy</span>
<div class="bg-surface-container-lowest p-5 rounded-lg border border-outline-variant/15 font-mono text-sm text-on-surface">
                    ./target/release/claude-codex --port 8080
                  </div>
</div>
</div>
</div>
<!-- Models List -->
<div class="w-full lg:w-[400px]" id="models">
<h3 class="text-on-surface text-3xl font-headline font-bold mb-8">Supported Models</h3>
<div class="overflow-hidden rounded-xl border border-outline-variant/15 bg-surface-container-low">
<table class="w-full text-left text-sm">
<thead class="bg-surface-container-high text-on-surface-variant">
<tr>
<th class="px-4 py-3 font-semibold">Model ID</th>
<th class="px-4 py-3 font-semibold text-right">Status</th>
</tr>
</thead>
<tbody class="divide-y divide-outline-variant/10">
<tr class="hover:bg-surface-container-highest transition-colors">
<td class="px-4 py-4 font-mono">gpt-5.4-hyper-vision</td>
<td class="px-4 py-4 text-right"><span class="px-2 py-0.5 rounded bg-secondary-container/20 text-secondary text-[10px] font-bold uppercase">Experimental</span></td>
</tr>
<tr class="hover:bg-surface-container-highest transition-colors">
<td class="px-4 py-4 font-mono">gpt-4o-2024-05-13</td>
<td class="px-4 py-4 text-right"><span class="px-2 py-0.5 rounded bg-primary-container/20 text-primary-fixed-dim text-[10px] font-bold uppercase">Stable</span></td>
</tr>
<tr class="hover:bg-surface-container-highest transition-colors">
<td class="px-4 py-4 font-mono">llama-3-70b-instruct</td>
<td class="px-4 py-4 text-right"><span class="px-2 py-0.5 rounded bg-primary-container/20 text-primary-fixed-dim text-[10px] font-bold uppercase">Stable</span></td>
</tr>
<tr class="hover:bg-surface-container-highest transition-colors">
<td class="px-4 py-4 font-mono">claude-3-opus-20240229</td>
<td class="px-4 py-4 text-right"><span class="px-2 py-0.5 rounded bg-surface-container-high text-on-surface-variant text-[10px] font-bold uppercase">Legacy</span></td>
</tr>
<tr class="hover:bg-surface-container-highest transition-colors">
<td class="px-4 py-4 font-mono">mistral-large-latest</td>
<td class="px-4 py-4 text-right"><span class="px-2 py-0.5 rounded bg-primary-container/20 text-primary-fixed-dim text-[10px] font-bold uppercase">Stable</span></td>
</tr>
</tbody>
</table>
</div>
</div>
</section>
<!-- Architecture Section -->
<section class="py-20 px-4" id="arch">
<h3 class="text-on-surface text-3xl font-headline font-bold mb-12 text-center">Engine Architecture</h3>
<div class="relative grid grid-cols-1 md:grid-cols-4 gap-4 items-stretch">
<!-- Auth -->
<div class="flex flex-col bg-surface-container-lowest p-6 border border-outline-variant/20 rounded-xl">
<div class="text-primary text-xs font-mono mb-4">src/auth/</div>
<h5 class="text-on-surface font-bold mb-2">Auth Handler</h5>
<p class="text-on-surface-variant text-xs mb-4">Validates inbound requests, manages JWT session states and OAuth 2.0 flow.</p>
<div class="mt-auto pt-4 border-t border-outline-variant/10 text-primary font-mono text-[10px]">VERIFY_TOKEN()</div>
</div>
<!-- Protocol -->
<div class="flex flex-col bg-surface-container-lowest p-6 border border-primary-container/40 rounded-xl relative">
<div class="absolute -top-2 left-1/2 -translate-x-1/2 bg-primary-container text-on-primary-container text-[10px] font-bold px-2 py-0.5 rounded">CORE</div>
<div class="text-primary-container text-xs font-mono mb-4">src/protocol/</div>
<h5 class="text-on-surface font-bold mb-2">Protocol Translation</h5>
<p class="text-on-surface-variant text-xs mb-4">Maps Anthropic tool-calling specs to OpenAI Function Calling formats in real-time.</p>
<div class="mt-auto pt-4 border-t border-outline-variant/10 text-primary-container font-mono text-[10px]">MAP_SPEC()</div>
</div>
<!-- Stream -->
<div class="flex flex-col bg-surface-container-lowest p-6 border border-outline-variant/20 rounded-xl">
<div class="text-secondary text-xs font-mono mb-4">src/stream/</div>
<h5 class="text-on-surface font-bold mb-2">SSE Transcoder</h5>
<p class="text-on-surface-variant text-xs mb-4">Handles async chunking and re-assembly for smooth typing animations in CLI.</p>
<div class="mt-auto pt-4 border-t border-outline-variant/10 text-secondary font-mono text-[10px]">CHUNK_RECV()</div>
</div>
<!-- Backends -->
<div class="flex flex-col bg-surface-container-lowest p-6 border border-outline-variant/20 rounded-xl">
<div class="text-tertiary text-xs font-mono mb-4">src/backends/</div>
<h5 class="text-on-surface font-bold mb-2">Backend Router</h5>
<p class="text-on-surface-variant text-xs mb-4">Pluggable connectors for OpenAI, Ollama, LocalAI, and Groq providers.</p>
<div class="mt-auto pt-4 border-t border-outline-variant/10 text-tertiary font-mono text-[10px]">POST_REQUEST()</div>
</div>
<!-- Connecting Line Overlay (Stylized) -->
<div class="hidden md:block absolute top-1/2 left-0 w-full h-[1px] bg-gradient-to-r from-transparent via-primary-container/30 to-transparent -z-10"></div>
</div>
</section>
<!-- CTA Footer -->
<footer class="py-20 border-t border-outline-variant/15 text-center flex flex-col items-center gap-8 px-4">
<div class="size-16 glow-accent text-primary">
<svg fill="none" viewbox="0 0 48 48" xmlns="http://www.w3.org/2000/svg">
<path d="M36.7273 44C33.9891 44 31.6043 39.8386 30.3636 33.69C29.123 39.8386 26.7382 44 24 44C21.2618 44 18.877 39.8386 17.6364 33.69C16.3957 39.8386 14.0109 44 11.2727 44C7.25611 44 4 35.0457 4 24C4 12.9543 7.25611 4 11.2727 4C14.0109 4 16.3957 8.16144 17.6364 14.31C18.877 8.16144 21.2618 4 24 4C26.7382 4 29.123 8.16144 30.3636 14.31C31.6043 8.16144 33.9891 4 36.7273 4C40.7439 4 44 12.9543 44 24C44 35.0457 40.7439 44 36.7273 44Z" fill="currentColor"></path>
</svg>
</div>
<div>
<h2 class="text-3xl font-headline font-bold mb-4">Start Building with Claude Codex</h2>
<p class="text-on-surface-variant max-w-md mx-auto mb-8">
                The open-source bridge for the next generation of AI-driven terminal interfaces.
              </p>
<div class="flex flex-wrap justify-center gap-4">
<a class="bg-primary-container text-on-primary-container px-10 py-4 rounded-lg font-bold flex items-center gap-2" href="#">
<span>Star on GitHub</span>
<span class="material-symbols-outlined text-sm" data-icon="star">star</span>
</a>
<a class="bg-surface-container-high text-on-surface px-10 py-4 rounded-lg font-bold" href="#">Read the Docs</a>
</div>
</div>
<div class="mt-20 flex flex-col md:flex-row justify-between w-full border-t border-outline-variant/10 pt-8 text-on-surface-variant/50 text-xs font-mono">
<p>© 2024 CLAUDE-CODEX OSS PROJECT</p>
<div class="flex gap-8 mt-4 md:mt-0">
<a class="hover:text-primary transition-colors" href="#">PRIVACY</a>
<a class="hover:text-primary transition-colors" href="#">LICENSE (MIT)</a>
<a class="hover:text-primary transition-colors" href="#">CONTRIBUTORS</a>
</div>
</div>
</footer>
</div>
</div>
</div>
</div>
</body></html>