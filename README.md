<div align="center">
  <h1>uu</h1>
  <p><strong>Universal utilities — zero-config developer tools that detect your project and do the right thing.</strong></p>

  <p>
    <a href="#install">Install</a> ·
    <a href="#commands">Commands</a> ·
    <a href="#supported-ecosystems">Ecosystems</a> ·
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

$ uu test                     $ uu ports
    detected Go (go.mod)          PORT      PID  COMMAND          USER
     running go test ./...        3000    12345  node             asher
        done ✓                    5432     1234  postgres         asher
```

No config files. No setup. `cd` into a project and go.

## Install

```bash
cargo install --git https://github.com/opus-workshop/uu
```

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/opus-workshop/uu.git
cd uu
cargo install --path crates/uu
```

</details>

## Commands

### `uu install`

Detect the project type and run the standard install command.

```
$ cd my-rust-project
$ uu install
    detected Rust (Cargo.toml)
     running cargo install --path .
        done ✓
```

Pass extra arguments after `--`:

```
$ uu install -- --release
    detected Rust (Cargo.toml)
     running cargo install --path . --release
```

### `uu run`

Detect the project type and run it.

```
$ uu run
    detected Go (go.mod)
     running go run .
```

For Python, it finds the right entry point automatically: `manage.py` (Django), `app.py` (Flask), or `main.py`.

### `uu test`

Detect the project type and run the test suite.

```
$ uu test
    detected Node.js (package.json)
     running npm test
```

### `uu clean`

Remove build artifacts. Shows what's being deleted and how much space you get back.

```
$ uu clean
    detected Rust (Cargo.toml)
     running cargo clean
    removing target/ (1.2 GB)
       freed 1.2 GB
```

### `uu ports`

See what's listening. Kill it by port number.

```
$ uu ports
    PORT      PID  COMMAND          USER
    3000    12345  node             asher
    5432     1234  postgres         asher
    8080     5678  java             asher

  3 listeners

$ uu ports 3000
    PORT      PID  COMMAND          USER
    3000    12345  node             asher

$ uu ports 3000 -k
    killing node (pid 12345, :3000)
```

### Dry run

Every project command supports `-n` / `--dry-run` to show what would happen without doing it:

```
$ uu install -n
    detected Rust (Cargo.toml)
  would run cargo install --path .

$ uu clean -n
    detected Node.js (package.json)
   would rm node_modules/ (287.3 MB)
  would free 287.3 MB
```

## Supported ecosystems

`uu` detects projects by looking for build system files. When multiple are present, it picks the most specific one.

| Priority | File | Ecosystem | `install` | `test` | `run` |
|:--------:|------|-----------|-----------|--------|-------|
| 1 | `Cargo.toml` | Rust | `cargo install --path .` | `cargo test` | `cargo run` |
| 2 | `go.mod` | Go | `go install ./...` | `go test ./...` | `go run .` |
| 3 | `mix.exs` | Elixir | `mix deps.get` + `mix compile` | `mix test` | `mix run` |
| 4 | `pyproject.toml` | Python | `pip install .` | `pytest` | `python main.py` |
| 5 | `package.json` | Node.js | `npm install`¹ | `npm test`¹ | `npm start`¹ |
| 6 | `build.gradle` | Gradle | `./gradlew build`² | `./gradlew test`² | `./gradlew run`² |
| 7 | `pom.xml` | Maven | `mvn install` | `mvn test` | `mvn compile exec:java` |
| 8 | `Gemfile` | Ruby | `bundle install` | `bundle exec rake test` | — |
| 9 | `meson.build` | Meson | `meson setup` + `install` | `meson test` | — |
| 10 | `CMakeLists.txt` | CMake | `cmake` build + install | `ctest` | — |
| 11 | `Makefile` | Make | `make && make install` | `make test` | `make run` |

¹ Detects your package manager from lockfile: npm, yarn, pnpm, or bun.
² Uses `./gradlew` wrapper if present, falls back to `gradle`.

Python auto-detects `uv` on your PATH and uses `uv pip install .` / `uv run pytest` when available.

## Project structure

```
uu/
├── crates/
│   ├── detect/     Shared project detection library
│   └── uu/         CLI binary — 5 commands
└── README.md
```

The `detect` crate is the engine. Add a new ecosystem once — every command learns it.

## Contributing

Contributions welcome! To add support for a new ecosystem:

1. Add a variant to `ProjectKind` in `crates/detect/src/lib.rs`
2. Add detection logic in `detect()`
3. Add the install/run/test/clean steps in each command module
4. Run the verify gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`

## License

[MIT](LICENSE)
