# Weplex — Design Specification

> Working name. Final name TBD.

## Vision

The terminal with a built-in pipeline engine for AI coding agents.

Powered by Claude. Open to any agent. A full-featured terminal that orchestrates AI agents through deterministic, multi-session pipelines. Each stage is a separate visible session. You see every step, control every handoff, mix any agents.

## Positioning

- **Deterministic pipeline orchestrator** — Rust state machine, not AI. YAML = law.
- **MCP-first orchestration** — Weplex MCP Server for completion detection + artifact passing. Universal standard.
- **Agent-agnostic** — any MCP-compatible CLI agent: Claude Code, OpenCode, Crush, Aider, Codex, Gemini CLI.
- **Multi-model pipelines** — mix Claude, DeepSeek, Qwen, GPT in one pipeline via OpenCode/Crush.
- **Interactive sessions** — each stage = full PTY session. User sees reasoning, can interact.
- **Two levels** — Weplex controls flow (pipeline), agent controls execution (within stage)
- **Claude = best experience** — deep integration (hooks, JSONL, cost) when binary=claude. Graceful degradation for others.
- Any terminal session (bash, zsh, ssh, etc.) works as expected

## Target Audience

1. Developers who use AI coding agents daily and want structured, visible workflows
2. Developers who want PM → Architect → Backend → Security pipelines without manual coordination
3. Teams that want standardized agent workflows across all developers
4. Anyone who has outgrown raw terminal tabs for AI agent work

## Competitive Landscape

See [COMPETITORS.md](./COMPETITORS.md) for full analysis (direct competitors, modern terminals, IDE terminals, risks, positioning).

**Summary**: No existing product combines a real terminal (PTY, shell, SSH) with AI agent session management. Weplex fills the empty quadrant: **native GUI terminal + AI agent intelligence**.

---

## Tech Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Runtime | Tauri 2.2+ | Lightweight (50-150MB RAM), Rust backend, cross-platform |
| Frontend | Svelte 5 + Vite | Best DX for Tauri, smallest bundle (~15KB) |
| Terminal | xterm.js (Canvas renderer) | Industry standard, used by VS Code |
| PTY | portable-pty (Rust) | Battle-tested (WezTerm), cross-platform |
| Styles | CSS Variables | Custom design system, theming |
| Icons | lucide-svelte | Consistent, MIT, Svelte package |
| Build | Vite + Cargo | Standard for Tauri + Svelte |

### Platform Rendering

| Platform | WebView Engine | xterm.js Status |
|----------|---------------|-----------------|
| macOS | WKWebView (WebKit) | Canvas renderer (avoid WebGL quirks) |
| Windows | WebView2 (Chromium) | Full support |
| Linux | WebKitGTK | Depends on distro version |

### Key Technical Risks

