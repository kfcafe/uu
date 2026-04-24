---
id: '7'
title: Add new ProjectKind match arms to all uu command files
slug: add-new-projectkind-match-arms-to-all-uu-command-f
status: open
priority: 2
created_at: '2026-03-22T19:12:24.261148Z'
updated_at: '2026-03-22T19:12:24.261148Z'
verify: cd /Users/asher/uu && cargo test 2>&1 | tail -5 | grep -q "0 failed"
fail_first: true
---

The `project-detect` crate now has 15 new `ProjectKind` variants:
Php, Dart { flutter: bool }, Sbt, Haskell { stack: bool }, Clojure { lein: bool },
Rebar, Dune, Perl, Julia, Nim, Crystal, Vlang, Gleam, Lua, Bazel.

Every exhaustive match on `ProjectKind` in the `uu` crate now fails to compile.
This parent unit covers adding match arms to all affected files.
