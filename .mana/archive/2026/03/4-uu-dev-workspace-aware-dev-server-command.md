---
id: '4'
title: uu dev — workspace-aware dev server command
slug: uu-dev-workspace-aware-dev-server-command
status: closed
priority: 2
created_at: '2026-03-03T03:36:28.682566Z'
updated_at: '2026-03-03T03:46:11.866556Z'
closed_at: '2026-03-03T03:46:11.866556Z'
close_reason: 'Auto-closed: all children completed'
verify: cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo run -- dev --help
fail_first: true
is_archived: true
tokens: 110
tokens_updated: '2026-03-03T03:36:28.683760Z'
---

Add `uu dev` command that detects Node.js workspaces and runs dev scripts concurrently with prefixed output.

**Behavior:**
- `uu dev` in a workspace → run ALL packages dev scripts concurrently
- `uu dev api web` → run only those packages dev scripts
- `uu dev` in a non-workspace → run `{manager} run dev`
- Ctrl+C kills all children cleanly

Parent bean — children implement the pieces.
