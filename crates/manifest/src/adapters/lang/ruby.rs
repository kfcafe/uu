//! Ruby language adapter — extracts classes, modules, methods, and attributes.

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Node, Parser};
use uu_detect::ProjectKind;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Field, Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct RubyAdapter;

impl Adapter for RubyAdapter {
    fn name(&self) -> &str {
        "ruby"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Ruby)
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec!["vendor", ".bundle"];

        for file in &ctx.files {
            if file.extension().and_then(|e| e.to_str()) != Some("rb") {
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

            extract_ruby(&tree.root_node(), &source, &rel, &mut fragment);
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

fn extract_ruby(root: &Node, source: &str, file: &str, fragment: &mut ManifestFragment) {
    walk_ruby(root, source, file, fragment, &[], false);
}

/// Recursively walk the AST, tracking module/class nesting and private section.
fn walk_ruby(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    name_stack: &[String],
    in_private: bool,
) {
    let mut cursor = node.walk();
    let mut is_private = in_private;

    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class" => {
                handle_class(&child, source, file, fragment, name_stack);
            }
            "module" => {
                handle_module(&child, source, file, fragment, name_stack);
            }
            "method" => {
                handle_method(&child, source, file, fragment, name_stack, is_private);
            }
            "call" => {
                // Check for `private`, `attr_accessor`, `attr_reader`, `attr_writer`
                let method = child
                    .child_by_field_name("method")
                    .map(|m| node_text(&m, source))
                    .unwrap_or_default();

                match method.as_str() {
                    "private" => {
                        // Check if it's a bare `private` (section marker) vs `private :method`
                        if child.child_by_field_name("arguments").is_none() {
                            is_private = true;
                        }
                    }
                    "public" => {
                        is_private = false;
                    }
                    "attr_accessor" | "attr_reader" | "attr_writer" => {
                        handle_attr(&child, source, fragment, name_stack, &method);
                    }
                    _ => {}
                }
            }
            "identifier" => {
                // Bare `private` or `public` can also appear as identifiers
                let text = node_text(&child, source);
                if text == "private" {
                    is_private = true;
                } else if text == "public" {
                    is_private = false;
                }
            }
            _ => {}
        }
    }
}

fn handle_class(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    name_stack: &[String],
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_name = qualified_name(name_stack, &name);

    // Check for superclass: `class X < Base`
    let mut implements = Vec::new();
    if let Some(superclass) = node.child_by_field_name("superclass") {
        // The superclass node wraps the actual type
        let mut sc_cursor = superclass.walk();
        for child in superclass.named_children(&mut sc_cursor) {
            let text = node_text(&child, source);
            if !text.is_empty() {
                implements.push(text);
            }
        }
        if implements.is_empty() {
            let text = node_text(&superclass, source);
            if !text.is_empty() {
                implements.push(text);
            }
        }
    }

    let type_def = TypeDef {
        name: name.clone(),
        source: file.to_string(),
        kind: TypeKind::Class,
        visibility: Visibility::Public,
        implements,
        ..Default::default()
    };
    fragment.types.insert(full_name.clone(), type_def);

    // Process class body
    if let Some(body) = node.child_by_field_name("body") {
        let mut new_stack = name_stack.to_vec();
        new_stack.push(name);
        walk_ruby(&body, source, file, fragment, &new_stack, false);
    }
}

