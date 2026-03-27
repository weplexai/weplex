# Weplex — Plugin Architecture

> Plugins extend Weplex beyond terminal sessions. Each plugin can register new session types, inject UI into controlled slots, and integrate external tools — without touching Weplex core.

## Motivation

Weplex core = terminal + AI agents + pipelines. This is the foundation for all users.

But developers also need browsers, databases, monitoring, notes — side by side with terminals. Instead of bloating core, Weplex provides a **Plugin API** that lets each tool be installed separately from the Marketplace.

**Key principle**: plugins are additive. Removing a plugin returns Weplex to its core state with zero side effects.

---

## Architecture Overview

```
Weplex Core (always present)
├── Sessions: terminal, agent, ssh
├── Spaces, Sidebar, Split View, Command Palette
├── Pipeline Engine
├── Plugin Host ← loads/unloads plugins
└── Plugin Tray ← one icon per active plugin

Weplex Marketplace (Phase 4)
├── Agents    (*.yaml → ~/.weplex/agents/)
├── Pipelines (*.yaml → ~/.weplex/pipelines/)
└── Plugins   (packages → ~/.weplex/plugins/)    ← NEW
```

### Plugin vs Agent vs Pipeline

| | Agent | Pipeline | Plugin |
|--|-------|----------|--------|
| What | CLI binary config | Multi-stage workflow | Weplex extension |
| Format | YAML | YAML | Svelte + Rust (optional) |
| Extends | Session behavior | Session orchestration | Weplex UI & functionality |
| Example | Claude, Aider | Feature pipeline | Browser, DB client |

---

## Plugin API

### Contract

```typescript
interface DeckPlugin {
  // Identity
  id: string;              // 'browser', 'database', 'monitoring'
  name: string;            // 'Built-in Browser'
  icon: string;            // lucide icon name or emoji
  version: string;         // semver

  // Optional: new session type
  sessionType?: {
    type: string;          // 'browser' → added to SessionType
    label: string;         // 'Browser' → shown in New Session dialog
    icon: string;          // '🌐'
    create(opts: Record<string, unknown>): Promise<SessionConfig>;
    destroy(sessionId: string): Promise<void>;
    render(container: HTMLElement, session: Session): void;
  };

  // Optional: tray panel (click on plugin icon in sidebar)
  trayPanel?: {
    component: SvelteComponent;
    width?: number;        // max 320px, default 280px
  };

  // Optional: custom pane header for plugin's session type
  // Rendered per-pane (each split pane has its own header)
  paneHeader?: {
    component: SvelteComponent;  // receives { session, paneId } as props
    // Plugin is responsible for fitting into 40px height.
    // For Browser plugin: [icon] [nav] [URL bar] [extension icons] [overflow]
  };

  // Lifecycle
  onActivate(): Promise<void>;
  onDeactivate(): Promise<void>;
}
```

### What plugins CAN do

```
✅ Register a new session type (appears in New Session dialog)
✅ Add one icon to Plugin Tray (sidebar bottom)
✅ Show a panel on tray icon click (max 320px wide)
✅ Provide a pane header component (rendered per split pane for plugin's session type)
✅ Use Tauri IPC (invoke Rust commands from their package)
✅ Store settings in ~/.weplex/plugins/<id>/config.json
✅ Register commands in Command Palette
```

### What plugins CANNOT do

```
❌ Modify Weplex layout or core UI structure
❌ Inject UI into arbitrary positions
❌ Override other plugins' UI
❌ Access other plugins' state
❌ Modify Spaces, Session List, or other core components
❌ Change sidebar width, split ratios, or global settings
❌ Run background processes without user consent
```

---

## UI Integration Points

### Plugin Tray

Fixed area at the bottom of the sidebar. One icon per installed plugin.

```
┌──────────────┐
│ Spaces       │  ← core
├──────────────┤
│ Sessions     │  ← core
│  ⚡ claude    │
│  >_ terminal │
│  🌐 github   │  ← session type from plugin
├──────────────┤
│ Plugin Tray  │  ← plugin icons
│  🌐  🗄️  📊   │     max 8, user-reorderable
└──────────────┘
```

**Rules:**
- Max 8 plugin icons in tray
- Icon size: 24px, fixed
- Order: user-controlled (drag to reorder)
- Tooltip: plugin name on hover
- Click: opens/closes plugin's tray panel
- Only one panel open at a time (like radio buttons)

### Tray Panel

Opens next to sidebar on plugin icon click. Anchored, not floating. Used for plugin management and settings — NOT for primary interaction (that happens in pane header).

