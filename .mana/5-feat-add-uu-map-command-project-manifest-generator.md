---
id: '5'
title: 'feat: add uu map command — project manifest generator'
slug: feat-add-uu-map-command-project-manifest-generator
status: open
priority: 2
created_at: '2026-03-19T09:31:26.007165Z'
updated_at: '2026-03-19T10:01:26.700360Z'
notes: |-
  ---
  2026-03-19T10:01:26.700350+00:00
  ## Attempt Failed (1s, 0 tokens, $0.000)

  ### What was tried

  - 0 tool calls over 0 turns in 1s

  ### Why it failed

  - Aborted

  ### Verify command

  `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo run -p uu -- map --help`

  ### Suggestion for next attempt

  - Agent was manually aborted. Review progress so far before retrying.
verify: cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo run -p uu -- map --help
fail_first: true
checkpoint: '5dbed6d84580b0b97138398edf0663309a43bdd4'
attempt_log:
- num: 1
  outcome: abandoned
  agent: pi-agent
  started_at: '2026-03-19T10:01:25.664552Z'
  finished_at: '2026-03-19T10:01:26.665336Z'
---

## Overview

Add a new `uu map` command that scans a codebase and produces a structured YAML manifest
of everything that exists: types, functions, modules, routes, API endpoints, data models,
and integrations. Uses tree-sitter AST parsing and framework-specific adapters to extract
semantic information, not just symbols.

This is a framework-aware project mapping tool. It works for every project type uu supports.
For a Rust project it extracts structs, enums, traits, and impls. For a Next.js project it
knows that `app/invoices/page.tsx` is a route. For a Django project it finds `urls.py` routes
and `models.py` entities. For a Go project with Gin, it finds router definitions. The output
is a single `.manifest.yaml` that gives any AI agent (or human) a complete map of the project
without reading source code.

Two layers of extraction:
1. **Language adapters** — always run for the detected language. Extract types, functions,
   modules, exports. Every supported language gets one.
2. **Framework adapters** — detected from dependencies. Extract semantic information: routes,
   data models, endpoints, auth config. Only run when the framework is detected.

## Architecture

### New crate: `crates/manifest/`

Library crate (`uu-manifest`) containing:
- `Adapter` trait and adapter registry
- `ManifestFragment` and `Manifest` types (serde-serializable)
- Merge logic for combining adapter outputs
- Diff logic for comparing manifests
- Built-in adapters for all supported languages and popular frameworks

**Key types:**

```rust
pub trait Adapter: Send + Sync {
    fn name(&self) -> &str;
    fn detect(&self, ctx: &ProjectContext) -> bool;
    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment>;
    fn priority(&self) -> u32; // higher runs first — language > framework > component
    fn layer(&self) -> AdapterLayer; // Language or Framework
}

pub enum AdapterLayer {
    Language,   // always runs if project kind matches
    Framework,  // detected from dependencies/config files
}

pub struct ProjectContext {
    pub root: PathBuf,
    pub kind: ProjectKind,              // from uu-detect
    pub files: Vec<PathBuf>,            // all source files (respects .gitignore)
    pub package_json: Option<Value>,    // cached, parsed
    pub cargo_toml: Option<Value>,      // cached, parsed
    pub go_mod: Option<String>,         // cached, raw
}

pub struct Manifest {
    pub project: ProjectMeta,           // name, kind, frameworks detected
    pub types: BTreeMap<String, TypeDef>,      // structs, classes, interfaces, enums
    pub functions: BTreeMap<String, Function>,  // top-level and exported functions
    pub modules: BTreeMap<String, Module>,      // files/packages as modules
    pub routes: BTreeMap<String, Route>,        // web routes (framework-detected)
    pub endpoints: BTreeMap<String, Endpoint>,  // API endpoints
    pub models: BTreeMap<String, DataModel>,    // ORM/database models
    pub auth: Option<AuthConfig>,
    pub components: Vec<Component>,             // UI components (frontend frameworks)
    pub integrations: Vec<Integration>,
}

pub struct TypeDef {
    pub name: String,
    pub source: String,                 // "src/auth.rs:12"
    pub kind: TypeKind,                 // Struct, Class, Interface, Enum, Trait, Protocol
    pub fields: Vec<Field>,
    pub methods: Vec<String>,           // method names
    pub visibility: Visibility,         // Public, Private, internal
    pub implements: Vec<String>,        // traits/interfaces/protocols
}

pub enum TypeKind {
    Struct, Class, Interface, Enum, Trait, Protocol, Union, TypeAlias,
}

pub struct Function {
    pub name: String,
    pub source: String,
    pub signature: String,              // "fn process(input: &str) -> Result<Output>"
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_test: bool,
}

pub struct Module {
    pub path: String,                   // "src/auth" or "app/models"
    pub file: String,
    pub exports: Vec<String>,           // exported symbol names
    pub imports: Vec<String>,           // imported modules/packages
}

pub struct DataModel {
    pub name: String,
    pub source: String,
    pub orm: String,                    // "prisma", "ecto", "django", "activerecord", "sqlalchemy", "gorm"
    pub fields: Vec<Field>,
    pub relations: Vec<Relation>,
    pub indexes: Vec<String>,
}

pub struct Route {
    pub path: String,                   // "/products"
    pub file: String,
    pub route_type: RouteType,          // Page, Layout, ApiRoute, Controller
    pub methods: Vec<String>,           // GET, POST, etc. (for API routes)
    pub handler: Option<String>,        // handler function name
}
```

