<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Weplex">
</p>

<h1 align="center">Weplex</h1>

<p align="center">
  The terminal with a built-in pipeline engine for AI coding agents.
  <br>
  Powered by Claude. Open to any agent.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="Apache 2.0 License"></a>
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey.svg" alt="macOS">
  <img src="https://img.shields.io/badge/tauri-2.x-blue.svg" alt="Tauri 2">
  <img src="https://img.shields.io/badge/svelte-5-orange.svg" alt="Svelte 5">
</p>

---

## What is Weplex?

A native terminal app that turns AI coding agents into structured, visible workflows.

Run Claude Code, Codex, Aider, Gemini CLI, or any agent — each in its own session with status tracking, cost monitoring, and automatic output capture. Chain them into pipelines: PM analyzes the task, Architect designs the approach, Backend implements, Security reviews — all orchestrated automatically.

**No AI in the orchestration layer.** Weplex is a deterministic Rust state machine that reads YAML pipeline definitions and executes them. Predictable, repeatable, inspectable.

## Features

### Terminal
- Full PTY terminal (bash, zsh, fish, ssh)
- Split views (horizontal + vertical)
- Session persistence across restarts
- Spaces for context switching (like Arc browser)

### Agent Intelligence
- Auto-detects running agents (Claude Code, Codex, Aider, Gemini CLI, OpenCode, Crush)
- Live status: working / idle / waiting for input / done
- Per-session cost tracking (Claude Code)
- Session notes and detail panel

### Pipeline Engine
- Define multi-stage workflows in YAML
- Each stage = a real interactive PTY session
- Sequential and parallel execution
- Artifact passing between stages
- Agent-agnostic: mix Claude, GPT, DeepSeek, Qwen in one pipeline
- Visual pipeline editor (drag & drop)

### Auto-Updater
- Built-in update checking via `update.weplex.xyz`
- One-click updates from the status bar

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Runtime | Tauri 2 (Rust backend) |
| Frontend | Svelte 5 + Vite |
| Terminal | xterm.js (Canvas renderer) |
| PTY | portable-pty (from WezTerm) |
| Styles | CSS Variables (custom design system) |
| Icons | lucide-svelte |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+

### Development

```bash
# Clone the repo
git clone https://github.com/shipooor/weplex.git
cd weplex

# Install dependencies
pnpm install

# Run in dev mode
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

Output: `src-tauri/target/release/bundle/dmg/Weplex_*.dmg`

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

## Project Structure

```
weplex/
├── src-tauri/          # Rust backend
│   └── src/
│       ├── main.rs             # Tauri commands & app setup
│       ├── pty_manager.rs      # PTY session management
│       ├── weplex_agents.rs    # Agent configuration (YAML)
│       ├── pipeline_engine.rs  # Pipeline orchestration
│       └── pipeline_parser.rs  # YAML pipeline parser
├── src/                # Svelte frontend
│   ├── App.svelte              # Root layout
│   └── lib/
│       ├── components/         # UI components
│       │   ├── sidebar/        # Session list, spaces, search
│       │   ├── terminal/       # xterm.js wrapper, splits
│       │   ├── header/         # Session header bar
│       │   ├── detail/         # Right detail panel
│       │   ├── status/         # Bottom status bar
│       │   └── overlays/       # Palette, settings, agents
│       ├── stores/             # Svelte 5 reactive state
│       └── utils/              # Helpers
└── update-worker/      # Cloudflare Worker (auto-updater endpoint)
```

## Roadmap

- [x] Terminal with PTY, splits, sessions, spaces
- [x] Agent detection and status tracking
- [x] Pipeline engine (sequential + parallel)
- [x] Visual pipeline editor
- [x] Auto-updater
- [ ] MCP-based completion detection
- [ ] Project Dashboard (multi-agent file conflict detection)
- [ ] Pipeline & Agent Marketplace
- [ ] Windows & Linux support

## License

[Apache 2.0](LICENSE)