```
┌──────────┬──────────────────┬───────────────────────┐
│ Sidebar  │ Plugin Panel     │ Content               │
│          │ (max 320px)      │                       │
│          │                  │                       │
│          │ Management UI,   │                       │
│          │ settings,        │                       │
│          │ status overview  │                       │
│          │                  │                       │
│          │                  │                       │
├──────────┤                  │                       │
│ 🌐 ←     │                  │                       │
└──────────┴──────────────────┴───────────────────────┘
```

**Browser plugin tray panel example** (management, not extensions):

```
┌──────────┬──────────────────┐
│          │ Browser          │
│ Sessions │                  │
│          │ Browsers:        │
│          │  🌐 Chrome  [on] │
│          │  🦊 Firefox [on] │
│          │  🔷 Edge   [off] │
│          │                  │
│          │ Active sessions: │
├──────────┤  🌐 github  (2)  │
│  🌐 ←    │  🦊 MDN     (1)  │
│          │                  │
│          │ ⚙️  Settings      │
│          │  Default browser │
│          │  CDP ports       │
│          │  Profile mgmt   │
└──────────┴──────────────────┘
```

### Pane Header

Each split pane has its own header bar. The header content is determined by the session type in that pane. Plugins provide a `paneHeader` component that Weplex renders per-pane.

```
Single session:
┌──────────┬────────────────────────────────────┐
│ Sidebar  │ 🌐 ← → ↻  github.com/user/repo    │  ← browser pane header
│          ├────────────────────────────────────┤
│          │ Chrome content                     │
└──────────┴────────────────────────────────────┘

Split: browser + terminal (each pane has own header):
┌──────────┬─────────────────┬──────────────────┐
│ Sidebar  │ 🌐 ← → github.. │ ~/project  main  │  ← two different headers
│          ├─────────────────┼──────────────────┤
│          │ Chrome content  │ $ npm run dev    │
│          │                 │ > ready :3000    │
└──────────┴─────────────────┴──────────────────┘

Split: Chrome + Firefox (same plugin, different sessions):
┌──────────┬─────────────────┬──────────────────┐
│ Sidebar  │ 🌐 ← → :3000    │ 🦊 ← → :3000     │  ← same plugin, two panes
│          ├─────────────────┼──────────────────┤
│          │ Chrome render   │ Firefox render   │
└──────────┴─────────────────┴──────────────────┘

Three-way: terminal + Chrome + Firefox:
┌──────────┬──────────┬──────────┬──────────────┐
│ Sidebar  │~/p main  │🌐 ← :3k  │🦊 ← :3k      │
│          ├──────────┼──────────┼──────────────┤
│          │ terminal │ Chrome   │ Firefox      │
└──────────┴──────────┴──────────┴──────────────┘
```

**Pane header layout for Browser plugin:**

```
┌─────────────────────────────────────────────────────────┐
│ 🌐  ← → ↻  │ 🔒 github.com/user/repo       │ 🛡️ 🔒 🌙 🧩 │
│ browser &   │ URL bar                       │ extensions  │
│ nav buttons │ (flexible width)              │ (max ~5 +   │
│             │                               │  overflow)  │
└─────────────────────────────────────────────────────────┘
```

Extension overflow menu (when >5 extensions):
```
Click 🧩 →  ┌──────────────────┐
            │  📝 Notion Clip   │
            │  🔧 React DevTools│
            │  🎨 ColorPicker   │
            │  ──────────────── │
            │  Manage...        │
            └──────────────────┘
```

Click on extension icon → opens extension popup as a small window anchored to the icon position. Same behavior as Chrome toolbar.

**Rules:**
- Header height: fixed (40px), plugin cannot change
- Each pane independently renders the header for its session type
- Core provides default pane header for terminal/agent/ssh sessions
- Plugin's `paneHeader` component receives `{ session, paneId }` as props
- No conflicts: each pane is sovereign over its own header
- Extension icons belong in pane header (not in tray panel) — one click access, familiar Chrome pattern

---

## Plugin Lifecycle

### Installation

```
User → Marketplace → Install Plugin
  └→ Download package to ~/.weplex/plugins/<id>/
  └→ Plugin appears in Plugin Tray (disabled)
  └→ User enables plugin → onActivate() called
```

### Activation

```
onActivate():
  - Plugin initializes its resources
  - Registers session type (if any)
  - Registers commands in Command Palette (if any)
  - Tray icon becomes interactive
```

### Deactivation

```
onDeactivate():
  - Plugin releases resources
  - Session type removed from New Session dialog
  - Existing sessions of this type: kept but marked inactive
  - Tray icon grayed out
  - Commands removed from Command Palette
```

