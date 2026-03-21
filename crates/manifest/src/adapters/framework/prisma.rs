//! Prisma adapter — extracts data models, fields, relations, and enums from schema.prisma.

use anyhow::Result;

use crate::adapters::{Adapter, AdapterLayer};
use crate::context::ProjectContext;
use crate::schema::{
    DataModel, Field, ManifestFragment, Relation, RelationKind, TypeDef, TypeKind,
};

pub struct PrismaAdapter;

impl Adapter for PrismaAdapter {
    fn name(&self) -> &str {
        "prisma"
    }

    fn detect(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("prisma/schema.prisma").exists() || ctx.root.join("schema.prisma").exists()
    }

    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment> {
        let schema_path = if ctx.root.join("prisma/schema.prisma").exists() {
            ctx.root.join("prisma/schema.prisma")
        } else {
            ctx.root.join("schema.prisma")
        };

        let source = std::fs::read_to_string(&schema_path)?;
        let rel_path = schema_path
            .strip_prefix(&ctx.root)
            .unwrap_or(&schema_path)
            .to_string_lossy()
            .to_string();

        Ok(parse_prisma_schema(&source, &rel_path))
    }

    fn priority(&self) -> u32 {
        50
    }

    fn layer(&self) -> AdapterLayer {
        AdapterLayer::Framework
    }
}

/// Parse a Prisma schema file line-by-line.
fn parse_prisma_schema(source: &str, file: &str) -> ManifestFragment {
    let mut fragment = ManifestFragment::default();
    let mut current_model: Option<ModelBuilder> = None;
    let mut current_enum: Option<EnumBuilder> = None;

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Start of a model block
        if trimmed.starts_with("model ") {
            if let Some(name) = extract_block_name(trimmed, "model") {
                current_model = Some(ModelBuilder {
                    name: name.to_string(),
                    fields: Vec::new(),
                    relations: Vec::new(),
                    indexes: Vec::new(),
                });
            }
            continue;
        }

        // Start of an enum block
        if trimmed.starts_with("enum ") {
            if let Some(name) = extract_block_name(trimmed, "enum") {
                current_enum = Some(EnumBuilder {
                    name: name.to_string(),
                    variants: Vec::new(),
                });
            }
            continue;
        }

        // End of block
        if trimmed == "}" {
            if let Some(model) = current_model.take() {
                let data_model = DataModel {
                    name: model.name.clone(),
                    source: file.to_string(),
                    orm: "prisma".to_string(),
                    fields: model.fields,
                    relations: model.relations,
                    indexes: model.indexes,
                };
                fragment.models.insert(model.name, data_model);
            }
            if let Some(enum_b) = current_enum.take() {
                let type_def = TypeDef {
                    name: enum_b.name.clone(),
                    source: file.to_string(),
                    kind: TypeKind::Enum,
                    variants: enum_b.variants,
                    ..Default::default()
                };
                fragment.types.insert(enum_b.name, type_def);
            }
            continue;
        }

        // Inside a model block — parse fields
        if let Some(ref mut model) = current_model {
            parse_model_field(trimmed, model);
            continue;
        }

        // Inside an enum block — collect variants
        if let Some(ref mut enum_b) = current_enum {
            // Each non-empty, non-comment line is a variant
            let variant = trimmed.split_whitespace().next().unwrap_or("");
            if !variant.is_empty() && !variant.starts_with("@@") {
                enum_b.variants.push(variant.to_string());
            }
        }
    }

    fragment
}

struct ModelBuilder {
    name: String,
    fields: Vec<Field>,
    relations: Vec<Relation>,
    indexes: Vec<String>,
}

struct EnumBuilder {
    name: String,
    variants: Vec<String>,
}

/// Extract the block name from `model Foo {` or `enum Bar {`.
fn extract_block_name<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?.trim();
    let name = rest.split(|c: char| c == '{' || c.is_whitespace()).next()?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Parse a single field line inside a model block.
