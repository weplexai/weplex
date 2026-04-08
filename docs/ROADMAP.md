# Weplex — Roadmap

> Strategic phases. Each phase is a self-contained value proposition.
> Detailed specs live in linked documents — this file is the map, not the territory.

---

## Phase 0: MVP Terminal + Pipeline Engine `done`

Ship the core product. Best terminal for AI coding agents + deterministic pipeline engine.

**Goal**: first public release, first users, validate the concept.

**Terminal** (done): Spaces, Profiles, Sessions, Agent detection, Usage panel, Notes, Command palette, Split views, Hyperspace, Agents panel. See [PROGRESS.md](./PROGRESS.md).

**Pipeline Engine** (done):
- [x] Agent format, pipeline YAML, visual editor, stage UI, security
- [x] Weplex MCP Server — JSON-RPC over Unix sockets, `deck_stage_complete()`, `deck_get_artifact()`, `deck_pipeline_info()`
- [x] Interactive pipeline sessions (each stage = full PTY session)
- [x] Profile-aware stages (inherit CLAUDE_CONFIG_DIR, API keys)

**Launch**: README, LICENSE, GitHub release, landing page (weplex.ai). See [LAUNCH.md](./LAUNCH.md).

**Success metric**: 50-100 GitHub stars, 5-10 daily users.

---

## Phase 1: Polish & Awareness `current`

Make Weplex reliable and visible. The "it just works" phase.

**Goal**: users stick around, word of mouth starts.

- Real-time agent status (thinking / idle / waiting for input)
- Notifications when agent finishes or gets stuck
- Smart session naming (auto-detect from cwd or agent output)
- Error detection (highlight agent errors in sidebar)
- Pipeline Dashboard — flow view with cost per stage
- Light theme
- Content: blog posts, demo videos, community engagement

**Success metric**: 200-500 stars, consistent daily usage, first community contributions.

---

## Phase 2: Deep Integration `done`

Claude Code deep features + advanced orchestration. The "wow, it knows everything" phase.

**Goal**: Weplex becomes indispensable for Claude Code power users.

- [x] Claude Code hooks (PreToolUse, PostToolUse, Stop, SubagentStart, SubagentStop) — real-time awareness via local HTTP server + jq-based scripts
- [x] CLAUDE.md context injection — workspace context prepended before session start
- [x] Sub-agent visibility — detect Claude's Agent tool sub-agents with lifecycle tracking
- [x] Git integration — branch + status via git CLI, hook-driven refresh
- [x] Session hierarchy — parent/child sessions, indented sidebar, aggregated status
- [x] Orchestration Dashboard — agent tree, timeline, activity feed, changed files, conflicts
- [x] Project Dashboard — cross-space cwd-based view, git status, conflicts
- [x] Space Dashboard — visual overview, sessions grouped by project
- [x] Unified agent resolution — `.claude/agents/*.md` + `~/.weplex/agents/*.yaml` (equal)
- [x] MCP Server v2 — cross-session communication via Unix socket pool (ipc_server.rs)

See [DESIGN.md](./DESIGN.md) for detailed specs. See [PROGRESS.md](./PROGRESS.md) for implementation details.

**Success metric**: 500+ stars, power users running 5+ sessions daily, "can't go back to raw terminal" feedback.

---

## Phase 3: Accounts & Collaboration `monetization`

Free accounts for sync/backup. Relay for collaboration. **All free during alpha** — billing after Dubai company + Stripe ready.

**Goal**: build user base and habits during alpha, then convert to paid.

**Accounts (done)**:
- Email + password / GitHub OAuth / Google OAuth
- Config sync & backup across devices (~20 KB per user)
- api.weplex.ai — auth + sync endpoints

**Collaboration (Team plan, $15-20/seat)** — three primitives:
1. **Teams** — create team, invite code, join. Owner/member roles.
2. **Share** — boolean toggle per Space/Session. Shared = visible to team. Default: private.
3. **Pipeline delegation** — stage `owner` field, artifact passing via relay.

Everything else is derived: shared spaces = spaces with `shared: true`, team awareness = shared session metadata in sidebar.

Implementation status:
- [x] Spectating — SpectatorView.svelte (read-only terminal view)
- [x] Pipeline relay events — spectating events on server (pipeline.gateway.ts)
- [x] Notification service — OS notifications for agent events
- [x] Presence store, chat store, team store — Svelte stores ready
- [ ] Weplex Relay (relay.weplex.ai) — WebSocket server deployment
- [ ] Context injection — CLAUDE.md prepend with artifacts from shared pipelines

