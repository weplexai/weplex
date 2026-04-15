# Weplex Client

> Tauri desktop app — terminal for AI coding agents.

## Tech Stack
- **Runtime**: Tauri 2.2+ (Rust backend)
- **Frontend**: Svelte 5 + Vite
- **Terminal**: xterm.js (Canvas renderer)
- **PTY**: portable-pty (Rust, from WezTerm)
- **HTTP**: tiny_http (local hook server)
- **Styles**: CSS Variables (custom design system)
- **Icons**: lucide-svelte

## Project Structure
```
weplex-client/
├── src-tauri/             # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs              # Tauri entry point, PTY commands, app setup (279 LOC)
│   │   ├── agents.rs            # Agent config parsing and listing
│   │   ├── claude.rs            # Session usage stats and state detection
│   │   ├── commands.rs          # Claude command file CRUD
│   │   ├── context.rs           # CLAUDE.local.md context injection
│   │   ├── git.rs               # Git branch/status and project config
│   │   ├── hooks.rs             # Hook script generation and profile sync
│   │   ├── marketplace.rs       # Marketplace package/skill saving
│   │   ├── mcp.rs               # MCP binary discovery and registration
│   │   ├── platform.rs          # URL opening and macOS traffic lights
│   │   ├── profile.rs           # Profile discovery and resource management
│   │   ├── skills.rs            # Skill discovery and reading
│   │   ├── store.rs             # Persistent JSON store with atomic writes
│   │   ├── utils.rs             # Path resolution, validation, sanitization
│   │   ├── yaml.rs              # YAML frontmatter parsing utilities
│   │   ├── pty_manager.rs       # PTY management (portable-pty)
│   │   ├── hook_server.rs       # Local HTTP server for Claude Code hook events
│   │   ├── ipc_server.rs        # Unix socket pool for MCP cross-session tools
│   │   ├── resources.rs         # Profile resource discovery/copy
│   │   ├── session_summary.rs   # Session activity notes persistence
│   │   ├── secure_store.rs      # Encrypted credential storage
│   │   ├── oauth_server.rs      # Local OAuth callback server
│   │   └── keychain.rs          # OS keychain integration
│   ├── mcp-server/        # Weplex MCP Server binary (weplex-mcp)
│   │   └── src/
│   │       ├── main.rs          # JSON-RPC entry point
│   │       ├── tools.rs         # MCP tool handlers
│   │       ├── ipc_client.rs    # Unix socket client to Tauri backend
│   │       └── mcp_protocol.rs  # MCP JSON-RPC protocol
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                   # Svelte frontend
│   ├── App.svelte         # Root layout (terminals + dashboards)
│   ├── main.ts            # Entry point
│   ├── lib/
│   │   ├── components/
│   │   │   ├── sidebar/   # Sidebar, spaces, session list (with hierarchy)
│   │   │   ├── terminal/  # xterm.js wrapper, split views
│   │   │   ├── dashboard/ # Orchestration, Project, Space dashboards
│   │   │   ├── header/    # Session header bar
│   │   │   ├── detail/    # Right detail panel (live activity, sub-agents, conflicts)
│   │   │   ├── status/    # Bottom status bar
│   │   │   └── overlays/  # Command palette, settings, new session, auth
│   │   ├── stores/        # Svelte 5 runes stores
│   │   │   ├── sessionStore    # Sessions CRUD, hierarchy, dashboards
│   │   │   ├── hookStore       # Hook events, activity tracking, conflict detection
│   │   │   ├── contextInjectionStore  # CLAUDE.md context block injection
│   │   │   ├── profileStore    # Multi-account profiles
│   │   │   ├── spaceStore      # Workspace spaces
│   │   │   ├── presenceStore   # Team presence sync
│   │   │   ├── chatStore       # Space chat messaging
│   │   │   ├── authStore       # Authentication state
│   │   │   ├── teamStore       # Team management
│   │   │   └── ...             # settings, folders, splits, notes, UI, drag
│   │   ├── types.ts       # All TypeScript types
│   │   └── utils/         # Detection, shortcuts, time, paths
│   └── styles/            # Global CSS (tokens)
├── package.json
└── vite.config.ts
```

## Key Features

### Terminal (Phase 0 — done)
Spaces, Profiles, Sessions, Agent detection (Claude/OpenCode/Aider/Gemini/Codex), Usage panel (JSONL), Split views, Hyperspace, Session notes, Command palette

### Claude Deep Integration (Phase 2 — done)
- **Hook Server**: localhost HTTP with bearer token auth, receives PreToolUse/PostToolUse/Stop/SubagentStart/SubagentStop from Claude Code via jq-based bash scripts
- **CLAUDE.md Injection**: prepends workspace context (space, sessions, cost) before Claude session start
- **Sub-agent Visibility**: tracks Claude's Agent tool sub-agents with start/stop lifecycle
- **Git Integration**: real-time branch + status via git CLI, hook-driven refresh after file modifications
- **Session Hierarchy**: parent/child sessions, collapse/expand in sidebar
- **Dashboards**: Orchestration (agent tree, timeline, activity feed), Project (cwd-based, git status), Space (grid overview)

### Accounts & Collaboration (Phase 3 — partial)
Auth (email + OAuth), teams, spaces API, chat, session presence

## Key Design Decisions
- Sidebar LEFT (Arc-style), collapsible (240px / 48px / overlay)
- Spaces for context switching (like Arc)
- Four session types: Agent / SSH / Terminal / Dashboard
- Detail panel RIGHT (toggle, 280px) with live activity + sub-agents + conflicts
- Split views (horizontal + vertical)
- Session persistence across app restarts
- All sessions rendered simultaneously (position: absolute) for instant switching
- Agent resolution: `.claude/agents/` + `~/.weplex/agents/` (both equal, no hierarchy)
- Claude-first strategy: deep features for Claude, basic support for others

## Development
```bash
pnpm install
pnpm tauri dev        # dev mode
pnpm tauri build      # production build
```

## Naming Conventions
- **Components**: PascalCase.svelte (Sidebar.svelte, SessionItem.svelte)
- **Stores**: camelCase.svelte.ts (sessionStore.svelte.ts)
- **Rust modules**: snake_case.rs (pty_manager.rs, hook_server.rs)
- **CSS variables**: --weplex-color-*, --weplex-space-*, --weplex-radius-*
- **Tauri commands**: snake_case (create_pty, get_git_branch, inject_claude_md)

## API Integration
Backend: api.weplex.ai (see ../weplex-server/)
- Auth: POST /auth/register, /auth/login, /auth/refresh, OAuth via /auth/github, /auth/google
- Sync: PUT /sync, GET /sync
- Teams: POST /teams, /teams/join, /teams/leave
- Spaces: CRUD /spaces, GET /spaces/:id/chat, /spaces/:id/sessions
- Collaboration: WebSocket /relay namespace (teams, chat, presence, spectating)
- OAuth desktop flow: open system browser → callback to localhost:19847 → exchange code for tokens