## Language Adapters (v1 — one per supported ProjectKind)

Every language uu-detect supports gets a language adapter. These extract types, functions,
modules using tree-sitter AST parsing.

### rust adapter
- Detects: ProjectKind::Cargo
- Extracts: pub structs, enums, traits, impls, pub functions, mod structure
- Tree-sitter grammar: tree-sitter-rust
- Also detects framework usage: axum (Router), actix-web, rocket from Cargo.toml deps

### go adapter
- Detects: ProjectKind::Go
- Extracts: structs, interfaces, exported functions (capitalized), package structure
- Tree-sitter grammar: tree-sitter-go
- Also detects: gin, echo, net/http route patterns from go.mod deps

### python adapter
- Detects: ProjectKind::Python
- Extracts: classes, functions, decorators, module structure
- Tree-sitter grammar: tree-sitter-python
- Also detects: fastapi, django, flask, sqlalchemy from pyproject.toml/requirements.txt

### typescript adapter
- Detects: ProjectKind::Node + tsconfig.json exists
- Extracts: interfaces, types, enums, exported functions, classes
- Tree-sitter grammar: tree-sitter-typescript
- Falls back to javascript adapter if no tsconfig

### javascript adapter
- Detects: ProjectKind::Node (when no tsconfig)
- Extracts: classes, exported functions, module.exports
- Tree-sitter grammar: tree-sitter-javascript

### elixir adapter
- Detects: ProjectKind::Elixir
- Extracts: modules, public functions (def), module attributes, behaviours
- Tree-sitter grammar: tree-sitter-elixir
- Also detects: phoenix, ecto from mix.exs deps

### java adapter
- Detects: ProjectKind::Gradle or ProjectKind::Maven
- Extracts: classes, interfaces, public methods, annotations
- Tree-sitter grammar: tree-sitter-java
- Also detects: spring-boot (from build.gradle/pom.xml deps)

### ruby adapter
- Detects: ProjectKind::Ruby
- Extracts: classes, modules, public methods, attr_accessor/reader/writer
- Tree-sitter grammar: tree-sitter-ruby
- Also detects: rails (from Gemfile)

### swift adapter
- Detects: ProjectKind::Swift
- Extracts: structs, classes, protocols, enums, public functions
- Tree-sitter grammar: tree-sitter-swift

### csharp adapter
- Detects: ProjectKind::DotNet
- Extracts: classes, interfaces, public methods, records
- Tree-sitter grammar: tree-sitter-c-sharp
- Also detects: ASP.NET controllers (from .csproj deps)

