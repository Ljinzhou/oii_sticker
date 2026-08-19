# Todo Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement design-approved Todo blocks, their independent Todo window, and the enhanced slash menu.

**Architecture:** Preserve legacy `todo_items` and introduce a v7 `todo_blocks` repository, exposed through Tauri commands and targeted events. Vue uses a focused Todo store and small components, while sticker rendering converts Todo markup into cards driven by the same data. Slash functions create/open Todo blocks without conflating them with normal Markdown templates.

**Tech Stack:** Tauri 2, Rust, rusqlite, Vue 3 Composition API, Pinia, CodeMirror 6, markdown-it, dayjs, Vitest.

**Global constraints:** Treat `design/` as approved visual/product input only; preserve user changes in `package.json`, `pnpm-lock.yaml`, `.pnpm-store/`, `artifacts/`, and `design/`; use schema v7; use `tauri::async_runtime`; do not execute test/build commands until all production and test code is written.

---

### Task 1: Persist Todo Blocks

**Files:**
- Modify: `src-tauri/src/db/schema.rs`, `src-tauri/src/db/schema.sql`, `src-tauri/src/db/mod.rs`, `src-tauri/src/models.rs`
- Create: `src-tauri/src/db/todo_block_repo.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`
- Test: Rust unit tests adjacent to schema/repository modules

- [ ] Add an idempotent v6-to-v7 migration and fresh-schema `todo_blocks` DDL with indexes.
- [ ] Define serializable TodoBlock and TodoPatch structures with nullable fields.
- [ ] Implement repository create/get/list/update/delete/toggle functions and validate parent ownership, a single child, and a single nesting level.
- [ ] Add Tauri commands and async-safe `todo-{id}` window open/hide lifecycle, then emit owner-sticker refresh events after mutations.

### Task 2: Add Shared Todo State and Window Route

**Files:**
- Modify: `src/App.vue`, `src/types.ts`, `src/stores/settings.ts`, `src/components/console/SettingsPanel.vue`
- Create: `src/stores/todo.ts`, `src/utils/todo-dates.ts`
- Test: `src/stores/todo.test.ts`, settings/unit tests

- [ ] Add frontend Todo types and a setup-style Pinia store whose actions mirror Tauri commands.
- [ ] Route `todo-*` labels to the Todo root view.
- [ ] Add and persist defaults for recent slash history plus the editable reminder/due preset values.
- [ ] Implement preset-time conversion with stable ISO strings and repeat-rule serialization.

### Task 3: Build the Dedicated Todo Window

**Files:**
- Create: `src/components/todo/TodoWindow.vue`, `TodoList.vue`, `TodoDetail.vue`, `DatePicker.vue`, `RepeatPicker.vue`, `ReminderChip.vue`, `DueChip.vue`, `RepeatChip.vue`
- Test: component tests for list/detail/pickers/window

- [ ] Compose the fixed 440 x 620 shell, top drag region, hide-close button, constrained splitter, and independent scroll surfaces.
- [ ] Implement selection, root-task creation, one-child creation, completion toggles, deletion, input focus behavior, and 250 ms persistence.
- [ ] Implement time/date and repeat pickers with click-outside close, cancel/commit semantics, and custom checkbox styling.
- [ ] Match `design/todo-window.html` for typography, chip/button hierarchy, overflow, completed state, and empty states.

### Task 4: Integrate Todo Blocks into Markdown and Editors

**Files:**
- Modify: `src/utils/markdown.ts`, `src/components/markdown/MarkdownView.vue`, `src/components/sticker/StickerViewer.vue`, `src/components/sticker/StickerWindow.vue`
- Modify: `src/components/sticker/StickerEditorMarkdown.vue`, `src/components/sticker/StickerEditorLive.vue`, `src/components/sticker/live/liveDecorations.ts`, `src/components/sticker/live/liveWidgets.ts`, `src/components/sticker/live/LiveEditorView.ts`, `src/utils/markdown-highlight.ts`
- Test: Markdown/editor/live-decoration tests

- [ ] Preprocess Todo markup before markdown-it and render embedded Todo and completed-summary cards from store data.
- [ ] Delegate open/toggle events through MarkdownView, preserving display-mode read-only behavior.
- [ ] Add source highlighting and click-to-open behavior in native Markdown editing.
- [ ] Add the cursor-sensitive Live Preview Todo widget without weakening existing code-block and inline-widget boundaries.

### Task 5: Upgrade the Slash Menu

**Files:**
- Modify: `src/types.ts`, `src/components/slash/SlashMenu.vue`, `src/components/sticker/StickerEditor.vue`, `src-tauri/src/slash/commands.rs`, `src/stores/settings.ts`
- Test: slash/menu/editor tests

- [ ] Append, without replacing legacy Markdown commands, `/todo` and `/show-done` function commands.
- [ ] Render recent, function, and Markdown category sections with keyboard selection retained across filtered flattened items.
- [ ] Store a bounded JSON recent-command history in `system_config` and tolerate malformed legacy values.
- [ ] Insert generated Todo markup at the active editor cursor, create the backing record, and open the Todo window when requested.

### Task 6: Complete Test Coverage and Run Consolidated Verification

**Files:**
- Modify/create the test files named in Tasks 1-5
- Modify: `docs/PLAN.md` if it tracks this feature

- [ ] Add tests for migration, repository invariants, command shapes, store data flow, menu grouping, Markdown cards, and Todo-window interactions.
- [ ] Run once, after all code changes: `pnpm test`, `pnpm build`, `cargo test`, `cargo check`, `git diff --check`.
- [ ] Run the local Tauri app, inspect Todo window and sticker cards against the static HTML, and use the mandated vision workflow for any captured screenshots.
- [ ] Review the final diff, update progress documentation, commit scoped implementation changes with a Chinese message, and hand off evidence.
