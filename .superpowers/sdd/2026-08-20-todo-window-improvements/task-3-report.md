# Task 3 Report: Multiple Direct Child Tasks

## Changed Files

- `src/components/todo/TodoList.test.ts` (created): covers two direct children, both titles, exactly one root-level add-child entry, and its `createChild` event.
- `src/components/todo/TodoList.vue`: groups children by `parent_id` in a computed `Map`, renders all direct children with stable child ids, and always renders the root-level add-child entry.
- `src-tauri/src/db/todo_block_repo.rs`: permits multiple direct children while retaining parent existence, sticker ownership, and one-level nesting validation; updates the repository test.
- `.superpowers/sdd/2026-08-20-todo-window-improvements/task-3-report.md` (created): this report.

## RED

### Frontend

The required command was attempted first:

```text
pnpm vitest run src/components/todo/TodoList.test.ts
```

It could not start Vitest because pnpm attempted a registry install and repeatedly received `EACCES` from `https://registry.npmmirror.com/`, including the `vitest` tarball. The worktree `node_modules` is a junction to the project root and lacks `.bin` shims.

After locating the already-installed local package entry point, the equivalent direct-binary RED command was run against the original `find` implementation:

```text
node node_modules/.pnpm/vitest@4.1.10_jsdom@30.0.1__4b4be0061adcf0dde5f80afbc5255c95/node_modules/vitest/vitest.mjs run src/components/todo/TodoList.test.ts
exit 1
AssertionError: expected [ DOMWrapper{ ... } ] to have a length of 2 but got 1
- Expected: 2
+ Received: 1
```

### Rust

```text
cargo test todo_block_repo::tests::creates_parent_and_multiple_direct_children --manifest-path src-tauri/Cargo.toml
exit 1
called `Result::unwrap()` on an `Err` value: 一个父任务最多只能有一个子任务
test result: FAILED. 0 passed; 1 failed
```

## GREEN

### Frontend

The same local direct Vitest entry point passed after implementation:

```text
Test Files  1 passed (1)
Tests  1 passed (1)
exit 0
```

### Rust

```text
cargo test todo_block_repo::tests --manifest-path src-tauri/Cargo.toml
exit 0
running 2 tests
test db::todo_block_repo::tests::creates_parent_and_multiple_direct_children ... ok
test db::todo_block_repo::tests::child_cannot_write_advanced_fields_and_delete_cascades ... ok
test result: ok. 2 passed; 0 failed
```

`git diff --check` also exited 0.

## Self-Review

- `childrenByParent` is a pure computed derivation from the props; roots and children use stable task IDs as keys.
- Each root always has one `.add-child` row and uses `@click.stop`; child rows never render another add-child row.
- The Rust create path still rejects nonexistent parents, cross-sticker parents, and parents that are themselves children.
- The test creates two siblings with the same parent and verifies a grandchild attempt remains rejected.
- Scope check found only the four Task 3 files above.

## Concerns

- The requested pnpm Vitest command remains blocked before test execution by registry `EACCES`; direct invocation of the already-present Vitest package was used for RED and GREEN instead.
- Rust tests emit the existing Windows linker informational warning; no test failures or new compiler warnings were reported.

## Commit

- `feat: allow multiple todo child tasks` (Task 3 commit)
