# univ-utils

`univ-utils` installs the `uu` CLI.

`uu` is a zero-config command-line tool for common project tasks across many ecosystems. It detects the project you're in and runs the right build, test, run, fmt, lint, clean, install, and doctor commands.

## Install

```sh
cargo install univ-utils
```

Then run:

```sh
uu --help
```

## What it does

You shouldn't have to remember whether a repo wants:

- `cargo test`
- `go test ./...`
- `dotnet test`
- `gradle check`
- `dart analyze`

`uu` detects the project and runs the right thing.

## Common commands

- `uu build`
- `uu check`
- `uu ci`
- `uu clean`
- `uu dev`
- `uu doctor`
- `uu fmt`
- `uu install`
- `uu install --default`
- `uu lint`
- `uu ports`
- `uu run`
- `uu test`

## Default installs

`uu install --default` makes known freshly installed commands win in your shell when a built-in adapter supports the ecosystem. It does not run repo-controlled post-install scripts. Current defaulting adapters inspect package metadata such as Cargo binary names, Node `package.json` `bin`, and Python `pyproject.toml` `[project.scripts]`.

## Project

Repository: https://github.com/kfcafe/uu

The full project README in the repository has the complete ecosystem support matrix and examples.
