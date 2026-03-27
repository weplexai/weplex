# Weplex — Product Vision

## What is it

Weplex is the terminal with a built-in pipeline engine for AI coding agents.

A full-featured terminal that orchestrates AI agents through deterministic, multi-session pipelines. Each pipeline stage runs as a separate visible session — you see every step, control every handoff, mix any agents. Powered by Claude. Open to any agent.

## The Problem

AI coding agents are powerful but the tooling around them is primitive. Developers face a set of painful, unsolved problems:

### Pain 1 — Blindness
You launch an agent, switch to something else, come back 20 minutes later. Is it done? Stuck? Waiting for input? Made a mess? You open the tab and scroll through a wall of text. This happens dozens of times a day with no good solution.

### Pain 2 — Coordination overhead
You want two agents working in parallel — frontend and backend, or implement + test. So you open two terminal tabs, manually copy context between them, and babysit both. Each agent has no idea the other exists. This is not orchestration — it's manual labor.

### Pain 3 — Cost blindness
$0.82 here, $2.40 there. You don't know what you're spending until you check the Anthropic dashboard — and by then it's too late to course-correct. There's no budget awareness in the workflow itself.

### Pain 4 — Context loss on restart
Close the terminal, restart the machine, and you're back to square one: re-explain the task, re-load context, figure out where the agent was. Agent resume exists (`--continue`) but the surrounding context is gone.

### Pain 5 — Silent conflicts
Two agents quietly editing the same file. You find out when there's a merge conflict — or worse, when one has silently overwritten the other's work.

---

## The Three Flows That Must Be Perfect

Weplex isn't a collection of features — it's three flows, executed flawlessly.

### Flow 1: Launch & Forget
*"I started an agent. I want to forget about it until it needs me."*

```
Today:  open terminal → run claude → periodically poke the tab
        → miss when it's stuck → no way to know it's done

Weplex:   start session → do other things → notification: "done" or "waiting for you"
        → open → see a summary of what changed
```

Requires: status detection, Stop hook notifications, activity summary in detail panel.

### Flow 2: Parallel Without Pain
*"I want two agents working in parallel without fighting each other."*

```
Today:  two tabs → manual babysitting → copy-paste context → discover conflict after the fact

Weplex:   Project Dashboard → see both agents, their files, any conflicts in real time
        → Weplex auto-injects context about the other agent into each CLAUDE.md
        → conflict warning before it becomes a problem
```

Requires: Project Dashboard, CLAUDE.md injection, PostToolUse hooks, conflict detection.

### Flow 3: Real Orchestration
*"I want to say 'implement this feature' and get back a reviewed, tested result."*

```
Today:  manually run PM, copy output, run architect, copy output, run backend...
        Most developers: skip half the steps or don't know the workflow exists

Weplex:   "Start Pipeline" → choose Feature template → describe task
        → PM session runs, finishes → output passed to Architect session
        → Architect finishes → Backend session starts → Security + Tester in parallel
        → Each stage = separate session in sidebar, visible, controllable
        → Pipeline Dashboard shows the full flow in real time
```

Weplex is the orchestrator. A Rust state machine reads the pipeline YAML, spawns each stage as a separate interactive PTY session, detects completion via MCP (or status polling fallback), and passes artifacts to the next stage. No AI in the orchestration layer — deterministic, predictable, repeatable.

Each stage is a full interactive session — user sees agent reasoning, can answer questions, grant permissions. Not headless one-shot execution.

Each stage can be any MCP-compatible agent: Claude Code, OpenCode, Crush, Codex, Gemini CLI, Aider — user chooses per stage. Through OpenCode/Crush, any LLM is available: Claude, GPT, DeepSeek, Qwen, Gemini, local models. One pipeline, multiple models.

**Weplex MCP Server** is the core orchestration mechanism. Agents call MCP tools to:
- `deck_stage_complete()` — signal stage completion (reliable detection)
- `deck_get_artifact("pm")` — read previous stage results (structured artifact passing)
- `deck_pipeline_info()` — know their role, task, stage index

**Two levels of orchestration:**
- **Level 1 (Weplex)**: pipeline flow — PM → Architect → Dev → Security → PM. Deterministic, YAML-defined.
- **Level 2 (Agent)**: within each stage, the agent decides how to execute. Claude might use Agent tool to delegate to frontend + backend sub-agents. Qwen might do it all itself. Weplex doesn't care — it waits for the stage to complete.