fn parse_model_field(line: &str, model: &mut ModelBuilder) {
    // Handle @@index, @@unique, @@map, etc.
    if line.starts_with("@@") {
        if line.starts_with("@@index") || line.starts_with("@@unique") {
            // Extract the fields list: @@index([field1, field2])
            if let Some(start) = line.find('[') {
                if let Some(end) = line.find(']') {
                    let index_str = &line[start..=end];
                    model.indexes.push(index_str.to_string());
                }
            }
        }
        return;
    }

    // Regular field: name Type? @attribute...
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let field_name = parts[0];
    // Skip if it looks like a block-level annotation
    if field_name.starts_with('@') {
        return;
    }

    let raw_type = parts[1];
    let optional = raw_type.ends_with('?');
    let is_array = raw_type.ends_with("[]");
    let clean_type = raw_type
        .trim_end_matches('?')
        .trim_end_matches("[]")
        .to_string();

    // Check if this is a relation field
    let has_relation = line.contains("@relation");

    if has_relation {
        let kind = if is_array {
            RelationKind::HasMany
        } else {
            RelationKind::BelongsTo
        };
        model.relations.push(Relation {
            name: field_name.to_string(),
            kind,
            target: clean_type,
        });
    } else {
        model.fields.push(Field {
            name: field_name.to_string(),
            type_name: clean_type,
            optional,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prisma_in_subdir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("prisma")).unwrap();
        std::fs::write(dir.path().join("prisma/schema.prisma"), "").unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(PrismaAdapter.detect(&ctx));
    }

    #[test]
    fn detect_prisma_at_root() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("schema.prisma"), "").unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };
        assert!(PrismaAdapter.detect(&ctx));
    }

    #[test]
    fn parse_models_fields_relations() {
        let schema = r#"
model User {
  id        Int      @id @default(autoincrement())
  email     String   @unique
  name      String?
  posts     Post[]   @relation("UserPosts")
  profile   Profile? @relation("UserProfile")

  @@index([email])
}

model Post {
  id       Int    @id @default(autoincrement())
  title    String
  author   User   @relation("UserPosts", fields: [authorId], references: [id])
  authorId Int
}
"#;

        let frag = parse_prisma_schema(schema, "prisma/schema.prisma");

        // User model
        assert!(frag.models.contains_key("User"));
        let user = &frag.models["User"];
        assert_eq!(user.orm, "prisma");
        assert_eq!(user.source, "prisma/schema.prisma");

        // User fields
        let field_names: Vec<&str> = user.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"email"));
        assert!(field_names.contains(&"name"));

        // name is optional
        let name_field = user.fields.iter().find(|f| f.name == "name").unwrap();
        assert!(name_field.optional);

        // User relations
        assert_eq!(user.relations.len(), 2);
        let posts_rel = user.relations.iter().find(|r| r.name == "posts").unwrap();
        assert_eq!(posts_rel.target, "Post");
        assert_eq!(posts_rel.kind, RelationKind::HasMany);

        let profile_rel = user.relations.iter().find(|r| r.name == "profile").unwrap();
        assert_eq!(profile_rel.target, "Profile");
        assert_eq!(profile_rel.kind, RelationKind::BelongsTo);

        // Indexes
        assert_eq!(user.indexes.len(), 1);
        assert!(user.indexes[0].contains("email"));

        // Post model
        assert!(frag.models.contains_key("Post"));
        let post = &frag.models["Post"];
        let post_field_names: Vec<&str> = post.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(post_field_names.contains(&"title"));
        assert!(post_field_names.contains(&"authorId"));

        // Post relation
        let author_rel = post.relations.iter().find(|r| r.name == "author").unwrap();
        assert_eq!(author_rel.target, "User");
        assert_eq!(author_rel.kind, RelationKind::BelongsTo);
    }

    #[test]
    fn parse_enums() {
        let schema = r#"
enum Role {
  USER
  ADMIN
  MODERATOR
}
"#;

        let frag = parse_prisma_schema(schema, "schema.prisma");

        assert!(frag.types.contains_key("Role"));
        let role = &frag.types["Role"];
        assert_eq!(role.kind, TypeKind::Enum);
        assert_eq!(role.variants.len(), 3);
        assert!(role.variants.contains(&"USER".to_string()));
        assert!(role.variants.contains(&"ADMIN".to_string()));
        assert!(role.variants.contains(&"MODERATOR".to_string()));
    }

    #[test]
    fn full_extract_from_tempdir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("prisma")).unwrap();
        std::fs::write(
            dir.path().join("prisma/schema.prisma"),
            r#"
model Item {
  id   Int    @id
  name String
}
"#,
        )
        .unwrap();

        let ctx = ProjectContext {
            root: dir.path().to_path_buf(),
            kind: uu_detect::ProjectKind::Node {
                manager: uu_detect::NodePM::Npm,
            },
            files: vec![],
            package_json: None,
            cargo_toml: None,
            go_mod: None,
        };

        let frag = PrismaAdapter.extract(&ctx).unwrap();
        assert!(frag.models.contains_key("Item"));
    }
}
