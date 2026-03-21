//! `uu map tree` — show module hierarchy as an ASCII tree.

use anyhow::Result;
use std::collections::BTreeMap;

use super::format::{bold, cyan, dim, green};
use super::CommonArgs;

/// A node in the module tree.
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    type_count: usize,
    fn_count: usize,
    file: String,
}

impl TreeNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            type_count: 0,
            fn_count: 0,
            file: String::new(),
        }
    }

    fn insert(&mut self, path: &[&str], type_count: usize, fn_count: usize, file: &str) {
        if path.is_empty() {
            self.type_count = type_count;
            self.fn_count = fn_count;
            self.file = file.to_string();
            return;
        }
        let child = self
            .children
            .entry(path[0].to_string())
            .or_insert_with(TreeNode::new);
        child.insert(&path[1..], type_count, fn_count, file);
    }
}

pub(crate) fn execute(args: CommonArgs) -> Result<()> {
    let root = super::resolve_root(args.path.as_ref())?;
    let manifest = super::build_manifest(&root, args.all)?;

    // Build the tree from module paths
    let mut tree = TreeNode::new();

    for (mod_name, module) in &manifest.modules {
        // Match types/functions whose source starts with this module's file
        let file_prefix = &module.file;

        let type_count = if file_prefix.is_empty() {
            0
        } else {
            manifest
                .types
                .values()
                .filter(|t| !t.source.is_empty() && t.source.starts_with(file_prefix))
                .count()
        };

        let fn_count = if file_prefix.is_empty() {
            0
        } else {
            manifest
                .functions
                .values()
                .filter(|f| !f.source.is_empty() && f.source.starts_with(file_prefix))
                .count()
        };

        let parts: Vec<&str> = mod_name.split("::").collect();
        tree.insert(&parts, type_count, fn_count, &module.file);
    }

    // Print header
    println!(
        "\n{} {}",
        bold(&manifest.project.name),
        dim(&format!("({})", manifest.project.kind)),
    );

    // Render the tree
    render_tree(&tree, "", true);
    println!();

    // Summary footer
    let total_types = manifest.types.len();
    let total_fns = manifest.functions.len();
    let total_modules = manifest.modules.len();
    println!(
        "{}",
        dim(&format!(
            "{total_modules} modules, {total_types} types, {total_fns} functions"
        ))
    );

    Ok(())
}

fn render_tree(node: &TreeNode, prefix: &str, is_root: bool) {
    let entries: Vec<(&String, &TreeNode)> = node.children.iter().collect();
    let count = entries.len();

    for (i, (name, child)) in entries.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_root {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };
        let child_prefix = if is_root {
            ""
        } else if is_last {
            "    "
        } else {
            "│   "
        };

        // Build the annotation (type/function counts)
        let mut annotations = Vec::new();
        if child.type_count > 0 {
            annotations.push(cyan(&format!(
                "{} type{}",
                child.type_count,
                if child.type_count == 1 { "" } else { "s" }
            )));
        }
        if child.fn_count > 0 {
            annotations.push(green(&format!(
                "{} fn{}",
                child.fn_count,
                if child.fn_count == 1 { "" } else { "s" }
            )));
        }

        let annotation = if annotations.is_empty() {
            String::new()
        } else {
            format!(" {}", dim(&format!("({})", annotations.join(", "))))
        };

        let display_name = if child.children.is_empty() {
            name.to_string()
        } else {
            bold(name)
        };

        println!("{prefix}{connector}{display_name}{annotation}");

        if !child.children.is_empty() {
            let new_prefix = format!("{prefix}{child_prefix}");
            render_tree(child, &new_prefix, false);
        }
    }
}