Requires: Pipeline engine (Rust), artifact passing, stage completion detection, Pipeline Dashboard.

---

## Pipeline Architecture

This is the core insight that separates Weplex from everything else.

**Weplex is a deterministic pipeline orchestrator for AI coding agents.** Not an AI that orchestrates — a Rust state machine that executes YAML-defined flows. Each stage is a separate PTY session, visible in the sidebar, with full output capture and artifact passing between stages.

### The Insight

```
What most developers do:
  "Write me a user authentication module"  →  one agent, one shot, hope for the best

What a pipeline does:
  PM reads the task and produces a brief         (Session 1: claude --print)
  → Architect designs the approach                (Session 2: claude --print, receives PM output)
  → Backend implements the plan                   (Session 3: claude --print or codex)
  → Security reviews for vulnerabilities          (Session 4: claude --print)
  → PM verifies requirements met                  (Session 5: claude --print)
  Each stage = separate session. Each visible. Each controllable.
```

### Two Modes

**Pipeline mode** — Weplex orchestrates, deterministic:
- User picks a pipeline template (⌘N → Pipeline), describes the task
- Weplex reads YAML → creates interactive sessions sequentially (or parallel where defined)
- Each stage = full interactive PTY session (user sees reasoning, can interact)
- Completion detected via Weplex MCP Server (`deck_stage_complete()`) or status polling fallback
- Artifacts passed via MCP (`deck_get_artifact()`) or shared filesystem
- User sees every stage in sidebar with status, cost, output

**Autopilot mode** — single agent session, agent decides:
- User just runs an agent (Claude, OpenCode, etc.) in a regular session
- Agent uses its own tools internally (black box)
- Weplex shows what it can via hooks/JSONL parsing
- Good for quick tasks where pipeline is overkill

### Two Levels of Orchestration

```
Level 1 — Weplex (deterministic, YAML-defined):
  Pipeline: PM → Architect → Developer → Security → PM
  Weplex controls: order, context passing, completion detection

Level 2 — Agent within stage (autonomous):
  Developer stage: Claude might use Agent tool → backend + frontend sub-agents
  Or: Qwen just does it all linearly
  Weplex doesn't care — it waits for the stage to complete
```

### Agent-Agnostic Design

**Agents = Weplex format.** `~/.weplex/agents/*.yaml` with a `binary` field:

```yaml
# ~/.weplex/agents/backend.yaml
name: backend
description: "Backend developer"
binary: claude              # or: codex, aider, qwen, gemini
prompt: "You are a senior backend developer..."
```

When `binary: claude`, Weplex gets deep integration (JSONL parsing, cost tracking, hooks, sub-agent visibility). When `binary: opencode` or `binary: crush`, any LLM model is available (DeepSeek, Qwen, GPT, Gemini, local models) with MCP support for reliable pipeline orchestration. When `binary: aider` or `binary: codex`, Weplex uses PTY status detection as fallback.

**Multi-model pipelines** — the killer feature. Mix models per stage via OpenCode/Crush:

```yaml
stages:
  - agent: pm
    binary: claude          # Opus for analysis (Max subscription)
  - agent: backend
    binary: opencode
    model: deepseek-coder   # DeepSeek for code (cheap, fast)
  - agent: reviewer
    binary: opencode
    model: qwen-max          # Qwen for review
```

**Claude Code agents** (`~/.claude/agents/*.md`) are also read and displayed in the agents panel, but Weplex agents (`.yaml`) are the pipeline-native format.

**Pipelines = Weplex format.** `~/.weplex/pipelines/*.yaml`:

```yaml
# ~/.weplex/pipelines/feature.yaml
name: Feature Development
description: Full pipeline from task intake to acceptance

stages:
  - name: pm
    agent: pm
    role: Study task, find related issues, produce Task Brief

  - name: architect
    agent: architect
    receives: [pm]
    role: Design approach based on Task Brief

  - name: implement
    agent: backend
    receives: [architect]
    role: Implement the planned changes

  - name: review
    parallel:
      - agent: security
        receives: [implement]
        role: Check for vulnerabilities
      - agent: tester
        receives: [implement]
        role: Write tests
        optional: true

  - name: accept
    agent: pm
    receives: [implement, security, tester]
    role: Verify requirements met, leave comment

layout:
  pm: { x: 100, y: 50 }
  architect: { x: 100, y: 200 }
  implement: { x: 100, y: 350 }
  security: { x: 0, y: 500 }
  tester: { x: 200, y: 500 }
  accept: { x: 100, y: 650 }
```

