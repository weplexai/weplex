<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Weplex">
</p>

<h1 align="center">Weplex</h1>

<p align="center">
  The platform for AI coding agents.
  <br>
  Powered by Claude. Open to any agent.
</p>

<p align="center">
  <a href="https://weplex.ai">Website</a> &middot;
  <a href="https://github.com/weplexai/weplex/releases">Download</a> &middot;
  <a href="LICENSE">Apache 2.0</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey.svg" alt="macOS">
  <img src="https://img.shields.io/badge/tauri-2.x-blue.svg" alt="Tauri 2">
  <img src="https://img.shields.io/badge/svelte-5-orange.svg" alt="Svelte 5">
</p>

---

## What is Weplex?

Not a terminal. Not an IDE. A **platform** for AI coding agents.

Run Claude Code, Codex, Aider, Gemini CLI, or any agent — each in its own session with real-time status, cost tracking, and output capture. Chain them into deterministic pipelines. Collaborate with your team. Browse and install community agents from the marketplace.

**No AI in the orchestration layer.** Weplex is a deterministic Rust state machine that reads YAML and executes it. Predictable, repeatable, inspectable.

## Features

### Spaces & Sessions
- Full PTY terminal (bash, zsh, fish, ssh)
- Spaces for context switching — each with own sessions, profile, and color
- Split views (horizontal + vertical), Hyperspace (all sessions across spaces)
- Session persistence across restarts
- Profiles with separate API keys and preferences

### Agent Intelligence
- Auto-detects running agents (Claude Code, Codex, Aider, Gemini CLI, OpenCode)
- Real-time status: thinking → active → idle → waiting → error
- Hook-driven awareness — every tool use, sub-agent spawn, file write
- Smart session naming from `-p` prompts or first user input
- Per-session cost tracking, error detection with sidebar highlighting
- OS notifications: finished, stuck (5min), waiting for input, errors

### Pipeline Engine
- Define multi-stage workflows in YAML
- Each stage = a real interactive PTY session (not headless)
- Sequential and parallel execution
- Artifact passing between stages via MCP
- Agent-agnostic: mix Claude, Aider, Codex in one pipeline
- Visual pipeline editor (drag & drop)
- Pipeline Dashboard with flow visualization, Gantt timeline, cost breakdown

### Claude Deep Integration
- Hook Server: intercepts PreToolUse, PostToolUse, Stop, SubagentStart, SubagentStop
- CLAUDE.md context injection — workspace context prepended before session start
- Sub-agent visibility with lifecycle tracking
- Git integration — real-time branch + status
- Orchestration, Project, and Space Dashboards

### Teams & Collaboration
- Teams with invite codes
- Shared spaces with session presence (see what teammates are working on)
- Space chat with mentions, replies, edits, typing indicators
- Collaborative pipelines — delegate stages to team members
- Session spectating — watch teammates' agent sessions in real-time (read-only)
- Conflict detection — alerts when two agents edit the same file

### Marketplace & Plugins
- Browse, install, rate, and publish agents and pipelines
- Plugin system with dynamic JS loading
- Browser plugin (Chrome via CDP) included

### Accounts
- Email + password, GitHub OAuth, Google OAuth
- Config sync & backup across devices
- Free for individuals. Always. Team features will be paid.

## Getting Started

### Download

Grab the latest release from [weplex.ai](https://weplex.ai) or [GitHub Releases](https://github.com/weplexai/weplex/releases).

macOS — Apple Silicon & Intel. Linux & Windows soon.

### Build from Source

Prerequisites: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) 22+, [pnpm](https://pnpm.io/) 10+

```bash
git clone https://github.com/weplexai/weplex.git
cd weplex

pnpm install
pnpm tauri dev
```

Production build:

```bash
pnpm tauri build
```

## Pipeline Example

Create `~/.weplex/pipelines/feature.yaml`:

```yaml
name: Feature Pipeline
description: Full feature development workflow
stages:
  - agent: pm
    role: Analyze the task, produce a Task Brief
  - agent: architect
    role: Design the implementation approach
    receives: [pm]
  - agent: backend
    role: Implement the feature
    receives: [architect]
  - parallel:
    - agent: security
      role: Review for vulnerabilities
      receives: [backend]
    - agent: tester
      role: Write and run tests
      receives: [backend]
```

Define agents in `~/.weplex/agents/`:

```yaml
# ~/.weplex/agents/backend.yaml
name: backend
description: Backend implementation agent
binary: claude
model: sonnet
prompt: |
  You are a backend developer. Write clean, production-ready code.
```

## Architecture

| Layer | Technology |
|-------|-----------|
| Runtime | Tauri 2.2+ (Rust backend) |
| Frontend | Svelte 5 + Vite |
| Terminal | xterm.js (Canvas renderer) |
| PTY | portable-pty (from WezTerm) |
| Hook Server | tiny_http (localhost, bearer token auth) |
| MCP Server | JSON-RPC over Unix domain sockets |
| Styles | CSS Variables (custom design system) |

```
weplex/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── main.rs                 # Tauri commands, hooks, git
│   │   ├── pty_manager.rs          # PTY session management
│   │   ├── pipeline_engine.rs      # Pipeline orchestration
│   │   ├── hook_server.rs          # Local HTTP for Claude hooks
│   │   ├── ipc_server.rs           # Unix socket for MCP
│   │   ├── plugin_host.rs          # Plugin discovery & lifecycle
│   │   └── weplex_agents.rs        # Agent YAML resolution
│   └── mcp-server/                 # Weplex MCP Server binary
├── src/                    # Svelte frontend
│   ├── App.svelte
│   └── lib/
│       ├── components/             # sidebar, terminal, dashboard, overlays
│       ├── stores/                 # Svelte 5 runes state management
│       ├── services/               # API clients, WebSocket, notifications
│       └── utils/                  # Detection, shortcuts, formatting
└── update-worker/          # Cloudflare Worker (auto-updater)
```

## Backend

The Weplex server ([weplex-api](https://github.com/weplexai/weplex-api)) handles authentication, config sync, teams, marketplace, and real-time collaboration via WebSocket.

## License

[Apache 2.0](LICENSE) — a terminal that sees every keystroke must be auditable.
