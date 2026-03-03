<div align="center">
  <h1>uu</h1>
  <p><strong>Universal utilities — zero-config developer tools that detect your project and do the right thing.</strong></p>

  <p>
    <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-stable-orange?logo=rust&logoColor=white"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey">
  </p>

  <p>
    <a href="#install">Install</a> ·
    <a href="#commands">Commands</a> ·
    <a href="#supported-ecosystems">Ecosystems</a> ·
    <a href="#contributing">Contributing</a> ·
    <a href="LICENSE">License</a>
  </p>
</div>

---

You shouldn't have to remember if it's `cargo install --path .` or `go install ./...` or `pip install .` or `npm install`. You shouldn't have to look up `lsof -iTCP -sTCP:LISTEN -nP` every time you need to find what's hogging port 3000.

Just type `uu`.

```
$ uu install                  $ uu clean
    detected Rust (Cargo.toml)      detected Node.js (package.json)
     running cargo install …        removing node_modules/ (287.3 MB)
        done ✓                         freed 287.3 MB

$ uu doctor                   $ uu test
    detected Rust (Cargo.toml)      detected Node.js (package.json)
  ✓ cargo                          running npm test
  ✓ rustfmt                           done ✓
  ✓ cargo-clippy

$ uu test                     $ uu ports
    detected Go (go.mod)          PORT      PID  COMMAND          USER
     running go test ./...        3000    12345  node             asher
        done ✓                    5432     1234  postgres         asher
```

No config files. No setup. `cd` into a project and go.

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

### `uu build`

Detect the project type and compile.

```
$ uu build
    detected Rust (Cargo.toml)
     running cargo build
        done ✓
```

Pass extra arguments after `--`:

```
$ uu build -- --release
    detected Rust (Cargo.toml)
     running cargo build --release
```

For Node.js projects, runs your `build` script. If there's no `build` script in package.json, skips gracefully instead of failing:

```
$ uu build
    detected Node.js (package.json)
     skipped no "build" script in package.json — nothing to build
```

### `uu check`

Typecheck or compile without running tests. Faster feedback than `uu test`.

```
$ uu check
    detected Rust (Cargo.toml)
     running cargo check
        done ✓
```

For Go, compiles test files without executing (`go test -run=^$ ./...`). For Node.js, runs your `typecheck` script. Ecosystems without a meaningful typecheck (Python, Ruby) bail with a suggestion.

### `uu ci`

Full CI pipeline in one command: format check → lint → test. Stops on first failure.

```
$ uu ci
    detected Rust (Cargo.toml)
     running cargo fmt --check
     running cargo clippy -- -D warnings
     running cargo test
        done ✓
```

Pass extra arguments after `--` (appended to the last step):

```
$ uu ci -- --no-fail-fast
```

### `uu install`

Detect and install.

```
$ uu install
    detected Rust (Cargo.toml)
     running cargo install --path .
        done ✓
```

For Node.js projects with a `bin` field in package.json, also links the binary onto your PATH:

```
$ uu install
    detected Node.js (package.json)
     running bun install
     running bun link
        done ✓
```

### `uu run`

Detect and run.

```
$ uu run
    detected Go (go.mod)
     running go run .
```

For Python, auto-detects the entry point: `manage.py` (Django), `app.py` (Flask), or `main.py`.

### `uu dev`

Start dev servers. In a monorepo, detects workspace packages and runs their `dev` scripts concurrently with colored, prefixed output.

```
$ uu dev
    detected Node.js workspace (pnpm, 6 packages)
     running agent · tsc --watch
     running api · doppler run -- tsx watch src/index.ts
     running desktop · cargo tauri dev
     running extension · vite build --watch
     running shared · tsc --watch
     running web · vite dev
       [api] Listening on :3001
       [web] VITE v6.4.1 ready in 200ms
```

Run specific packages only:

```
$ uu dev api web
    detected Node.js workspace (pnpm, 2 packages)
     running api · doppler run -- tsx watch src/index.ts
     running web · vite dev
```

Open the first detected localhost URL in your browser with `-o`:

```
$ uu dev api web -o
    detected Node.js workspace (pnpm, 2 packages)
     running api · doppler run -- tsx watch src/index.ts
     running web · vite dev
     opening http://localhost:5173/
```

In a non-workspace project, runs the `dev` script directly (`pnpm run dev`, `npm run dev`, etc.).

### `uu test`

Detect and test.

```
$ uu test
    detected Node.js (package.json)
     running npm test
```

### `uu lint`

Detect and lint.

```
$ uu lint
    detected Rust (Cargo.toml)
     running cargo clippy -- -D warnings
        done ✓
```

For Python, uses `ruff check .` if available, falls back to `flake8`. Some ecosystems (Maven, Meson, CMake, Make) have no standard linter — `uu lint` tells you what to try.

### `uu fmt`

Detect and format. **May modify files.**

```
$ uu fmt
    detected Rust (Cargo.toml)
     running cargo fmt
        done ✓
```

For Python, uses `ruff format .` if available, falls back to `black .`.

### `uu clean`

Remove build artifacts. Shows what's deleted and how much space you get back.

```
$ uu clean
    detected Rust (Cargo.toml)
     running cargo clean
    removing target/ (1.2 GB)
       freed 1.2 GB
```

### `uu doctor`