### How Pipeline Engine Works

```
1. User: ⌘N → Pipeline tab → picks template + cwd + task + profile → Launch
2. Weplex: reads pipeline.yaml, starts Weplex MCP Server
3. For each stage:
   a. Create interactive PTY session with agent binary (claude, opencode, crush, etc.)
   b. Inject Weplex MCP Server config so agent has pipeline tools
   c. Inject stage prompt (role + task) when agent is ready
   d. User interacts with agent (sees reasoning, answers questions, grants permissions)
   e. Agent calls deck_stage_complete() via MCP → Weplex advances to next stage
   f. Fallback: status polling (idle detection) for non-MCP agents
4. Parallel stages: create all sessions, wait for all to complete
5. Pipeline complete → notify user
```

**Artifact passing**: via Weplex MCP Server. Agent calls `deck_get_artifact("pm")` to read structured output from previous stages. Fallback: shared filesystem (all stages work in same cwd, files written by stage N are readable by stage N+1).

**Completion detection**: via MCP tool call `deck_stage_complete()` (reliable). Fallback: JSONL polling for Claude, timeout for others.

### What This Means

**For individual developers:**
- One click → professional-grade workflow (PM, architect, impl, security)
- See every stage as a separate session — cost, output, status
- Mix agents: Claude for architecture, cheaper model for implementation
- Pause between stages, re-run a failed stage, skip optional stages

**For teams:**
- Standardized pipeline templates — everyone follows the same process
- Share on GitHub — company-wide dev standard in a YAML file
- Pipeline cost tracking — how much did this feature actually cost?

**For the ecosystem:**
- Community agents and pipeline templates via Weplex marketplace
- Package format: `agent.yaml` + `weplex.yaml` (agents), `pipeline.yaml` + `weplex.yaml` (pipelines)
- Both install to `~/.weplex/` — Weplex's own ecosystem

### Pipeline Use Cases

Pipelines are not just for code development. They work for any repeatable multi-step workflow.

#### Code: Feature Development
```yaml
# PM → Architect → Backend+Frontend → Security + Tests → PM Review
# Classic software pipeline. Each stage = separate session, full visibility.
stages:
  - name: pm
    agent: pm
    role: Study task, produce Task Brief
  - name: architect
    agent: architect
    receives: [pm]
    role: Design approach
  - name: implement
    agent: backend
    receives: [architect]
    role: Implement changes
  - name: review
    parallel:
      - agent: security
        receives: [implement]
        role: Check vulnerabilities
      - agent: tester
        receives: [implement]
        role: Write tests
  - name: accept
    agent: pm
    receives: [implement, security, tester]
    role: Verify requirements met
```

#### Content: Ship Post + Dashboard Update
Real use case: after finishing a hackathon project, write a ship/lesson post and update the tracking dashboard. Done 3-5 times per day.

```yaml
name: Ship Post
description: Write post (EN + RU) + update dashboard data

stages:
  - name: write
    agent: content-writer
    role: |
      Read twitter/strategy.md and twitter/templates.md for tone of voice.
      Write a SHIP or LESSON post (EN + RU).
      Include: what was built, tech stack, links, tag protocol.
      Follow "no self-hype" rule.

  - name: update
    agent: dashboard-updater
    receives: [write]
    role: |
      Update dashboard/data.js: hackathon status, notes, shipPost link.
      Update dashboard/content-data.js: add post entry with text, tags, url.
```

Without pipeline: one big Claude session reads strategy.md, templates.md, data.js, content-data.js every time. Pollutes context, repeats work.

With pipeline: "Start Pipeline → Ship Post → task: yo-savings, missed deadline by 2h". Two focused sessions, each with minimal context, done in 2 minutes.

#### Ops: Security Audit
```yaml
stages:
  - name: scan
    agent: security-scanner
    role: Check for OWASP top 10, dependency vulnerabilities, exposed secrets
  - name: report
    agent: report-writer
    receives: [scan]
    role: Write structured security report with severity levels and fix suggestions
```

