//! Adapter trait and registry for manifest extraction.

pub mod framework;
pub mod lang;

use anyhow::Result;

use crate::context::ProjectContext;
use crate::schema::ManifestFragment;

/// Whether an adapter provides language-level or framework-level extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterLayer {
    /// Always runs when the project language matches.
    Language,
    /// Only runs when the specific framework is detected.
    Framework,
}

/// An adapter that extracts symbols and semantics from a project.
///
/// Language adapters run for every project of that language (extracting types,
/// functions, modules). Framework adapters run only when the framework is
/// detected (extracting routes, models, endpoints).
pub trait Adapter: Send + Sync {
    /// Human-readable name (e.g. "rust", "next.js").
    fn name(&self) -> &str;

    /// Returns `true` if this adapter should run for the given project.
    fn detect(&self, ctx: &ProjectContext) -> bool;

    /// Extract a manifest fragment from the project.
    fn extract(&self, ctx: &ProjectContext) -> Result<ManifestFragment>;

    /// Priority — higher values run first. Language adapters typically use
    /// 100, framework adapters use 50.
    fn priority(&self) -> u32;

    /// Which layer this adapter belongs to.
    fn layer(&self) -> AdapterLayer;
}

/// Returns all registered adapters, sorted by priority (highest first).
pub fn all_adapters() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(lang::rust::RustAdapter),
        Box::new(lang::go::GoAdapter),
        Box::new(lang::python::PythonAdapter),
        Box::new(lang::typescript::TypeScriptAdapter),
        Box::new(lang::javascript::JavaScriptAdapter),
        Box::new(lang::elixir::ElixirAdapter),
        Box::new(lang::java::JavaAdapter),
        Box::new(lang::ruby::RubyAdapter),
        Box::new(lang::swift::SwiftAdapter),
        Box::new(lang::csharp::CSharpAdapter),
        Box::new(lang::c_cpp::CCppAdapter),
        Box::new(lang::zig::ZigAdapter),
        Box::new(framework::nextjs::NextJsAdapter),
        Box::new(framework::express::ExpressAdapter),
        Box::new(framework::prisma::PrismaAdapter),
        Box::new(framework::shadcn::ShadcnAdapter),
        Box::new(framework::authjs::AuthJsAdapter),
        Box::new(framework::django::DjangoAdapter),
        Box::new(framework::fastapi::FastApiAdapter),
        Box::new(framework::axum::AxumAdapter),
        Box::new(framework::phoenix::PhoenixAdapter),
        Box::new(framework::ecto::EctoAdapter),
        Box::new(framework::rails::RailsAdapter),
        Box::new(framework::spring::SpringAdapter),
        Box::new(framework::gin::GinAdapter),
        Box::new(framework::gorm::GormAdapter),
        Box::new(framework::aspnet::AspNetAdapter),
    ]
}
