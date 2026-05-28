//! Filter CRUD operations
//!
//! This module provides a `FilterManager` for managing saved filters with
//! idiomatic Rust APIs.

use super::error::FilterError;
use super::types::{SavedFilter, raw_from_saved, saved_from_raw, RawFilterStorage};
use crate::types::QueryCriteria;
use std::fs;
use std::path::PathBuf;

/// Manager for filter operations
///
/// Encapsulates the storage path and provides methods for filter CRUD operations.
///
/// # Examples
///
/// ```no_run
/// use tagr::filters::FilterManager;
/// use std::path::PathBuf;
///
/// let manager = FilterManager::new(PathBuf::from("~/.config/tagr/filters.toml"));
/// let filters = manager.list().unwrap();
/// ```
pub struct FilterManager {
    path: PathBuf,
    auto_backup: bool,
}

impl FilterManager {
    /// Create a new `FilterManager` with the specified storage path
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self {
            path,
            auto_backup: true,
        }
    }

    /// Create a `FilterManager` with auto-backup disabled
    #[must_use]
    pub const fn without_backup(path: PathBuf) -> Self {
        Self {
            path,
            auto_backup: false,
        }
    }

    /// Enable or disable auto-backup
    pub const fn set_auto_backup(&mut self, enabled: bool) {
        self.auto_backup = enabled;
    }

    /// Load raw filters from the storage file.
    fn load_raw(&self) -> Result<RawFilterStorage, FilterError> {
        if !self.path.exists() {
            return Ok(RawFilterStorage::default());
        }

        let contents = fs::read_to_string(&self.path)?;
        let storage: RawFilterStorage = toml::from_str(&contents)?;
        Ok(storage)
    }

    /// Save raw filters to the storage file.
    fn save_raw(&self, storage: &RawFilterStorage) -> Result<(), FilterError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        if self.auto_backup && self.path.exists() {
            let backup_path = self.path.with_extension("toml.backup");
            fs::copy(&self.path, backup_path)?;
        }

        let toml = toml::to_string_pretty(storage)?;
        fs::write(&self.path, toml)?;

        Ok(())
    }

    /// Create a new filter
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The filter name is invalid
    /// - The filter criteria is invalid
    /// - A filter with the same name already exists
    /// - The storage file cannot be saved
    pub fn create(
        &self,
        name: &str,
        description: String,
        criteria: QueryCriteria,
    ) -> Result<SavedFilter, FilterError> {
        let mut storage = self.load_raw()?;

        let saved = SavedFilter::new(name.to_string(), description, criteria);
        saved.validate().map_err(FilterError::InvalidCriteria)?;

        let raw = raw_from_saved(&saved);

        if storage.filters.iter().any(|f| f.name == name) {
            return Err(FilterError::AlreadyExists(name.to_string()));
        }

        storage.filters.push(raw);
        self.save_raw(&storage)?;

        Ok(saved)
    }

    /// Get a filter by name
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The storage file cannot be loaded
    /// - The filter is not found
    pub fn get(&self, name: &str) -> Result<SavedFilter, FilterError> {
        let storage = self.load_raw()?;
        storage
            .filters
            .into_iter()
            .find(|f| f.name == name)
            .map(saved_from_raw)
            .ok_or_else(|| FilterError::NotFound(name.to_string()))
    }

    /// Update an existing filter
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The filter is not found
    /// - The filter criteria is invalid
    /// - The storage file cannot be saved
    pub fn update(&self, filter: SavedFilter) -> Result<(), FilterError> {
        let mut storage = self.load_raw()?;

        filter.validate().map_err(FilterError::InvalidCriteria)?;

        let raw = raw_from_saved(&filter);
        if let Some(existing) = storage.filters.iter_mut().find(|f| f.name == filter.name) {
            *existing = raw;
        } else {
            return Err(FilterError::NotFound(filter.name));
        }

        self.save_raw(&storage)?;
        Ok(())
    }

    /// Delete a filter by name
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The filter is not found
    /// - The storage file cannot be saved
    pub fn delete(&self, name: &str) -> Result<SavedFilter, FilterError> {
        let mut storage = self.load_raw()?;

        let pos = storage
            .filters
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| FilterError::NotFound(name.to_string()))?;

        let raw = storage.filters.remove(pos);
        self.save_raw(&storage)?;

        Ok(saved_from_raw(raw))
    }

    /// Rename a filter
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The old filter is not found
    /// - The new name is invalid
    /// - A filter with the new name already exists
    /// - The storage file cannot be saved
    pub fn rename(&self, old_name: &str, new_name: String) -> Result<(), FilterError> {
        let mut storage = self.load_raw()?;

        super::types::validate_filter_name(&new_name)
            .map_err(|e| FilterError::InvalidName(new_name.clone(), e))?;

        if storage.filters.iter().any(|f| f.name == new_name) {
            return Err(FilterError::AlreadyExists(new_name));
        }

        let filter = storage
            .filters
            .iter_mut()
            .find(|f| f.name == old_name)
            .ok_or_else(|| FilterError::NotFound(old_name.to_string()))?;

        filter.name = new_name;

        self.save_raw(&storage)?;

        Ok(())
    }

    /// List all filters
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if the storage file cannot be loaded.
    pub fn list(&self) -> Result<Vec<SavedFilter>, FilterError> {
        let storage = self.load_raw()?;
        Ok(storage.filters.into_iter().map(saved_from_raw).collect())
    }

    /// Record filter usage (increment use count, update `last_used` timestamp)
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The filter is not found
    /// - The storage file cannot be saved
    pub fn record_use(&self, name: &str) -> Result<(), FilterError> {
        let mut storage = self.load_raw()?;

        let filter = storage
            .filters
            .iter_mut()
            .find(|f| f.name == name)
            .ok_or_else(|| FilterError::NotFound(name.to_string()))?;

        filter.use_count += 1;
        filter.last_used = chrono::Utc::now();

        self.save_raw(&storage)?;

        Ok(())
    }

    /// Export filters to a file
    ///
    /// If `filter_names` is empty, exports all filters.
    /// Otherwise, exports only the specified filters.
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - Any specified filter is not found
    /// - The export file cannot be written
    pub fn export(
        &self,
        export_path: &PathBuf,
        filter_names: &[String],
    ) -> Result<(), FilterError> {
        let storage = self.load_raw()?;

        let filters_to_export = if filter_names.is_empty() {
            storage.filters
        } else {
            let mut exported = Vec::new();
            for name in filter_names {
                let filter = storage
                    .filters
                    .iter()
                    .find(|f| f.name == *name)
                    .ok_or_else(|| FilterError::NotFound(name.clone()))?;
                exported.push(filter.clone());
            }
            exported
        };

        let export_storage = RawFilterStorage {
            filters: filters_to_export,
        };

        if let Some(parent) = export_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml = toml::to_string_pretty(&export_storage)?;
        fs::write(export_path, toml)?;

        Ok(())
    }

    /// Import filters from a file
    ///
    /// # Arguments
    /// * `import_path` - Path to the file to import from
    /// * `overwrite` - If true, overwrite existing filters with the same name
    /// * `skip_existing` - If true, skip filters that already exist (only if overwrite is false)
    ///
    /// # Returns
    /// A tuple of (`imported_count`, `skipped_count`)
    ///
    /// # Errors
    ///
    /// Returns `FilterError` if:
    /// - The import file cannot be read
    /// - The storage file cannot be saved
    pub fn import(
        &self,
        import_path: &PathBuf,
        overwrite: bool,
        skip_existing: bool,
    ) -> Result<(usize, usize), FilterError> {
        let mut storage = self.load_raw()?;

        let contents = fs::read_to_string(import_path)?;
        let import_storage: RawFilterStorage = toml::from_str(&contents)?;

        let mut imported = 0;
        let mut skipped = 0;

        for filter in import_storage.filters {
            let exists = storage.filters.iter().any(|f| f.name == filter.name);
            if exists {
                if overwrite {
                    if let Some(existing) =
                        storage.filters.iter_mut().find(|f| f.name == filter.name)
                    {
                        *existing = filter;
                    }
                    imported += 1;
                } else if skip_existing {
                    skipped += 1;
                } else {
                    return Err(FilterError::AlreadyExists(filter.name));
                }
            } else {
                storage.filters.push(filter);
                imported += 1;
            }
        }

        self.save_raw(&storage)?;

        Ok((imported, skipped))
    }

    /// Get the storage path
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TagExpr, TagName};
    use std::env;

    fn temp_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("tagr_test_{name}.toml"))
    }

    fn sample_criteria() -> QueryCriteria {
        QueryCriteria {
            tag_expr: Some(TagExpr::Tag(TagName::new("rust").unwrap())),
            ..Default::default()
        }
    }

    #[test]
    fn test_create_and_load_filter() {
        let path = temp_path("create_load");
        let _ = fs::remove_file(&path);
        let manager = FilterManager::without_backup(path.clone());

        let result = manager.create("test-filter", "Test".to_string(), sample_criteria());
        assert!(result.is_ok());

        let loaded = manager.get("test-filter");
        assert!(loaded.is_ok());
        assert_eq!(loaded.unwrap().name, "test-filter");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_delete_filter() {
        let path = temp_path("delete");
        let _ = fs::remove_file(&path);
        let manager = FilterManager::without_backup(path.clone());

        manager
            .create("to-delete", String::new(), sample_criteria())
            .unwrap();

        let result = manager.delete("to-delete");
        assert!(result.is_ok());

        let loaded = manager.get("to-delete");
        assert!(loaded.is_err());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_rename_filter() {
        let path = temp_path("rename");
        let _ = fs::remove_file(&path);
        let manager = FilterManager::without_backup(path.clone());

        manager
            .create("old-name", String::new(), sample_criteria())
            .unwrap();

        let result = manager.rename("old-name", "new-name".to_string());
        assert!(result.is_ok());

        assert!(manager.get("old-name").is_err());
        assert!(manager.get("new-name").is_ok());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_export_import() {
        let storage_path = temp_path("export_storage");
        let export_path = temp_path("export_file");
        let import_path = temp_path("import_storage");
        let _ = fs::remove_file(&storage_path);
        let _ = fs::remove_file(&export_path);
        let _ = fs::remove_file(&import_path);

        let manager = FilterManager::without_backup(storage_path.clone());
        let import_manager = FilterManager::without_backup(import_path.clone());

        manager
            .create("filter1", String::new(), sample_criteria())
            .unwrap();
        manager
            .create("filter2", String::new(), sample_criteria())
            .unwrap();

        manager.export(&export_path, &[]).unwrap();

        let (imported, skipped) = import_manager.import(&export_path, false, false).unwrap();
        assert_eq!(imported, 2);
        assert_eq!(skipped, 0);

        let filters = import_manager.list().unwrap();
        assert_eq!(filters.len(), 2);

        let _ = fs::remove_file(&storage_path);
        let _ = fs::remove_file(&export_path);
        let _ = fs::remove_file(&import_path);
    }
}
