use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    /// I/O error when reading/writing schema file
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML serialization/deserialization error
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// TOML serialization error
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// Circular alias reference detected
    #[error("Circular alias detected: {0}")]
    CircularAlias(String),

    /// Invalid tag format (e.g., contains reserved delimiter)
    #[error("Invalid tag format: {0}")]
    InvalidTag(String),

    /// Alias already exists
    #[error("Alias '{0}' already exists for '{1}'")]
    AliasExists(String, String),

    /// Tag not found in schema
    #[error("Tag '{0}' not found in schema")]
    TagNotFound(String),
}

/// Type alias for cleaner function signatures
pub type Result<T> = std::result::Result<T, SchemaError>;
