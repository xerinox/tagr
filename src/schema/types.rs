use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::error::{Result, SchemaError};

/// Reserved delimiter for hierarchical tags
pub const HIERARCHY_DELIMITER: char = ':';

/// Tag schema managing aliases (synonyms) and hierarchical relationships
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TagSchema {
    /// Maps alias → canonical tag (e.g., "js" → "javascript")
    #[serde(default)]
    pub aliases: HashMap<String, String>,

    /// Maps canonical tag → set of aliases (reverse index for efficient lookup)
    #[serde(skip)]
    reverse_aliases: HashMap<String, HashSet<String>>,

    /// Path to the schema file for persistence
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl TagSchema {
    /// Create a new empty schema
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load schema from a TOML file, or create default if file doesn't exist
    ///
    /// # Errors
    /// Returns error if file exists but cannot be read or parsed
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let mut schema: Self = toml::from_str(&content)?;
            schema.path = Some(path.to_path_buf());
            schema.build_reverse_index();
            Ok(schema)
        } else {
            let mut schema = Self::new();
            schema.path = Some(path.to_path_buf());
            Ok(schema)
        }
    }

    /// Save schema to its configured path
    ///
    /// # Errors
    /// Returns error if path not set or file cannot be written
    pub fn save(&self) -> Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            SchemaError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Schema path not set",
            ))
        })?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add an alias mapping (e.g., "js" → "javascript")
    ///
    /// # Errors
    /// Returns error if:
    /// - Alias contains reserved delimiter (canonical can be hierarchical)
    /// - Alias already exists with different canonical
    /// - Adding alias would create circular reference
    pub fn add_alias(&mut self, alias: &str, canonical: &str) -> Result<()> {
        // Validate alias doesn't contain reserved delimiter
        // (canonical CAN be hierarchical, e.g., js → lang:javascript)
        if alias.contains(HIERARCHY_DELIMITER) {
            return Err(SchemaError::InvalidTag(format!(
                "Alias '{}' contains reserved delimiter '{}'",
                alias, HIERARCHY_DELIMITER
            )));
        }

        // Check if alias already exists with different canonical
        if let Some(existing) = self.aliases.get(alias) {
            if existing != canonical {
                return Err(SchemaError::AliasExists(
                    alias.to_string(),
                    existing.clone(),
                ));
            }
            // Same mapping already exists, no-op
            return Ok(());
        }

        // Check for circular reference: canonical cannot be an alias to alias
        if self.would_create_cycle(alias, canonical) {
            return Err(SchemaError::CircularAlias(format!(
                "Adding alias '{}' → '{}' would create circular reference",
                alias, canonical
            )));
        }

        // Add to forward and reverse indices
        self.aliases
            .insert(alias.to_string(), canonical.to_string());
        self.reverse_aliases
            .entry(canonical.to_string())
            .or_default()
            .insert(alias.to_string());

        Ok(())
    }

    /// Remove an alias
    ///
    /// # Errors
    /// Returns error if alias doesn't exist
    pub fn remove_alias(&mut self, alias: &str) -> Result<()> {
        let canonical = self
            .aliases
            .remove(alias)
            .ok_or_else(|| SchemaError::TagNotFound(alias.to_string()))?;

        // Remove from reverse index
        if let Some(aliases) = self.reverse_aliases.get_mut(&canonical) {
            aliases.remove(alias);
            if aliases.is_empty() {
                self.reverse_aliases.remove(&canonical);
            }
        }

        Ok(())
    }

    /// Canonicalize a tag (resolve alias to canonical form)
    ///
    /// For hierarchical tags (containing `:`), canonicalizes each level separately
    /// Returns the canonical tag, or the input if no alias exists
    #[must_use]
    pub fn canonicalize(&self, tag: &str) -> String {
        // Check if it's a hierarchical tag
        if tag.contains(HIERARCHY_DELIMITER) {
            // Canonicalize each level separately
            tag.split(HIERARCHY_DELIMITER)
                .map(|level| {
                    self.aliases
                        .get(level)
                        .map_or_else(|| level.to_string(), Clone::clone)
                })
                .collect::<Vec<_>>()
                .join(&HIERARCHY_DELIMITER.to_string())
        } else {
            // Simple tag - direct lookup
            self.aliases
                .get(tag)
                .map_or_else(|| tag.to_string(), Clone::clone)
        }
    }

    /// Get all aliases for a canonical tag
    #[must_use]
    pub fn get_aliases(&self, canonical: &str) -> Vec<String> {
        self.reverse_aliases
            .get(canonical)
            .map_or_else(Vec::new, |set| set.iter().cloned().collect())
    }

    /// Expand a tag into all its synonyms (canonical + all aliases)
    #[must_use]
    pub fn expand_synonyms(&self, tag: &str) -> Vec<String> {
        let canonical = self.canonicalize(tag);
        let mut synonyms = vec![canonical.clone()];

        if let Some(aliases) = self.reverse_aliases.get(&canonical) {
            synonyms.extend(aliases.iter().cloned());
        }

        synonyms
    }

    /// Expand a tag with hierarchy (e.g., "lang:rust" → ["lang", "lang:rust"])
    ///
    /// Also includes all synonyms for each level
    #[must_use]
    pub fn expand_with_hierarchy(&self, tag: &str) -> Vec<String> {
        let mut expanded = HashSet::new();

        // Canonicalize the input tag (handles hierarchical canonicalization)
        let canonical = self.canonicalize(tag);

        // Add the canonical form and extract hierarchy
        expanded.insert(canonical.clone());
        let mut current = canonical.as_str();
        while let Some(pos) = current.rfind(HIERARCHY_DELIMITER) {
            current = &current[..pos];
            expanded.insert(current.to_string());
        }

        // Add the original input and its hierarchy (if different from canonical)
        if tag != canonical {
            expanded.insert(tag.to_string());
            let mut current = tag;
            while let Some(pos) = current.rfind(HIERARCHY_DELIMITER) {
                current = &current[..pos];
                expanded.insert(current.to_string());
            }
        }

        // For each level in the canonical form, add all synonyms
        for level in canonical.split(HIERARCHY_DELIMITER) {
            for synonym in self.expand_synonyms(level) {
                expanded.insert(synonym);
            }
        }

        expanded.into_iter().collect()
    }

    /// Get all children of a hierarchical tag (e.g., "lang:*" pattern)
    ///
    /// This method should be used with database prefix scanning for efficiency
    #[must_use]
    pub fn get_hierarchy_prefix(&self, tag: &str) -> String {
        let canonical = self.canonicalize(tag);
        format!("{}{}", canonical, HIERARCHY_DELIMITER)
    }

    /// List all aliases in the schema
    #[must_use]
    pub fn list_aliases(&self) -> Vec<(String, String)> {
        let mut aliases: Vec<_> = self
            .aliases
            .iter()
            .map(|(alias, canonical)| (alias.clone(), canonical.clone()))
            .collect();
        aliases.sort();
        aliases
    }

    /// Build reverse index from aliases map (used after deserialization)
    fn build_reverse_index(&mut self) {
        self.reverse_aliases.clear();
        for (alias, canonical) in &self.aliases {
            self.reverse_aliases
                .entry(canonical.clone())
                .or_default()
                .insert(alias.clone());
        }
    }

    /// Check if adding an alias would create a circular reference
    ///
    /// When adding alias → canonical, we need to check if canonical eventually
    /// resolves back to alias through the existing alias chain
    fn would_create_cycle(&self, alias: &str, canonical: &str) -> bool {
        let mut visited = HashSet::new();
        let mut current = canonical;

        // Follow the chain from canonical to see if it leads back to alias
        while let Some(next) = self.aliases.get(current) {
            if next == alias {
                return true;
            }
            if !visited.insert(current.to_string()) {
                // Already visited this node, there's a cycle
                return true;
            }
            current = next;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_alias() {
        let mut schema = TagSchema::new();
        schema.add_alias("js", "javascript").unwrap();

        assert_eq!(schema.canonicalize("js"), "javascript");
        assert_eq!(schema.canonicalize("javascript"), "javascript");
    }

    #[test]
    fn test_duplicate_alias() {
        let mut schema = TagSchema::new();
        schema.add_alias("js", "javascript").unwrap();

        // Same mapping is idempotent
        schema.add_alias("js", "javascript").unwrap();

        // Different mapping fails
        let result = schema.add_alias("js", "ecmascript");
        assert!(matches!(result, Err(SchemaError::AliasExists(_, _))));
    }

    #[test]
    fn test_circular_alias_direct() {
        let mut schema = TagSchema::new();
        schema.add_alias("a", "b").unwrap();

        let result = schema.add_alias("b", "a");
        assert!(matches!(result, Err(SchemaError::CircularAlias(_))));
    }

    #[test]
    fn test_circular_alias_indirect() {
        let mut schema = TagSchema::new();
        schema.add_alias("a", "b").unwrap();
        schema.add_alias("b", "c").unwrap();

        let result = schema.add_alias("c", "a");
        assert!(matches!(result, Err(SchemaError::CircularAlias(_))));
    }

    #[test]
    fn test_expand_synonyms() {
        let mut schema = TagSchema::new();
        schema.add_alias("js", "javascript").unwrap();
        schema.add_alias("es", "javascript").unwrap();

        let mut synonyms = schema.expand_synonyms("js");
        synonyms.sort();
        assert_eq!(synonyms, vec!["es", "javascript", "js"]);
    }

    #[test]
    fn test_hierarchy_expansion() {
        let schema = TagSchema::new();
        let expanded = schema.expand_with_hierarchy("lang:rust:async");

        assert!(expanded.contains(&"lang:rust:async".to_string()));
        assert!(expanded.contains(&"lang:rust".to_string()));
        assert!(expanded.contains(&"lang".to_string()));
    }

    #[test]
    fn test_hierarchy_with_aliases() {
        let mut schema = TagSchema::new();
        schema.add_alias("language", "lang").unwrap();

        let expanded = schema.expand_with_hierarchy("language:rust");

        assert!(expanded.contains(&"lang:rust".to_string()));
        assert!(expanded.contains(&"lang".to_string()));
        assert!(expanded.contains(&"language:rust".to_string()));
        assert!(expanded.contains(&"language".to_string()));
    }

    #[test]
    fn test_reserved_delimiter_validation() {
        let mut schema = TagSchema::new();

        // Cannot use delimiter in alias (left side)
        let result = schema.add_alias("lang:js", "javascript");
        assert!(matches!(result, Err(SchemaError::InvalidTag(_))));

        // CAN use delimiter in canonical (right side) - this is allowed now
        let result = schema.add_alias("js", "lang:javascript");
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_alias() {
        let mut schema = TagSchema::new();
        schema.add_alias("js", "javascript").unwrap();
        schema.add_alias("es", "javascript").unwrap();

        schema.remove_alias("js").unwrap();

        assert_eq!(schema.canonicalize("js"), "js");
        assert_eq!(schema.canonicalize("es"), "javascript");
    }

    #[test]
    fn test_get_aliases() {
        let mut schema = TagSchema::new();
        schema.add_alias("js", "javascript").unwrap();
        schema.add_alias("es", "javascript").unwrap();

        let mut aliases = schema.get_aliases("javascript");
        aliases.sort();
        assert_eq!(aliases, vec!["es", "js"]);
    }
}
