//! Ecto adapter — extracts data models from Ecto.Schema modules.

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{DataModel, Field, ManifestFragment, Relation, RelationKind};

pub struct EctoAdapter;

impl Adapter for EctoAdapter {
    fn name(&self) -> &str {
        "ecto"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        let mix_path = ctx.root.join("mix.exs");
        let content = match std::fs::read_to_string(mix_path) {
            Ok(s) => s,
            Err(_) => return false,
        };
        content.contains(":ecto_sql") || content.contains(":ecto,") || content.contains(":ecto}")
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let mut fragment = ManifestFragment::default();

        for file in &ctx.files {
            if file
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| e != "ex")
            {
                continue;
            }

            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Only parse files that use Ecto.Schema
            if !source.contains("Ecto.Schema") && !source.contains("schema ") {
                continue;
            }

            let rel = file
                .strip_prefix(&ctx.root)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();

            extract_ecto_schemas(&source, &rel, &mut fragment);
        }

        Ok(fragment)
    }

    fn priority(&self) -> u32 {
        50
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

/// Parse Ecto schema definitions from an Elixir source file.
fn extract_ecto_schemas(source: &str, file: &str, fragment: &mut ManifestFragment) {
    let mut current_module: Option<String> = None;
    let mut in_schema_block = false;
    let mut table_name = String::new();
    let mut fields: Vec<Field> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Track module name: `defmodule MyApp.User do`
        if let Some(rest) = trimmed.strip_prefix("defmodule ") {
            let rest = rest.trim();
            let name = rest.split(|c: char| c.is_whitespace()).next().unwrap_or("");
            if !name.is_empty() {
                current_module = Some(name.to_string());
            }
            continue;
        }

        // Start of schema block: `schema "users" do`
        if trimmed.starts_with("schema ") && trimmed.contains(" do") {
            if let Some(tbl) = extract_quoted_string(trimmed) {
                table_name = tbl;
                in_schema_block = true;
                fields.clear();
                relations.clear();
            }
            continue;
        }

        // End of schema block
        if in_schema_block && trimmed == "end" {
            let model_name = current_module.as_deref().unwrap_or(&table_name).to_string();

            // Use the short name (last segment after dots)
            let short_name = model_name
                .rsplit('.')
                .next()
                .unwrap_or(&model_name)
                .to_string();

            let model = DataModel {
                name: short_name.clone(),
                source: file.to_string(),
                orm: "ecto".to_string(),
                fields: std::mem::take(&mut fields),
                relations: std::mem::take(&mut relations),
                indexes: vec![],
            };
            fragment.models.insert(short_name, model);
            in_schema_block = false;
            continue;
        }

        if !in_schema_block {
            continue;
        }

        // Parse field: `field :name, :string`
        if trimmed.starts_with("field ") {
            if let Some((name, type_name)) = parse_field_line(trimmed) {
                fields.push(Field {
                    name,
                    type_name,
                    optional: false,
                });
            }
            continue;
        }

        // Parse associations
        if trimmed.starts_with("has_many ") {
            if let Some((name, target)) = parse_assoc_line(trimmed, "has_many") {
                relations.push(Relation {
                    name,
                    kind: RelationKind::HasMany,
                    target,
                });
            }
        } else if trimmed.starts_with("has_one ") {
            if let Some((name, target)) = parse_assoc_line(trimmed, "has_one") {
                relations.push(Relation {
                    name,
                    kind: RelationKind::HasOne,
                    target,
                });
            }
        } else if trimmed.starts_with("belongs_to ") {
            if let Some((name, target)) = parse_assoc_line(trimmed, "belongs_to") {
                relations.push(Relation {
                    name,
                    kind: RelationKind::BelongsTo,
                    target,
                });
            }
        } else if trimmed.starts_with("many_to_many ") {
            if let Some((name, target)) = parse_assoc_line(trimmed, "many_to_many") {
                relations.push(Relation {
                    name,
                    kind: RelationKind::ManyToMany,
                    target,
                });
            }
        }
    }
}

/// Parse `field :name, :type` returning (name, type_name).
fn parse_field_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("field ")?.trim();
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.is_empty() {
        return None;
    }

    let name = parts[0].trim().trim_start_matches(':').to_string();
    let type_name = if parts.len() > 1 {
        parts[1].trim().trim_start_matches(':').to_string()
    } else {
        "string".to_string()
    };

    Some((name, type_name))
}