1. **xterm.js + WKWebView (macOS)**: Known detection issue (#3575). Mitigation: patch `isSafari` in Platform.ts, use Canvas renderer
2. **portable-pty blocking I/O**: Needs thread management for multiple PTYs. Proven pattern from WezTerm/Terminon
3. **Cross-platform WebView consistency**: Test early on all three platforms

---

## Layout Architecture

### Primary Layout: Sidebar + Terminal

```
+-------------+----------------------------------------------------------+
| Sidebar     | Header                                                   |
| (240px)     +----------------------------------------------------------+
|             |                                                          |
|             |                                                          |
|             |                                                          |
|             |               xterm.js Terminal                          |
|             |               (all remaining space)                      |
|             |                                                          |
|             |                                                          |
|             |                                                          |
|             +----------------------------------------------------------+
|             | Status Bar                                               |
+-------------+----------------------------------------------------------+
```

### With Detail Panel (toggle Cmd+I)

```
+-------------+---------------------------------------+------------------+
| Sidebar     | Header                                | Detail Panel     |
| (240px)     +---------------------------------------+ (280px)          |
|             |                                       |                  |
|             |                                       | Git info         |
|             |          xterm.js Terminal             | Cost tracking    |
|             |          (shrinks but remains primary) | Agent info       |
|             |                                       | MCP status       |
|             |                                       |                  |
|             +---------------------------------------+                  |
|             | Status Bar                            |                  |
+-------------+---------------------------------------+------------------+
```

### With Split View (Cmd+D horizontal, Cmd+Shift+D vertical)

```
+-------------+----------------------------------------------------------+
| Sidebar     | Header (active session)                                  |
|             +-----------------------------+----------------------------+
|             |                             |                            |
|             |   Terminal 1                |   Terminal 2               |
|             |   (active)                  |   (secondary)              |
|             |                             |                            |
|             |                             |                            |
|             +-----------------------------+----------------------------+
|             | Status Bar                                               |
+-------------+----------------------------------------------------------+
```

### Sidebar States

| State | Width | Trigger | Behavior |
|-------|-------|---------|----------|
| Expanded | 240px | Default, Cmd+B toggle | Full session list with names |
| Collapsed | 48px | Cmd+B toggle | Icons + status dots only |
| Overlay | 240px over terminal | Hover on collapsed sidebar | Appears over terminal, doesn't push it |

---

## Sidebar Design

### Structure (top to bottom)

```
+-------------------+
| Spaces            |  Color-coded context switcher (like Arc)
+-------------------+
| Search            |  Cmd+F: fuzzy search across all sessions
+-------------------+
| PINNED            |  Manually pinned sessions (always visible)
|   session         |
|   session         |
+-------------------+
| TODAY             |  Auto-grouped by time
|   session         |
|   session         |
+-------------------+
| THIS WEEK         |
|   session         |
+-------------------+
| OLDER             |  Collapsible, for forgotten sessions
|   session         |
+-------------------+
| Footer            |  Stats + New Session button
+-------------------+
```

### Spaces

Spaces are contexts (like Arc). Each space has:
- Name + color + icon (auto-generated from first letter, customizable later)
- Own set of sessions (pinned + regular)
- **Profile** assignment (which account/env to use)
- Appearance overrides (sidebar background, etc. — future)
- Switch: click icon or Cmd+1/2/3

Displayed as colored pills/icons at the top of sidebar:

```
  [◆] [H] [W] [P] [+]
   ^    ^   ^   ^   ^
   |    |   |   |   +-- Create new (opens modal)
   |    |   |   +------ Personal (green)     → Profile: Default
   |    |   +---------- Work (blue)          → Profile: Work
   |    +-------------- Hackathons (purple)  → Profile: Default
   +------------------- Hyperspace (always first, see Hyperspace section)
```

Users can create/edit/delete spaces. Each space references a Profile (defaults to "Default").

**Create Space** — `[+]` button opens a modal:

```
┌───────────────────────────────────┐
│  New Space                        │
│                                   │
│  Name:     [                  ]   │
│  Color:    ● ● ● ● ● ●          │
│                                   │
│  Profile:  [Default          ▼]   │
│                                   │
│  ▶ Appearance                     │
│                                   │
│          [Cancel]  [Create]       │
└───────────────────────────────────┘
```

- Name + Color are required (Color has 6 preset options, matching Space Colors from design system)
- Icon auto-generates from first letter of Name
- Profile defaults to "Default" — single-account users don't need to touch it
- Appearance section (collapsed) — reserved for future settings (sidebar background, etc.)

**Edit Space** — right-click on space pill → Edit, or Settings → Spaces:

```
┌───────────────────────────────────┐
│  Edit Space: Work                 │
│                                   │
│  Name:     [Work              ]   │
│  Color:    ● ● ● ● ● ●          │
│                                   │
│  Profile:  [Work             ▼]   │
│            ┌────────────────────┐  │
│            │ Default            │  │
│            │ Work          ✓   │  │
│            │ ───────────────    │  │
│            │ + New Profile...   │  │
│            └────────────────────┘  │
│                                   │
│  ▶ Appearance                     │
│                                   │
│     [Cancel]  [Save]  [Delete]    │
└───────────────────────────────────┘
```

Profile dropdown includes a "New Profile..." shortcut that opens the Profile creation flow (see Profiles section).

### Hyperspace (All Sessions View)

Hyperspace is a **system-level meta-space** that shows all sessions across all spaces in one place. It is always present as the first pill in the SpaceSwitcher and cannot be deleted, renamed, or recolored.

**Analogies:**
- macOS Mission Control — see all windows across all desktops
- Linear "My Issues" — spans across all teams/projects
- Slack "All DMs" — global list above workspaces

**Why it exists:**
1. **Monitoring** — "what's happening right now?" at a glance. 5 agents in 3 spaces — visible without switching
2. **Quick jump** — switch to any session without "first switch space, then find session"
3. **Cross-space management** — drag sessions between space groups, see aggregate stats
4. **Unique differentiator** — no terminal app offers a unified cross-context view

#### Placement in SpaceSwitcher

```
  [◆] [H] [W] [P] [+]
   ^    ^   ^   ^   ^
   |    |   |   |   +-- Create new space
   |    |   |   +------ Personal (green)
   |    |   +---------- Work (blue)
   |    +-------------- Hackathons (purple)
   +------------------- Hyperspace (always first, special visual)
```

**Visual distinction from regular spaces:**
- Icon: `Layers` from lucide-svelte (stacked layers) — not a letter
- Color: neutral (white/gray on dark, dark gray on light) — not from `SPACE_COLORS`
- Cannot be deleted, recolored, or assigned a Profile
- Tooltip on hover: "All Spaces"
- Keyboard: **Cmd+0** (before Cmd+1/2/3 for regular spaces)

#### Sidebar Content in Hyperspace

Hyperspace does NOT use pinned/unpinned zones or folders from individual spaces. It has its own **grouping system** with a switcher at the top:

```
+-------------------+
| Search            |
+-------------------+
| Group by: Space · Status · Project
|           ─────
+-------------------+
|                   |
|  (grouped list)   |
|                   |
+-------------------+
| Footer (aggregate)|
+-------------------+
```

The grouping switcher is three text buttons, active one underlined. Selection persists in localStorage. Default: **Space**.

#### Grouping: By Space (default)

Groups sessions by their parent space. Most natural — preserves the user's own organization.

```
+----------------------------+
| ● HACKATHONS (2)           |  <- purple left stripe
|   ⚡ ● wallet-auth         |
|   ⚡ ◐ test-runner          |
+----------------------------+
| ● WORK (3)                 |  <- blue left stripe
|   ⚡ ● api-refactor         |
|   ↗  ● ssh-prod             |
|   >_ ○ migrations           |
+----------------------------+
| ● PERSONAL (1)             |  <- green left stripe
|   >_ ○ side-project         |
+----------------------------+
```

- Groups are collapsible (like folders)
- Space color shown as a left border stripe on the group header
- Sessions within a group sorted by last activity (most recent first)
- Empty spaces hidden by default (or shown collapsed with "(0)")

#### Grouping: By Status

Monitoring mode — answers "what needs my attention?"

```
+----------------------------+
| ● ACTIVE (3)               |
|   ⚡ ● wallet-auth      [H] |  <- space badge
|   ⚡ ● api-refactor     [W] |
|   ↗  ● ssh-prod          [W] |
+----------------------------+
| ◐ WAITING (1)              |
|   ⚡ ◐ test-runner       [H] |
+----------------------------+
| ○ IDLE (2)                 |
|   >_ ○ migrations        [W] |
|   >_ ○ side-project      [P] |
+----------------------------+
| ✕ ERROR (0)                |  <- hidden when empty
+----------------------------+
```

- Active sessions always on top — highest priority
- Each session shows a **space badge** (first letter + space color background, 16-18px circle)
- Group order is fixed: Active → Waiting → Idle → Error
- Empty groups hidden

#### Grouping: By Project (Directory)

Groups sessions by `cwd` — useful to spot multiple agents in the same repo.

```
+----------------------------+
| weplex (3)                   |  <- basename of cwd
|   ⚡ ● wallet-auth      [H] |    ~/projects/weplex (tooltip)
|   ⚡ ◐ test-runner       [H] |
|   >_ ○ old-build         [P] |
+----------------------------+
| backend (2)                |
|   ⚡ ● api-refactor      [W] |    ~/areal/backend (tooltip)
|   >_ ○ migrations        [W] |
+----------------------------+
| No directory (1)           |
|   ↗  ● ssh-prod           [W] |
+----------------------------+
```

- Group name: `basename` of cwd, full path in tooltip
- Highlights potential conflicts (2+ agents in the same repo)
- Sessions without a cwd grouped under "No directory"

#### Space Badge (Hyperspace only)

In Hyperspace, each session shows a **space badge** — a small indicator of which space it belongs to. Badge is not shown inside regular spaces (redundant).

```
Default state (one line, 36px):
  <icon> <dot> <name>                <badge>
  ⚡      ●     wallet-auth            [H]

Hover state (expanded):
  +--------------------------------------+
  | ⚡ ● wallet-auth              [H] ...|
  |    feat/jwt · 2h · $0.82            |
  +--------------------------------------+
```

Badge: circle/pill with first letter of space name, filled with space color. Size: 16-18px, positioned at the right side of the session row.

When grouping "By Space", badges are optional (the group header already identifies the space). When grouping "By Status" or "By Project", badges are required.

#### Interaction Model

**Click a session:**
- Activates the session (terminal shows it immediately)
- **Stays in Hyperspace** — does not switch to the parent space
- Header bar shows a subtle space-color accent so the user knows which space context they're in

This is the key UX decision: Hyperspace is a fully functional workspace, not a read-only monitor. Users can work from Hyperspace, jumping between sessions from different spaces without context switching.

**Drag & Drop:**
- Grouping "By Space": drag a session between space groups = `moveToSpace()`. Natural cross-space session management
- Grouping "By Status" / "By Project": drag & drop disabled (reordering between status groups is meaningless)

**Context menu (right-click):**

Same as regular space context menu, with `Move to Space` being especially useful:

```
Pin / Unpin          (in the parent space)
Rename
Move to Space >      [Hackathons, Work, Personal]
---
Restart
Kill
```

**New Session from Hyperspace:**
- Opens standard New Session dialog
- Space selector is mandatory (already exists in the dialog design)
- After creation, session appears in the corresponding space group

#### Hyperspace Footer

Shows aggregate stats across all spaces:

```
  ● 3 active · ◐ 1 waiting · ○ 2 idle · $8.40 today
  [+ New Session]
```

Compare with regular space footer:
```
  ● 1 active · ○ 1 idle · $2.40 today
  [+ New Session]
```

#### What Hyperspace Does NOT Have

- **Folders** — folders are a per-space concept, not applicable to Hyperspace
- **Pinned/Unpinned zones** — replaced by the grouping system (space / status / project)
- **Time-based grouping** (Today / This Week) — Hyperspace focuses on "what's happening now", not "when"
- **Profile assignment** — Hyperspace is a view, not a context. Sessions inherit profiles from their parent spaces
- **Appearance overrides** — no custom background color or theme

#### Implementation Notes

Hyperspace is a **view**, not a storage concept. Sessions still belong to their parent spaces (via `spaceId`). Hyperspace aggregates and re-groups them on the fly.

- Constant: `HYPERSPACE_ID = '__hyperspace__'` — never stored in `spaces[]`
- SpaceSwitcher: Hyperspace pill is hardcoded before the dynamic space list
- Sidebar: when `activeSpaceId === HYPERSPACE_ID`, render grouped layout instead of pinned/unpinned zones
- sessionStore: existing `getBySpace()` unchanged. New method `getAllGrouped(groupBy: 'space' | 'status' | 'project')` for Hyperspace
- Keyboard: Cmd+0 activates Hyperspace. Cmd+1/2/3 for regular spaces (existing)
- Persistence: selected grouping mode saved in localStorage

#### Edge Cases

| Scenario | Behavior |
|----------|----------|
| Single space exists | Hyperspace shows same sessions as that space (but with status/project grouping available) |
| No sessions | Same empty state as regular spaces |
| 50+ sessions | Groups collapsed by default (except Active in status mode). Virtual scroll if needed |
| Session killed/deleted | Disappears from Hyperspace reactively |
| Space deleted | Orphaned sessions move to Default space, reflected in Hyperspace |
| App starts in Hyperspace | Remembers last active session in Hyperspace independently from per-space tracking |

### Session Item

**Default state** (one line, 36px height):
```
  <icon> <dot> <name>
  >_     ●     wallet-auth
```

**Hover state** (expanded, shows details + actions):
```
  +-------------------------------+
  | >_ ● wallet-auth          ... |  <- ... = menu button
  |    feat/jwt · 2h · $0.82     |  <- branch, uptime, cost
  +-------------------------------+
```

**Context menu** (right-click or ... button):
- Pin / Unpin
- Rename
- Duplicate
- Move to Space >
- ---
- Restart
- Kill

### Session Icons (by type)

| Type | Icon | Detection |
|------|------|-----------|
| Claude Code | lightning bolt | command contains `claude` |
| OpenCode | code brackets | command contains `opencode` |
| Aider | pencil | command contains `aider` |
| Gemini CLI | sparkle | command contains `gemini` |
| Codex | circle | command contains `codex` |
| SSH | arrow-up-right | command starts with `ssh` |
| Terminal | terminal prompt | everything else |

### Session Status Indicators

| Status | Dot | Color | Detection |
|--------|-----|-------|-----------|
| Active | ● | green (#10B981) | Agent is processing/generating output |
| Waiting | ◐ | amber (#F59E0B) | Agent waiting for user input |
| Idle | ○ | gray (#6B7280) | No activity for > 5 min |
| Error | ✕ | red (#EF4444) | Process exited with error |
| New | ◇ | blue (#3B82F6) | Just created, no activity yet |

### Auto-Grouping Logic

| Group | Rule |
|-------|------|
| PINNED | Manually pinned by user |
| TODAY | Session created/active today |
| THIS WEEK | Session created/active this week |
| OLDER | Everything else |

Sessions within groups sorted by last activity (most recent first).

### Sidebar Footer

```
  ● 3 active · ○ 2 idle · $2.40 today
  [+ New Session]
```

---

## Header Bar

Shows context of the **currently selected session**.

### Agent Session Header
```
  <icon> <name>          <branch>         <agent> <model>
  ⚡     wallet-auth      feat/jwt-refresh  Claude  opus-4-6

  [M 3] [+142 -38]   45K tokens   $0.82        [Toggle Detail Panel]
```

### SSH Session Header
```
  ↗  production-server    root@155.212.129.116    Connected 45m

  [Latency: 23ms]                                [Toggle Detail Panel]
```

### Terminal Session Header
```
  >_  build-process       ~/projects/web          bash

  [Running: npm run dev]                          [Toggle Detail Panel]
```

---

## Detail Panel (Right, 280px, Toggle)

### Tabs

The detail panel has two tabs at the top: **Info** (default) and **Tasks**.

```
+---------------------+
| [Info]  [Tasks]     |  <- tab switcher
+---------------------+
```

### Info Tab (default)

#### For Agent Sessions

```
+---------------------+
| GIT                 |  <- collapsible section
|   feat/jwt-refresh  |
|   M auth.service.ts |
|   A login.dto.ts    |
|   D old-helper.ts   |
|   +142 -38          |
+---------------------+
| COST                |
|   Session:  $0.82   |
|   Today:    $2.40   |
|   This week: $14.20 |
+---------------------+
| AGENT               |
|   Claude Code 2.1   |
|   Model: opus-4-6   |
|   Auth: OAuth (Max) |
|   Tokens: 45K/200K  |
+---------------------+
| MCP SERVERS         |
|   ● chrome          |
|   ● digitalocean    |
|   ○ github (off)    |
+---------------------+
```

#### For SSH Sessions

```
+---------------------+
| CONNECTION          |
|   Host: 155.212...  |
|   User: root        |
|   Port: 22          |
|   Latency: 23ms     |
|   Uptime: 45m       |
+---------------------+
| TRANSFERS           |
|   (if any active)   |
+---------------------+
```

#### For Terminal Sessions

```
+---------------------+
| PROCESS             |
|   Command: npm dev  |
|   PID: 12345        |
|   Uptime: 1h 23m    |
+---------------------+
| GIT                 |
|   main              |
|   M package.json    |
|   +12 -3            |
+---------------------+
```

### Tasks Tab

> **Status: Designed, not implemented.**

A persistent high-level progress tracker — answers "what's done, what's left" without re-asking the agent. Shows big-picture tasks (deploy API, record demo, submit), not file-level changes.

```
+---------------------+
| [Info]  [Tasks]     |
+---------------------+
| TASKS               |
|                     |
| ☑ Set up project    |
| ☑ Implement auth    |
| ☐ Deploy API    ●   |  <- ● = critical priority dot
| ☐ Record demo   ●   |
| ☐ Submit         ◐  |  <- ◐ = high
| ☐ Publish npm    ○  |  <- ○ = medium
|                     |
| [+ Add task]        |
+---------------------+
```

**Priority indicators** (optional dot, right-aligned):
- `●` Critical (red)
- `◐` High (amber)
- `○` Medium (gray)
- No dot = Low

**Interactions:**
- Click checkbox → toggle done/todo
- Click task text → inline edit
- Right-click → Set priority / Delete
- `[+ Add task]` → new empty row, cursor in text field
- Drag to reorder (grab handle on hover)

**Data model:**
```typescript
interface SessionTask {
  id: string;
  text: string;
  done: boolean;
  priority: 'critical' | 'high' | 'medium' | 'low';
  order: number;
}
```

Tasks persist with the session. Stored alongside session data in `~/.config/weplex/sessions/`.

**Phased evolution:**
- **MVP**: manual CRUD, user types or pastes tasks from agent output
- **Phase 2**: Stop hook suggests task list updates (user confirms)
- **Phase 3**: MCP tool `deck_update_tasks()` — agent updates programmatically. Same component shows pipeline stage progress when a pipeline is running

---

## Status Bar (Bottom)

```
+------------------------------------------------------------------------+
| ● 3 active · ○ 2 idle · $2.40 today                    Cmd+K palette  |
+------------------------------------------------------------------------+
```

Left side: aggregate session stats
Right side: command palette hint

---

## Terminal Decorations

> **Status: Designed, not implemented.** Phase 2 feature.

Terminal Decorations are inline contextual actions that appear over the terminal output when the user hovers over a detected pattern (file path, URL, command, git ref, etc.). They are rendered via the **xterm.js Decoration API** — DOM elements anchored to a specific row/col in the terminal buffer that scroll with the content.

### Why This Matters

Weplex is an agent-aware terminal. Agents produce structured, actionable output: file paths, commands, git branches, error traces. Decorations turn passive text into interactive elements — without leaving the terminal or breaking the flow.

No other terminal app does this at this level. Warp comes closest (block-based output), but Weplex's approach works with any PTY output, not just Warp's proprietary shell integration.

### Technical Approach

xterm.js `registerDecoration()` API:

```typescript
const marker = terminal.registerMarker(rowOffset);
const decoration = terminal.registerDecoration({
  marker,
  x: colPosition,   // column where decoration anchors
  width: 1,
  height: 1,
  layer: 'top'      // renders above canvas, below selection
});

decoration.onRender(element => {
  // mount a Svelte component or plain DOM into element
});
```

Decorations are anchored to buffer markers — they scroll with terminal content and survive resize. The action bar is a floating DOM element, not painted into the canvas, so it doesn't interfere with text selection.

### UX Model: Hover-Triggered Action Bar

Actions appear **only on hover** over a detected pattern. This keeps the terminal clean during normal use.

```
Normal view (no hover):
  rm -rf ~/Library/"Application Support"/com.apple.wallpaper/aerials/videos/

On hover over the path:
  rm -rf ~/Library/"Application Support"/com.apple.wallpaper/aerials/videos/
         ──────────────────────────────────────────────────────────────────
                                                  [📂 Finder] [>_ Terminal] [⎘ Copy]
                                                  ↑ action bar, appears after 150ms
```

- Hovered pattern gets a subtle underline (not bold, not colored — just underline)
- Action bar floats below the line as a pill: `8px padding, 6px radius, surface+border bg`
- Bar disappears on mouse leave with 300ms grace period (so the user can move cursor to the bar)
- Only one bar visible at a time

### Pattern Registry

Each pattern type has a detector (regex or parser) and a set of actions:

| Pattern | Example | Actions |
|---------|---------|---------|
| **File path** | `~/Library/Application Support/...` | Open in Finder · Open in new terminal · Copy path |
| **Command** | `rm -rf ~/path/...` | Run in new session · Copy |
| **URL** | `https://github.com/...` | Open in browser · Copy |
| **Git branch** | `feat/jwt-refresh` | Checkout · Create worktree · Copy |
| **File path with line** | `src/auth.ts:42` | Open in editor (future) · Copy |
| **Error/stacktrace line** | `at processOrder (order.ts:88)` | Open file:line · Copy |
| **npm / cargo / pnpm command** | `pnpm run dev` | Run in new session · Copy |

Pattern detection runs on each new line of terminal output. Matches are stored with their buffer row/col for decoration placement.

### Agent-Specific Decorations (Phase 3)

For agent sessions with structured output (Claude tool use, file edits), richer decorations are possible:

```
┌─ Claude: Write file ────────────────────────────────┐
│  src/lib/stores/sessionStore.svelte.ts  +42 -12     │
│                             [Show diff] [Open] [✓]  │
└──────────────────────────────────────────────────────┘
```

This requires a structured output parser per agent (Claude JSONL already partially implemented). Phase 3 feature.

### Gutter Icons (Alternative)

For always-visible indicators without hover, a gutter icon can be placed at the right edge of the terminal:

```
  rm -rf ~/Library/"Application Support"/com.apple.wallpaper/    ⚡
                                                                   ^
                                                          click = dropdown
```

Low-opacity (30%) until hover. Click shows action dropdown. Complements hover bar for keyboard users.

### Architecture

```
PTY Output Stream
      │
      ▼
Pattern Detector (per line)     ← regex rules, extensible
      │ matches: [{row, col, len, type, value}]
      ▼
Decoration Registry             ← xterm.js registerDecoration()
      │
      ▼
Action Bar Component            ← Svelte, mounted into decoration DOM element
      │
      ▼
Action Handler
  ├── openInFinder(path)        ← Tauri shell::open()
  ├── openInNewSession(cwd)     ← sessionStore.create({cwd})
  ├── runInNewSession(cmd, cwd) ← sessionStore.create({command: cmd, cwd})
  ├── copyToClipboard(text)     ← navigator.clipboard
  └── openInBrowser(url)        ← Tauri shell::open()
```

Pattern Detector is **extensible by design** — same extension API (Phase 4) will allow third-party pattern + action registration.

### Implementation Notes

- Pattern detection: run on each `terminal.onData` / output chunk, per-line, lightweight regex
- Decoration lifetime: decoration created when pattern detected, disposed when line scrolls out of buffer
- Performance: max 50 active decorations at any time (older ones auto-disposed), no decoration on high-throughput output (throttle: skip if >10 lines/sec)
- Resize: xterm.js handles re-anchoring decorations on terminal resize automatically
- Not shown: during active agent streaming (too much noise). Decorations activate after output settles (1s idle threshold)

---

## Overlays & Dialogs

### Command Palette (Cmd+K)

Centered overlay, fuzzy search:

```
  +-----------------------------------------------+
  |  > Type a command or session name...           |
  +-----------------------------------------------+
  |  SESSIONS                                      |
  |    Switch to wallet-auth              Cmd+1    |
  |    Switch to openui-svelte            Cmd+2    |
  |    New session...                     Cmd+N    |
  |  ACTIONS                                       |
  |    Kill current session               Cmd+W    |
  |    Toggle detail panel                Cmd+I    |
  |    Toggle sidebar                     Cmd+B    |
  |    Split horizontal                   Cmd+D    |
  |    Split vertical                     Cmd+Shift+D |
  |  SETTINGS                                      |
  |    Theme...                                    |
  |    Font size...                                |
  +-----------------------------------------------+
```

### Quick Switcher (Cmd+P)

Same overlay but filtered to sessions only. Fuzzy search by session name, directory, branch.

### New Session Dialog

```
  +-----------------------------------------------+
  |  New Session                                   |
  |                                                |
  |  Directory                                     |
  |  +------------------------------------------+  |
  |  | ~/Documents/Hackathons/projects/         |  |
  |  +------------------------------------------+  |
  |  (browse button + recent directories list)     |
  |                                                |
  |  Command                                       |
  |  +------------------------------------------+  |
  |  | claude                                   |  |
  |  +------------------------------------------+  |
  |                                                |
  |  Presets:                                      |
  |  [claude] [claude --chrome] [aider]            |
  |  [opencode] [ssh] [bash]                       |
  |                                                |
  |  Space:   [Hackathons v]                       |
  |  Profile: [Inherit from Space v]               |
  |  Pin:     [ ]                                  |
  |                                                |
  |              [Cancel]  [Create]                |
  +-----------------------------------------------+
```

### Settings Overlay

Full-screen overlay with sections:

- **General**: default shell, default directory, startup behavior
- **Appearance**: theme (dark/light/custom), font family, font size, line height
- **Profiles**: manage profiles (add/edit/delete, link accounts, env vars)
- **Sidebar**: default state (expanded/collapsed), auto-grouping rules
- **Spaces**: manage spaces (add/edit/delete, colors, profile assignment)
- **Sessions**: persistence settings, idle timeout, auto-kill threshold
- **Agents**: configure agent detection rules, cost tracking preferences
- **Keybindings**: customize all shortcuts
- **About**: version, license, links

---

## Session Types & Detection

### Auto-Detection Logic

When a new session is created and a command is run:

```
command contains "claude"    -> type: agent, agent: claude-code
command contains "opencode"  -> type: agent, agent: opencode
command contains "aider"     -> type: agent, agent: aider
command contains "gemini"    -> type: agent, agent: gemini
command starts with "ssh "   -> type: ssh
else                         -> type: terminal
```

Detection runs once when the first command is executed. Can be manually overridden.

### Agent-Specific Parsers

Each agent has a parser module that extracts metadata from terminal output:

**Claude Code Parser:**
- Detects model from output ("Powered by claude-opus-4-6")
- Parses `/cost` output for token/cost data
- Detects auth type (OAuth vs API key) from startup messages
- Status: active when streaming, waiting when showing prompt

**OpenCode Parser:**
- Detects model from sidebar/status output
- Token tracking from OpenCode's built-in display

**Aider Parser:**
- Detects model from startup
- Cost tracking from Aider's built-in cost display

**Generic Parser (fallback):**
- Basic activity detection (output = active, no output = idle)
- No cost tracking

---

## Authentication Awareness

We do NOT manage auth. Each agent handles its own authentication.
Our app detects the auth type for display purposes only:

| Auth Type | How We Detect | What We Show |
|-----------|--------------|--------------|
| Claude OAuth (Pro) | Startup output parsing | "Auth: OAuth (Pro)" |
| Claude OAuth (Max) | Startup output parsing | "Auth: OAuth (Max)" |
| Claude API Key | Startup output parsing | "Auth: API Key" |
| Claude Max (5x) | Startup output parsing | "Auth: Max (5x)" |
| Other agents | Agent-specific parsing | Agent name + detected plan |

### Cost Display Logic

| User Type | Cost Display |
|-----------|-------------|
| API Key user | Dollar cost ($0.82) + token count |
| OAuth Pro/Max | Token count + usage percentage (where available) |
| Unknown/other | Session duration only |

---

## Claude Code Deep Integration

> **Status: Designed, not implemented.** Phased rollout — Hooks in Phase 2, MCP Server in Phase 3-4.

Weplex controls the PTY, the environment, and the launch arguments of every Claude Code session. This opens up a spectrum of integration possibilities — from lightweight context injection to full multi-agent orchestration — all using Claude Code's **official extension points** (hooks, MCP, CLAUDE.md). No patching, no monkey-patching.

### Integration Layers

```
Already implemented:
  PTY read/write, env vars (profiles), CLAUDE_CONFIG_DIR,
  --resume / --continue flags, JSONL file parsing

Phase 2 — Hooks + Context:
  PreToolUse / PostToolUse / Stop hooks, CLAUDE.md injection

Phase 3-4 — MCP Server:
  Weplex MCP → Claude sees and controls other sessions
  → true multi-agent orchestration
```

### Layer 1: Hooks Injection (Phase 2)

Claude Code supports hooks — shell commands executed on lifecycle events. Weplex automatically injects its own hooks into the session's Claude config when a session is created. The user never configures this manually.

| Hook | Trigger | What Weplex Does |
|------|---------|----------------|
| `PreToolUse` | Before every tool call (Write, Bash, Edit…) | Log to detail panel, optionally show confirmation UI for destructive ops |
| `PostToolUse` | After tool call completes | Update detail panel (show diff for file writes, show output for Bash), trigger notification |
| `Stop` | Agent finished responding | Update session status to `waiting`, show notification if window is unfocused |
| `PreCompact` | Before context compaction | Save session snapshot, show warning in header bar |

Hook scripts are minimal shell one-liners that call back into Weplex via a local HTTP endpoint or temp file signal (no complex IPC needed).

### Layer 2: CLAUDE.md Context Injection (Phase 2)

Weplex prepends a context block to the project's CLAUDE.md before the session starts. Claude reads this as part of its system context — no extra prompting needed.

```markdown
<!-- Injected by Weplex — do not edit this block -->
## Weplex Workspace Context
- Space: Hackathons | Session: wallet-auth
- Active sessions in this space:
    - test-runner (idle) — ~/projects/weplex
    - ssh-prod (active) — production server
- Cost today: $2.40 | Budget remaining: $2.60
<!-- End Weplex block -->
```

This gives Claude passive awareness of the workspace state. It can reference other sessions in its reasoning without any extra tools.

The injected block is always at the top, always up-to-date (re-written on session restart), and clearly delimited so Claude and humans can distinguish it from project-specific instructions.

### Layer 3: Weplex MCP Server (Phase 3-4)

The most powerful integration. Weplex runs a local MCP server and automatically registers it in each Claude Code session's MCP config. This gives Claude **active control** over the Weplex workspace.

#### MCP Tools exposed to Claude

```
deck_list_sessions()
  → [{id, name, status, cwd, spaceId, agentType, cost}]

deck_create_session(cwd, command, spaceId?, name?)
  → sessionId
  Creates a real PTY session visible in the sidebar

deck_read_output(sessionId, lastN?)
  → last N lines of terminal output from that session

deck_send_input(sessionId, text)
  → sends keystrokes to another session's PTY

deck_get_context()
  → {activeSpace, allSpaces, totalCostToday, settings}
```

#### What this enables

**Single agent, multi-session task:**
```
User: "Refactor the auth module and add tests in parallel"

Claude:
  1. deck_create_session(cwd="./src", command="claude", name="refactor-auth")
  2. deck_create_session(cwd="./src", command="claude", name="auth-tests")
  3. deck_send_input("refactor-auth", "Refactor auth.service.ts — extract validation")
  4. deck_send_input("auth-tests", "Write characterization tests for auth.service.ts")
  5. Polls deck_read_output() to track progress
  6. Reports back when both complete
```

Two new sessions appear in the sidebar. The user watches them work in real time.

**Cross-session awareness:**
```
User: "What's the backend agent doing?"

Claude (in frontend session):
  deck_read_output("backend-session", lastN=20)
  → "...creating migration for users table, adding index on wallet_address..."

Claude: "The backend agent is adding a DB index on wallet_address.
         You may want to update your API types to match."
```

**Orchestration session:**
A dedicated "orchestrator" session can be created that does no coding itself — it only uses Weplex MCP tools to spawn, monitor, and coordinate worker sessions. This is Phase 4 territory.

### What We Never Do

- No modification of Claude's binary or source
- No interception/modification of Claude's output before it reaches xterm.js
- No auto-injection of prompts mid-session without user action
- No sending inputs to a session without explicit user intent (MCP tools are called by Claude, which the user directed)
- MCP server is local-only, never networked

---

## Session Hierarchy & Orchestration Dashboards

> **Status: Designed, not implemented.** Phase 3-4. Requires Weplex MCP Server.

When Claude calls `deck_create_session()` via MCP, Weplex records the calling session as the parent. This `parentId` relationship unlocks two things: **visual hierarchy in the sidebar** and a new **dashboard session type** that replaces the terminal with a live visual overview.

### Session Hierarchy in the Sidebar

Child sessions are indented under their parent in the sidebar. They are full PTY sessions — clicking one opens its terminal as normal. The parent shows an aggregated status dot: active if any child is active, waiting if all children are waiting.

```
⚡ ● orchestrator                ← user launched this
   ⚡ ● refactor-auth            ← spawned by orchestrator via MCP
   ⚡ ● auth-tests               ← spawned by orchestrator via MCP
   >_ ○ build-check              ← spawned by orchestrator via MCP
```

**Hover state on orchestrator (parent):**
```
  +----------------------------------------+
  | ⚡ ● orchestrator               ... [▼] |  ← collapse/expand children
  |    3 children · $1.24 · 2 active       |
  +----------------------------------------+
```

**Collapse/expand:** `[▼]` collapses children into the parent row. Parent row shows child count and aggregate stats. This keeps the sidebar clean when an orchestration is running in the background.

**Auto-cleanup:** When all children finish (status: idle/error), they remain visible but dimmed. A "dismiss children" action collapses them permanently into the parent's history.

**Data model addition:**
```typescript
interface Session {
  // ... existing fields ...
  parentId?: string;      // set when created via deck_create_session() MCP call
  isOrchestrator?: boolean; // true if this session has ever spawned children
}
```

Children are derived on the fly: `sessions.filter(s => s.parentId === id)`.

### Dashboard Session Type

A new session type: `dashboard`. No PTY — the terminal area is replaced by a Svelte-rendered visual component. Created explicitly by the user ("New Dashboard") or automatically alongside an orchestrator session.

Three dashboard variants:

---

#### Variant 1: Orchestration Dashboard

Attached to a specific orchestrator session. Shows the real-time state of the agent tree.

```
╔══════════════════════════════════════════════════════╗
║  Refactor auth module                                ║
║  ⚡ orchestrator  ●  running  4m  $1.24 total        ║
╠══════════════════════════════════════════════════════╣
║  AGENTS                                              ║
║  ⚡ refactor-auth   feat/jwt  ●  active  $0.52       ║
║  ⚡ auth-tests      feat/jwt  ●  active  $0.41       ║
║  >_ build-check     main      ○  idle    —           ║
╠══════════════════════════════════════════════════════╣
║  TIMELINE                                 now        ║
║  refactor-auth  ████████████████░░░░░░░░  ●          ║
║  auth-tests     ██████████░░░░░░░░░░░░░░  ●          ║
║  build-check    ████░░░░░░░░░░░░░░░░░░░░  ○          ║
╠══════════════════════════════════════════════════════╣
║  ACTIVITY                                            ║
║  10:04  refactor-auth  Write   auth.service.ts  +42  ║
║  10:03  auth-tests     Write   auth.spec.ts     +88  ║
║  10:02  build-check    Bash    npm test         ✓    ║
║  10:01  refactor-auth  Edit    auth.guard.ts    +7   ║
╠══════════════════════════════════════════════════════╣
║  CHANGED FILES                                       ║
║  M  auth/auth.service.ts       refactor-auth         ║
║  A  auth/auth.spec.ts          auth-tests            ║
║  M  auth/auth.guard.ts         refactor-auth         ║
╚══════════════════════════════════════════════════════╝
```

Activity feed is powered by **PostToolUse hooks** — every file write, bash command, and edit from every child session flows into this view in real time.

---

#### Variant 2: Project Dashboard

Attached to a directory (cwd), not a specific session. Aggregates all sessions working in the same repository — regardless of which space they belong to.

```
╔══════════════════════════════════════════════════════╗
║  ~/projects/weplex                     3 sessions      ║
║  ● 2 active  ○ 1 idle  $3.40 today                  ║
╠══════════════════════════════════════════════════════╣
║  SESSIONS                                            ║
║  ⚡ wallet-auth   feat/jwt  ● active  $0.82   [H]    ║
║  ⚡ test-runner   feat/jwt  ◐ waiting $0.31   [H]    ║
║  >_ build-watch   main      ○ idle    —       [W]    ║
╠══════════════════════════════════════════════════════╣
║  GIT                                                 ║
║  feat/jwt  +3 commits ahead of main                  ║
║  M  auth/auth.service.ts                             ║
║  A  auth/auth.spec.ts                                ║
║  M  auth/login.dto.ts                                ║
╠══════════════════════════════════════════════════════╣
║  ⚠ CONFLICTS                                         ║
║  auth/auth.service.ts                                ║
║  └─ wallet-auth and test-runner both editing         ║
╚══════════════════════════════════════════════════════╝
```

**Conflict detection** — when two sessions have the same file in their PostToolUse history within a short window, Weplex flags it. Not a hard block — just a visible warning.

Space badges `[H]` / `[W]` show which space each session belongs to (same as Hyperspace).

---

#### Variant 3: Space Dashboard

A visual overview of an entire space. Replaces the session list view for users who prefer a board layout over a sidebar list.

```
╔══════════════════════════════════════════════════════╗
║  Hackathons                          $8.40 today     ║
╠══════════════════════╦═══════════════════════════════╣
║  ~/projects/weplex     ║  ~/projects/areal-backend     ║
║  ─────────────────   ║  ─────────────────────────    ║
║  ⚡ wallet-auth  ●   ║  ⚡ api-refactor   ●           ║
║  ⚡ test-runner  ◐   ║  >_ migrations    ○            ║
║  >_ build-watch  ○   ║                               ║
╠══════════════════════╩═══════════════════════════════╣
║  ● 3 active  ◐ 1 waiting  ○ 2 idle  5 sessions      ║
╚══════════════════════════════════════════════════════╝
```

Sessions grouped by project (cwd). Compact cards — click opens the session in the sidebar.

---

### Where Dashboards Live in the Sidebar

Dashboard sessions appear in the sidebar like any other session — same list, same position. Their icon distinguishes them:

| Type | Icon | Note |
|------|------|------|
| Orchestration Dashboard | `LayoutDashboard` | auto-created alongside orchestrator |
| Project Dashboard | `FolderKanban` | user-created, pinned to cwd |
| Space Dashboard | `LayoutGrid` | one per space, optional |

Clicking a dashboard session switches the main area to the dashboard view instead of a terminal. The header bar shows the dashboard title and type instead of the usual session info.

### The Bigger Picture

This is a new product category. Weplex is no longer just a terminal emulator with agent awareness — it becomes the **visual layer for multi-agent workflows**:

```
Other tools:
  terminal emulators  → show text output, no structure
  project managers    → show tasks, no live agent state
  AI coding agents    → work autonomously, invisible to each other

Deck:
  live visualization of what AI agents are doing, together,
  on your project — with full terminal access when you need it
```

The session hierarchy + dashboard views make the invisible visible: instead of guessing what agents are doing, you see it. Instead of coordinating agents manually, the orchestration dashboard shows the full picture at a glance.

---

## Agents & Pipelines

> **Status: Designed, not implemented.** Phase 0 (MVP).

Weplex is a deterministic pipeline orchestrator for AI coding agents. A Rust state machine reads YAML pipeline definitions, spawns each stage as a separate PTY session, waits for completion, captures output, and passes it to the next stage. No AI in the orchestration layer — predictable, repeatable, auditable.

**Deck IS the orchestrator.** Not Claude, not any AI model. Weplex controls pipeline flow. Agents control execution within each stage.

### Design Principles

1. **Deck orchestrates deterministically.** YAML = law. Stages execute in defined order. No AI decides to skip steps.
2. **Agent-agnostic.** Any CLI agent on any stage. `binary: claude`, `binary: codex`, `binary: aider`. User picks per stage.
3. **Two levels of orchestration.** Level 1 (Weplex): pipeline flow. Level 2 (agent): how to execute a stage. Claude might use Agent tool internally — Weplex doesn't care, it waits for exit.
4. **Claude = best experience.** When `binary: claude`, Weplex gets bonus features: JSONL cost tracking, hooks visibility, sub-agent detection. Others: PTY output + exit code only.
5. **Own ecosystem.** Both agents and pipelines live in `~/.weplex/`. Claude Code agents (`~/.claude/agents/`) are also read and displayed but are not the pipeline-native format.

### Agent Format (Weplex Standard)

Agent-agnostic YAML format with a `binary` field:

```yaml
# ~/.weplex/agents/backend.yaml
name: backend
description: Backend developer
binary: claude              # claude | codex | aider | gemini | custom
model: opus                 # optional, binary-specific
prompt: |
  You are a senior backend developer. Follow project conventions.
  Use TypeORM for database, class-validator for DTOs.
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Agent identifier |
| `description` | yes | What this agent does |
| `binary` | yes | CLI binary to run (`claude`, `codex`, `aider`, `gemini`, or path) |
| `model` | no | Model override (binary-specific: `opus`/`sonnet` for Claude, etc.) |
| `prompt` | yes | System prompt / instructions for the agent |
| `one_shot` | no | Command template for one-shot mode (default: auto-detected from binary) |

Storage: `~/.weplex/agents/*.yaml`

**Claude Code agents** (`~/.claude/agents/*.md`) are also displayed in the agents panel for reference, but pipeline stages use Weplex agents.

**Per-binary command templates** (how Weplex spawns a one-shot stage):

| Binary | One-shot command |
|--------|-----------------|
| `claude` | `claude --print "{role}\n\nContext:\n{artifacts}\n\nTask:\n{task}"` |
| `codex` | `codex "{role}\n\nContext:\n{artifacts}\n\nTask:\n{task}"` |
| `aider` | `aider --message "{role}\n\nContext:\n{artifacts}\n\nTask:\n{task}" --yes` |
| `gemini` | `gemini --prompt "{role}\n\nContext:\n{artifacts}\n\nTask:\n{task}"` |
| custom | User-defined template in agent YAML `one_shot` field |

### Pipeline Format (Weplex Standard)

Pipelines are YAML files defining a sequence of agent stages. Each stage = a separate PTY session.

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

Two parts:
- `stages` — executable definition (portable, human-readable)
- `layout` — visual canvas coordinates (optional, auto-generated by editor)

#### Pipeline Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Pipeline identifier |
| `description` | yes | What this pipeline does |
| `stages` | yes | Ordered list of steps |
| `layout` | no | Canvas node positions (auto-generated) |

Stage fields:

| Field | Description |
|-------|-------------|
| `name` | Unique stage identifier (used in `receives`) |
| `agent` | Agent name (must exist in `~/.weplex/agents/`) |
| `role` | Instruction injected to agent at runtime |
| `receives` | List of stage names whose output is passed as context |
| `optional` | Stage can be skipped (default: false) |
| `parallel` | List of stages that run concurrently |

Storage: `~/.weplex/pipelines/*.yaml`

### Visual Editor

New users likely have no agents or pipelines. They can install from marketplace or create their own via visual editors.

**Agent editor** — form UI:
- Fields: name, description, binary (dropdown: claude/codex/aider/gemini/custom), model (optional), prompt (textarea)
- Generates `.yaml` file
- Saves to `~/.weplex/agents/`

**Pipeline editor** — visual canvas (n8n-style):
- Agents from `~/.weplex/agents/` appear as available nodes
- Drag agent onto canvas → add stage
- Connect nodes → define sequence and `receives`
- Group nodes → parallel execution
- Click node → edit `role` instruction
- Canvas auto-generates `pipeline.yaml` with both `stages` and `layout`
- Phased rollout: list builder (MVP) → full canvas (later)

### Package Format (Marketplace)

Agent and pipeline files stay clean — marketplace metadata is a separate `weplex.yaml` that lives in the registry, never installed locally.

**Agent package:**
```
my-backend-agent/
├── agent.yaml        # Weplex format → installs to ~/.weplex/agents/
└── weplex.yaml         # marketplace metadata → stays in registry
```

**Pipeline package:**
```
my-feature-pipeline/
├── pipeline.yaml     # Weplex format → installs to ~/.weplex/pipelines/
└── weplex.yaml         # marketplace metadata → stays in registry
```

**weplex.yaml (shared format for both):**
```yaml
author: blackmesa
version: 1.0.0
tags: [backend, nestjs, typescript]
icon: server              # lucide icon name (marketplace card)
category: backend         # backend | frontend | mobile | database | security | testing | devops | development | bugfix | refactoring
license: MIT
repository: github.com/blackmesa/weplex-agents
requires:                 # pipeline-only: agents needed
  - pm
  - architect
  - backend
```

**Installation:**
- Agent: Weplex copies `agent.yaml` → `~/.weplex/agents/{name}.yaml`
- Pipeline: Weplex copies `pipeline.yaml` → `~/.weplex/pipelines/{name}.yaml`, checks `requires`, offers to install missing agents

### Pipeline Engine

**Deck is the orchestrator.** A Rust state machine that reads YAML, spawns PTY sessions, and passes artifacts between stages. No AI in the orchestration layer.

```
User clicks "Start Pipeline" → picks template + cwd + task description
  │
  ▼
Weplex reads pipeline.yaml
  │
  For each stage (sequential):
  │
  ├─ 1. Resolve agent → read ~/.weplex/agents/{name}.yaml
  ├─ 2. Build command → binary + one_shot template + role + task + artifacts from `receives`
  ├─ 3. Spawn PTY session (visible in sidebar as pipeline child)
  ├─ 4. Wait for process exit (+ timeout)
  ├─ 5. Capture output → store as named artifact
  ├─ 6. Check exit code → success: next stage / failure: pause pipeline
  │
  For parallel stages:
  ├─ Spawn all simultaneously, wait for all to complete
  │
  Pipeline complete → notify user
```

**Per-stage session in sidebar:**
```
▼ Feature: add auth (pipeline)
  ✓ PM              $0.42  done
  ✓ Architect        $0.85  done
  ● Backend          $1.20  running...
  ○ Security         —      waiting
  ○ Tester           —      waiting
  ○ PM Review        —      waiting
```

Each stage = real PTY session. Click to open terminal, see full output.

**Artifact passing:** output of stage N is captured from PTY buffer (last N lines or structured markers `<!-- DECK:artifact -->`). Injected into stage N+1's prompt as context. User can inspect what was passed in the Pipeline Dashboard.

### Pipeline Dashboard

When a pipeline is running, Weplex shows stage progress:

- Each stage is a clickable card — opens that session's terminal
- Color-coded by status (green=done, pulse=active, gray=waiting)
- Cost shown per stage (Claude stages via JSONL) and total
- Parallel stages rendered side by side
- Artifact preview: what was passed between stages

### Starting a Pipeline

Via command palette (Cmd+K → "Start Pipeline") or sidebar footer:

```
┌─────────────────────────────────────┐
│  Start Pipeline                     │
│                                     │
│  Template:  [Feature Development ▼] │
│                                     │
│  Working directory:                 │
│  [~/projects/weplex              ]    │
│                                     │
│  Task:                              │
│  [Implement Hyperspace feature  ]   │
│                                     │
│  Space: [Hackathons ▼]              │
│                                     │
│          [Cancel]  [Start]          │
└─────────────────────────────────────┘
```

### Marketplace

> **Status: Phase 4.** GitHub-based install in Phase 3. In-app marketplace in Phase 4.

**Deck owns the marketplace.** Installation, discovery, and updates all through Weplex UI.

Two-level ecosystem:

**Agents**: community publishes agent configs per stack + binary. `backend-go` (claude), `frontend-vue` (codex), `security-solidity` (claude). One Go developer writes the ideal agent — all Go developers install it.

**Pipelines**: composition of agents. `go-microservice-feature` = pm + architect + backend-go + tester-go + security. Weplex's exclusive differentiator — no other tool has this.

**Distribution (progressive):**
```
Phase 3  GitHub: weplex install github.com/user/deck-agents/backend-go
Phase 4  In-app marketplace: search, ratings, verified publishers
```

**Team use case**: company creates private pipeline library. `weplex install acme/pipelines` → all devs work to same standard. New hire = one command.

**Monetization**: Free = community marketplace; Team = private library + cost analytics; Enterprise = private registry + audit.

### Storage Locations

| What | Location | Owner |
|------|----------|-------|
| Weplex agents | `~/.weplex/agents/*.yaml` | Weplex |
| Pipelines | `~/.weplex/pipelines/*.yaml` | Weplex |
| Weplex config | `~/.weplex/config.yaml` | Weplex |
| Marketplace cache | `~/.weplex/marketplace/` | Weplex |
| Claude Code agents (read-only) | `~/.claude/agents/*.md` | Claude Code |

---

## Session Persistence

Sessions survive app restart:

### What is saved
- Session ID, name, space, pinned status
- Working directory
- Command that was run
- Profile override (if different from space default)
- Position in sidebar (order)
- Split layout state

### What is NOT saved (restored on reopen)
- Terminal scrollback (PTY is dead after app close)
- Agent state (agent needs to re-auth/resume)

### Behavior on reopen
1. App opens with previous layout (sidebar state, splits)
2. Persisted sessions shown as "disconnected" state
3. User can click to restart a session (runs same command in same directory)
4. Or dismiss/delete stale sessions

---

## Profiles

Profiles solve multi-account usage. A user may have multiple AI agent accounts (personal, work, client projects) and needs to seamlessly use different accounts in different contexts.

### Concept

- **Profile** = a named identity with its own agent configuration (env vars, config directories)
- **Space** references a Profile (many Spaces can share one Profile)
- **Session** inherits Profile from its Space, but can override it (rare case)
- Profiles are **optional** — single-account users never need to touch them

```
Profile → Space → Session
(1)       (many)   (many)

Example:
  Profile "Default"  → Space "Hackathons", Space "Pet Projects"
  Profile "Work"     → Space "Areal Backend", Space "Areal Frontend"
```

### Data Model

```typescript
interface Profile {
  id: string;
  name: string;              // "Default", "Work", "Client A"
  isDefault: boolean;        // exactly one, cannot be deleted
  configDir: string | null;  // e.g. "~/.claude-work" (null = system default)
  envVars: Record<string, string>;  // additional env var overrides
  linkedAccount?: {          // display info only, detected after auth
    email?: string;
    plan?: string;           // "Pro", "Max", "API Key"
  };
}
```

### Default Profile

- Always exists, cannot be deleted
- `configDir: null` — uses whatever the system has (`~/.claude/` by default)
- `envVars: {}` — no overrides, pure system environment
- New users have only "Default" — everything works out of the box, zero config

### How It Works (Under the Hood)

When Weplex creates a PTY for a session:

```
1. Determine Profile: session.profile ?? session.space.profile ?? defaultProfile
2. Start with system env vars
3. If profile.configDir → set CLAUDE_CONFIG_DIR=<configDir>
4. Merge profile.envVars (overrides system vars)
5. Spawn PTY with resulting environment
```

The user never sees env vars or config paths (unless they open Advanced settings).

### First Launch — Auto-Discovery

On first launch, Weplex scans for existing Claude configurations:

**Step 1: Scan filesystem**
```
~/.claude/           → always check (default location)
~/.claude-*/         → glob for named configs (e.g. ~/.claude-work)
~/.config/claude/    → alternative config path
```

**Step 2: Parse shell config for hints**
```
~/.zshrc, ~/.bashrc, ~/.zprofile, ~/.bash_profile
  → look for CLAUDE_CONFIG_DIR= assignments
  → extract variable name, value, and nearby comments for naming hints
```

**Step 3: Present to user**
```
┌─────────────────────────────────────────────┐
│  Welcome to Deck!                           │
│                                             │
│  Found existing Claude accounts:            │
│                                             │
│  ✅ Default (~/.claude/)                    │
│     OAuth linked                            │
│                                             │
│  ✅ Work (~/.claude-work/)                  │
│     OAuth linked                            │
│     detected from .zshrc                    │
│                                             │
│  [Import Both]  [Skip, set up later]        │
└─────────────────────────────────────────────┘
```

- Profile names are inferred from directory names (`claude-work` → "Work") or shell config comments
- Weplex does NOT copy or move anything — it references existing directories
- Imported profiles are immediately functional (OAuth tokens already there)

### Creating a New Profile (Guided Flow)

For users who don't have existing configs — or want to add a new account:

```
Settings → Profiles → [+ New Profile]

┌─────────────────────────────────────┐
│  New Profile                        │
│                                     │
│  Name: [                        ]   │
│                                     │
│  Claude Account                     │
│  [Link Claude Account]              │
│                                     │
│  ▶ Advanced                         │
│                                     │
│            [Cancel]  [Create]       │
└─────────────────────────────────────┘
```

**"Link Claude Account" flow:**
1. Weplex creates config directory: `~/.config/weplex/profiles/<name>/`
2. Opens an embedded terminal running `claude auth login` with `CLAUDE_CONFIG_DIR` pointing to that directory
3. User completes OAuth in browser (standard Claude flow)
4. Weplex detects successful auth, shows account info
5. Profile is ready

**Advanced section (collapsed by default):**
```
┌─────────────────────────────────────┐
│  ▼ Advanced                         │
│                                     │
│  Config Directory                   │
│  [~/.config/weplex/profiles/work  ]   │
│                                     │
│  Environment Variables              │
│  ┌─────────────────────────────────┐│
│  │ KEY              │ VALUE        ││
│  │ NODE_ENV         │ production   ││
│  │ [+ Add Variable]               ││
│  └─────────────────────────────────┘│
└─────────────────────────────────────┘
```

### Managing Profiles

```
Settings → Profiles

┌─────────────────────────────────────┐
│  Profiles                           │
│                                     │
│  Default                     ★      │
│  ✅ OAuth linked                    │
│  Used by: Hackathons, Pet Projects  │
│                                     │
│  Work                        [Edit] │
│  ✅ OAuth linked (user@corp.com)    │
│  Used by: Areal Backend, Areal FE   │
│                                     │
│  [+ New Profile]                    │
└─────────────────────────────────────┘
```

**Edit Profile:**
```
┌─────────────────────────────────────┐
│  Edit Profile: Work                 │
│                                     │
│  Name: [Work                    ]   │
│                                     │
│  Claude Account: ✅ Linked          │
│  user@company.com (Max plan)        │
│  [Re-link]  [Unlink]               │
│                                     │
│  ▶ Advanced                         │
│                                     │
│         [Cancel]  [Save]  [Delete]  │
└─────────────────────────────────────┘
```

### Assigning Profile to Space

In Space settings (or when creating a new Space):

```
┌─────────────────────────────────────┐
│  Edit Space: Areal Backend          │
│                                     │
│  Name:    [Areal Backend        ]   │
│  Color:   [● blue]                  │
│  Profile: [Work              ▼]     │
│           ┌──────────────────┐      │
│           │ Default          │      │
│           │ Work        ✓    │      │
│           │ ─────────────    │      │
│           │ + New Profile... │      │
│           └──────────────────┘      │
│                                     │
│            [Cancel]  [Save]         │
└─────────────────────────────────────┘
```

### Session-Level Override

Rare case: one session in a Space needs a different profile. Available in session context menu:

```
Right-click session → Profile → [Default | Work | ...]
```

Or in New Session dialog:

```
  Profile: [Inherit from Space ▼]   ← default
           ┌──────────────────────┐
           │ Inherit from Space   │
           │ Default              │
           │ Work                 │
           └──────────────────────┘
```

### Agent-Agnostic Design

Profiles work for any CLI agent, not just Claude Code:

| Agent | What Profile Controls |
|-------|----------------------|
| Claude Code | `CLAUDE_CONFIG_DIR` → separate OAuth sessions |
| Aider | `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` via env vars |
| OpenCode | `ANTHROPIC_API_KEY` via env vars |
| Gemini CLI | `GOOGLE_API_KEY` or `GEMINI_API_KEY` via env vars |
| Codex | `OPENAI_API_KEY` via env vars |
| SSH | Custom env vars, different SSH keys via `GIT_SSH_COMMAND` |

The guided "Link Account" flow is Claude-specific. For other agents, users configure env vars in the Advanced section.

### Storage

```
~/.config/weplex/
├── profiles.json              # Profile definitions
└── profiles/                  # Auto-created config dirs
    ├── work/                  # Created by "Link Account" flow
    │   └── (claude config)
    └── client-a/
        └── (claude config)
```

Existing directories (`~/.claude/`, `~/.claude-work/`) are referenced in-place, not copied.

---

## Keyboard Shortcuts

### Navigation
| Action | macOS | Linux/Win |
|--------|-------|-----------|
| Command palette | Cmd+K | Ctrl+K |
| Quick switch sessions | Cmd+P | Ctrl+P |
| Toggle sidebar | Cmd+B | Ctrl+B |
| Toggle detail panel | Cmd+I | Ctrl+I |
| Settings | Cmd+, | Ctrl+, |

### Sessions
| Action | macOS | Linux/Win |
|--------|-------|-----------|
| New session | Cmd+N | Ctrl+N |
| Close/kill session | Cmd+W | Ctrl+W |
| Next session | Cmd+Down | Ctrl+Down |
| Previous session | Cmd+Up | Ctrl+Up |
| Hyperspace (all sessions) | Cmd+0 | Ctrl+0 |
| Switch to space 1/2/3 | Cmd+1/2/3 | Ctrl+1/2/3 |

### Terminal
| Action | macOS | Linux/Win |
|--------|-------|-----------|
| Split horizontal | Cmd+D | Ctrl+D |
| Split vertical | Cmd+Shift+D | Ctrl+Shift+D |
| Close split pane | Cmd+Shift+W | Ctrl+Shift+W |
| Focus next pane | Cmd+] | Ctrl+] |
| Focus prev pane | Cmd+[ | Ctrl+[ |
| Search in terminal | Cmd+F | Ctrl+F |
| Increase font | Cmd+= | Ctrl+= |
| Decrease font | Cmd+- | Ctrl+- |
| Reset font | Cmd+0 | Ctrl+0 |

---

## Design System

### Colors (Dark Theme — Primary)

```
Background:        #0A0A0F     // App background, near-black with slight purple
Sidebar BG:        #12121A     // Slightly lighter than bg
Surface:           #1A1A25     // Cards, hover states, panels
Surface Hover:     #22222F     // Interactive hover
Border:            #2A2A3A     // Subtle dividers
Border Active:     #3A3A4F     // Active/focused borders

Text Primary:      #E8E8ED     // Main text
Text Secondary:    #9898A8     // Labels, descriptions
Text Muted:        #6B6B80     // Hints, timestamps
Text Inverse:      #0A0A0F     // Text on colored backgrounds

Active (green):    #10B981     // Session active, success
Waiting (amber):   #F59E0B     // Waiting for input
Idle (gray):       #6B7280     // No activity
Error (red):       #EF4444     // Errors, destructive actions
New (blue):        #3B82F6     // New sessions, info
Accent (purple):   #8B5CF6     // Brand, links, highlights

Space Colors:      #8B5CF6, #3B82F6, #10B981, #F59E0B, #EF4444, #EC4899
```

### Colors (Light Theme)

```
Background:        #FAFAFA
Sidebar BG:        #F3F3F6
Surface:           #FFFFFF
Border:            #E5E5EA
Text Primary:      #1A1A1F
Text Secondary:    #6B6B80
Text Muted:        #9898A8
Accent:            #7C3AED

(Status colors remain the same — they work on both themes)
```

### Typography

```
UI Font:           'Inter', system-ui, sans-serif
Terminal Font:     'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace
Mono (UI):         'JetBrains Mono', monospace  // For code snippets in UI

Font Sizes:
  xs:    11px    // Timestamps, hints
  sm:    12px    // Secondary text, labels
  base:  13px    // Primary UI text
  md:    14px    // Section headers
  lg:    16px    // Dialog titles
  xl:    20px    // Page titles

Terminal default:  14px (configurable 10-24px)

Line Height:
  UI:      1.5
  Terminal: 1.2 (xterm.js default)

Font Weight:
  normal:  400
  medium:  500
  bold:    600
```

### Spacing

```
4px grid system:
  space-1:   4px
  space-2:   8px
  space-3:   12px
  space-4:   16px
  space-5:   20px
  space-6:   24px
  space-8:   32px
  space-10:  40px
  space-12:  48px
```

### Border Radius

```
  radius-sm:  4px     // Small elements, badges
  radius-md:  6px     // Buttons, inputs
  radius-lg:  8px     // Cards, panels
  radius-xl:  12px    // Dialogs, overlays
  radius-full: 9999px // Pills, dots
```

### Shadows

```
  shadow-sm:    0 1px 2px rgba(0,0,0,0.2)
  shadow-md:    0 4px 12px rgba(0,0,0,0.3)
  shadow-lg:    0 8px 24px rgba(0,0,0,0.4)
  shadow-overlay: 0 16px 48px rgba(0,0,0,0.5)
```

### Animations

```
  duration-fast:   100ms   // Hover, focus
  duration-normal: 200ms   // Expand/collapse, show/hide
  duration-slow:   300ms   // Overlay appear, layout shifts

  easing-default:  cubic-bezier(0.4, 0, 0.2, 1)
  easing-bounce:   cubic-bezier(0.34, 1.56, 0.64, 1)
```

---

## Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| macOS (Apple Silicon) | Primary | Main development platform |
| macOS (Intel) | Supported | Same binary via universal build |
| Linux (x86_64) | Supported | WebKitGTK required |
| Linux (ARM64) | Supported | Raspberry Pi, etc. |
| Windows 10/11 | Supported | WebView2 (pre-installed on Win 11) |

## License

MIT
