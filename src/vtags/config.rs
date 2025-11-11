use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualTagConfig {
    pub enabled: bool,
    pub cache_metadata: bool,
    pub cache_ttl_seconds: u64,
    pub size_categories: SizeCategoryConfig,
    pub extension_types: HashMap<String, Vec<String>>,
    pub time: TimeConfig,
    pub git: GitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeCategoryConfig {
    pub tiny: String,
    pub small: String,
    pub medium: String,
    pub large: String,
    pub huge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeConfig {
    pub recent: u32,
    pub stale: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    pub enabled: bool,
    pub detect_repo: bool,
}

impl Default for VirtualTagConfig {
    fn default() -> Self {
        let mut extension_types = HashMap::new();
        extension_types.insert(
            "source".to_string(),
            vec![
                ".rs".to_string(),
                ".py".to_string(),
                ".js".to_string(),
                ".go".to_string(),
                ".cpp".to_string(),
                ".c".to_string(),
                ".java".to_string(),
                ".ts".to_string(),
            ],
        );
        extension_types.insert(
            "document".to_string(),
            vec![
                ".md".to_string(),
                ".txt".to_string(),
                ".pdf".to_string(),
                ".doc".to_string(),
                ".docx".to_string(),
                ".org".to_string(),
            ],
        );
        extension_types.insert(
            "config".to_string(),
            vec![
                ".toml".to_string(),
                ".yaml".to_string(),
                ".yml".to_string(),
                ".json".to_string(),
                ".ini".to_string(),
                ".conf".to_string(),
            ],
        );
        extension_types.insert(
            "image".to_string(),
            vec![
                ".png".to_string(),
                ".jpg".to_string(),
                ".jpeg".to_string(),
                ".gif".to_string(),
                ".svg".to_string(),
                ".webp".to_string(),
            ],
        );
        extension_types.insert(
            "archive".to_string(),
            vec![
                ".zip".to_string(),
                ".tar".to_string(),
                ".gz".to_string(),
                ".7z".to_string(),
                ".rar".to_string(),
                ".bz2".to_string(),
            ],
        );

        Self {
            enabled: true,
            cache_metadata: true,
            cache_ttl_seconds: 300,
            size_categories: SizeCategoryConfig {
                tiny: "1KB".to_string(),
                small: "100KB".to_string(),
                medium: "1MB".to_string(),
                large: "10MB".to_string(),
                huge: "100MB".to_string(),
            },
            extension_types,
            time: TimeConfig {
                recent: 7,
                stale: 180,
            },
            git: GitConfig {
                enabled: true,
                detect_repo: true,
            },
        }
    }
}

impl VirtualTagConfig {
    #[must_use]
    pub fn parse_size(&self, size_str: &str) -> Option<u64> {
        use byte_unit::Byte;

        // Try parsing with byte-unit
        Byte::parse_str(size_str, true)
            .ok()
            .map(byte_unit::Byte::as_u64)
    }

    #[must_use]
    pub fn get_size_threshold(&self, category: &str) -> Option<u64> {
        match category {
            "tiny" => self.parse_size(&self.size_categories.tiny),
            "small" => self.parse_size(&self.size_categories.small),
            "medium" => self.parse_size(&self.size_categories.medium),
            "large" => self.parse_size(&self.size_categories.large),
            "huge" => self.parse_size(&self.size_categories.huge),
            _ => None,
        }
    }
}