/// Parse an association line like `has_many :posts, Post`.
fn parse_assoc_line(line: &str, keyword: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix(keyword)?.trim();
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].trim().trim_start_matches(':').to_string();
    let target = parts[1].trim().to_string();

    Some((name, target))
}

/// Extract a double-quoted string from a line.
fn extract_quoted_string(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let end = s[start + 1..].find('"')?;
    Some(s[start + 1..start + 1 + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_ecto() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mix.exs"),
            r#"
defmodule MyApp.MixProject do
  defp deps do
    [{:ecto_sql, "~> 3.10"}, {:postgrex, ">= 0.0.0"}]
  end
end
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(EctoAdapter.detect(&ctx));
    }

    #[test]
    fn no_detect_without_ecto() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mix.exs"),
            r#"
defmodule MyApp.MixProject do
  defp deps do
    [{:phoenix, "~> 1.7"}]
  end
end
"#,
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(!EctoAdapter.detect(&ctx));
    }

    #[test]
    fn extract_schema_fields() {
        let source = r#"
defmodule MyApp.User do
  use Ecto.Schema

  schema "users" do
    field :name, :string
    field :email, :string
    field :age, :integer
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_ecto_schemas(source, "lib/my_app/user.ex", &mut fragment);

        assert!(fragment.models.contains_key("User"));
        let model = &fragment.models["User"];
        assert_eq!(model.orm, "ecto");

        let field_names: Vec<&str> = model.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"email"));
        assert!(field_names.contains(&"age"));

        let age_field = model.fields.iter().find(|f| f.name == "age").unwrap();
        assert_eq!(age_field.type_name, "integer");
    }

    #[test]
    fn extract_associations() {
        let source = r#"
defmodule MyApp.Post do
  use Ecto.Schema

  schema "posts" do
    field :title, :string
    belongs_to :author, MyApp.User
    has_many :comments, MyApp.Comment
    many_to_many :tags, MyApp.Tag
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_ecto_schemas(source, "lib/my_app/post.ex", &mut fragment);

        assert!(fragment.models.contains_key("Post"));
        let model = &fragment.models["Post"];

        assert_eq!(model.relations.len(), 3);

        let author = model.relations.iter().find(|r| r.name == "author").unwrap();
        assert_eq!(author.kind, RelationKind::BelongsTo);
        assert_eq!(author.target, "MyApp.User");

        let comments = model
            .relations
            .iter()
            .find(|r| r.name == "comments")
            .unwrap();
        assert_eq!(comments.kind, RelationKind::HasMany);

        let tags = model.relations.iter().find(|r| r.name == "tags").unwrap();
        assert_eq!(tags.kind, RelationKind::ManyToMany);
    }

    #[test]
    fn has_one_association() {
        let source = r#"
defmodule MyApp.User do
  use Ecto.Schema

  schema "users" do
    field :name, :string
    has_one :profile, MyApp.Profile
  end
end
"#;
        let mut fragment = ManifestFragment::default();
        extract_ecto_schemas(source, "lib/my_app/user.ex", &mut fragment);

        let model = &fragment.models["User"];
        let profile = model
            .relations
            .iter()
            .find(|r| r.name == "profile")
            .unwrap();
        assert_eq!(profile.kind, RelationKind::HasOne);
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        let lib = dir.path().join("lib/my_app");
        std::fs::create_dir_all(&lib).unwrap();

        std::fs::write(
            dir.path().join("mix.exs"),
            "defmodule MyApp do\n  defp deps do\n    [{:ecto_sql, \"~> 3.10\"}]\n  end\nend\n",
        )
        .unwrap();

        std::fs::write(
            lib.join("user.ex"),
            "defmodule MyApp.User do\n  use Ecto.Schema\n  schema \"users\" do\n    field :name, :string\n  end\nend\n",
        )
        .unwrap();

        let ctx = crate::context::ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Elixir { escript: false },
            files: vec![lib.join("user.ex")],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        assert!(EctoAdapter.detect(&ctx));
        let frag = EctoAdapter.extract(&ctx).unwrap();
        assert!(frag.models.contains_key("User"));
    }
}
