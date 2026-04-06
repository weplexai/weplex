# Weplex — Progress Log

## Что сделано

### Rust backend (Tauri / src-tauri)

#### PTY Manager (`pty_manager.rs`)
- `PtyManager` — менеджер PTY-сессий на основе `portable-pty` (WezTerm)
- `create()` — открывает PTY-пару, спавнит логин-шелл (`$SHELL -l`)
- Поддержка кастомных переменных окружения (env_vars) — нужно для профилей
- Правильный tilde-expand для cwd (`~`, `~/path`, `~path`)
- Explicit `cd` после старта шелла (фикс macOS zsh session restore)
- Запуск произвольной команды через PTY (для агентов)
- Читающий поток в отдельном треде: буфер 8 KB, дроссель на бёрстах ≥ 32 KB (защита от флуда IPC)
- `write()` — передача ввода в PTY
- `resize()` — изменение размера PTY (SIGWINCH)
- `kill()` — завершение сессии + остановка читающего треда через `AtomicBool`

#### Tauri команды (`main.rs`)
- `create_pty` — создать PTY-сессию
- `write_pty` — отправить данные в PTY
- `resize_pty` — изменить размер окна PTY
- `kill_pty` — завершить PTY-сессию
- `get_new_claude_session` — найти JSONL-файл сессии Claude, созданный после заданного timestamp (birthtime)
- `get_claude_usage` — распарсить JSONL-файл Claude и агрегировать usage (input/output/cache tokens, turns, model)
- `list_dirs` — автодополнение директорий (для поля cwd в диалоге)
- `discover_profiles` — обнаружить существующие конфиги Claude (`~/.claude-*`, `~/.config/claude`, парсинг `.zshrc`/`.bashrc`)

---

### Svelte frontend (src/)

#### Типы (`lib/types.ts`)
- `Session` — полная модель сессии (тип, статус, агент, спейс, папка, git, SSH, PID, Claude metadata)
- `Space`, `Folder`, `Profile`, `AppSettings`, `SplitPane` — остальные модели
- `SPACE_COLORS`, `SPACE_BG_COLORS` — палитры цветов (10 основных + 24 для фона)
- `AGENT_ICONS`, `SESSION_TYPE_ICONS`, `STATUS_COLORS` — константы UI

#### Сторы (Svelte 5 runes, `$state`)

**sessionStore** (`lib/stores/sessionStore.svelte.ts`)
- Создание/активация/переименование/закрытие сессий
- Persistence в `localStorage`
- Умная сортировка: pinned (в папках и standalone) + unpinned
- Drag-to-reorder через дробный `order`
- `moveToSpace` — переместить сессию в другой спейс
- Отслеживание `hasOutput` — для умного restore (не resume пустые сессии)
- `stats` — кол-во активных, idle, суммарный cost

**spaceStore** (`lib/stores/spaceStore.svelte.ts`)
- Создание/удаление/обновление спейсов
- Переключение спейсов с направлением перехода (`left`/`right`) для анимации
- Запоминание последней активной сессии для каждого спейса
- Поддержка `bgColor` — тинт хрома спейса

**profileStore** (`lib/stores/profileStore.svelte.ts`)
- Профили Claude: `configDir`, `envVars`, `linkedAccount`
- Default profile (всегда присутствует)
- Persistence в `localStorage`

**settingsStore** (`lib/stores/settingsStore.svelte.ts`)
- Глобальные настройки: шрифт, размер, тема, шелл, директория по умолчанию

**uiStore** (`lib/stores/uiStore.svelte.ts`)
- Управление боковой панелью: ширина (180–450px), скрытие/показ
- Persistence ширины и состояния в `localStorage`
- Управление оверлеями: command palette, quick switcher, new session, settings, space modal
- Управление панелью Detail

**folderStore** / **dragStore** — папки и состояние drag-and-drop