See [COLLABORATIVE.md](./COLLABORATIVE.md) for full design: auth, sharing model, relay, data model, UX flows, security, monetization.

**Success metric**: 200+ users, 20+ teams using relay, relay stable under load. First paying teams after billing launch.

---

## Phase 4: Marketplace & Plugins

Weplex becomes a platform with an ecosystem.

**Goal**: community-driven growth, network effects.

**Marketplace** (backend done):
- [x] Marketplace registry (NestJS) — CRUD, search, ratings, publishing
- [ ] Agent marketplace — browse, install community agents (frontend)
- [ ] Pipeline marketplace — browse, install community pipelines (frontend)
- [ ] GitHub-based distribution: `weplex install github.com/user/repo`

**Plugin system** (architecture done):
- [x] Plugin API (session type, tray icon, tray panel, pane header)
- [x] Plugin Host — load/unload from ~/.weplex/plugins/ (plugin_host.rs)
- [x] Plugin loader — dynamic JS import (pluginLoader.ts)
- [x] Browser plugin — Chrome via CDP (plugins/browser.rs)
- [ ] More plugins from community

See [PLUGINS.md](./PLUGINS.md) for plugin architecture. See [IDEAS.md](./IDEAS.md) for marketplace details.

**Success metric**: 10+ community agents/pipelines, 3+ plugins, marketplace generating discovery.

---

## Phase 5: Enterprise

Features that justify custom pricing for larger organizations.

**Goal**: enterprise deals, $10K+ ARR per customer.

- Task tracker sync (Linear, Jira, GitHub Issues — bidirectional)
- SSO (Google, GitHub, SAML)
- Audit log (who spectated what, pipeline history)
- E2E encryption for PTY streams
- Self-hosted relay (only on demand, not proactive)
- Team analytics dashboard

**Success metric**: first enterprise deal, self-hosted relay requested by paying customer.

---

## Backlog (no phase assigned)

Feature ideas without a timeline. See [IDEAS.md](./IDEAS.md) for full list.

- Terminal recording & replay
- Session templates (predefined command + cwd + profile)
- Global sessions (pinned across all spaces)
- SSH enhancements (latency indicator, tunnel management)
- Weplex Assistant (Claude session that manages your agents/pipelines)
- Terminal Decorations (hover-triggered inline actions)
- Mobile-responsive web viewer (spectating via browser)

---

## Non-goals

- Built-in AI (Weplex orchestrates deterministically, doesn't think)
- Mobile app
- Integrated code editor (this is a terminal, not an IDE)

---

## Architecture Principles

1. **Deterministic orchestrator** — Rust state machine, not AI. YAML is law
2. **MCP-first** — Weplex MCP Server for completion detection + artifact passing
3. **Interactive sessions** — each pipeline stage = full PTY, not headless
4. **Two levels** — Weplex controls flow (deterministic), agent controls execution (autonomous)
5. **Progressive complexity** — simple terminal → pipelines → collaboration → platform
6. **Apache 2.0 always** — terminal that sees every keystroke must be auditable

---

## Monetization Summary

**Alpha (current):** all features free. Billing after Dubai company + Stripe ready.

| Tier | Price | Key value |
|------|-------|-----------|
| **Free** | $0 | Terminal + pipelines + agents + account + sync/backup |
| **Team** | $15-20/seat | Relay: spectating, pipeline delegation, Team Dashboard, context injection |
| **Enterprise** | Custom | Task tracker sync, SSO, audit, E2E encryption |

Alpha cost: ~$50-100/month (VPS + bandwidth). No investors needed.

See [COLLABORATIVE.md](./COLLABORATIVE.md) for alpha strategy, conversion funnel, and revenue projections.

---

## Document Map

| Document | What it covers |
|----------|---------------|
| [DESIGN.md](./DESIGN.md) | UI/UX specs, layout, components, design system |
| [PRODUCT.md](./PRODUCT.md) | Vision, positioning, competitive landscape, monetization |
| [COLLABORATIVE.md](./COLLABORATIVE.md) | Team features: auth, relay, spectating, context injection |
| [PLUGINS.md](./PLUGINS.md) | Plugin architecture, Browser plugin reference |
| [LAUNCH.md](./LAUNCH.md) | Go-to-market strategy, launch timeline, channels |
| [PROGRESS.md](./PROGRESS.md) | Implementation log — what's built and how |
| [IDEAS.md](./IDEAS.md) | Feature backlog, future ideas |
| [COMPETITORS.md](./COMPETITORS.md) | Competitive analysis |