### Uninstallation

```
User → Settings → Plugins → Uninstall
  └→ onDeactivate() called
  └→ Plugin files removed from ~/.weplex/plugins/<id>/
  └→ Plugin data optionally cleaned (ask user)
  └→ Existing sessions closed
```

---

## File Structure

```
~/.weplex/
├── agents/                # Agent YAML configs
├── pipelines/             # Pipeline YAML configs
└── plugins/               # Plugin packages
    └── browser/
        ├── weplex-plugin.json   # Plugin manifest
        ├── dist/              # Compiled Svelte components
        │   ├── index.js
        │   ├── TrayPanel.js
        │   ├── SessionHeader.js
        │   └── BrowserView.js
        ├── rust/              # Optional: Tauri plugin (compiled)
        │   └── libbrowser.dylib
        └── config.json        # Plugin settings (user-editable)
```

### Plugin Manifest

```json
{
  "id": "browser",
  "name": "Built-in Browser",
  "version": "0.1.0",
  "icon": "globe",
  "description": "Multi-browser integration with Chrome, Firefox, and Edge. Full extension support via CDP.",
  "author": "Deck Team",
  "license": "MIT",
  "entry": "dist/index.js",
  "rust_plugin": "rust/libbrowser.dylib",
  "permissions": [
    "shell:chrome",
    "shell:firefox",
    "shell:edge",
    "network:localhost:9222-9230",
    "fs:read:browser-profiles"
  ],
  "min_deck_version": "1.0.0"
}
```

---

## Reference Plugin: Browser

The Browser plugin is the first plugin, serving as the reference implementation for the Plugin API.

### How It Works

Instead of embedding a browser engine, Weplex launches the user's installed browsers in `--app` mode and controls them via Chrome DevTools Protocol (CDP). Both Chrome and Firefox (86+) support CDP.

```
Weplex (Tauri + Svelte)
┌──────────────────────┐
│ Svelte UI            │
│ (sidebar, tabs,      │    CDP :9222       ┌──────────────────┐
│  URL bar, extensions)│◄─────────────────►│ Chrome           │
│                      │                    │ Extensions ✅     │
│                      │    CDP :9223       ├──────────────────┤
│                      │◄─────────────────►│ Firefox          │
│                      │                    │ Add-ons ✅        │
│                      │    CDP :9224       ├──────────────────┤
│                      │◄─────────────────►│ Edge (Chromium)  │
│                      │                    │ Extensions ✅     │
└──────────────────────┘                    └──────────────────┘
```

**Key insight**: Browsers are already installed on the user's system. No need to download source code, compile, or embed an engine. CDP is a documented, stable protocol.

### Multi-Browser Support

One plugin controls multiple browser engines via the same protocol:

```bash
# Each browser launches with its own debugging port
chrome  --remote-debugging-port=9222 --app=about:blank
firefox --remote-debugging-port=9223
edge    --remote-debugging-port=9224 --app=about:blank
```

| Browser | CDP Support | Extensions | Profile Dir (macOS) |
|---------|-------------|------------|---------------------|
| Chrome | Full | Chrome Web Store | `~/Library/Application Support/Google/Chrome/Default/Extensions/` |
| Firefox | Since v86 | Firefox Add-ons (AMO) | `~/Library/Application Support/Firefox/Profiles/*.default/extensions/` |
| Edge | Full (= Chromium) | Chrome Web Store + Edge Add-ons | `~/Library/Application Support/Microsoft Edge/Default/Extensions/` |
| Safari | Web Inspector Protocol (limited) | Safari Extensions | Not via CDP — separate integration |

### Browser Session Type

```typescript
interface BrowserSessionConfig {
  type: 'browser';
  browser: 'chrome' | 'firefox' | 'edge';
  url: string;
  cdpPort: number;      // auto-assigned per browser instance
  profile?: string;     // browser profile (for multi-account isolation)
}
```

### What the Browser Plugin Provides

| API Point | Implementation |
|-----------|---------------|
| `sessionType` | type: 'browser', sub-selection: Chrome / Firefox / Edge |
| `trayPanel` | Browser management: installed browsers, active sessions, settings |
| `paneHeader` | URL bar + nav buttons + extension icons + overflow menu (per-pane) |
| `onActivate` | Detect installed browsers, prepare CDP ports |
| `onDeactivate` | Close CDP connections (optionally close browser processes) |

### New Session Dialog (with Browser plugin)

