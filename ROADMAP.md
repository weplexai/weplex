# Weplex — Roadmap

## Phase 0: MVP Launch (current)

Ship the core terminal + pipeline engine. The terminal with deterministic multi-agent pipelines.

### Terminal Foundation
- [x] Spaces (Arc-style context switching)
- [x] Profiles (multi-account, env var isolation)
- [x] Sessions (create, persist, restore, agent auto-resume)
- [x] Agent detection (Claude, Aider, OpenCode, Gemini, Codex)
- [x] Usage panel (tokens, model, turns, cache stats from Claude JSONL)
- [x] Session notes
- [x] Command palette + quick switcher
- [x] Settings overlay + profile management
- [x] Drag-to-reorder, folders, sidebar search
- [x] Agents panel (reads ~/.claude/agents/ + ~/.weplex/agents/)
- [ ] Header bar (branch, model, cost for active session)
- [ ] Split views (Cmd+D horizontal, Cmd+Shift+D vertical)

### Pipeline Engine (MVP)
- [x] Weplex agent format: `~/.weplex/agents/*.yaml` with `binary` field (agent-agnostic)
- [x] Pipeline engine: Rust state machine — read YAML, track state, manage stages
- [x] Pipeline sidebar: stage sessions grouped with ✓/●/○ status indicators
- [x] "Start Pipeline" dialog (⌘N → Pipeline tab) — pick template + cwd + task + profile
- [x] Agent editor — form UI for Weplex agent YAML format (binary, model, prompt)
- [x] Pipeline visual editor — n8n-style canvas with pan/zoom, drag-to-reorder, Bézier connections
- [x] 3 built-in pipeline templates: feature, bugfix, security-audit
- [x] Security: binary allowlist, env var denylist, output buffer cap, run cleanup
- [ ] **Deck MCP Server** — the core orchestration mechanism:
  - Agents call `deck_stage_complete()` to signal completion
  - Agents call `deck_get_artifact("stage-name")` to read previous stage results
  - Agents call `deck_pipeline_info()` to know their role, task, stage index
  - Works with any MCP-compatible agent (Claude Code, OpenCode, Crush)
  - Universal standard: Anthropic, OpenAI, Google, DeepSeek, Qwen all support MCP
- [ ] Interactive sessions: each stage = full PTY session (not headless `claude -p`)
  - User sees agent reasoning, can answer questions, grant permissions
  - Stage completion detected via MCP tool call (reliable) or status polling (fallback)
  - Artifact passing via MCP (structured) or shared filesystem (universal)
- [ ] Profile-aware: pipeline stages inherit auth context (CLAUDE_CONFIG_DIR, API keys)

### Launch checklist
- [ ] README with demo GIF
- [ ] LICENSE file (MIT)
- [ ] Git repo, clean history
- [ ] GitHub release (macOS binary + Linux)
- [ ] Landing page (one-pager)

See [LAUNCH.md](./LAUNCH.md) for full launch plan.

---

## Phase 1: Polish & Awareness

Make Weplex the best way to observe and manage AI agent sessions.

- [ ] Real-time agent status (thinking / idle / waiting for input)
- [ ] Notification when agent finishes or gets stuck
- [ ] Smart session naming (auto-detect from cwd or agent output)
- [ ] Parse Claude tool use → show file changes in detail panel (Claude bonus)
- [ ] Git diff panel (files agent touched this session)
- [ ] Error detection (highlight agent errors in sidebar)
- [ ] Light theme
- [ ] Pipeline Dashboard — flow view with cost per stage, clickable sessions
- [ ] More binary adapters (OpenCode, Crush, Gemini CLI, custom scripts)

---

## Phase 2: Deep Integration & Canvas

Agent-specific deep features + visual pipeline editor.

### Deep Integration (bonus features per binary)
- [ ] Claude Code hooks injection (PreToolUse, PostToolUse, Stop) — when binary=claude
- [ ] Sub-agent visibility — Claude's Agent tool sub-agents detected via hooks
- [ ] CLAUDE.md context injection — Weplex prepends space, sessions, budget
- [ ] Cost tracking per stage (from Claude JSONL, OpenCode logs)
- [ ] OpenCode integration — LSP support, multi-model switching mid-session

