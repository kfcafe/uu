<div align="center">
  <h1>uu</h1>
  <p><strong>One command for common project tasks. Zero config.</strong></p>

  <p>
    <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-stable-orange?logo=rust&logoColor=white"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey">
  </p>

  <p>
    <a href="#install">Install</a> ·
    <a href="#commands">Commands</a> ·
    <a href="#supported-ecosystems">Ecosystems</a> ·
    <a href="#how-it-works">How It Works</a>
  </p>
</div>

---

You shouldn't have to remember if this repo wants `cargo test`, `go test ./...`, `dotnet test`, `gradle check`, or `dart analyze`.

`uu` detects the project and runs the right thing.

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

It works across 32 project types. No config files. No setup. `cd` into a project and go.

## Install

```sh
cargo install univ-utils
```

The published package is `univ-utils`. The installed command is still `uu`.

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
| `uu check` | Typecheck without running tests |
| `uu ci` | Run the CI pipeline: format check → lint → test |
| `uu clean` | Remove build artifacts and show how much space you get back |
| `uu dev` | Start dev servers — workspace-aware when possible |
| `uu doctor` | Check that required tools are installed |
| `uu fmt` | Auto-format code |
| `uu install` | Install the project |
| `uu lint` | Run the linter |
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

## Supported Ecosystems

`uu` detects projects by looking for build system files. When multiple are present, it picks the most specific one (Cargo.toml beats Makefile, Kotlin beats generic Gradle/Maven when `.kt` sources are present).