```
┌─ New Session ──────────────────────────┐
│                                        │
│  ⚡ Agent                               │
│  >_ Terminal                            │
│  ↗  SSH                                │
│  ── plugins ──                         │
│  🌐 Browser                            │
│     ├── 🌐 Chrome                      │
│     ├── 🦊 Firefox                     │
│     └── 🔷 Edge                        │
│                                        │
│  URL: [https://                    ]   │
│                                        │
└────────────────────────────────────────┘
```

### CDP Integration (Rust side)

```
Tauri commands (in rust plugin):
├── browser_detect_installed()    → scan system for available browsers
├── browser_launch(browser, port) → spawn browser process with CDP
├── browser_connect(port)         → WebSocket to CDP
├── cdp_create_tab(port, url)     → Target.createTarget
├── cdp_navigate(port, tabId, url)→ Page.navigate
├── cdp_close_tab(port, tabId)    → Target.closeTarget
├── cdp_get_tabs(port)            → Target.getTargets
├── cdp_get_extensions(browser)   → read from browser profile dir
├── cdp_open_extension_popup(browser, id) → open extension popup window
└── cdp_on_event(port, callback)  → listen to CDP events
```

### Browser Extension Icons in Pane Header

Extension icons are displayed in the **pane header** of each browser session (right side, next to URL bar) — not in the tray panel. This matches the familiar Chrome/Firefox toolbar pattern.

**Discovery:** the plugin reads installed extensions from each browser's profile directory:

```
Chrome:  ~/Library/Application Support/Google/Chrome/Default/Extensions/
Firefox: ~/Library/Application Support/Firefox/Profiles/*.default/extensions/
Edge:    ~/Library/Application Support/Microsoft Edge/Default/Extensions/

Each extension dir → read manifest.json → extract name, icon, popup path
```

**Display:** up to ~5 extension icons visible in pane header, rest in overflow menu (🧩).

**Interaction:** click on extension icon → open extension popup as a small browser window anchored below the icon. Same UX as Chrome toolbar.

**Per-browser:** each pane shows extensions for its own browser engine:
```
Chrome pane header:  │ 🔒 github.com     │ 🛡️ 🔒 🌙 🧩 │  ← Chrome extensions
Firefox pane header: │ 🔒 github.com     │ 🛡️ 🔒 🧩    │  ← Firefox add-ons
```

### Tray Panel (Management Only)

Extensions are in the **pane header** (see above). The tray panel is for browser management:

```
┌──────────┬──────────────────┐
│          │ Browser          │
│ Sessions │                  │
│          │ Browsers:        │
│          │  🌐 Chrome  [on] │
│          │  🦊 Firefox [on] │
│          │  🔷 Edge   [off] │
│          │                  │
│          │ Active sessions: │
├──────────┤  🌐 github  (2)  │
│  🌐 ←    │  🦊 MDN     (1)  │
│          │                  │
│          │ ⚙️  Settings      │
│          │  Default browser │
│          │  CDP ports       │
│          │  Profile mgmt   │
└──────────┴──────────────────┘
```

### Visual Layout

```
Terminal session (core pane header):
┌──────────┬────────────────────────────────────┐
│ Sessions │  ~/project  main                   │  ← core pane header
│  ⚡claude ├────────────────────────────────────┤
│  🌐 app  │  $ npm run dev                     │
│  >_ term │  > ready on localhost:3000          │
├──────────┤                                    │
│  🌐      │                                    │
└──────────┴────────────────────────────────────┘

Browser session (URL bar + extensions in pane header):
┌──────────┬──────────────────────────────────────────┐
│ Sessions │ 🌐 ← → ↻ │ 🔒 github.com/repo │ 🛡️ 🔒 🌙 │  ← browser pane header
│  ⚡claude ├──────────────────────────────────────────┤
│  🌐 app ◄│                                          │
│  🦊 docs │  GitHub repository page                  │
│  >_ term │                                          │
├──────────┤                                          │
│  🌐      │                                          │
└──────────┴──────────────────────────────────────────┘

Split: terminal + browser (independent pane headers):
┌──────────┬──────────────────┬───────────────────────┐
│ Sessions │  ~/project main  │ 🌐 ← → │:3000│ 🛡️🔒🌙│
│  ⚡claude ├──────────────────┼───────────────────────┤
│  🌐 app  │  $ npm run dev   │  Chrome content       │
│  >_ term │  > ready :3000   │                       │
├──────────┤                  │                       │
│  🌐      │                  │                       │
└──────────┴──────────────────┴───────────────────────┘

Cross-browser split (extensions per-browser in each pane):
┌──────────┬───────────────────────┬───────────────────────┐
│ Sessions │ 🌐 ← → │:3000│ 🛡️🔒🌙│ 🦊 ← → │:3000│ 🛡️🔒 │
│  ⚡claude ├───────────────────────┼───────────────────────┤
│  🌐 app  │  Your app as          │  Your app as          │
│  🦊 app  │  seen in Chrome       │  seen in Firefox      │
│  >_ term │  + Chrome extensions  │  + Firefox add-ons    │
├──────────┤                       │                       │
│  🌐      │                       │                       │
└──────────┴───────────────────────┴───────────────────────┘

Three-way split (terminal + Chrome + Firefox):
┌──────────┬────────────┬──────────────┬──────────────┐
│ Sessions │ ~/p main   │🌐 ← │:3k│🛡️🔒│🦊 ← │:3k│🛡️🔒│
│  ⚡claude ├────────────┼──────────────┼──────────────┤
│  🌐 app  │ > fix      │ Chrome       │ Firefox      │
│  🦊 app  │   css...   │ [result]     │ [result]     │
│  >_ term │            │              │              │
├──────────┤            │              │              │
│  🌐      │            │              │              │
└──────────┴────────────┴──────────────┴──────────────┘
```