#### Research: Competitor Analysis
```yaml
stages:
  - name: research
    agent: researcher
    role: Study the competitor website, extract features, pricing, positioning
  - name: compare
    agent: analyst
    receives: [research]
    role: Compare with our product, identify gaps and opportunities
  - name: write
    agent: content-writer
    receives: [compare]
    role: Write a summary for the team with actionable recommendations
```

**Key insight**: pipelines are not a developer-only feature. They're a **workflow automation tool** that happens to live in a terminal. Content creators, researchers, ops engineers — anyone who uses AI agents for repeatable multi-step work benefits from pipelines.

### Agents = Pre-Loaded Context

The biggest hidden cost of working with AI agents: **re-explaining context every session**. Where are the credentials? What's the project structure? What's the tone of voice? What conventions do we follow? The agent searches, reads, sometimes finds the wrong file — burning tokens and time on the same discovery every single time.

Weplex agents solve this. The `prompt` field is pre-loaded context per role:

```yaml
# Server ops agent — knows where credentials are
name: server-ops
binary: claude
prompt: |
  Server credentials are in ~/Documents/servers/access.md
  Always read this file FIRST. Never ask where credentials are.

# Content writer — knows the brand voice
name: content-writer
binary: claude
prompt: |
  Read twitter/strategy.md for brand voice and twitter/templates.md for post formats.
  Rules: no self-hype, self-ironic tone ok, always tag @protocol.
  EN + RU versions for every post.

# Dashboard updater — knows the data structure
name: dashboard-updater
binary: claude
prompt: |
  Dashboard data is in dashboard/data.js and dashboard/content-data.js.
  data.js: hackathon entries with status, tier, content object.
  content-data.js: posts array with id, type, text, textRu, tags, url, metrics.
  Never create new files — only update existing arrays.

# Backend dev — knows project conventions
name: backend-nestjs
binary: claude
prompt: |
  NestJS project. TypeORM + PostgreSQL. Follow patterns in src/modules/.
  DTOs with class-validator. Entities with snake_case columns.
  Always add Swagger decorators. Never use synchronize:true.
```

Without agents: "Hey Claude, find my server credentials... they're somewhere in Documents... yeah that file... now SSH into the staging server and..."

With agents: "Start server-ops → deploy latest to staging". Agent already knows where everything is.

This compounds with pipelines. Each pipeline stage gets a specialized agent with pre-loaded context. The content-writer doesn't waste tokens reading data.js structure. The dashboard-updater doesn't waste tokens reading strategy.md. Each agent knows exactly what it needs — nothing more, nothing less.

**Agent = role + binary + pre-loaded knowledge.** Not just "which model to run" — but "which model, knowing what".

---

## Positioning

**Primary**: Powered by Claude. Open to any agent.

**One-liner**: The terminal with a built-in pipeline engine for AI coding agents. Deterministic multi-session orchestration, agent-agnostic, open source, cross-platform.

Not trying to be:
- A general-purpose terminal (iTerm, Warp, Alacritty)
- An AI IDE (Cursor, Windsurf)
- A cloud terminal (SSH manager)
- Another AI wrapper

Trying to be:
> **The orchestration layer for AI coding agents — starting with Claude, open to all**

**Target user**: Developer who uses AI coding agents daily (Claude Code, OpenCode, Codex, Aider) and wants structured multi-model workflows, not just raw terminal sessions.

**Secondary audience**: Teams that want standardized agent workflows across all developers.

### Key differentiators
1. **Deterministic pipeline engine** — Rust state machine orchestrates multi-session pipelines from YAML. Each stage = interactive PTY session, visible, controllable. MCP-first orchestration. No competitor has this.
2. **Agent-agnostic + multi-model** — any MCP-compatible agent: Claude Code, OpenCode (131k⭐), Crush, Aider, Codex, Gemini CLI. Via OpenCode/Crush: any LLM (DeepSeek, Qwen, GPT, local). Mix models per stage in one pipeline.
3. **Two levels of orchestration** — Weplex controls pipeline flow (deterministic), agent controls execution within each stage (autonomous). Claude might spawn sub-agents, Qwen does it alone — Weplex doesn't care.
4. **Deep Claude integration** — when Claude is the agent, Weplex gets bonus features: JSONL parsing, cost tracking, hooks, sub-agent visibility. Best experience with Claude, works with anything.
5. **Pipeline + Agent marketplace** — Weplex-owned ecosystem. Community shares reusable agents and pipelines.
6. **Open source (MIT)** — terminal sees every keystroke — trust requires auditability
7. **Lightweight** — Tauri (50-150MB RAM), not Electron
8. **Multi-account profiles** — personal/work/client per Space

