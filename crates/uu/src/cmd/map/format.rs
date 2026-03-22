//! Output formatting — markdown, colored terminal display, and shared helpers.

use uu_manifest::schema::*;
use uu_manifest::Manifest;

// -- ANSI styling ------------------------------------------------------------

/// Colored, bold text for terminal output.
pub(super) fn bold(text: &str) -> String {
    format!("\x1b[1m{text}\x1b[0m")
}

pub(super) fn dim(text: &str) -> String {
    format!("\x1b[2m{text}\x1b[0m")
}

pub(super) fn cyan(text: &str) -> String {
    format!("\x1b[36m{text}\x1b[0m")
}

pub(super) fn green(text: &str) -> String {
    format!("\x1b[32m{text}\x1b[0m")
}

pub(super) fn yellow(text: &str) -> String {
    format!("\x1b[33m{text}\x1b[0m")
}

pub(super) fn magenta(text: &str) -> String {
    format!("\x1b[35m{text}\x1b[0m")
}

#[allow(dead_code)]
pub(super) fn red(text: &str) -> String {
    format!("\x1b[31m{text}\x1b[0m")
}

// -- Symbol display for terminal ---------------------------------------------

pub(super) fn print_type(key: &str, t: &TypeDef) {
    let kind_str = match t.kind {
        TypeKind::Struct => "struct",
        TypeKind::Class => "class",
        TypeKind::Interface => "interface",
        TypeKind::Enum => "enum",
        TypeKind::Trait => "trait",
        TypeKind::Protocol => "protocol",
        TypeKind::Union => "union",
        TypeKind::TypeAlias => "type alias",
    };
    let vis = visibility_label(&t.visibility);

    println!(
        "{} {} {}",
        cyan(&format!("type {key}")),
        dim(&format!("({kind_str})")),
        vis,
    );
    if !t.source.is_empty() {
        println!("  {} {}", dim("source:"), t.source);
    }

    if !t.fields.is_empty() {
        println!("  {}:", dim("fields"));
        for f in &t.fields {
            let opt = if f.optional { " (optional)" } else { "" };
            println!("    {}: {}{}", bold(&f.name), f.type_name, dim(opt));
        }
    }

    if !t.variants.is_empty() {
        println!("  {}:", dim("variants"));
        for v in &t.variants {
            println!("    {v}");
        }
    }

    if !t.methods.is_empty() {
        println!("  {} {}", dim("methods:"), t.methods.join(", "));
    }

    if !t.implements.is_empty() {
        println!("  {} {}", dim("implements:"), t.implements.join(", "));
    }
    println!();
}

pub(super) fn print_function(key: &str, f: &Function) {
    let vis = visibility_label(&f.visibility);
    let async_label = if f.is_async {
        format!(" {}", yellow("async"))
    } else {
        String::new()
    };

    println!("{}{} {}", green(&format!("fn {key}")), async_label, vis,);
    if !f.source.is_empty() {
        println!("  {} {}", dim("source:"), f.source);
    }
    if !f.signature.is_empty() {
        println!("  {} {}", dim("sig:"), f.signature);
    }
    println!();
}

pub(super) fn print_route(key: &str, r: &Route) {
    let methods = if r.methods.is_empty() {
        String::new()
    } else {
        format!(" [{}]", r.methods.join(", "))
    };
    let route_type = match r.route_type {
        RouteType::Page => "page",
        RouteType::Layout => "layout",
        RouteType::ApiRoute => "api",
        RouteType::Controller => "controller",
        RouteType::Middleware => "middleware",
    };

    println!(
        "{}{} {}",
        magenta(&format!("route {key}")),
        methods,
        dim(&format!("({route_type})")),
    );
    if !r.path.is_empty() {
        println!("  {} {}", dim("path:"), r.path);
    }
    if !r.file.is_empty() {
        println!("  {} {}", dim("file:"), r.file);
    }
    if let Some(handler) = &r.handler {
        println!("  {} {}", dim("handler:"), handler);
    }
    println!();
}

pub(super) fn print_module(key: &str, m: &Module) {
    println!("{}", yellow(&format!("module {key}")));
    if !m.file.is_empty() {
        println!("  {} {}", dim("file:"), m.file);
    }
    if !m.exports.is_empty() {
        println!("  {} {}", dim("exports:"), m.exports.join(", "));
    }
    if !m.imports.is_empty() {
        println!("  {} {}", dim("imports:"), m.imports.join(", "));
    }
    println!();
}

