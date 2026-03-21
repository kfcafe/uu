<div align="center">
  <h1>uu</h1>
  <p><strong>One command for every project. Zero config.</strong></p>

  <p>
    <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-stable-orange?logo=rust&logoColor=white"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey">
  </p>

  <p>
    <a href="#install">Install</a> ·
    <a href="#commands">Commands</a> ·
    <a href="#map">Map</a> ·
    <a href="#supported-ecosystems">Ecosystems</a> ·
    <a href="#how-it-works">How It Works</a>
  </p>
</div>

---

You shouldn't have to remember if it's `cargo install --path .` or `go install ./...` or `pip install .`. You shouldn't have to google `lsof` flags every time port 3000 is stuck.

`uu` detects your project and runs the right thing.

```
$ uu install                  $ uu clean
    detected Rust (Cargo.toml)      detected Node.js (package.json)
     running cargo install …        removing node_modules/ (287.3 MB)
        done ✓                         freed 287.3 MB

$ uu doctor                   $ uu test
    detected Rust (Cargo.toml)      detected Go (go.mod)
  ✓ cargo                          running go test ./...
  ✓ rustfmt                           done ✓
  ✓ cargo-clippy
```

It works across 13 ecosystems. No config files. No setup. `cd` into a project and go.

## Install

```sh
cargo install --git https://github.com/kfcafe/uu
```

<details>
<summary>Build from source</summary>

```sh
git clone https://github.com/kfcafe/uu.git
cd uu
cargo install --path crates/uu
```

</details>

## Commands

Every command auto-detects your project type. Pass extra arguments after `--`. Use `-n` / `--dry-run` to preview without executing.