### Visual Canvas (done in MVP)
- [x] Canvas editor (n8n-style) — drag agent nodes, connect, parallel branches
- [x] Canvas with pan/zoom (trackpad scroll, ctrl+scroll)
- [ ] Auto-layout — generate `layout` coordinates from `stages`
- [ ] YAML preview alongside visual editor

### Git Integration
- [ ] Git status polling per session (branch, modified files)
- [ ] Conflict detection: warn when two sessions modify same file
- [ ] Git worktree auto-isolation (reactive: detect branch → offer isolate)

---

## Phase 3: Advanced Orchestration

Session hierarchy, dashboards, advanced MCP.

- [ ] Weplex MCP Server v2 — cross-session communication (list/create/read/send to other sessions)
- [ ] Session hierarchy — parentId, children indented in sidebar
- [ ] Orchestration Dashboard — agent tree, timeline, activity feed, changed files
- [ ] Project Dashboard — sessions by cwd, git status, conflict detection
- [ ] Space Dashboard — visual board view
- [ ] Terminal Decorations — hover-triggered inline actions (path, URL, command)
- [ ] Pipeline pause/resume, re-run stage, skip optional stage
- [ ] Pipeline error strategies (stop / retry / skip / ask user)

---

## Phase 4: Marketplace & Teams

Weplex becomes a platform with an ecosystem.

### Plugin System
- [ ] Plugin API contract (session type, tray icon, tray panel, session header)
- [ ] Plugin Host — load/unload plugins from ~/.weplex/plugins/
- [ ] Plugin Tray — sidebar bottom, one icon per plugin, max 8
- [ ] Plugin permissions model (declared in manifest, approved on install)
- [ ] Browser plugin — first reference plugin (Chrome via CDP). See [PLUGINS.md](./PLUGINS.md)

### Marketplace
- [ ] Agent marketplace — browse, install via Weplex UI (→ ~/.weplex/agents/)
- [ ] Pipeline marketplace — browse, install via Weplex UI (→ ~/.weplex/pipelines/)
- [ ] Plugin marketplace — browse, install via Weplex UI (→ ~/.weplex/plugins/)
- [ ] Package format: `agent.yaml`/`pipeline.yaml`/`weplex-plugin.json` + `weplex.yaml` (marketplace metadata)
- [ ] Dependency check: pipeline `requires` → install missing agents
- [ ] GitHub-based distribution: `weplex install github.com/user/repo/agent-name`
- [ ] In-app marketplace with search, ratings, verified publishers

### Teams
- [ ] Private pipeline/agent library per team
- [ ] Team cost view — aggregate spend per team, per project
- [ ] Session activity log — audit trail
- [ ] SSO, access controls
- [ ] Quick reply: approve/deny agent prompts from sidebar

### Monetization
- Free: community marketplace, all core features, pipeline engine
- Team: private library + cost analytics
- Enterprise: private registry + audit + SSO

---

## Non-goals (for now)

- Built-in AI (Weplex orchestrates deterministically, doesn't think)
- Cloud sync (local-first always)
- Mobile app
- Integrated editor (this is a terminal, not an IDE)

---

## Architecture Principles

1. **Deck = orchestrator** — deterministic Rust state machine, not AI. YAML is law.
2. **Agent-agnostic** — any MCP-compatible CLI agent on any pipeline stage. Claude Code, OpenCode, Crush, Aider, Codex, Gemini CLI.
3. **MCP-first** — Weplex MCP Server is the primary orchestration mechanism. Agents signal completion, request artifacts, and get pipeline context via MCP tools. Universal standard supported by Anthropic, OpenAI, Google, DeepSeek, Qwen.
4. **Interactive sessions** — each pipeline stage is a full PTY session. User sees reasoning, answers questions, grants permissions. Not headless `claude -p`.
5. **Two levels** — Weplex controls flow (deterministic), agent controls execution (autonomous)
6. **Multi-model pipelines** — mix Claude, DeepSeek, Qwen, GPT in one pipeline via OpenCode/Crush as binary. Each stage can use a different model/provider.
7. **Own ecosystem** — `~/.weplex/agents/` and `~/.weplex/pipelines/` are Weplex's territory
8. **Progressive complexity** — simple terminal for beginners, pipelines for power users, canvas for architects
9. **MIT always** — terminal that sees every keystroke must be auditable