Check detection and tool availability. Useful when commands fail because a tool is missing.

```
$ uu doctor
    detected Rust (Cargo.toml)

  ✓ cargo
  ✓ rustfmt
  ✓ cargo-clippy
```

Missing tools show as `✗`:

```
$ uu doctor
    detected Python (pyproject.toml)

  ✓ pip
  ✓ python
  ✗ pytest
  ✓ ruff
```

If no project is detected, prints the list of supported project files.

### `uu ports`

See what's listening. Kill by port number.

```
$ uu ports
    PORT      PID  COMMAND          USER
    3000    12345  node             asher
    5432     1234  postgres         asher
    8080     5678  java             asher

  3 listeners

$ uu ports 3000 -k
    killing node (pid 12345, :3000)
```

### Dry run

Every project command supports `-n` / `--dry-run`:

```
$ uu install -n
    detected Rust (Cargo.toml)
  would run cargo install --path .
```

## Supported Ecosystems

`uu` detects projects by looking for build system files. When multiple are present, it picks the most specific one.

| Priority | File | Ecosystem | `build` | `check` | `ci` | `install` | `test` | `run` | `dev` | `fmt` | `lint` |
|:--------:|------|-----------|---------|---------|------|-----------|--------|-------|-------|-------|--------|
| 1 | `Cargo.toml` | Rust | `cargo build` | `cargo check` | fmt‑check + clippy + test | `cargo install --path .` | `cargo test` | `cargo run` | `cargo run`⁵ | `cargo fmt` | `cargo clippy` |
| 2 | `go.mod` | Go | `go build ./...` | `go test -run=^$ ./...` | gofmt check + vet + test | `go install ./...` | `go test ./...` | `go run .` | `go run .`⁵ | `gofmt -w .` | `go vet ./...` |
| 3 | `mix.exs` | Elixir | `mix compile` | `mix compile --warnings-as-errors` | format‑check + compile + test | `mix deps.get` + `mix compile` | `mix test` | `mix run` | `mix run`⁵ | `mix format` | `mix compile --warnings-as-errors` |
| 4 | `pyproject.toml` | Python | `python -m build` | —¹ | ruff fmt‑check + check + pytest | `pip install .` | `pytest` | `python main.py` | `python main.py`⁵ | `ruff format .`² | `ruff check .`² |
| 5 | `package.json` | Node.js | `npm run build`³ | `npm run typecheck`³ | lint + test³ | `npm install`³⁷ | `npm test`³ | `npm start`³ | `npm run dev`³⁶ | `npm run format`³ | `npm run lint`³ |
| 6 | `build.gradle` | Gradle | `./gradlew build`⁴ | `./gradlew build -x test`⁴ | `./gradlew check`⁴ | `./gradlew build`⁴ | `./gradlew test`⁴ | `./gradlew run`⁴ | `./gradlew run`⁴⁵ | `./gradlew spotlessApply`⁴ | `./gradlew check`⁴ |
| 7 | `pom.xml` | Maven | `mvn package` | `mvn -DskipTests package` | `mvn test` | `mvn install` | `mvn test` | — | — | — | — |
| 8 | `Gemfile` | Ruby | `bundle exec rake build` | —¹ | `bundle exec rake test` | `bundle install` | `bundle exec rake test` | `rubocop -a` | `rubocop -a`⁵ | `rubocop` | `rubocop` |
| 9 | `meson.build` | Meson | `meson setup` + `compile` | `meson compile` | `meson test` | `meson setup` + `install` | `meson test` | — | — | — | — |
| 10 | `CMakeLists.txt` | CMake | `cmake -B` + `--build` | `cmake -B` + `--build` | `ctest` | `cmake` build + install | `ctest` | — | — | — | — |
| 11 | `Makefile` | Make | `make` | `make` | `make test` | `make && make install` | `make test` | `make run` | `make run`⁵ | — | — |

¹ No built-in typecheck — `uu check` bails with suggestions (mypy/pyright for Python, Sorbet for Ruby).
² Falls back to `black`/`flake8` if ruff is not installed. Bails with install instructions if neither is found.
³ Detects your package manager from lockfile: npm, yarn, pnpm, or bun.
⁴ Uses `./gradlew` wrapper if present, falls back to `gradle`.
⁵ Falls back to same command as `run` (no dev/prod distinction in this ecosystem).
⁶ Workspace-aware: in monorepos, `uu dev` runs all packages' `dev` scripts concurrently. Use `uu dev pkg1 pkg2` to select specific packages. Add `-o` to open the first localhost URL in your browser.
⁷ If package.json has a `bin` field, also runs `<pm> link` to make the CLI available on PATH.

Python auto-detects `uv` on your PATH and uses `uv pip install .` / `uv run pytest` when available.

## Project Structure

```
uu/
├── crates/
│   ├── detect/     Shared project detection library
│   └── uu/         CLI binary
└── README.md
```

The `detect` crate is the engine. Add a new ecosystem once — every command learns it.

## Contributing

To add support for a new ecosystem:

1. Add a variant to `ProjectKind` in `crates/detect/src/lib.rs`
2. Add detection logic in `detect()`
3. Add the build/install/run/test/fmt/lint/ci/clean steps in each command module
4. Run the verify gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`

## Stack

Rust · clap

## License

[MIT](LICENSE)