| Command | What it does |
|---------|-------------|
| `uu build` | Compile the project |
| `uu check` | Typecheck without running tests — faster feedback loop |
| `uu ci` | Full CI pipeline: format check → lint → test (stops on first failure) |
| `uu clean` | Remove build artifacts, show how much space you get back |
| `uu dev` | Start dev servers — workspace-aware, runs packages concurrently |
| `uu doctor` | Check that required tools are installed |
| `uu fmt` | Auto-format code |
| `uu install` | Install the project (and link binaries for Node.js `bin` packages) |
| `uu lint` | Run the linter |
| `uu map` | Generate and explore a project manifest — [see below](#map) |
| `uu ports` | See what's listening on every port — kill with `uu ports 3000 -k` |
| `uu run` | Run the project |
| `uu test` | Run the test suite |

### Workspace-aware dev servers

`uu dev` is the most opinionated command. In a Node.js monorepo, it detects workspace packages and runs their `dev` scripts concurrently with colored, prefixed output:

```
$ uu dev
    detected Node.js workspace (pnpm, 6 packages)
     running agent · tsc --watch
     running api · doppler run -- tsx watch src/index.ts
     running web · vite dev
       [api] Listening on :3001
       [web] VITE v6.4.1 ready in 200ms
```

Run specific packages with `uu dev api web`. Add `-o` to open the first localhost URL in your browser.

## Map

`uu map` is a codebase intelligence tool. It uses tree-sitter to parse your source code and extract a structured manifest of every type, function, module, route, model, and integration — across 11 languages and 16 frameworks.

### Generate a manifest

```sh
uu map                      # writes .map.yaml
uu map --format json        # writes .map.json
uu map --format md          # writes .map.md (readable markdown)
uu map --stdout             # print to stdout instead of file
uu map --diff               # show what changed since last generation
uu map -n                   # dry run — show counts without writing
```

### Query a symbol

Look up any type, function, module, or route by name. Shows fields, methods, source location, trait implementations, and cross-references.

```
$ uu map query Manifest
type Manifest (struct)
  source: crates/manifest/src/schema.rs:18
  fields:
    project: ProjectMeta
    types: BTreeMap<String, TypeDef>
    functions: BTreeMap<String, Function>
    modules: BTreeMap<String, Module>
    routes: BTreeMap<String, Route>
    ...

$ uu map query Adapter --refs
type Adapter (trait)
  source: crates/manifest/src/adapters/mod.rs:25
  methods: name, detect, extract, priority, layer

  referenced by:
    fn all_adapters
    type RustAdapter
    type GoAdapter
    type PythonAdapter
    ...
```

Typo? It suggests corrections:

```
$ uu map query Manifes
No symbol found matching 'Manifes'
Did you mean:
  Manifest
  ManifestDiff
  ManifestFragment
```

### Search across symbols

Find everything related to a concept across the entire project:

```
$ uu map search auth
Found 8 matches for 'auth':

  Types:
    AuthConfig          (struct, 2 fields)    src/schema.rs:271
    AuthJsAdapter       (struct, 5 methods)   src/adapters/framework/authjs.rs:9
  Functions:
    authenticate        (pub fn ...)          src/auth.rs:15
  Routes:
    [POST] /api/auth/login                    src/routes/auth.rs
```

Filter by category with `-c`:

```sh
uu map search detect -c fn      # only functions
uu map search user -c types     # only types
```

### Codebase statistics

```
$ uu map stats
Project: uu (Rust)

Summary
  Types:      58
  Functions:  40
  Modules:    57

Visibility
  Types:     52 public, 6 internal, 0 private
  Functions: 19 public, 21 internal, 0 private

Type breakdown
  Structs          50
  Enums             7
  Traits            1

Top modules by symbol count
  manifest::schema        18 types, 2 functions
  detect::lib              3 types, 9 functions
  uu::runner               1 type, 5 functions

Traits
  Adapter → RustAdapter, GoAdapter, PythonAdapter, ...
```

### Module tree

```
$ uu map tree
uu (Rust)
detect
└── lib (3 types, 9 fns)
manifest
├── adapters (2 types, 1 fn)
│   ├── framework
│   │   ├── aspnet (1 type)
│   │   ├── axum (1 type)
│   │   └── ... (13 more)
│   └── lang
│       ├── rust (1 type)
│       ├── go (1 type)
│       └── ... (9 more)
├── context (1 type, 1 fn)
├── diff (2 types, 3 fns)
└── schema (18 types, 2 fns)
uu
├── cmd
│   ├── map (2 types)
│   │   ├── format (15 fns)
│   │   ├── generate (1 type)
│   │   ├── query (1 type)
│   │   └── ...
│   └── ... (10 more)
└── runner (1 type, 5 fns)

57 modules, 58 types, 40 functions
```

### Supported languages & frameworks

`uu map` uses tree-sitter for accurate AST-level extraction — no regex, no guessing.

**Languages:** Rust, Go, Python, TypeScript, JavaScript, Elixir, Java, Ruby, Swift, C#, C/C++

**Frameworks:** Next.js, Express, Prisma, shadcn/ui, Auth.js, Axum, Django, FastAPI, Phoenix, Ecto, Rails, Spring, Gin, GORM, ASP.NET

## Supported Ecosystems

`uu` detects projects by looking for build system files. When multiple are present, it picks the most specific one (Cargo.toml beats Makefile).

| Priority | File | Ecosystem | `build` | `check` | `ci` | `install` | `test` | `run` | `dev` | `fmt` | `lint` |
|:--------:|------|-----------|---------|---------|------|-----------|--------|-------|-------|-------|--------|
| 1 | `Cargo.toml` | Rust | `cargo build` | `cargo check` | fmt‑check + clippy + test | `cargo install --path .` | `cargo test` | `cargo run` | `cargo run` | `cargo fmt` | `cargo clippy` |
| 2 | `go.mod` | Go | `go build ./...` | `go test -run=^$ ./...` | gofmt check + vet + test | `go install ./...` | `go test ./...` | `go run .` | `go run .` | `gofmt -w .` | `go vet ./...` |
| 3 | `mix.exs` | Elixir | `mix compile` | `mix compile --warnings-as-errors` | format‑check + compile + test | `mix deps.get` + `mix compile` | `mix test` | `mix run` | `mix run` | `mix format` | `mix compile --warnings-as-errors` |
| 4 | `pyproject.toml` | Python | `python -m build` | — | ruff fmt‑check + check + pytest | `pip install .` | `pytest` | `python main.py` | `python main.py` | `ruff format .`¹ | `ruff check .`¹ |
| 5 | `package.json` | Node.js | `npm run build`² | `npm run typecheck`² | lint + test² | `npm install`²³ | `npm test`² | `npm start`² | `npm run dev`²⁴ | `npm run format`² | `npm run lint`² |
| 6 | `build.gradle` | Gradle | `./gradlew build`⁵ | `./gradlew build -x test`⁵ | `./gradlew check`⁵ | `./gradlew build`⁵ | `./gradlew test`⁵ | `./gradlew run`⁵ | `./gradlew run`⁵ | `./gradlew spotlessApply`⁵ | `./gradlew check`⁵ |
| 7 | `pom.xml` | Maven | `mvn package` | `mvn -DskipTests package` | `mvn test` | `mvn install` | `mvn test` | — | — | — | — |
| 8 | `Gemfile` | Ruby | `bundle exec rake build` | — | `bundle exec rake test` | `bundle install` | `bundle exec rake test` | `rubocop -a` | `rubocop -a` | `rubocop` | `rubocop` |
| 9 | `Package.swift` | Swift | `swift build` | `swift build` | build + test | `swift build -c release` | `swift test` | `swift run` | `swift run` | — | — |
| 10 | `*.csproj` | .NET | `dotnet build` | `dotnet build` | fmt‑check + build + test | `dotnet publish` | `dotnet test` | `dotnet run` | `dotnet watch run` | `dotnet format` | `dotnet format`⁶ |
| 11 | `meson.build` | Meson | `meson setup` + `compile` | `meson compile` | `meson test` | `meson setup` + `install` | `meson test` | — | — | — | — |
| 12 | `CMakeLists.txt` | CMake | `cmake -B` + `--build` | `cmake -B` + `--build` | `ctest` | `cmake` build + install | `ctest` | — | — | — | — |
| 13 | `Makefile` | Make | `make` | `make` | `make test` | `make && make install` | `make test` | `make run` | `make run` | — | — |

¹ Falls back to `black`/`flake8` if ruff is not installed.
² Detects your package manager from lockfile: npm, yarn, pnpm, or bun.
³ If package.json has a `bin` field, also runs `<pm> link` to make the CLI available on PATH.
⁴ Workspace-aware: in monorepos, runs all packages' `dev` scripts concurrently.
⁵ Uses `./gradlew` wrapper if present, falls back to `gradle`.
⁶ Uses `dotnet format --verify-no-changes` for lint (style check mode).

> [!NOTE]
> Python auto-detects `uv` and uses it when available. Ecosystems without a standard typecheck (Python, Ruby) bail with a suggestion instead of failing silently. Node.js `uu build` skips gracefully if no `build` script exists.

## How It Works

`uu` is three crates:

- **`uu-detect`** — scans the current directory for build system files (`Cargo.toml`, `go.mod`, `package.json`, etc.) and returns a `ProjectKind` with ecosystem-specific metadata. When multiple files exist, language-specific ones win over generic build systems.
- **`uu-manifest`** — the map engine. Uses tree-sitter to parse source files via language and framework adapters. Produces a structured manifest of types, functions, modules, routes, models, and integrations. Supports diffing between manifests.
- **`uu`** — the CLI binary. Each command maps the detected `ProjectKind` to the right shell command and runs it. The `map` command adds subcommands for querying and exploring the manifest interactively.

The detection library is the engine. Add a new ecosystem once and every command learns it automatically.

```
uu/
├── crates/
│   ├── detect/       Project detection library
│   ├── manifest/     Tree-sitter manifest generator (11 languages, 16 frameworks)
│   └── uu/           CLI binary
└── README.md
```

## Contributing

1. Add a variant to `ProjectKind` in `crates/detect/src/lib.rs`
2. Add detection logic in `detect()`
3. Add the build/install/run/test/fmt/lint/ci/clean steps in each command module
4. To add a new language adapter: create `crates/manifest/src/adapters/lang/<name>.rs` implementing the `Adapter` trait
5. To add a new framework adapter: create `crates/manifest/src/adapters/framework/<name>.rs`
6. Verify: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
