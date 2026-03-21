//! Manifest schema types — the data model for project manifests.
//!
//! All types are serde-serializable and designed for clean YAML output.
//! Empty collections and `None` values are omitted during serialization.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn is_false(v: &bool) -> bool {
    !*v
}

// -- Top-level manifest ------------------------------------------------------

/// A complete project manifest describing all discovered symbols, routes,
/// models, and integrations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub project: ProjectMeta,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: BTreeMap<String, TypeDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub functions: BTreeMap<String, Function>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, Module>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub routes: BTreeMap<String, Route>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, Endpoint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, DataModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<Integration>,
}

/// Metadata about the project and how the manifest was generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectMeta {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frameworks: Vec<String>,
    pub generated_at: String,
    pub uu_version: String,
}

// -- Fragment (adapter output) -----------------------------------------------

/// Partial manifest produced by a single adapter. Merged into a full
/// [`Manifest`] after all adapters run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ManifestFragment {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: BTreeMap<String, TypeDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub functions: BTreeMap<String, Function>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, Module>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub routes: BTreeMap<String, Route>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, Endpoint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, DataModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<Integration>,
}

// -- Types -------------------------------------------------------------------

/// A type definition (struct, class, enum, trait, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TypeDef {
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "TypeKind::is_default")]
    pub kind: TypeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Visibility::is_public")]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
}

/// Classification of a type definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TypeKind {
    #[default]
    Struct,
    Class,
    Interface,
    Enum,
    Trait,
    Protocol,
    Union,
    TypeAlias,
}

impl TypeKind {
    pub fn is_default(&self) -> bool {
        *self == TypeKind::Struct
    }
}

// -- Functions ---------------------------------------------------------------

/// A function or method extracted from source code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Function {
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    #[serde(default, skip_serializing_if = "Visibility::is_public")]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_async: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_test: bool,
}

// -- Modules -----------------------------------------------------------------

/// A module or file with its exports and imports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Module {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
}

// -- Data models -------------------------------------------------------------

/// An ORM/database model with fields and relations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DataModel {
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub orm: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<String>,
}

/// A field on a type or model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Field {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}

/// A relation between data models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Relation {
    pub name: String,
    #[serde(default)]
    pub kind: RelationKind,
    pub target: String,
}

/// The cardinality of a data model relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RelationKind {
    #[default]
    HasOne,
    HasMany,
    BelongsTo,
    ManyToMany,
}

// -- Routes and endpoints ----------------------------------------------------

/// A route (page, layout, API route, controller, middleware).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Route {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default)]
    pub route_type: RouteType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

/// Classification of a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RouteType {
    #[default]
    Page,
    Layout,
    ApiRoute,
    Controller,
    Middleware,
}

/// An API endpoint with method, handler, and middleware.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Endpoint {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub method: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub handler: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub middleware: Vec<String>,
}

// -- Components and integrations ---------------------------------------------

/// A UI component (React, Vue, Svelte, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Component {
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Field>,
}

/// An external service integration (database, queue, API, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Integration {
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<String>,
}

/// Authentication configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strategy: String,
}

// -- Shared enums ------------------------------------------------------------

/// Symbol visibility level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Internal,
}

impl Visibility {
    pub fn is_public(&self) -> bool {
        *self == Visibility::Public
    }
}