### c_cpp adapter
- Detects: ProjectKind::CMake or ProjectKind::Meson or ProjectKind::Make
- Extracts: structs, typedefs, function declarations (from headers), enums
- Tree-sitter grammar: tree-sitter-c and tree-sitter-cpp
- Scans .h/.hpp files for public API

## Framework Adapters (v1 — popular frameworks per language)

Framework adapters are detected from dependency files and extract semantic information
(routes, data models, endpoints) that language adapters cannot.

### nextjs adapter
- Detects: "next" in package.json dependencies
- Extracts: pages (app router file conventions), API routes (exported HTTP methods),
  layouts, middleware, server actions
- Populates: routes, endpoints

### prisma adapter
- Detects: prisma/schema.prisma exists
- Extracts: models, fields, relations, enums, indexes
- Simple line-by-line parser (Prisma schema is regular, no tree-sitter needed)
- Populates: models

### express adapter
- Detects: "express" in package.json dependencies
- Extracts: app.get/post/put/delete route definitions
- Uses tree-sitter to find method calls on express app/router objects
- Populates: routes, endpoints

### django adapter
- Detects: "django" in pyproject.toml/requirements.txt
- Extracts: urls.py route definitions, models.py model classes, views
- Populates: routes, models

### fastapi adapter
- Detects: "fastapi" in pyproject.toml/requirements.txt
- Extracts: @app.get/post/etc decorated functions, Pydantic models
- Populates: routes, endpoints, models (from Pydantic)

