<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Weplex">
</p>

<h1 align="center">Weplex</h1>

<p align="center">
  A workspace for AI coding agents.
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
</p>

<p align="center">
  <a href="https://weplex.ai">
    <img src="https://weplex.ai/screenshots/hero.png" width="800" alt="Weplex workspace screenshot">
  </a>
</p>

---

Run multiple AI coding agents across multiple projects without losing your mind.

Weplex gives each project its own **Space** — a workspace with its own sessions, color, and profile. Switch between projects in one click. See all your agents at a glance.

Works with **Claude Code**, **Aider**, **Gemini CLI**, **Codex**, and **OpenCode**. Claude Code gets deep integration. Others are auto-detected and fully supported.

## Features

**Spaces** — One per project. Own sessions, color, grain texture, and profile. Switch context instantly.

**Sessions** — Full PTY terminal. Split views (horizontal + vertical). Hyperspace mode to see all sessions across spaces. Session persistence across restarts.

**Agent Detection** — Auto-detects which agent is running. Real-time status: thinking → active → idle → waiting → error.

**Profiles** — Work, Personal, or as many as you need. Each with its own API keys and preferences. Switch in one click.

**Claude Code Integration** — Hook server intercepts tool use, sub-agent spawns, and file changes. CLAUDE.md context injection. Sub-agent visibility tree. Orchestration dashboard.

**Dashboards** — Orchestration (agent tree + activity feed), Project (git status + file changes), Space (all sessions overview with costs).

**Cost Tracking** — Reads JSONL usage files. Per-session spend in the sidebar.

## Download

Grab the latest release from [weplex.ai](https://weplex.ai) or [GitHub Releases](https://github.com/weplexai/weplex/releases).

macOS — Apple Silicon & Intel. Linux & Windows coming.

## Build from Source

Prerequisites: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) 22+, [pnpm](https://pnpm.io/) 10+

```bash
git clone https://github.com/weplexai/weplex.git
cd weplex
pnpm install
pnpm tauri dev
```

## Architecture

| Layer | Technology |
|-------|-----------|
| Runtime | Tauri 2.2+ (Rust) |
| Frontend | Svelte 5 + Vite |
| Terminal | xterm.js |
| PTY | portable-pty |
| Hook Server | tiny_http |

## License

[Apache 2.0](LICENSE) — a terminal that sees every keystroke must be auditable.