#### Терминал (`lib/components/terminal/TerminalView.svelte`)
- xterm.js с WebGL рендерером (fallback на canvas)
- `FitAddon` — автоподгонка размера под контейнер
- `WebLinksAddon` — кликабельные ссылки
- Кастомная цветовая схема (тёмная, акцент #8B5CF6)
- Передача Cmd/Ctrl shortcut'ов на глобальный обработчик (не перехватывает ⌘B, ⌘K и т.д.)
- Cursor скрыт до первого вывода (нет артефакта блока 0,0)
- Drag & drop файлов/папок — вставляет escaped-путь в PTY
- Умные статусы: `active` → `waiting` (4s для агентов, 15s для терминала) → `idle` (30s)
- Визуальный индикатор статуса — цветная точка 7px в сайдбаре рядом с именем сессии (см. ниже)
- Авто-resume Claude при восстановлении сессии: `--resume <uuid>` или `--continue`
- Захват Claude session ID из вывода PTY + polling файловой системы (до 12 попыток, 5s интервал)
- Polling usage каждые 30s (`get_claude_usage`)
- Profile env vars: передаются в `create_pty` если спейс привязан к профилю

#### Сайдбар
- `Sidebar.svelte` — основная панель, drag-to-resize
- `SpaceSwitcher.svelte` — переключатель спейсов (вверху сайдбара)
- `SessionItem.svelte` — элемент сессии с индикатором статуса, context menu, drag-to-reorder
- `FolderItem.svelte` — папка с сессиями, collapsible
- `SidebarSearch.svelte` — поиск по сессиям
- `SidebarFooter.svelte` — нижняя панель (настройки, статистика)

#### Оверлеи
- `CommandPalette.svelte` — полный поиск + quick switcher по сессиям (⌘K / ⌘P)
- `NewSessionDialog.svelte` — диалог создания сессии с автодополнением cwd
- `SpaceModal.svelte` — создание/редактирование спейса (цвет, bgColor, профиль)
- `Settings.svelte` — настройки приложения + управление профилями Claude

#### Прочие компоненты
- `Header.svelte` — заголовок активной сессии (breadcrumb, ветка git, статус)
- `StatusBar.svelte` — нижняя статусная строка
- `DetailPanel.svelte` — правая панель с метаданными сессии (tokens, cost, model)
- `SessionIcon.svelte` — иконка типа/агента сессии

#### Утилиты
- `detection.ts` — авто-определение типа сессии и агента по команде
- `time.ts` — форматирование времени
- `shortcuts.svelte.ts` — глобальные клавиатурные шорткаты
- `splitTree.ts` — чистые функции для рекурсивного дерева сплитов (create, split, close, find, resize)

#### Split Views
- Рекурсивное бинарное дерево: `SplitLeaf` (сессия) | `SplitBranch` (два потомка + direction + ratio)
- `splitStore.svelte.ts` — стор раскладок по спейсам, персистенция в localStorage с валидацией схемы
- `SplitContainer.svelte` — рекурсивный компонент-рендерер дерева (self-import)
- `SplitDivider.svelte` — drag-to-resize разделитель (pointer capture, accent подсветка)
- Шорткаты: ⌘D (split horizontal), ⌘⇧D (split vertical), ⌘] / ⌘[ (focus next/prev), ⌘W (close pane)
- Каждая панель — отдельная сессия с PTY
- Фокус-индикатор: subtle accent border на активной панели
- Sidebar интеграция: клик по сессии показывает её в фокусированной панели

#### Root (`App.svelte`)
- Компоновка: Sidebar + Main area + DetailPanel
- Split layout: SplitContainer рендерит видимые сессии, остальные — hidden-mounted для сохранения PTY
- Sidebar reveal zone (8px strip) при скрытом сайдбаре
- Тинт фона из `space.bgColor` через `color-mix`
- Empty state с кнопками ⌘N / ⌘K
- `$effect` синхронизирует активную сессию с split store

---

### Фонты
- JetBrains Mono Nerd Font (Regular, Bold, Italic, BoldItalic) — в `/public/fonts/`

---

### Визуальные статусы сессий

Индикатор — круглая точка 7px в `SessionItem.svelte`, цвет из `STATUS_COLORS`.

| Статус | Цвет | Hex | Поведение точки |
|--------|------|-----|-----------------|
| `active` | зелёный | `#10b981` | Пульсирует (scale 1→0.75, opacity 1→0.45, 1.4s loop) |
| `waiting` | жёлтый | `#f59e0b` | Статичная |
| `idle` | серый | `#6b7280` | Статичная |
| `new` | синий | `#3b82f6` | Статичная |
| `error` | красный | `#ef4444` | Статичная |
| `disconnected` | тёмно-серый | `#6b6b80` | Статичная |

Только `active` анимирована (`@keyframes dot-pulse`). Переходы между статусами:
- Агент: `active` → `waiting` через 4s → `idle` через 30s
- Терминал: `active` → `idle` через 15s
- При новом выводе в PTY — сразу сбрасывается в `active`

---

### Hyperspace (All Sessions View)

Системный мета-спейс, показывающий все сессии со всех спейсов.

- `HYPERSPACE_ID = '__hyperspace__'` в `types.ts`
- Первый пилл в SpaceSwitcher (иконка Layers из lucide-svelte, `Cmd+0`)
- `Cmd+1..9` — переключение на спейс по индексу
- `spaceStore` — полная поддержка Hyperspace: `activate()`, `switchToNext/Previous()`, `activeSpace`, `isHyperspace`
- `sessionStore.getAllGrouped(groupBy)` — группировка сессий по space / status / project
- `sessionStore.activateForSpace(HYPERSPACE_ID)` — запоминает последнюю активную сессию в Hyperspace
- `sessionStore.activate()` — отслеживает активную сессию в Hyperspace
- `sessionStore.stats` — расширен полем `waiting` (ранее входил в `idle`)
- Slider в Sidebar — Hyperspace slide первый (index 0), обычные спейсы начинаются с index 1
- Swipe жест — работает с Hyperspace (можно свайпнуть вправо на первый спейс и обратно)
- `HyperspaceView.svelte` — переключатель группировки (Space · Status · Project), persisted в localStorage
- Collapsible группы с цветной полосой (при группировке по Space)
- Статусные точки в заголовках групп (при группировке по Status)
- `SpaceBadge.svelte` — бейдж спейса (первая буква + цвет, 18px круг) на каждой сессии при группировке By Status / By Project
- `SessionItem.svelte` — расширен optional `badgeLetter` / `badgeColor` props
- Агрегированный footer в Hyperspace (active/waiting/idle counts + cost + New Session)
- Полноценное рабочее пространство (не read-only) — клик активирует сессию, остаёмся в Hyperspace
- Не реализовано (v2): drag & drop между space-группами = `moveToSpace()`
- Не имеет: folders, pinned/unpinned zones, profile, time-based grouping

---

## Реализовано (Phase 2 — Claude Deep Integration)

### Hook Server & Claude Code Hooks

Real-time awareness of Claude Code tool use через local HTTP server + bash hook scripts.

- `hook_server.rs` — lightweight HTTP на 127.0.0.1, рандомный порт, bearer token auth (256-bit, `/dev/urandom`)
- 5 хук-скриптов (jq-based, no shell injection): `pre-tool-use.sh`, `post-tool-use.sh`, `stop-hook.sh`, `subagent-start.sh`, `subagent-stop.sh`
- Hook registration в `~/.claude/settings.json` (PreToolUse, PostToolUse, Stop, SubagentStart, SubagentStop)
- Session-map (`~/.weplex/session-map/`) для маппинга cwd → session_id (с `$HOME → ~` нормализацией)
- Secure: token + port файлы 0600, скрипты 0700, path /hook + auth validation, 64KB body limit
- Cleanup: `cleanup_hook_files()` на `RunEvent::Exit`

### CLAUDE.md Context Injection

Workspace awareness через prepend контекстного блока в CLAUDE.md перед стартом сессии.

- `inject_claude_md` / `remove_claude_md_injection` Tauri commands
- Контекстный блок: space, session name, profile, other sessions, cost
- `strip_weplex_block()` — safe stripping с обработкой missing end marker, end-before-start
- Path validation: `canonicalize()` + `is_dir()` для предотвращения path traversal
- `contextInjectionStore.svelte.ts` — строит блок из session/space/profile stores
- Auto-injection в `TerminalView.svelte` перед `create_pty` для Claude сессий

### Sub-agent Visibility

Детекция sub-агентов Claude (Agent tool) через два механизма.

- SubagentStart/SubagentStop hooks — primary source (agent_type, agent_id)
- PreToolUse(Agent) fallback — когда нативные субагент-хуки недоступны
- `nativeSubagentSessions` set предотвращает дубликаты между двумя механизмами
- Stop от parent → все running sub-agents помечаются completed
- Stop-before-start race condition: создаётся completed запись
- `MAX_SUB_AGENTS = 100`, eviction oldest completed
- UI: секция SUB-AGENTS в DetailPanel с пульсирующей точкой для running, duration для completed

### Git Integration

Real-time git branch и status для сессий.

- `get_git_branch` — `git rev-parse --abbrev-ref HEAD`, detached HEAD → `detached@<hash>`
- `get_git_status` — `git status --porcelain -unormal` (safe для больших repo), renamed files → new path, cap 200 files
- Fetch при старте сессии + debounced refresh (2s) при PostToolUse (Write/Edit/Bash)
- Hook listener per TerminalView, proper cleanup (unlistenHook, pendingTimers)
- Заполняет `session.branch` и `session.gitFiles` — Header и DetailPanel показывают реальные данные

### Session Hierarchy

Parent/child сессии в sidebar.

- `parentId` и `childCollapsed` поля в Session
- Pipeline stages: первый stage = parent, остальные = children
- Sidebar: дети скрыты из основного списка, рендерятся под parent с 18px indent
- Chevron toggle (▸/▾) для collapse/expand с rotation animation
- `getAggregatedStatus()` — parent dot показывает агрегированный статус детей
- `kill()` — промотирует orphaned children на top-level
- `moveToSpace()` — очищает parentId при перемещении
- `statusOverride` prop в SessionItem для aggregated status

### Orchestration Dashboards

Новый тип сессии `dashboard` — Svelte компонент вместо терминала.

- **Orchestration Dashboard**: agent tree + timeline bars + real-time activity feed + changed files + conflicts. Live timer ($effect + setInterval), $derived.by для computed данных
- **Project Dashboard**: все сессии по cwd (cross-space), git branch + file status, conflicts
- **Space Dashboard**: визуальный обзор space, сессии сгруппированы по cwd в responsive grid
- `createDashboard()`, `createProjectDashboard()`, `createSpaceDashboard()` в sessionStore
- Command Palette: "New Project Dashboard", "New Space Dashboard"
- Pipeline auto-creates orchestration dashboard для multi-stage runs
- Icon `▦` для dashboard-сессий

### Unified Agent Resolution

Pipeline engine резолвит агентов из всех источников одинаково.

- `~/.claude/agents/*.md` — Claude user agents (конвертируются в WeplexAgent с binary=claude)
- `{cwd}/.claude/agents/*.md` — Claude project agents (override user)
- `~/.weplex/agents/*.yaml` — Weplex native agents (override Claude on conflict)
- `collect_all_agents(cwd)` — мержит все три источника
- `agent_map` — обязательный параметр `prepare_run()`, no hidden fallback
- Оба формата равноправны — без иерархии между экосистемами

### Conflict Detection

Детекция конфликтов когда два агента редактируют один файл.

- hookStore: `recentEdits` Map с 1-минутным sliding window
- Два PostToolUse на один файл в окне = conflict warning
- Periodic cleanup каждые 5 минут
- UI: секция CONFLICTS в DetailPanel и Orchestration/Project Dashboards
- `getConflictsForSession()` / `hookStore.conflicts` API

---

## Спроектировано (ещё не реализовано)

### Terminal Decorations

Inline contextual actions поверх terminal output при hover на детектированные паттерны (пути, команды, URL, git-ветки и т.д.). Спецификация в DESIGN.md "## Terminal Decorations".

- xterm.js Decoration API — DOM-элементы, привязанные к позиции в буфере, скроллятся с контентом
- Hover-triggered action bar (появляется через 150ms, исчезает через 300ms grace period)
- Pattern Registry: file path, command, URL, git branch, file:line, npm/cargo команда
- Actions: Open in Finder, Open in new terminal, Run in new session, Copy, Open in browser
- Agent-specific rich decorations (Claude tool use, file edits) — Phase 3
- Performance: max 50 активных decorations, throttle при >10 lines/sec, активируются после 1s idle

### MCP Server v2

Cross-session communication tools для Claude.

- `deck_list_sessions()` — список всех сессий с метаданными
- `deck_create_session()` — создание PTY сессии изнутри Claude (с parentId)
- `deck_read_output()` — чтение вывода другой сессии
- `deck_send_input()` — отправка ввода в другую сессию
- `deck_get_context()` — контекст workspace (spaces, cost, settings)

---

## Ключевые архитектурные решения

| Решение | Обоснование |
|---------|-------------|
| Svelte 5 runes (`$state`, `$derived`, `$effect`) | Реактивность без store subscription boilerplate |
| portable-pty (Rust) | Нативный PTY на macOS/Linux/Windows |
| Все сессии рендерятся сразу | Нет задержки при переключении — xterm не пересоздаётся |
| Polling Claude JSONL напрямую | Нет зависимости от Claude API — читаем файлы локально |
| Session ID захват двумя путями | PTY output regex + file birthtime polling |
| `color-mix` для space tint | CSS-нативно, без JS вычислений |
| Дробный `order` для drag-reorder | Нет перенумерации при вставке между элементами |
