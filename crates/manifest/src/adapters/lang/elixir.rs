//! Elixir language adapter — extracts modules, functions, and behaviours.

use std::path::Path;

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct ElixirAdapter;

impl Adapter for ElixirAdapter {
    fn name(&self) -> &str {
        "elixir"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Elixir { .. })
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_elixir::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["_build", "deps"];

        for file in &ctx.files {
            if !matches!(
                file.extension().and_then(|e| e.to_str()),
                Some("ex" | "exs")
            ) {
                continue;
            }
            if should_skip(file, &ctx.root, &skip_dirs) {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tree = match parser.parse(&source, None) {
                Some(t) => t,
                None => continue,
            };

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_elixir(&tree.root_node(), &source, &rel, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        100
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Language
    }
}

fn should_skip(file: &Path, root: &Path, skip_dirs: &[&str]) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if skip_dirs.contains(&name.as_ref()) {
                return true;
            }
        }
    }
    false
}

/// Walk the AST looking for defmodule, def, defp, @behaviour, use, import, alias.
fn extract_elixir(node: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_elixir(node, source, file, fragment, &[]);
}

/// Find the `arguments` child of a `call` node.
/// In Elixir's tree-sitter grammar, `arguments` is a child, not a field.
fn find_arguments<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments");
    result
}

/// Find the `do_block` child of a `call` node.
fn find_do_block<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "do_block");
    result
}

/// Get the `target` field (identifier) of a `call` node.
fn call_target_text(node: &Node, source: &str) -> String {
    // `target` is a field on call nodes
    if let Some(target) = node.child_by_field_name("target") {
        return node_text(&target, source);
    }
    // Fallback: first identifier child
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            return node_text(&child, source);
        }
    }
    String::new()
}