See [COMPETITORS.md](./COMPETITORS.md) for full competitive analysis.

## Current state (v0.1)

A solid terminal foundation:

- **Spaces** — Arc-style context switching. Each space has its own color, profile, and session list
- **Hyperspace** — unified view of all sessions across all spaces, with 3 grouping modes
- **Profiles** — isolated environments with separate API keys / config dirs
- **Session persistence** — sessions survive app restarts, agents auto-resume (`--resume <uuid>`)
- **Agent detection** — recognizes Claude Code, OpenCode, Aider, Gemini CLI, Codex by command
- **Usage panel** — reads Claude's JSONL session files: tokens, model, turns, cache stats
- **Session notes** — markdown notes attached to each session
- **WebGL terminal** — GPU-accelerated rendering, bundled JetBrains Mono Nerd Font

## Roadmap

Each stage is a self-contained value proposition. Users get value at v0.1 — they don't need to wait for v0.3.

```
v0.1  Best terminal for AI coding agents
      → status indicators, cost tracking, profiles, Hyperspace, session persistence
      → "I want this instead of iTerm for Claude/Codex/Aider"

v0.2  Pipeline Engine (MVP)
      → Weplex MCP Server, interactive multi-session pipelines, multi-model support
      → "one click and Weplex runs PM → Architect → Backend → Security as interactive sessions"

v0.3  Deep Integration & Canvas
      → Claude hooks, visual canvas editor, dashboards, conflict detection
      → "I see every stage, every cost, every artifact. Full control."

v1.0  Marketplace & Teams
      → Agent/pipeline marketplace, team libraries, cost analytics
      → sell to enterprise
```

### v0.1 — Foundation (in progress)
- [x] Detect which agent is running (claude, aider, opencode, gemini, codex)
- [x] Capture session ID, model, auth type
- [x] Read token/cost usage from JSONL session files
- [x] Session notes with persistence
- [x] Hyperspace — all sessions across all spaces in one view
- [x] Spaces with profiles, colors, session carousel
- [x] Agents panel (reads from ~/.claude/agents/, no hardcode)
- [ ] Notifications when agent finishes or gets stuck
- [ ] Header bar, split views

### v0.2 — Pipeline Engine
- [x] Weplex agent format: `~/.weplex/agents/*.yaml` with `binary` field (agent-agnostic)
- [x] Pipeline engine: Rust state machine — read YAML, track stages, manage runs
- [x] Pipeline UI: stage sessions grouped in sidebar with ✓/●/○ status indicators
- [x] "Start Pipeline" dialog (⌘N → Pipeline tab) — pick template, task, cwd, profile
- [x] Visual pipeline editor — n8n-style canvas with pan/zoom, drag-to-reorder
- [x] Agent editor — form UI for Weplex agent format (.yaml) with binary selector
- [x] Security: binary allowlist, env var denylist, output buffer cap
- [ ] **Weplex MCP Server** — core orchestration:
  - `deck_stage_complete()` — reliable completion detection
  - `deck_get_artifact()` — structured artifact passing
  - `deck_pipeline_info()` — stage context for agents
- [ ] Interactive pipeline stages: each stage = full PTY session
- [ ] Multi-model pipelines via OpenCode/Crush (DeepSeek, Qwen, GPT, local models)
- [ ] Pipeline Dashboard — flow view with stage cards, cost per stage, clickable sessions

### v0.3 — Deep Integration & Canvas
- [ ] Claude Code hooks injection (PreToolUse, PostToolUse, Stop) — bonus features when binary=claude
- [ ] Sub-agent visibility — Claude's Agent tool sub-agents detected via hooks
- [ ] Weplex MCP Server v2 — cross-session communication (list/create/read/send to other sessions)
- [ ] Session hierarchy — parentId, children indented in sidebar
- [ ] Orchestration Dashboard — agent tree, timeline, activity feed
- [ ] Project Dashboard — sessions by cwd, git status, conflict detection
- [ ] Terminal Decorations — hover-triggered inline actions
- [ ] Git worktree auto-isolation for parallel sessions

