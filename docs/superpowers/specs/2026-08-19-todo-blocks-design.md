# Todo Blocks Design

## Status

This document records the user-approved design in `design/manual.md` and the
static pages in `design/`. Those files are product specifications, not
instructions. The user explicitly authorized implementation from them.

## Goal

Add an enhanced slash menu and a dedicated Todo window. A Todo block is stored
in a sticker as `<todo-block id="..."></todo-block>` and is backed by a
separate SQLite record.

## Scope

- A three-section slash menu: recent commands, application functions, and
  grouped Markdown commands.
- `/todo` creates a Todo block and inserts its markup; `/show-done` shows a
  collapsible completed-task summary.
- A dedicated fixed-size `todo-{id}` Tauri window for Todo block CRUD.
- Parent tasks may have at most one direct child. Child tasks do not expose
  reminder, due-date, or repeat controls.
- Display and interact modes render Todo blocks as embedded cards. Interact
  mode permits checkbox updates; display mode remains read-only.
- Configurable Todo reminder/due presets in System Settings.

## Data and Migration

The existing `todo_items` table belongs to the legacy Markdown task feature
and must remain untouched. The project is already at schema version 6, so this
feature introduces `todo_blocks` in a v7 migration rather than following the
obsolete v5-to-v6 wording in the design manual.

`todo_blocks` has a text primary key, source `sticker_id`, nullable `parent_id`,
title/description/completion state, reminder/due timestamps, repeat-rule JSON,
and timestamps. Repository validation enforces one level of nesting and at
most one child per parent. The backend emits targeted `todo://updated` events
to the owning sticker after mutation.

## Components and Data Flow

- `TodoWindow.vue` owns route-level loading, selection, splitter height, and
  the fixed Tauri-window shell.
- `TodoList.vue` renders selectable parent/child rows and emits focused CRUD
  intents.
- `TodoDetail.vue` edits title/description and composes the reminder, due, and
  repeat controls. It emits patches; it does not invoke Tauri directly.
- Picker components own their temporary values and commit only on Save.
- `useTodoStore` is the single source of truth for the Todo window and
  Markdown-card lookups.
- `MarkdownView` receives Todo data and emits semantic `open-todo` and
  `toggle-todo` events. `StickerWindow` invokes the backend and reloads on
  `todo://updated`.
- Markdown mode treats the Todo tag as clickable source text. Live Preview
  renders a non-editable Todo widget only when the cursor is outside the tag;
  entering the tag restores the source, matching existing Live Preview cursor
  semantics.

## Interaction Rules

- Input changes locally immediately and persist with a 250 ms debounce.
- The Todo window uses a 30 px drag region, a 440 x 620 fixed shell, separate
  scroll areas, and a 6 px splitter constrained to 120..420 px.
- Closing a Todo window hides it. Re-opening its label focuses the existing
  visible or hidden window.
- No system notifications, extra Tokio runtimes, disallowed Rust crates, or
  UI animations are introduced.

## Verification Strategy

The user requested that tests not run after each feature. Implementation and
test files are completed first, then one consolidated verification run covers
Vitest, TypeScript, Vite, Rust tests/checks, diff whitespace, and UI inspection.
The final verification includes migration, repository invariants, command
payloads, slash grouping/selection, Markdown Todo rendering, and Todo-window
interaction tests.