fn handle_module(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    name_stack: &[String],
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_name = qualified_name(name_stack, &name);

    // Ruby modules → Trait (used for mixins)
    let type_def = TypeDef {
        name: name.clone(),
        source: file.to_string(),
        kind: TypeKind::Trait,
        visibility: Visibility::Public,
        ..Default::default()
    };
    fragment.types.insert(full_name.clone(), type_def);

    // Also create a Module entry
    let module = Module {
        path: full_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(full_name.clone(), module);

    // Process module body
    if let Some(body) = node.child_by_field_name("body") {
        let mut new_stack = name_stack.to_vec();
        new_stack.push(name);
        walk_ruby(&body, source, file, fragment, &new_stack, false);
    }
}

fn handle_method(
    node: &Node,
    source: &str,
    file: &str,
    fragment: &mut ManifestFragment,
    name_stack: &[String],
    in_private: bool,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let visibility = if in_private {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let params = node
        .child_by_field_name("parameters")
        .map(|p| node_text(&p, source))
        .unwrap_or_default();
    let signature = format!("def {}{}", name, params);

    let qualified = if name_stack.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", name_stack.join("::"), name)
    };

    let function = Function {
        name: name.clone(),
        source: file.to_string(),
        signature,
        visibility,
        ..Default::default()
    };
    fragment.functions.insert(qualified, function);

    // Record as method on parent type
    if !name_stack.is_empty() {
        let parent_full = name_stack.join("::");
        if let Some(type_def) = fragment.types.get_mut(&parent_full) {
            type_def.methods.push(name);
        }
    }
}

fn handle_attr(
    node: &Node,
    source: &str,
    fragment: &mut ManifestFragment,
    name_stack: &[String],
    attr_type: &str,
) {
    if name_stack.is_empty() {
        return;
    }

    let parent_full = name_stack.join("::");

    // Extract symbol arguments: attr_accessor :name, :email
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut cursor = args.walk();
        for child in args.named_children(&mut cursor) {
            let text = node_text(&child, source);
            // Strip leading : from symbols
            let field_name = text.strip_prefix(':').unwrap_or(&text).to_string();
            if field_name.is_empty() {
                continue;
            }

            let type_name = match attr_type {
                "attr_accessor" => "read/write".to_string(),
                "attr_reader" => "read".to_string(),
                "attr_writer" => "write".to_string(),
                _ => String::new(),
            };

            if let Some(type_def) = fragment.types.get_mut(&parent_full) {
                type_def.fields.push(Field {
                    name: field_name,
                    type_name,
                    optional: false,
                });
            }
        }
    }
}

fn qualified_name(stack: &[String], name: &str) -> String {
    if stack.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", stack.join("::"), name)
    }
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ruby(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_ruby(
            &tree.root_node(),
            source,
            "app/models/user.rb",
            &mut fragment,
        );
        fragment
    }

    #[test]
    fn class_detected() {
        let frag = parse_ruby(
            r#"
class User
  def initialize(name)
    @name = name
  end
end
"#,
        );

        assert!(frag.types.contains_key("User"));
        let td = &frag.types["User"];
        assert_eq!(td.kind, TypeKind::Class);
    }

    #[test]
    fn class_inheritance() {
        let frag = parse_ruby(
            r#"
class Admin < User
  def admin?
    true
  end
end
"#,
        );

        let td = &frag.types["Admin"];
        assert!(td.implements.contains(&"User".to_string()));
    }

    #[test]
    fn module_detected_as_trait() {
        let frag = parse_ruby(
            r#"
module Authenticatable
  def authenticate(password)
    true
  end
end
"#,
        );

        assert!(frag.types.contains_key("Authenticatable"));
        let td = &frag.types["Authenticatable"];
        assert_eq!(td.kind, TypeKind::Trait);
    }

    #[test]
    fn attr_accessor_creates_fields() {
        let frag = parse_ruby(
            r#"
class User
  attr_accessor :name, :email
  attr_reader :id
end
"#,
        );

        let td = &frag.types["User"];
        assert_eq!(td.fields.len(), 3);

        let name_field = td.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.type_name, "read/write");

        let id_field = td.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_field.type_name, "read");
    }

    #[test]
    fn private_section_changes_visibility() {
        let frag = parse_ruby(
            r#"
class Service
  def public_method
  end

  private

  def private_method
  end
end
"#,
        );

        let pub_func = frag
            .functions
            .values()
            .find(|f| f.name == "public_method")
            .unwrap();
        assert_eq!(pub_func.visibility, Visibility::Public);

        let priv_func = frag
            .functions
            .values()
            .find(|f| f.name == "private_method")
            .unwrap();
        assert_eq!(priv_func.visibility, Visibility::Private);
    }

    #[test]
    fn nested_module_and_class() {
        let frag = parse_ruby(
            r#"
module MyApp
  class User
    def greet
    end
  end
end
"#,
        );

        assert!(frag.types.contains_key("MyApp"));
        assert!(frag.types.contains_key("MyApp::User"));
        assert!(frag.functions.contains_key("MyApp::User.greet"));
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = RubyAdapter;

        let ruby_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Ruby,
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&ruby_ctx));

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
