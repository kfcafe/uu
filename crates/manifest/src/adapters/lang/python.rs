//! Python language adapter — extracts classes, functions, and modules.

use std::path::Path;

use anyhow::Result;
use project_detect::ProjectKind;
use tree_sitter::{Node, Parser};

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{Function, ManifestFragment, Module, TypeDef, TypeKind, Visibility};

pub struct PythonAdapter;

impl Adapter for PythonAdapter {
    fn name(&self) -> &str {
        "python"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        matches!(ctx.kind, ProjectKind::Python { .. })
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into())?;

        let skip_dirs: Vec<&str> = vec![
            "__pycache__",
            ".venv",
            "venv",
            ".tox",
            ".mypy_cache",
            ".pytest_cache",
            "site-packages",
            ".eggs",
            "egg-info",
        ];

        for file in &ctx.files {
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("py")) {
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

            let is_test = is_test_file(&rel);
            extract_python(&tree.root_node(), &source, &rel, is_test, &mut fragment);
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
            // Skip directories ending with .egg-info
            if name.ends_with(".egg-info") {
                return true;
            }
        }
    }
    false
}

/// Check if a file is a test file based on naming conventions.
fn is_test_file(rel_path: &str) -> bool {
    let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
    filename.starts_with("test_") || filename.ends_with("_test.py")
}

/// Derive the module path from a relative file path.
/// e.g. "src/myapp/models.py" → "src.myapp.models"
/// e.g. "src/myapp/__init__.py" → "src.myapp"
fn module_path_from_file(rel_path: &str) -> String {
    let without_ext = rel_path.strip_suffix(".py").unwrap_or(rel_path);
    let dotted = without_ext.replace('/', ".");
    // __init__ modules represent the package itself
    dotted
        .strip_suffix(".__init__")
        .unwrap_or(&dotted)
        .to_string()
}

fn extract_python(
    root: &Node,
    source: &str,
    file: &str,
    is_test: bool,
    fragment: &mut ManifestFragment,
) {
    let module_name = module_path_from_file(file);
    let module = Module {
        path: module_name.clone(),
        file: file.to_string(),
        ..Default::default()
    };
    fragment.modules.insert(module_name.clone(), module);

    // Walk top-level statements
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "class_definition" => {
                extract_class(&child, source, file, &module_name, fragment);
            }
            "function_definition" => {
                extract_function(&child, source, file, &module_name, is_test, fragment);
            }
            "decorated_definition" => {
                extract_decorated(&child, source, file, &module_name, is_test, fragment);
            }
            _ => {}
        }
    }
}

fn extract_class(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let full_name = format!("{}.{}", module_name, name);

    // Extract base classes from superclasses (argument_list)
    let mut implements = Vec::new();
    if let Some(superclasses) = node.child_by_field_name("superclasses") {
        let mut cursor = superclasses.walk();
        for child in superclasses.named_children(&mut cursor) {
            let text = node_text(&child, source);
            // Skip keyword arguments like metaclass=ABCMeta
            if !text.contains('=') && !text.is_empty() {
                implements.push(text);
            }
        }
    }

    let visibility = if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    };

    // Extract methods from the class body
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        extract_class_methods(&body, source, file, &full_name, fragment, &mut methods);
    }

    let type_def = TypeDef {
        name: name.clone(),
        source: file.to_string(),
        kind: TypeKind::Class,
        visibility,
        implements,
        methods,
        ..Default::default()
    };
    fragment.types.insert(full_name, type_def);
}

