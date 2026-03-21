//! Manifest diffing — compare two manifests to find what changed.

use std::collections::BTreeMap;
use std::fmt;

use crate::schema::*;

/// Diff between two manifests, broken down by section.
#[derive(Debug, Clone, Default)]
pub struct ManifestDiff {
    pub types: SectionDiff<TypeDef>,
    pub functions: SectionDiff<Function>,
    pub modules: SectionDiff<Module>,
    pub routes: SectionDiff<Route>,
    pub endpoints: SectionDiff<Endpoint>,
    pub models: SectionDiff<DataModel>,
    pub auth_changed: bool,
    pub components_added: Vec<Component>,
    pub components_removed: Vec<Component>,
    pub integrations_added: Vec<Integration>,
    pub integrations_removed: Vec<Integration>,
}

/// Added, removed, and changed entries for a keyed section.
#[derive(Debug, Clone)]
pub struct SectionDiff<T> {
    pub added: BTreeMap<String, T>,
    pub removed: BTreeMap<String, T>,
    pub changed: BTreeMap<String, (T, T)>,
}

impl<T> Default for SectionDiff<T> {
    fn default() -> Self {
        Self {
            added: BTreeMap::new(),
            removed: BTreeMap::new(),
            changed: BTreeMap::new(),
        }
    }
}

impl ManifestDiff {
    /// Returns `true` if there are no differences.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
            && self.functions.is_empty()
            && self.modules.is_empty()
            && self.routes.is_empty()
            && self.endpoints.is_empty()
            && self.models.is_empty()
            && !self.auth_changed
            && self.components_added.is_empty()
            && self.components_removed.is_empty()
            && self.integrations_added.is_empty()
            && self.integrations_removed.is_empty()
    }
}

impl<T> SectionDiff<T> {
    /// Returns `true` if this section has no changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Compare two manifests and produce a structured diff.
pub fn diff(old: &Manifest, new: &Manifest) -> ManifestDiff {
    ManifestDiff {
        types: diff_maps(&old.types, &new.types),
        functions: diff_maps(&old.functions, &new.functions),
        modules: diff_maps(&old.modules, &new.modules),
        routes: diff_maps(&old.routes, &new.routes),
        endpoints: diff_maps(&old.endpoints, &new.endpoints),
        models: diff_maps(&old.models, &new.models),
        auth_changed: old.auth != new.auth,
        components_added: new
            .components
            .iter()
            .filter(|c| !old.components.contains(c))
            .cloned()
            .collect(),
        components_removed: old
            .components
            .iter()
            .filter(|c| !new.components.contains(c))
            .cloned()
            .collect(),
        integrations_added: new
            .integrations
            .iter()
            .filter(|i| !old.integrations.contains(i))
            .cloned()
            .collect(),
        integrations_removed: old
            .integrations
            .iter()
            .filter(|i| !new.integrations.contains(i))
            .cloned()
            .collect(),
    }
}

/// Diff two `BTreeMap` sections, producing added/removed/changed entries.
fn diff_maps<T: PartialEq + Clone>(
    old: &BTreeMap<String, T>,
    new: &BTreeMap<String, T>,
) -> SectionDiff<T> {
    let mut result = SectionDiff::default();

    for (key, old_val) in old {
        match new.get(key) {
            None => {
                result.removed.insert(key.clone(), old_val.clone());
            }
            Some(new_val) if old_val != new_val => {
                result
                    .changed
                    .insert(key.clone(), (old_val.clone(), new_val.clone()));
            }
            _ => {}
        }
    }

    for (key, new_val) in new {
        if !old.contains_key(key) {
            result.added.insert(key.clone(), new_val.clone());
        }
    }

    result
}

// -- Display -----------------------------------------------------------------

impl fmt::Display for ManifestDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "No changes");
        }

        write_section(f, "Types", &self.types)?;
        write_section(f, "Functions", &self.functions)?;
        write_section(f, "Modules", &self.modules)?;
        write_section(f, "Routes", &self.routes)?;
        write_section(f, "Endpoints", &self.endpoints)?;
        write_section(f, "Models", &self.models)?;

        if self.auth_changed {
            writeln!(f, "Auth:")?;
            writeln!(f, "  ~ configuration changed")?;
        }

        for c in &self.components_added {
            writeln!(f, "Components:")?;
            writeln!(f, "  + {}", c.name)?;
        }
        for c in &self.components_removed {
            writeln!(f, "  - {}", c.name)?;
        }

        for i in &self.integrations_added {
            writeln!(f, "Integrations:")?;
            writeln!(f, "  + {}", i.name)?;
        }
        for i in &self.integrations_removed {
            writeln!(f, "  - {}", i.name)?;
        }

        Ok(())
    }
}

fn write_section<T>(f: &mut fmt::Formatter<'_>, label: &str, diff: &SectionDiff<T>) -> fmt::Result {
    if diff.is_empty() {
        return Ok(());
    }
    writeln!(f, "{label}:")?;
    for key in diff.added.keys() {
        writeln!(f, "  + {key}")?;
    }
    for key in diff.removed.keys() {
        writeln!(f, "  - {key}")?;
    }
    for key in diff.changed.keys() {
        writeln!(f, "  ~ {key}")?;
    }
    Ok(())
}