### Profile Isolation

Multiple sessions of the same browser can use different profiles:

```
🌐 GitHub (work profile)   → Chrome --profile-directory="Work"
🌐 Gmail (personal)        → Chrome --profile-directory="Personal"
🦊 MDN (default)           → Firefox -P default
```

Different profiles = different accounts, cookies, extensions. No conflicts.

### Requirements

- At least one supported browser installed (Chrome, Firefox, or Edge)
- No additional downloads, compilation, or disk space needed

### Limitations

- Requires browser(s) installed (plugin shows which are available)
- Separate process per browser engine — lightweight but multiple processes
- Window compositing on macOS requires native API for seamless embedding
- Browser updates independently — CDP protocol is stable across versions
- Safari uses Web Inspector Protocol (not CDP) — requires separate adapter, lower priority

---

## Future Plugin Ideas

| Plugin | Session Type | Tray Panel | Pane Header |
|--------|-------------|------------|-------------|
| **Browser** | 🌐 browser (Chrome, Firefox, Edge) | Browser extensions per engine | URL bar + nav + browser icon |
| **Database** | 🗄️ database | Connections list | Query bar + connection selector |
| **Monitoring** | 📊 monitoring | Alert list | Dashboard selector |
| **Notes** | 📝 notes | Note tree | Note title + tags |
| **API Client** | 🔌 api | Request history | Method + URL bar |
| **Docker** | 🐳 docker | Container list | Container selector |
| **Logs** | 📋 logs | Stream list | Filter bar |

All follow the same contract: session type + tray icon + optional panel + optional pane header.
Each pane header is independent — multiple plugin sessions can coexist in splits without conflicts.

---

## Design Constraints

### Why not free-form UI injection?

Plugins that can inject UI anywhere lead to:
1. **Layout wars** — two plugins fight for the same space
2. **Broken design** — inconsistent spacing, colors, sizing
3. **Fragile updates** — Weplex core changes break plugin layouts
4. **Security risk** — malicious plugins overlay sensitive UI

The tray model prevents all of this. One icon, one panel, one header slot.

### Plugin Tray Rules

| Rule | Value | Reason |
|------|-------|--------|
| Max plugins in tray | 8 | Physical space limit |
| Icon size | 24px | Consistent with sidebar |
| Panel max width | 320px | Preserve content area |
| Panel height | Match sidebar | Consistent alignment |
| Open panels | 1 at a time | No overlap, no confusion |
| Order | User-controlled | User decides priority |
| Pane header height | 40px (fixed) | Prevent layout shift |
| Pane headers | Per-pane, not global | Each split pane renders its own |

### Permissions

Plugins declare required permissions in manifest. User approves on install.

```json
{
  "permissions": [
    "shell:chrome",              // launch Chrome process
    "network:localhost:9222",    // CDP WebSocket
    "fs:read:chrome-extensions"  // read extension manifests
  ]
}
```

No permission = no access. Weplex enforces at runtime.

---

## Relation to Roadmap

Plugin API and Marketplace are part of **Phase 4** (see [ROADMAP.md](./ROADMAP.md)).

However, the **Plugin API contract** should be designed in Phase 2-3 and the Browser plugin can serve as an internal proof-of-concept before the public Marketplace launches.

```
Phase 2:  Design Plugin API contract (this document)
Phase 3:  Internal plugins (Browser as first)
Phase 4:  Public Marketplace (agents + pipelines + plugins)
```