fn extract_class_methods(
    body: &Node,
    source: &str,
    file: &str,
    class_name: &str,
    fragment: &mut ManifestFragment,
    methods: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                let method_name = extract_method(&child, source, file, class_name, &[], fragment);
                if let Some(name) = method_name {
                    methods.push(name);
                }
            }
            "decorated_definition" => {
                let decorators = collect_decorators(&child, source);
                if let Some(func_node) = child.child_by_field_name("definition") {
                    if func_node.kind() == "function_definition" {
                        let method_name = extract_method(
                            &func_node,
                            source,
                            file,
                            class_name,
                            &decorators,
                            fragment,
                        );
                        if let Some(name) = method_name {
                            methods.push(name);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract a method from a class body, returning its name.
fn extract_method(
    node: &Node,
    source: &str,
    file: &str,
    class_name: &str,
    decorators: &[String],
    fragment: &mut ManifestFragment,
) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    let method_name = node_text(&name, source);

    let visibility = if method_name.starts_with('_') && !method_name.starts_with("__") {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let is_async =
        node.kind() == "function_definition" && source[node.byte_range()].starts_with("async ");

    let params = node
        .child_by_field_name("parameters")
        .map(|p| node_text(&p, source))
        .unwrap_or_default();

    let decorator_prefix = decorators
        .iter()
        .map(|d| format!("@{} ", d))
        .collect::<String>();
    let async_prefix = if is_async { "async " } else { "" };
    let signature = format!(
        "{}{}def {}{}",
        decorator_prefix, async_prefix, method_name, params
    );

    let qualified = format!("{}.{}", class_name, method_name);

    let function = Function {
        name: method_name.clone(),
        source: file.to_string(),
        signature,
        visibility,
        is_async,
        ..Default::default()
    };
    fragment.functions.insert(qualified, function);

    Some(method_name)
}

fn extract_function(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    is_test: bool,
    fragment: &mut ManifestFragment,
) {
    extract_function_inner(node, source, file, module_name, is_test, &[], fragment);
}

fn extract_function_inner(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    is_test: bool,
    decorators: &[String],
    fragment: &mut ManifestFragment,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, source),
        None => return,
    };

    let visibility = if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let is_async = source[node.byte_range()].starts_with("async ");
    let func_is_test = is_test || name.starts_with("test_");

    let params = node
        .child_by_field_name("parameters")
        .map(|p| node_text(&p, source))
        .unwrap_or_default();

    let decorator_prefix = decorators
        .iter()
        .map(|d| format!("@{} ", d))
        .collect::<String>();
    let async_prefix = if is_async { "async " } else { "" };
    let signature = format!("{}{}def {}{}", decorator_prefix, async_prefix, name, params);

    let qualified = format!("{}.{}", module_name, name);

    let function = Function {
        name,
        source: file.to_string(),
        signature,
        visibility,
        is_async,
        is_test: func_is_test,
    };
    fragment.functions.insert(qualified, function);
}

fn extract_decorated(
    node: &Node,
    source: &str,
    file: &str,
    module_name: &str,
    is_test: bool,
    fragment: &mut ManifestFragment,
) {
    let decorators = collect_decorators(node, source);

    if let Some(definition) = node.child_by_field_name("definition") {
        match definition.kind() {
            "function_definition" => {
                extract_function_inner(
                    &definition,
                    source,
                    file,
                    module_name,
                    is_test,
                    &decorators,
                    fragment,
                );
            }
            "class_definition" => {
                extract_class(&definition, source, file, module_name, fragment);
            }
            _ => {}
        }
    }
}

fn collect_decorators(node: &Node, source: &str) -> Vec<String> {
    let mut decorators = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "decorator" {
            // The decorator text includes @, strip it
            let text = node_text(&child, source);
            let name = text.strip_prefix('@').unwrap_or(&text).trim().to_string();
            decorators.push(name);
        }
    }
    decorators
}

fn node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_python(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_python(
            &tree.root_node(),
            source,
            "myapp/models.py",
            false,
            &mut fragment,
        );
        fragment
    }

    fn parse_python_test(source: &str) -> ManifestFragment {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut fragment = ManifestFragment::default();
        extract_python(
            &tree.root_node(),
            source,
            "tests/test_models.py",
            true,
            &mut fragment,
        );
        fragment
    }

    #[test]
    fn class_detected() {
        let frag = parse_python(
            r#"
class User:
    def __init__(self, name):
        self.name = name
"#,
        );

        assert!(frag.types.contains_key("myapp.models.User"));
        let td = &frag.types["myapp.models.User"];
        assert_eq!(td.kind, TypeKind::Class);
        assert_eq!(td.visibility, Visibility::Public);
        assert_eq!(td.source, "myapp/models.py");
    }

    #[test]
    fn class_inheritance() {
        let frag = parse_python(
            r#"
class Admin(User, Serializable):
    pass
"#,
        );

        let td = &frag.types["myapp.models.Admin"];
        assert!(td.implements.contains(&"User".to_string()));
        assert!(td.implements.contains(&"Serializable".to_string()));
    }

    #[test]
    fn module_level_function() {
        let frag = parse_python(
            r#"
def create_user(name, email):
    return User(name, email)
"#,
        );

        assert!(frag.functions.contains_key("myapp.models.create_user"));
        let func = &frag.functions["myapp.models.create_user"];
        assert_eq!(func.name, "create_user");
        assert_eq!(func.visibility, Visibility::Public);
    }

    #[test]
    fn private_function_by_underscore() {
        let frag = parse_python(
            r#"
def _validate_email(email):
    pass
"#,
        );

        let func = &frag.functions["myapp.models._validate_email"];
        assert_eq!(func.visibility, Visibility::Private);
    }

    #[test]
    fn async_function_detected() {
        let frag = parse_python(
            r#"
async def fetch_data(url):
    pass
"#,
        );

        let func = &frag.functions["myapp.models.fetch_data"];
        assert!(func.is_async);
    }

    #[test]
    fn class_methods_tracked() {
        let frag = parse_python(
            r#"
class Service:
    def process(self):
        pass

    def _internal(self):
        pass
"#,
        );

        let td = &frag.types["myapp.models.Service"];
        assert!(td.methods.contains(&"process".to_string()));
        assert!(td.methods.contains(&"_internal".to_string()));

        let process = &frag.functions["myapp.models.Service.process"];
        assert_eq!(process.visibility, Visibility::Public);

        let internal = &frag.functions["myapp.models.Service._internal"];
        assert_eq!(internal.visibility, Visibility::Private);
    }

    #[test]
    fn decorated_function() {
        let frag = parse_python(
            r#"
class Config:
    @property
    def name(self):
        return self._name

    @staticmethod
    def create():
        pass
"#,
        );

        let td = &frag.types["myapp.models.Config"];
        assert!(td.methods.contains(&"name".to_string()));
        assert!(td.methods.contains(&"create".to_string()));

        let name_func = &frag.functions["myapp.models.Config.name"];
        assert!(name_func.signature.contains("@property"));

        let create_func = &frag.functions["myapp.models.Config.create"];
        assert!(create_func.signature.contains("@staticmethod"));
    }

    #[test]
    fn test_file_marks_functions_as_test() {
        let frag = parse_python_test(
            r#"
def test_create_user():
    pass

def helper():
    pass
"#,
        );

        let test_func = frag
            .functions
            .values()
            .find(|f| f.name == "test_create_user")
            .unwrap();
        assert!(test_func.is_test);

        let helper_func = frag
            .functions
            .values()
            .find(|f| f.name == "helper")
            .unwrap();
        // In a test file, all functions are marked is_test
        assert!(helper_func.is_test);
    }

    #[test]
    fn module_created_from_file() {
        let frag = parse_python("");

        assert!(frag.modules.contains_key("myapp.models"));
        let module = &frag.modules["myapp.models"];
        assert_eq!(module.file, "myapp/models.py");
        assert_eq!(module.path, "myapp.models");
    }

    #[test]
    fn test_file_detection() {
        assert!(is_test_file("tests/test_models.py"));
        assert!(is_test_file("test_main.py"));
        assert!(is_test_file("models_test.py"));
        assert!(!is_test_file("models.py"));
        assert!(!is_test_file("testing.py"));
    }

    #[test]
    fn private_class_detected() {
        let frag = parse_python(
            r#"
class _InternalHelper:
    pass
"#,
        );

        let td = &frag.types["myapp.models._InternalHelper"];
        assert_eq!(td.visibility, Visibility::Private);
    }

    #[test]
    fn detect_returns_correct_values() {
        let adapter = PythonAdapter;

        let python_ctx = ProjectContext {
            root: std::path::PathBuf::from("/tmp"),
            kind: ProjectKind::Python { uv: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(adapter.detect(&python_ctx));

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