| Priority | File | Ecosystem | `build` | `check` | `ci` | `install` | `test` | `run` | `dev` | `fmt` | `lint` |
|:--------:|------|-----------|---------|---------|------|-----------|--------|-------|-------|-------|--------|
| 1 | `Cargo.toml` | Rust | `cargo build` | `cargo check` | fmt-check + clippy + test | `cargo install --path .` | `cargo test` | `cargo run` | `cargo run` | `cargo fmt` | `cargo clippy` |
| 2 | `go.mod` | Go | `go build ./...` | `go test -run=^$ ./...` | gofmt check + vet + test | `go install ./...` | `go test ./...` | `go run .` | `go run .` | `gofmt -w .` | `go vet ./...` |
| 3 | `mix.exs` | Elixir | `mix compile` | `mix compile --warnings-as-errors` | format-check + compile + test | `mix deps.get` + compile | `mix test` | `mix run` | `mix run` | `mix format` | `mix compile --warnings-as-errors` |
| 4 | `pyproject.toml` / `setup.py` / `setup.cfg` | Python | `python -m build` / `uv run python -m build` | — | ruff/black + pytest | `pip install .` / `uv tool install .` | `pytest` / `uv run pytest` | `main.py` / `app.py` / `manage.py` | same as `run` | `ruff format` / `black` | `ruff check` / `flake8` |
| 5 | `package.json` | Node.js | `<pm> run build` | `<pm> run typecheck` | `<pm> run lint` + `test` | `<pm> install` (+ `link` for `bin`) | `<pm> test` | `<pm> start` | workspace-aware `<pm> run dev` | `<pm> run format` | `<pm> run lint` |
| 6 | `build.gradle(.kts)` or `pom.xml` + Kotlin sources | Kotlin | `gradle build` / `mvn package` | `gradle build -x test` / `mvn -DskipTests package` | `gradle check` / `mvn test` | `gradle build` / `mvn install` | `gradle test` / `mvn test` | `gradle run` / `mvn compile exec:java` | same as `run` | `spotlessApply` / — | `gradle check` / — |
| 7 | `build.gradle` / `build.gradle.kts` | Gradle | `gradle build` | `gradle build -x test` | `gradle check` | `gradle build` | `gradle test` | `gradle run` | `gradle run` | `spotlessApply` | `gradle check` |
| 8 | `pom.xml` | Maven | `mvn package` | `mvn -DskipTests package` | `mvn test` | `mvn install` | `mvn test` | `mvn compile exec:java` | `mvn compile exec:java` | — | — |
| 9 | `build.sbt` | Scala | `sbt compile` | `sbt compile` | `sbt scalafmtCheckAll` + `test` | `sbt package` | `sbt test` | `sbt run` | `sbt ~run` | `sbt scalafmtAll` | — |
| 10 | `Gemfile` | Ruby | `bundle exec rake build` | — | `bundle exec rake test` | `bundle install` | `bundle exec rake test` | `bundle exec ruby app.rb` | `bundle exec ruby app.rb` | `rubocop -a` | `rubocop` |
| 11 | `Package.swift` | Swift | `swift build` | `swift build` | build + test | `swift build -c release` | `swift test` | `swift run` | `swift run` | — | — |
| 12 | `*.xcworkspace` / `*.xcodeproj` | Xcode | `xcodebuild build` | `xcodebuild build` | `xcodebuild build` + `test` | `xcodebuild -configuration Release build` | `xcodebuild test` | — | — | — | `xcodebuild analyze` |
| 13 | `build.zig` | Zig | `zig build` | `zig build` | `zig fmt --check .` + `zig build test` | `zig build -Doptimize=ReleaseSafe` | `zig build test` | `zig build run` | `zig build run` | `zig fmt .` | — |
| 14 | `*.csproj` / `*.sln` | .NET | `dotnet build` | `dotnet build --no-restore` | `dotnet format --verify-no-changes` + build + test | `dotnet publish -c Release` | `dotnet test` | `dotnet run` | `dotnet watch run` | `dotnet format` | `dotnet format --verify-no-changes` |
| 15 | `composer.json` | PHP | — | — | `vendor/bin/phpunit` | `composer install` | `vendor/bin/phpunit` | `php -S localhost:8000` | `php -S localhost:8000` | — | `vendor/bin/phpstan analyse` |
| 16 | `pubspec.yaml` | Dart / Flutter | `dart compile exe bin/main.dart` / `flutter build` | `dart analyze` | `dart format --set-exit-if-changed .` + analyze + test | `dart pub get` / `flutter pub get` | `dart test` / `flutter test` | `dart run` / `flutter run` | same as `run` | `dart format .` | `dart analyze` |
| 17 | `stack.yaml` / `*.cabal` | Haskell | `stack build` / `cabal build` | `stack build --fast` / `cabal build` | build + test | `stack install` / `cabal install` | `stack test` / `cabal test` | `stack run` / `cabal run` | same as `run` | — | `hlint .` |
| 18 | `project.clj` / `deps.edn` | Clojure | `lein compile` / `clj -T:build` | — | `lein test` / `clj -M:test` | `lein install` / `clj -T:build install` | `lein test` / `clj -M:test` | `lein run` / `clj -M -m main` | same as `run` | — | `lein eastwood` / — |
| 19 | `rebar.config` | Erlang | `rebar3 compile` | `rebar3 compile` | compile + eunit | `rebar3 get-deps` + compile | `rebar3 eunit` | `rebar3 shell` | `rebar3 shell` | — | `rebar3 dialyzer` |
| 20 | `dune-project` | OCaml | `dune build` | `dune build` | build + test | `dune build` + `dune install` | `dune test` | `dune exec .` | `dune exec .` | `dune fmt` | — |
| 21 | `cpanfile` / `Makefile.PL` | Perl | `perl Makefile.PL` + `make` | — | `prove -l` | `cpanm --installdeps .` | `prove -l` | `perl app.pl` | `perl app.pl` | — | `perlcritic .` |
| 22 | `Project.toml` | Julia | — | — | `Pkg.test()` | `Pkg.instantiate()` | `Pkg.test()` | `julia --project src/main.jl` | `julia --project src/main.jl` | — | — |
| 23 | `DESCRIPTION` / `renv.lock` | R | `R CMD build .` | — | `R CMD check --no-manual .` | `R CMD INSTALL .` | `R CMD check --no-manual .` | — | — | — | — |
| 24 | `*.nimble` | Nim | `nimble build` | `nimble check` | `nimble test` | `nimble install` | `nimble test` | `nimble run` | `nimble run` | — | `nimble check` |
| 25 | `shard.yml` | Crystal | `shards build` | `crystal build --no-codegen` | `crystal spec` | `shards install` | `crystal spec` | `crystal run src/main.cr` | `crystal run src/main.cr` | `crystal tool format .` | — |
| 26 | `v.mod` | V | `v .` | `v .` | `v test .` | `v install .` | `v test .` | `v run .` | `v run .` | `v fmt .` | — |
| 27 | `gleam.toml` | Gleam | `gleam build` | `gleam check` | `gleam test` | `gleam deps download` | `gleam test` | `gleam run` | `gleam run` | `gleam format` | — |
| 28 | `*.rockspec` | Lua | — | — | `luarocks test` | `luarocks install --deps-only .` | `luarocks test` | `lua init.lua` | `lua init.lua` | — | — |
| 29 | `MODULE.bazel` / `WORKSPACE` | Bazel | `bazel build //...` | `bazel build //...` | `bazel test //...` | `bazel build //...` | `bazel test //...` | `bazel run //:main` | `bazel run //:main` | `buildifier .` | `bazel test //...` |
| 30 | `meson.build` | Meson | `meson setup builddir` + compile | `meson compile -C builddir` | `meson test -C builddir` | setup + compile + install | `meson test -C builddir` | — | — | — | — |
| 31 | `CMakeLists.txt` | CMake | `cmake -B build` + `--build` | `cmake -B build` + `--build` | `ctest --test-dir build` | `cmake --install build` | `ctest --test-dir build` | — | — | — | — |
| 32 | `Makefile` | Make | `make` | `make` | `make test` | `make && make install` | `make test` | `make run` | `make run` | — | — |

> [!NOTE]
> Python auto-detects `uv` and uses it when available. Node.js detects npm/yarn/pnpm/bun from lockfiles. Kotlin is detected ahead of generic Gradle/Maven when `uu` sees Kotlin source files. `run`/`dev` stay intentionally unsupported for Xcode and R because they need project-specific entrypoint or scheme selection.

## How It Works

`uu` is intentionally simple:

- **[`project-detect`](https://github.com/kfcafe/project-detect)** identifies what kind of project you're in
- **`univ-utils`** maps that project kind to the right shell commands for common tasks
- the **`uu`** binary executes those commands, with consistent dry-run and directory handling

The goal is not to become a new build system. The goal is to remove the friction of remembering how every ecosystem wants to be driven.

## Contributing

1. Add a variant to `ProjectKind` in `project-detect/src/lib.rs`
2. Add detection logic in `project-detect/src/lib.rs` (`detect()` / `detect_in()`)
3. Add the build/install/run/test/fmt/lint/ci/clean steps in each `crates/uu/src/cmd/*.rs` module
4. Verify: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