/// Recursively walk AST, tracking the current module context.
fn walk_elixir(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    module_stack: &[String],
) {
    if node.kind() == "call" {
        let target_text = call_target_text(node, source);
        match target_text.as_str() {
            "defmodule" => {
                handle_defmodule(node, source, file, fragment, module_stack);
                return;
            }
            "def" => {
                handle_def(
                    node,
                    source,
                    file,
                    fragment,
                    module_stack,
                    Visibility::Public,
                );
                return;
            }
            "defp" => {
                handle_def(
                    node,
                    source,
                    file,
                    fragment,
                    module_stack,
                    Visibility::Private,
                );
                return;
            }
            "use" | "import" | "alias" => {
                handle_module_directive(node, source, fragment, module_stack, &target_text);
                return;
            }
            _ => {}
        }
    }

    // Handle @behaviour via unary_operator (@)
    if node.kind() == "unary_operator" {
        let text = node_text(node, source);
        if text.starts_with("@behaviour") {
            handle_behaviour(node, source, fragment, module_stack);
            return;
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_elixir(&child, source, file, fragment, module_stack);
    }
}

fn handle_defmodule(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    parent_modules: &[String],
) {
    // The arguments child contains the module name as its first named child (an alias node)
    let module_name = match find_arguments(node) {
        Some(args) => first_named_child_text(&args, source),
        None => return,
    };

    if module_name.is_empty() {
        return;
    }

    let type_def = TypeDef {
        name: module_name.clone(),
        source: file.to_string(),
        kind: TypeKind::Class,
        visibility: Visibility::Public,
        ..Default::default()
    };
    fragment.types.insert(module_name.clone(), type_def);

    let module = Module {
        path: module_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(module_name.clone(), module);

    // Walk the do_block for nested definitions
    let mut new_stack = parent_modules.to_vec();
    new_stack.push(module_name);

    if let Some(do_block) = find_do_block(node) {
        let mut cursor = do_block.walk();
        for child in do_block.named_children(&mut cursor) {
            walk_elixir(&child, source, file, fragment, &new_stack);
        }
    }
}

fn handle_def(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    module_stack: &[String],
    visibility: Visibility,
) {
    // Arguments of `def` contain the function definition.
    // For `def create_user(attrs)`, arguments has a `call` child: `create_user(attrs)`
    //   where `create_user` is the target.
    // For `def init`, arguments has just an `identifier` child.
    let args = match find_arguments(node) {
        Some(a) => a,
        None => return,
    };

    let func_name = extract_func_name(&args, source);
    if func_name.is_empty() {
        return;
    }

    let qualified_name = if module_stack.is_empty() {
        func_name.clone()
    } else {
        format!("{}.{}", module_stack.last().unwrap(), func_name)
    };

    let sig = node_text(node, source);
    let signature = sig.lines().next().unwrap_or("").trim().to_string();

    let function = Function {
        name: func_name,
        source: file.to_string(),
        signature,
        visibility,
        ..Default::default()
    };

    if let Some(module_name) = module_stack.last() {
        if let Some(type_def) = fragment.types.get_mut(module_name) {
            type_def.methods.push(function.name.clone());
        }
    }

    fragment.functions.insert(qualified_name, function);
}

/// Extract the function name from the arguments of a def/defp call.
fn extract_func_name(args_node: &Node, source: &str) -> String {
    let mut cursor = args_node.walk();
    for child in args_node.named_children(&mut cursor) {
        match child.kind() {
            "call" => {
                // `def func_name(params)` — the nested call's target is the name
                return call_target_text(&child, source);
            }
            "identifier" => {
                // `def func_name` (no params)
                return node_text(&child, source);
            }
            "binary_operator" => {
                // `def func_name(params) when guard` — left side has the call
                if let Some(left) = child.child_by_field_name("left") {
                    if left.kind() == "call" {
                        return call_target_text(&left, source);
                    }
                    return node_text(&left, source);
                }
            }
            _ => {}
        }
    }
    String::new()
}

fn handle_module_directive(
    node: &Node,
    source: &str,
    fragment: &mut ManifestFragment,
    module_stack: &[String],
    directive: &str,
) {
    let Some(module_name) = module_stack.last() else {
        return;
    };

    let arg_text = match find_arguments(node) {
        Some(args) => first_named_child_text(&args, source),
        None => return,
    };

    if arg_text.is_empty() {
        return;
    }

    let import_str = format!("{} {}", directive, arg_text);

    if let Some(module) = fragment.modules.get_mut(module_name) {
        module.imports.push(import_str);
    }
}

fn handle_behaviour(
    node: &Node,
    source: &str,
    fragment: &mut ManifestFragment,
    module_stack: &[String],
) {
    let Some(module_name) = module_stack.last() else {
        return;
    };

    let text = node_text(node, source);
    if let Some(rest) = text.strip_prefix("@behaviour ") {
        let behaviour_name = rest.trim().to_string();
        if let Some(type_def) = fragment.types.get_mut(module_name) {
            type_def.implements.push(behaviour_name);
        }
    }
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn first_named_child_text(node: &Node, source: &str) -> String {
    let mut cursor = node.walk();
    if let Some(child) = node.named_children(&mut cursor).next() {
        return node_text(&child, source);
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_elixir(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_elixir::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_elixir(&tree.root_node(), source, "lib/my_app.ex", &mut fragment);
        fragment
    }

    #[test]
    fn defmodule_detected_as_class() {
        let frag = parse_elixir(
            r#"
defmodule MyApp.Users do
end
"#,
        );

        assert!(frag.types.contains_key("MyApp.Users"));
        let td = &frag.types["MyApp.Users"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.source, "lib/my_app.ex");
    }

    #[test]
    fn def_vs_defp_visibility() {
        let frag = parse_elixir(
            r#"
defmodule MyApp.Users do
  def create_user(attrs) do
    attrs
  end

  defp validate(attrs) do
    attrs
  end
end
"#,
        );

        assert!(
            frag.functions.contains_key("MyApp.Users.create_user"),
            "Expected create_user, got keys: {:?}",
            frag.functions.keys().collect::<Vec<_>>()
        );
        let create = &frag.functions["MyApp.Users.create_user"];
        assert_eq!(create.visibility, Visibility::Public);
        assert_eq!(create.name, "create_user");

        assert!(
            frag.functions.contains_key("MyApp.Users.validate"),
            "Expected validate, got keys: {:?}",
            frag.functions.keys().collect::<Vec<_>>()
        );
        let validate = &frag.functions["MyApp.Users.validate"];
        assert_eq!(validate.visibility, Visibility::Private);
        assert_eq!(validate.name, "validate");
    }

    #[test]
    fn behaviour_detected_as_implements() {
        let frag = parse_elixir(
            r#"
defmodule MyApp.Worker do
  @behaviour GenServer
  @behaviour Supervisor

  def init(state) do
    {:ok, state}
  end
end
"#,
        );

        let td = &frag.types["MyApp.Worker"];
        assert!(td.implements.contains(&"GenServer".to_string()));
        assert!(td.implements.contains(&"Supervisor".to_string()));
    }

    #[test]
    fn use_import_alias_tracked() {
        let frag = parse_elixir(
            r#"
defmodule MyApp.Accounts do
  use GenServer
  import Ecto.Query
  alias MyApp.Repo
end
"#,
        );

        let module = &frag.modules["MyApp.Accounts"];
        assert!(
            module.imports.iter().any(|i| i.contains("GenServer")),
            "Expected GenServer in imports: {:?}",
            module.imports
        );
        assert!(
            module.imports.iter().any(|i| i.contains("Ecto.Query")),
            "Expected Ecto.Query in imports: {:?}",
            module.imports
        );
        assert!(
            module.imports.iter().any(|i| i.contains("MyApp.Repo")),
            "Expected MyApp.Repo in imports: {:?}",
            module.imports
        );
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = ElixirAdapter;

        let elixir_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Elixir { escript: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&elixir_ctx));

        let rust_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Cargo,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!adapter.detect(&rust_ctx));
    }
}
