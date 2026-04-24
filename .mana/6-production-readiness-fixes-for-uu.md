---
id: '6'
title: Production-readiness fixes for uu
slug: production-readiness-fixes-for-uu
status: open
priority: 1
created_at: '2026-03-20T15:49:31.701309Z'
updated_at: '2026-03-20T15:49:31.701309Z'
labels:
- review
- production
- quality
verify: cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test
---

## Goal
Address the highest-value code quality and correctness gaps found in the review so `uu` is closer to production-grade.

## Scope
Track the concrete fixes needed for runtime correctness, CLI surface completeness, and release hardening.

## Review findings to address
- `uu dev` can exit 0 even when a workspace dev process fails.
- Project detection only checks the current directory; nested directories inside a repo fail.
- `uu map --adapters` is exposed in help but currently ignored.
- `uu map --format` accepts invalid values silently and falls back to YAML.
- Generated manifests never populate `project.frameworks` even when framework adapters are detected.
- Node workspace detection misses common `package.json.workspaces.packages` object form.
- Repo has no CI workflow enforcing fmt/clippy/tests.
- Runtime behavior coverage is weaker than the raw test count suggests.

## Acceptance criteria
- Child units exist for each concrete fix area.
- Each unit has a verify gate tied to the behavior it is meant to protect.
- The set is small enough for agents to execute independently.