### v1.0 — Marketplace & Teams
- [ ] Agent marketplace — browse, install agents via Weplex UI (installs to ~/.weplex/agents/)
- [ ] Pipeline marketplace — browse, install pipelines (installs to ~/.weplex/pipelines/)
- [ ] Package format: `agent.yaml`/`pipeline.yaml` + `weplex.yaml` (marketplace metadata)
- [ ] Dependency check: pipeline `requires` → install missing agents
- [ ] GitHub distribution: `weplex install github.com/user/repo/agent-name`
- [ ] Team private pipeline/agent library
- [ ] Team cost view — aggregate spend per team, per project
- [ ] Session activity log — audit trail
- [ ] SSO, access controls

## Git Worktree Isolation (Phase 3)

Problem: multiple agents in the same repo create file conflicts.
Solution: Weplex auto-creates git worktrees for parallel agent sessions.

Three paths to worktree creation:
1. **Upfront** — user specifies branch in New Session Dialog
2. **Reactive** — agent creates a branch → Weplex offers to isolate into worktree
3. **Manual** — user clicks "Isolate" in session context menu

Reactive is the most natural — detected via `git branch --show-current` polling (already needed for header bar). When branch changes from main to feature/* and other sessions exist in same repo → prompt.

Lifecycle: create worktree → agent works in isolation → on session close: merge? keep? delete?

## Marketplace Architecture

Core principle: Weplex core stays lightweight. Agents and pipelines are the ecosystem layer.

```
Weplex Core (MIT, always free)
├── Terminal (PTY, xterm.js)
├── Sessions, Spaces, Profiles
├── Pipeline engine (Rust state machine)
├── Agent detection & panel
├── Visual editors (agent form, pipeline canvas)
└── Marketplace client

Weplex ecosystem (~/.weplex/):
├── agents/*.yaml   → agent-agnostic format with binary field
└── pipelines/*.yaml → pipeline definitions

Also reads (Claude bonus):
└── ~/.claude/agents/*.md → Claude Code native agents (displayed in panel)
```

### Package Format

Agent package: `agent.yaml` (Weplex format) + `weplex.yaml` (marketplace metadata)
Pipeline package: `pipeline.yaml` (Weplex format) + `weplex.yaml` (marketplace metadata)

`weplex.yaml` stays in registry, never installed locally. Agent/pipeline files stay clean.

### Distribution (progressive)

| Stage | Distribution | Effort |
|-------|-------------|--------|
| Phase 2 | Manual: create via visual editors | None |
| Phase 3 | CLI: `weplex install github.com/user/repo` | Days |
| Phase 4 | In-app marketplace with search, ratings | Weeks |

### Why This Creates a Moat

The pipeline engine + agent-agnostic format is Weplex-exclusive. No other tool has deterministic multi-session pipelines with per-stage agent choice. A live pipeline ecosystem can't be copied. The more community pipelines → the more value for new users → the more users → better pipelines.

## Why This Could Be a Unicorn

### The window is open — but not forever

AI agent workflows are in the "tabs era" — everyone is running them in raw terminal windows with no tooling layer. This is exactly where browsers were before Arc, where git hosting was before GitHub, where project management was before Linear. The category winner hasn't been decided yet.

The window: **12-18 months** before JetBrains, GitHub Copilot, or a well-funded startup moves seriously into this space. Warp is closest but going in a different direction (their own AI assistant, closed source). No one is building the cross-agent orchestration visualization layer.

### The market is large and growing fast

The addressable market today: ~500K developers actively using Claude Code, Aider, OpenCode, or Gemini CLI. Growing 2x every few months as agent-first development becomes mainstream.

Enterprise is the real prize: a team of 5 engineers each running 3-4 agents simultaneously generates enormous value from visibility and coordination tools. At $30/seat/month, a 50-person engineering team is $18K ARR — and they'll pay it to prevent wasted agent work and cost overruns.

### Network effects

The moment Weplex adds team features (v1.0), it gains network effects:
- Every new engineer on a team makes Weplex more valuable for the whole team (shared spaces, shared activity logs)
- Teams with Weplex have a coordination advantage over teams without it
- Once a team standardizes on Weplex, switching cost is high (session history, space configs, workflow integrations)

### The data moat

Weplex is the only tool with full visibility into what AI agents are doing across an entire development workflow:
- Which files are touched, how often, by which agent
- Cost per task, per project, per team
- Which agent workflows produce good outcomes vs waste
- Failure patterns (stuck sessions, conflict hotspots)

This data enables future intelligent features: smart cost budgeting, workflow recommendations, automatic orchestration patterns. No one else can build this without the terminal layer.

### New product category

```
Existing tools:
  terminal emulators  → text I/O, no agent awareness
  AI code editors     → one agent, no multi-session coordination
  project managers    → tasks and specs, no live agent state
  CI/CD pipelines     → automated but not interactive

Weplex:
  live visual workspace for multi-agent AI development
  — the missing layer between the developer and their fleet of agents
```

This is not a feature of an existing product. It's a new product category, at the right moment in history.

---

## Open Questions & Risks

Честный список того что неизвестно и что может пойти не так. Нужно проверять экспериментально, а не декларировать как решённое.

### Q1: Будет ли pipeline engine надёжно работать с разными агентами?

**Риск: средний.** Weplex — детерминированный оркестратор (Rust state machine). Основные вопросы:

| Аспект | Claude Code | OpenCode/Crush | Aider/Codex | Решение |
|--------|------------|----------------|-------------|---------|
| MCP support | ✅ | ✅ | ❌ | MCP-first, PTY fallback |
| Completion detection | MCP + JSONL + hooks | MCP | Status polling | Weplex MCP Server = universal |
| Artifact passing | MCP tools | MCP tools | Shared filesystem | MCP-first, filesystem fallback |
| Models | Claude only | Any (DeepSeek, Qwen, GPT...) | Any | Multi-model via OpenCode |
| Cost tracking | JSONL parsing | Logs | Not available | Per-binary, graceful degradation |
| Interactive mode | ✅ | ✅ | ✅ | All run as PTY sessions |

**Mitigation**: Weplex MCP Server covers Claude Code + OpenCode + Crush (MCP-compatible). For non-MCP agents (Aider, Codex): PTY status polling + shared filesystem. Each new binary = config in allowlist.

---

### Q2: Не убьёт ли xterm.js + WKWebView на macOS производительность при 10+ сессиях?

**Риск: средний.** Уже есть известный баг xterm.js + WKWebView (#3575), частично решён через Canvas renderer. Но 10+ одновременных активных PTY с output — неизвестная нагрузка.

Все сессии рендерятся одновременно (position: absolute, visibility hidden) — это архитектурное решение для мгновенного переключения. При большом количестве сессий с активным output может быть проблема.

**Что нужно проверить:** нагрузочный тест с 10-15 параллельными Claude-сессиями.

---

### Q3: Как Claude Code будет эволюционировать?

**Риск: средний → пересмотрен в сторону низкого.** Weplex строится поверх Claude Code как платформы. Anthropic может:
- Добавить нативную multi-agent UI (снижает дифференциацию)
- Изменить формат JSONL файлов (ломает cost tracking)
- Закрыть hooks API или изменить MCP протокол
- Выпустить собственный terminal wrapper

**Mitigation (пересмотрен):** Anthropic — AI safety компания, не UX компания. Hooks, JSONL и MCP созданы ими специально для сторонних интеграций — у них нет мотивации их ломать. Чем лучше Weplex → тем ценнее Claude Code → тем больше Max подписок. Это alignment of incentives, не риск. Если Anthropic выпустит свой GUI — это подтвердит категорию. Arc процветает несмотря на Chrome. Так же как VSCode не убил сторонние инструменты, а породил экосистему.

---

### Q4: Достаточно ли велика аудитория для продукта?

**Риск: низкий, но требует мониторинга.** Сейчас ~500K активных пользователей Claude Code и других CLI-агентов. Растёт быстро. Но:
- Большинство используют агентов редко — не целевая аудитория
- Целевой пользователь: запускает 2+ агентов ежедневно — их пока меньшинство
- Рынок формируется прямо сейчас

**Что следить:** adoption rate Claude Code Max планов (proxy для "power user"), количество открытых сессий в типичной рабочей сессии.

---

### Q5: Не перегружен ли продукт концепциями?

**Риск: реальный.** Spaces, Hyperspace, Profiles, Dashboards (3 вида), Decorations, Hierarchy, Pipelines, MCP Server — много концепций для нового пользователя.

**Принцип:** каждый слой должен быть опциональным и появляться только когда нужен:
- Новый пользователь видит: просто красивый терминал
- Power user открывает: Spaces, Profiles
- Multi-agent user открывает: Hyperspace, Dashboards
- Enterprise user открывает: Pipelines, Team features

Прогрессивное раскрытие сложности — не показывать всё сразу.

---

## Why not Warp

Warp is a great terminal with AI chat features. Weplex is different:

- Warp's AI is their own assistant. Weplex orchestrates any AI coding agent through deterministic pipelines.
- Warp is closed source. Weplex is MIT — for a tool that sees every keystroke, this matters
- Warp adds AI to a terminal. Weplex builds a pipeline engine around agent capabilities
- Warp is one session with a chat panel. Weplex is multi-session orchestration: Spaces, pipelines, cross-session visibility

## Why not Superset

Superset runs many agents in parallel on isolated git worktrees. Weplex is different:

- Superset is a code editor wrapper. Weplex is a real terminal — PTY, shell, SSH, any command
- Superset = parallel agents on N independent tasks. Weplex = deterministic pipeline on one task (PM→Architect→Backend→Security), each stage a separate session
- Superset leaves you with N outputs to review. Weplex gives you one reviewed, tested result with full stage visibility
- Superset is Elastic License (no commercial forks). Weplex is MIT
- Superset requires worktrees as mandatory UX. Weplex offers them reactively when relevant

## Monetization

**Open Core model.** Core is MIT and always free — this is non-negotiable for a terminal app (trust, adoption, community). Team features are paid.

| Tier | Price | What's included |
|------|-------|-----------------|
| **Free (Core)** | $0 forever | Full terminal, all single-user features, Hyperspace, Profiles, Dashboards, community pipeline templates |
| **Team** | $20/seat/month | Private team pipeline library, shared agent templates, team cost analytics, SSO |
| **Enterprise** | Custom | Private template registry, audit log, on-prem, SLA, custom integrations |

### Why pipelines change the monetization story

Without pipelines: "pay for shared spaces" — weak value prop, easy to ignore.

With pipelines: a team creates `acme-frontend-feature` pipeline with their specific agent configs. Every frontend developer on the team works with identical Claude settings. One person updates an agent template — everyone gets it. New hire installs Weplex + `weplex install acme/pipelines` → immediately works to team standard.

This is not a convenience tool. This is an **engineering standard** for the company. Companies don't cancel their engineering standards.

The path from free to paid becomes natural:
1. Developer discovers community pipelines, starts using them personally (free)
2. Developer customizes a pipeline for their team's stack → needs private library (Team)
3. CTO sees cost analytics: "$12K saved on security reviews this quarter" → Enterprise deal

### Marketplace (Phase 4)

Weplex owns the marketplace. Discovery, install, updates all via Weplex UI.

```
Community:  weplex install github.com/user/weplex-agents/backend-go
Team:       weplex install acme-corp/pipelines  (private, Team tier)
Enterprise: Internal registry — versioned, audited, access-controlled
```

Two levels: agents (`~/.weplex/agents/`) and pipelines (`~/.weplex/pipelines/`). Pipeline `requires` field lists needed agents — Weplex offers to install missing ones.

The marketplace creates network effects no competitor can replicate: more community templates → more value for new users → more users → better templates. A live pipeline ecosystem cannot be copied.

Growth path:
1. Open source → viral adoption among Claude Code power users
2. Developer brings Weplex to their team → Team tier (private pipeline library)
3. Engineering manager sees ROI (cost per feature, standardized workflows) → Enterprise

The individual developer tier is the growth engine. The team pipeline library is the revenue engine.

## License

**MIT.** Rationale:

- Terminal = security-sensitive app (sees all keystrokes). Open source = trust
- All major terminal competitors are open source (Ghostty, Alacritty, WezTerm, Kitty)
- MIT maximizes adoption, minimizes friction — closed source terminal from unknown author = dead on arrival
- Open Core for monetization: free core + paid team features (GitHub, GitLab, HashiCorp model)
- Code is not the moat. Speed of iteration, community, and brand are.

## Tech stack

- **Tauri 2** (Rust backend, WebView frontend) — native performance, no Electron bloat
- **Svelte 5** — reactive UI with runes
- **xterm.js** + WebGL renderer — GPU-accelerated terminal
- **portable-pty** (Rust) — PTY management, same lib as WezTerm
- **MIT license**
- Cross-platform: macOS, Linux, Windows