pub(super) fn print_model(key: &str, m: &DataModel) {
    println!(
        "{} {}",
        cyan(&format!("model {key}")),
        if m.orm.is_empty() {
            String::new()
        } else {
            dim(&format!("({})", m.orm))
        },
    );
    if !m.source.is_empty() {
        println!("  {} {}", dim("source:"), m.source);
    }
    if !m.fields.is_empty() {
        println!("  {}:", dim("fields"));
        for f in &m.fields {
            let opt = if f.optional { " (optional)" } else { "" };
            println!("    {}: {}{}", bold(&f.name), f.type_name, dim(opt));
        }
    }
    if !m.relations.is_empty() {
        println!("  {}:", dim("relations"));
        for r in &m.relations {
            println!("    {} → {} ({:?})", r.name, r.target, r.kind);
        }
    }
    println!();
}

fn visibility_label(v: &Visibility) -> String {
    match v {
        Visibility::Public => String::new(),
        Visibility::Private => dim("(private)"),
        Visibility::Internal => dim("(internal)"),
    }
}

// -- Cross-reference helpers -------------------------------------------------

/// Find symbols in the manifest that reference the given name.
/// Searches function signatures, type fields, and implements lists.
pub(super) fn find_references(name: &str, manifest: &Manifest) -> Vec<String> {
    let mut refs = Vec::new();
    let name_lower = name.to_lowercase();

    // Functions whose signatures mention this name
    for (key, func) in &manifest.functions {
        if key.to_lowercase() != name_lower && func.signature.to_lowercase().contains(&name_lower) {
            refs.push(format!("fn {key}"));
        }
    }

    // Types whose fields reference this name
    for (key, typedef) in &manifest.types {
        if key.to_lowercase() == name_lower {
            continue;
        }
        let references_name = typedef
            .fields
            .iter()
            .any(|f| f.type_name.to_lowercase().contains(&name_lower))
            || typedef
                .implements
                .iter()
                .any(|i| i.to_lowercase() == name_lower);
        if references_name {
            refs.push(format!("type {key}"));
        }
    }

    // Models whose fields reference this name
    for (key, model) in &manifest.models {
        if key.to_lowercase() == name_lower {
            continue;
        }
        let references_name = model
            .fields
            .iter()
            .any(|f| f.type_name.to_lowercase().contains(&name_lower))
            || model
                .relations
                .iter()
                .any(|r| r.target.to_lowercase() == name_lower);
        if references_name {
            refs.push(format!("model {key}"));
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

// -- "Did you mean?" suggestions ---------------------------------------------

/// Find symbol names similar to `query` for typo suggestions.
pub(super) fn find_similar(query: &str, manifest: &Manifest) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut candidates: Vec<(usize, String)> = Vec::new();

    let all_names: Vec<&str> = manifest
        .types
        .keys()
        .chain(manifest.functions.keys())
        .chain(manifest.modules.keys())
        .chain(manifest.routes.keys())
        .chain(manifest.models.keys())
        .map(|s| s.as_str())
        .collect();

    for name in all_names {
        let name_lower = name.to_lowercase();

        // Substring match
        if name_lower.contains(&query_lower) || query_lower.contains(&name_lower) {
            candidates.push((0, name.to_string()));
            continue;
        }

        // Simple edit distance (skip if names are very different lengths)
        if name_lower.len().abs_diff(query_lower.len()) <= 3 {
            let dist = edit_distance(&query_lower, &name_lower);
            if dist <= 3 {
                candidates.push((dist, name.to_string()));
            }
        }
    }

    candidates.sort_by_key(|(dist, name)| (*dist, name.clone()));
    candidates.into_iter().map(|(_, name)| name).collect()
}

/// Simple Levenshtein distance for typo suggestions.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// -- Markdown output ---------------------------------------------------------

/// Render a full manifest as a Markdown document.
pub(super) fn to_markdown(manifest: &Manifest) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Project Map: {}\n\n", manifest.project.name));
    out.push_str(&format!(
        "**Language:** {}  \n**Generated:** {}  \n**uu version:** {}\n\n",
        manifest.project.kind, manifest.project.generated_at, manifest.project.uu_version
    ));

    // Summary
    let type_count = manifest.types.len();
    let fn_count = manifest.functions.len();
    let mod_count = manifest.modules.len();
    let route_count = manifest.routes.len();
    let endpoint_count = manifest.endpoints.len();
    let model_count = manifest.models.len();

    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "| Category | Count |\n|----------|-------|\n| Types | {type_count} |\n| Functions | {fn_count} |\n| Modules | {mod_count} |\n"
    ));
    if route_count > 0 {
        out.push_str(&format!("| Routes | {route_count} |\n"));
    }
    if endpoint_count > 0 {
        out.push_str(&format!("| Endpoints | {endpoint_count} |\n"));
    }
    if model_count > 0 {
        out.push_str(&format!("| Models | {model_count} |\n"));
    }
    out.push('\n');

    // Types
    if !manifest.types.is_empty() {
        out.push_str("## Types\n\n");
        for (name, t) in &manifest.types {
            let kind = match t.kind {
                TypeKind::Struct => "struct",
                TypeKind::Class => "class",
                TypeKind::Interface => "interface",
                TypeKind::Enum => "enum",
                TypeKind::Trait => "trait",
                TypeKind::Protocol => "protocol",
                TypeKind::Union => "union",
                TypeKind::TypeAlias => "type alias",
            };
            out.push_str(&format!("### `{name}` ({kind})\n\n"));
            if !t.source.is_empty() {
                out.push_str(&format!("*Source: {}*\n\n", t.source));
            }

            if !t.fields.is_empty() {
                out.push_str("| Field | Type | Optional |\n|-------|------|----------|\n");
                for f in &t.fields {
                    let opt = if f.optional { "✓" } else { "" };
                    out.push_str(&format!("| `{}` | `{}` | {} |\n", f.name, f.type_name, opt));
                }
                out.push('\n');
            }

            if !t.variants.is_empty() {
                out.push_str(&format!(
                    "**Variants:** {}\n\n",
                    t.variants
                        .iter()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if !t.methods.is_empty() {
                out.push_str(&format!(
                    "**Methods:** {}\n\n",
                    t.methods
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if !t.implements.is_empty() {
                out.push_str(&format!("**Implements:** {}\n\n", t.implements.join(", ")));
            }

            out.push_str("---\n\n");
        }
    }

    // Functions
    if !manifest.functions.is_empty() {
        out.push_str("## Functions\n\n");
        out.push_str(
            "| Name | Signature | Source | Async |\n|------|-----------|--------|-------|\n",
        );
        for (name, f) in &manifest.functions {
            let async_mark = if f.is_async { "✓" } else { "" };
            let sig = if f.signature.is_empty() {
                "—".to_string()
            } else {
                format!("`{}`", f.signature)
            };
            out.push_str(&format!(
                "| `{name}` | {sig} | {} | {async_mark} |\n",
                f.source
            ));
        }
        out.push('\n');
    }

    // Modules
    if !manifest.modules.is_empty() {
        out.push_str("## Modules\n\n");
        out.push_str("| Module | File | Exports |\n|--------|------|---------|\n");
        for (name, m) in &manifest.modules {
            let exports = if m.exports.is_empty() {
                "—".to_string()
            } else {
                m.exports.join(", ")
            };
            out.push_str(&format!("| `{name}` | {} | {exports} |\n", m.file));
        }
        out.push('\n');
    }

    // Routes
    if !manifest.routes.is_empty() {
        out.push_str("## Routes\n\n");
        out.push_str("| Path | Methods | Type | File |\n|------|---------|------|------|\n");
        for (name, r) in &manifest.routes {
            let methods = if r.methods.is_empty() {
                "—".to_string()
            } else {
                r.methods.join(", ")
            };
            let route_type = format!("{:?}", r.route_type);
            let display_path = if r.path.is_empty() { name } else { &r.path };
            out.push_str(&format!(
                "| `{display_path}` | {methods} | {route_type} | {} |\n",
                r.file
            ));
        }
        out.push('\n');
    }

    // Models
    if !manifest.models.is_empty() {
        out.push_str("## Models\n\n");
        for (name, m) in &manifest.models {
            let orm = if m.orm.is_empty() {
                String::new()
            } else {
                format!(" ({})", m.orm)
            };
            out.push_str(&format!("### `{name}`{orm}\n\n"));
            if !m.fields.is_empty() {
                out.push_str("| Field | Type | Optional |\n|-------|------|----------|\n");
                for f in &m.fields {
                    let opt = if f.optional { "✓" } else { "" };
                    out.push_str(&format!("| `{}` | `{}` | {} |\n", f.name, f.type_name, opt));
                }
                out.push('\n');
            }
            if !m.relations.is_empty() {
                out.push_str("**Relations:**\n\n");
                for r in &m.relations {
                    out.push_str(&format!("- `{}` → `{}` ({:?})\n", r.name, r.target, r.kind));
                }
                out.push('\n');
            }
            out.push_str("---\n\n");
        }
    }

    // Auth
    if let Some(auth) = &manifest.auth {
        out.push_str("## Auth\n\n");
        if !auth.strategy.is_empty() {
            out.push_str(&format!("**Strategy:** {}\n\n", auth.strategy));
        }
        if !auth.providers.is_empty() {
            out.push_str(&format!("**Providers:** {}\n\n", auth.providers.join(", ")));
        }
    }

    // Components
    if !manifest.components.is_empty() {
        out.push_str("## Components\n\n");
        out.push_str("| Name | File | Props |\n|------|------|-------|\n");
        for c in &manifest.components {
            let props = if c.props.is_empty() {
                "—".to_string()
            } else {
                c.props
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.type_name))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!("| `{}` | {} | {} |\n", c.name, c.file, props));
        }
        out.push('\n');
    }

    out
}
