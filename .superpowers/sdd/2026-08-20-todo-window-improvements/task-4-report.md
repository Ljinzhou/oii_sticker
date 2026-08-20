# Task 4 Report

## Changed Files

- `src-tauri/src/db/schema.rs`
  - Bumped `SCHEMA_VERSION` from 7 to 8.
  - Added the first-install default `default_todo_always_on_top=1`.
  - Added transactional `migrate_v7_to_v8` using `INSERT OR IGNORE`.
  - Updated fixed schema-version assertions from 7 to 8.
  - Added coverage for preserving an existing `0` and inserting a missing default `1`.
- `src-tauri/src/lib.rs`
  - Applied the setting when creating Todo windows and before showing existing windows.
  - Applied changes to all `todo-` windows when the setting is updated.
  - Logged `set_always_on_top` failures with `tracing::warn!` without blocking Todo operations.
- `src/components/console/SettingsPanel.vue`
  - Added the Todo default topmost checkbox before the time presets.

## TDD Evidence

### RED

Command:

```text
cargo test schema::tests::migrate_v7_to_v8_adds_todo_window_topmost_default --manifest-path src-tauri/Cargo.toml
```

Result: expected failure before production implementation:

```text
test db::schema::tests::migrate_v7_to_v8_adds_todo_window_topmost_default ... FAILED
assertion `left == right` failed
left: 7
right: 8
test result: FAILED. 0 passed; 1 failed
```

### GREEN

The same command after implementation:

```text
test db::schema::tests::migrate_v7_to_v8_adds_todo_window_topmost_default ... ok
test result: ok. 1 passed; 0 failed; 104 filtered out
```

Schema test suite:

```text
cargo test schema::tests --manifest-path src-tauri/Cargo.toml
test result: ok. 7 passed; 0 failed; 98 filtered out
```

Full Rust tests:

```text
cargo test --manifest-path src-tauri/Cargo.toml
test result: ok. 105 passed; 0 failed
```

Rust check:

```text
cargo check --manifest-path src-tauri/Cargo.toml
Finished `dev` profile
EXIT_CODE=0
```

Frontend verification:

```text
pnpm build
```

The command was attempted twice, but pnpm could not access the configured registry and emitted repeated `EACCES` errors while resolving/downloading dependencies. The worktree had no usable `node_modules/.bin/vite` or `vue-tsc` links. Direct local binaries were then used:

```text
vue-tsc --noEmit
src/components/todo/TodoDetail.test.ts(5,1): error TS6133: 'TodoDatePicker' is declared but its value is never read.
```

This pre-existing error is outside the Task 4 file scope. Direct Vite bundling succeeded:

```text
vite v6.4.3 building for production...
✓ 746 modules transformed.
✓ built in 6.04s
EXIT_CODE=0
```

## Self-Review

- Existing user values are preserved by `INSERT OR IGNORE`; missing values receive `1`.
- Existing and newly created Todo windows use the current database setting.
- Runtime setting changes affect only labels beginning with `todo-`; the existing label format is unchanged.
- Every `set_always_on_top` failure is warning-only and cannot prevent window creation, display, focus, or config persistence.
- No second Todo window state store was introduced.
- `git diff --check` passed.
- The diff contains only the three brief-listed business files plus this report.

## Concerns

- Initial Task 4 verification: `pnpm build` could not complete because registry access returned `EACCES`.
- Initial Task 4 verification: `vue-tsc --noEmit` was blocked by the unused import in `src/components/todo/TodoDetail.test.ts`; the review follow-up below removes it.
- Direct Vite bundling passed, and Rust tests/checks passed. Windows linker output contains the existing `linker_messages` warning.

## Commit

Commit message: `feat: add todo window topmost setting`.

The final commit ID is reported in the task handoff because amending this report changes the commit hash.

## Review Follow-up

Original Task 4 commit: `271b1836f24736d91675567d22ea237e74ad2264`.

### Fixes

- Removed the unused `TodoDatePicker` import from `src/components/todo/TodoDetail.test.ts`, resolving the `TS6133` blocker found by `vue-tsc`.
- Extended `migrate_v6_to_v7_creates_todo_blocks_for_existing_database` to assert that its current=6 to v8 migration chain inserts `default_todo_always_on_top` with value `1`.

### Verification Commands and Output

```text
cargo test schema::tests --manifest-path src-tauri/Cargo.toml

running 7 tests
test db::schema::tests::migrate_v5_to_v6_fixes_bg_color_default ... ok
test db::schema::tests::migrate_v7_to_v8_adds_todo_window_topmost_default ... ok
test db::schema::tests::migrate_v6_to_v7_creates_todo_blocks_for_existing_database ... ok
test db::schema::tests::run_migrations_upgrades_v4_to_v5 ... ok
test db::schema::tests::init_schema_creates_all_tables ... ok
test db::schema::tests::migrate_v2_to_v3_idempotent ... ok
test db::schema::tests::migrate_failure_rolls_back ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
```

```text
cargo test --manifest-path src-tauri/Cargo.toml
test result: ok. 105 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Doc-tests oii_sticker_lib: 0 passed; 0 failed
```

```text
cargo check --manifest-path src-tauri/Cargo.toml
Finished `dev` profile [unoptimized + debuginfo]
EXIT_CODE=0
```

`pnpm build` was attempted first. It could not finish because the configured registry repeatedly returned `EACCES` while pnpm tried to fetch packages. Existing local entry points were used instead:

```text
node node_modules/.pnpm/vue-tsc@2.2.12_typescript@5.6.3/node_modules/vue-tsc/bin/vue-tsc.js --noEmit
EXIT_CODE=0

node node_modules/.pnpm/vite@6.4.3_sass@1.102.0_yaml@2.9.0/node_modules/vite/bin/vite.js build
✓ 746 modules transformed.
✓ built in 5.82s
VITE_BUILD_EXIT=0
```

```text
git diff --check
EXIT_CODE=0
```