### rails adapter
- Detects: "rails" in Gemfile
- Extracts: config/routes.rb route definitions, app/models/*.rb ActiveRecord models,
  app/controllers/*.rb controllers
- Populates: routes, models, endpoints

### phoenix adapter
- Detects: "phoenix" in mix.exs deps
- Extracts: router.ex route definitions, Ecto schemas, controllers
- Populates: routes, models, endpoints

### ecto adapter
- Detects: "ecto" in mix.exs deps (may run without phoenix)
- Extracts: Ecto.Schema definitions — fields, types, associations
- Populates: models

### axum adapter
- Detects: "axum" in Cargo.toml dependencies
- Extracts: Router::new().route() definitions, handler function signatures
- Populates: routes, endpoints

### spring adapter
- Detects: "spring-boot" in build.gradle/pom.xml
- Extracts: @RestController/@Controller classes, @GetMapping/etc annotations,
  @Entity JPA models
- Populates: routes, endpoints, models

### gin adapter
- Detects: "github.com/gin-gonic/gin" in go.mod
- Extracts: r.GET/POST/etc route definitions
- Populates: routes, endpoints

### gorm adapter
- Detects: "gorm.io/gorm" in go.mod
- Extracts: struct types that embed gorm.Model
- Populates: models

### aspnet adapter
- Detects: "Microsoft.AspNetCore" in .csproj
- Extracts: Controller classes, [HttpGet]/[HttpPost] attributes, Entity Framework models
- Populates: routes, endpoints, models

### shadcn adapter
- Detects: components.json exists (shadcn config file)
- Extracts: installed UI components from components/ui/*.tsx
- Populates: components

### authjs adapter
- Detects: "@auth/core" or "next-auth" in package.json
- Extracts: providers, session strategy
- Populates: auth

## CLI Interface

```
uu map [OPTIONS] [PATH]

Arguments:
  [PATH]  Project directory (default: current directory)

Options:
  -o, --output <FILE>     Output file (default: .manifest.yaml)
  -f, --format <FORMAT>   Output format: yaml, json (default: yaml)
      --stdout             Write to stdout instead of file
      --detect-only        Show detected language and frameworks without generating manifest
      --diff               Show diff against existing manifest
      --adapters <LIST>    Only run specific adapters (comma-separated)
  -n, --dry-run            Show what would be generated without writing
  -h, --help               Print help
```

## Files to create/modify

### New files:
- crates/manifest/Cargo.toml
- crates/manifest/src/lib.rs — public API, Manifest/ManifestFragment types, merge logic
- crates/manifest/src/schema.rs — all serde types (TypeDef, Function, Route, etc.)
- crates/manifest/src/context.rs — ProjectContext builder (file scanning, dep caching)
- crates/manifest/src/diff.rs — manifest diffing
- crates/manifest/src/adapters/mod.rs — Adapter trait, registry, detection
- crates/manifest/src/adapters/lang/ — language adapters directory
  - rust.rs, go.rs, python.rs, typescript.rs, javascript.rs, elixir.rs,
    java.rs, ruby.rs, swift.rs, csharp.rs, c_cpp.rs
- crates/manifest/src/adapters/framework/ — framework adapters directory
  - nextjs.rs, prisma.rs, express.rs, django.rs, fastapi.rs, rails.rs,
    phoenix.rs, ecto.rs, axum.rs, spring.rs, gin.rs, gorm.rs, aspnet.rs,
    shadcn.rs, authjs.rs
- crates/uu/src/cmd/map.rs — CLI command

### Modified files:
- Cargo.toml — add manifest crate to workspace
- crates/uu/Cargo.toml — add uu-manifest dependency
- crates/uu/src/main.rs — add Map subcommand
- crates/uu/src/cmd/mod.rs — export map module

## Tests

### Unit tests (in crates/manifest/):
- Each language adapter: parse sample source files, verify types/functions extracted
- Each framework adapter: given mock project structure, verify routes/models/endpoints
- Manifest merge: fragments combine correctly, no duplicates
- Manifest diff: detect added/removed/changed entries
- Adapter detection: given mock dependency files, correct adapters are selected

### Integration tests (in crates/uu/tests/):
- map_help_shows_in_main_help: uu --help includes "map"
- map_detect_only: create tempdir with Cargo.toml, verify detected adapters shown
- map_rust_project: tempdir with .rs files, verify types/functions in manifest
- map_node_project: tempdir with package.json + ts files, verify manifest
- map_stdout_flag: verify --stdout outputs to stdout not file
- map_dry_run: verify -n shows plan without writing
- map_format_json: verify --format json produces valid JSON

## Dependencies

New workspace dependencies:
- tree-sitter = "0.24"
- tree-sitter-rust, tree-sitter-go, tree-sitter-python, tree-sitter-typescript,
  tree-sitter-javascript, tree-sitter-java, tree-sitter-ruby, tree-sitter-swift,
  tree-sitter-c, tree-sitter-cpp, tree-sitter-c-sharp, tree-sitter-elixir
- serde_yaml = "0.9"

Note: tree-sitter grammars are compiled into the binary. This will increase binary size
but is the right tradeoff for zero-config behavior — matches uu philosophy.

The prisma, django (urls.py), rails (routes.rb), and phoenix (router.ex) adapters
use simpler parsers where the file format is regular enough to not need tree-sitter.

## Performance target

< 1s on a large project (500+ source files). Tree-sitter parsing is fast (single-digit
milliseconds per file). Main cost is file I/O.

## Design principles

- Every language uu supports gets a language adapter — no second-class citizens
- Adapters are independent — each produces a ManifestFragment, core merges them
- Detection is cheap — file existence and dependency lookups, no parsing
- Extraction is deterministic — same input always produces same output, no LLM calls
- Source references included — every item has file:line for jump-to-definition
- The manifest header includes generation timestamp and uu version for staleness detection
- Framework adapters are optional — a Rust project with no web framework still gets
  a useful manifest of its types and functions

## Do not

- Do not add LLM or network dependencies — fully offline and deterministic
- Do not parse file contents when file conventions suffice (Next.js routing, Rails conventions)
- Do not refactor existing uu commands or the detect crate beyond adding the new subcommand
- Do not implement watch/auto-update mode — that comes later
- Do not try to resolve cross-file references in v1 — that is a major complexity increase.
  Just extract what is in each file. Cross-file analysis comes in v2.
