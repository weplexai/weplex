# Weplex

> The terminal with a built-in pipeline engine for AI coding agents. Powered by Claude. Open to any agent.

## Tech Stack
- **Runtime**: Tauri 2.2+ (Rust backend)
- **Frontend**: Svelte 5 + Vite
- **Terminal**: xterm.js (Canvas renderer)
- **PTY**: portable-pty (Rust, from WezTerm)
- **Styles**: CSS Variables (custom design system)
- **Icons**: lucide-svelte
- **License**: MIT

## Project Structure
```
weplex/
├── CLAUDE.md              # This file
├── DESIGN.md              # Full design specification
├── src-tauri/             # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs        # Tauri entry point
│   │   ├── pty.rs         # PTY management (portable-pty)
│   │   ├── session.rs     # Session state & persistence
│   │   └── commands.rs    # Tauri IPC commands
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                   # Svelte frontend
│   ├── App.svelte         # Root layout
│   ├── main.ts            # Entry point
│   ├── lib/
│   │   ├── components/    # UI components
│   │   │   ├── sidebar/   # Sidebar, spaces, session list
│   │   │   ├── terminal/  # xterm.js wrapper
│   │   │   ├── header/    # Session header bar
│   │   │   ├── detail/    # Right detail panel
│   │   │   ├── status/    # Bottom status bar
│   │   │   └── overlays/  # Command palette, settings, new session
│   │   ├── stores/        # Svelte stores (sessions, spaces, settings)
│   │   ├── parsers/       # Agent-specific output parsers
│   │   ├── theme/         # Design system, CSS variables
│   │   └── utils/         # Helpers
│   └── styles/            # Global CSS (tokens, reset)
├── package.json
└── vite.config.ts
```

## Key Design Decisions
- Sidebar LEFT (Arc-style), collapsible (240px / 48px / overlay)
- Spaces for context switching (like Arc)
- Three session types: Agent (auto-detect) / SSH / Terminal
- Detail panel RIGHT (toggle, 280px)
- Split views (horizontal + vertical)
- Session persistence across app restarts
- Agent-agnostic: Claude Code, OpenCode, Crush, Aider, Gemini CLI, Codex
- MCP-first pipeline orchestration: Weplex MCP Server for completion detection + artifact passing
- Multi-model pipelines: mix Claude, DeepSeek, Qwen, GPT via OpenCode/Crush as binary
- Auth-aware: detects OAuth vs API key, adjusts cost display. Profiles with CLAUDE_CONFIG_DIR
- Interactive pipeline stages: each stage = full PTY session (not headless)
- See DESIGN.md for full specification

## Documentation
- **DESIGN.md** — full design specification (layout, components, design system)
- **PRODUCT.md** — product vision, positioning, extension architecture, license rationale
- **ROADMAP.md** — phased roadmap (MVP → awareness → connected sessions → extensions → orchestration)
- **COMPETITORS.md** — competitive landscape analysis
- **IDEAS.md** — feature ideas backlog
- **PROGRESS.md** — implementation log (what's done)

## Development
```bash
# Install dependencies
pnpm install

# Dev mode (frontend + Tauri)
pnpm tauri dev

# Build for production
pnpm tauri build
```

## Naming Conventions
- **Components**: PascalCase.svelte (Sidebar.svelte, SessionItem.svelte)
- **Stores**: camelCase.ts (sessionStore.ts, spaceStore.ts)
- **Rust modules**: snake_case.rs (pty_manager.rs, session.rs)
- **CSS variables**: --weplex-color-*, --weplex-space-*, --weplex-radius-*
- **Tauri commands**: snake_case (create_session, kill_session)
