use crate::vtags::config::VirtualTagConfig;
use crate::vtags::types::{SizeCategory, SizeCondition, VirtualTag};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Invalid virtual tag format: {0}")]
    InvalidFormat(String),
    #[error("Unknown virtual tag prefix: {0}")]
    UnknownPrefix(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Invalid size: {0}")]
    InvalidSize(String),
    #[error("Invalid date: {0}")]
    InvalidDate(String),
    #[error("Invalid range: {0}")]
    InvalidRange(String),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
}

impl TryFrom<&str> for VirtualTag {
    type Error = ParseError;

    /// Parse a virtual tag string like "modified:today" or "ext:.rs"
    /// 
    /// For size-based tags with byte values (e.g., "size:>1MB"), this uses default
    /// configuration. To use custom size category thresholds, use `VirtualTag::parse_with_config`.
    ///
    /// # Examples
    /// ```
    /// use tagr::vtags::VirtualTag;
    ///
    /// let tag: VirtualTag = "modified:today".try_into().unwrap();
    /// let tag: VirtualTag = "size:>1MB".try_into().unwrap();
    /// let tag: VirtualTag = "ext:.rs".try_into().unwrap();
    /// ```
    fn try_from(input: &str) -> Result<Self, Self::Error> {
        Self::parse_with_config(input, &VirtualTagConfig::default())
    }
}

impl VirtualTag {
    /// Parse a virtual tag with a custom configuration.
    /// 
    /// This is useful when you need custom size category thresholds.
    ///
    /// # Errors
    /// Returns an error if the input string cannot be parsed into a valid virtual tag.
    pub fn parse_with_config(input: &str, config: &VirtualTagConfig) -> Result<Self, ParseError> {
        let (prefix, value) = input
            .split_once(':')
            .ok_or_else(|| ParseError::InvalidFormat(input.to_string()))?;

        match prefix {
            "modified" => Ok(Self::Modified(value.try_into()?)),
            "created" => Ok(Self::Created(value.try_into()?)),
            "accessed" => Ok(Self::Accessed(value.try_into()?)),
            "size" => Ok(Self::Size(parse_size(value, config)?)),
            "ext" => Ok(Self::Extension(value.to_string())),
            "ext-type" => Ok(Self::ExtensionType(value.try_into()?)),
            "dir" => Ok(Self::Directory(PathBuf::from(value))),
            "path" => Ok(Self::Path(parse_path_pattern(value)?)),
            "depth" => Ok(Self::Depth(value.try_into()?)),
            "perm" => Ok(Self::Permission(value.try_into()?)),
            "lines" => Ok(Self::Lines(value.try_into()?)),
            "git" => Ok(Self::Git(value.try_into()?)),
            _ => Err(ParseError::UnknownPrefix(prefix.to_string())),
        }
    }
}

fn parse_size(value: &str, config: &VirtualTagConfig) -> Result<SizeCondition, ParseError> {
    match value {
        "empty" => Ok(SizeCondition::Empty),
        "tiny" => Ok(SizeCondition::Category(SizeCategory::Tiny)),
        "small" => Ok(SizeCondition::Category(SizeCategory::Small)),
        "medium" => Ok(SizeCondition::Category(SizeCategory::Medium)),
        "large" => Ok(SizeCondition::Category(SizeCategory::Large)),
        "huge" => Ok(SizeCondition::Category(SizeCategory::Huge)),
        _ if value.starts_with('>') => {
            let size_str = &value[1..];
            let size = config
                .parse_size(size_str)
                .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
            Ok(SizeCondition::GreaterThan(size))
        }
        _ if value.starts_with('<') => {
            let size_str = &value[1..];
            let size = config
                .parse_size(size_str)
                .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
            Ok(SizeCondition::LessThan(size))
        }
        _ if value.starts_with('=') => {
            let size_str = &value[1..];
            let size = config
                .parse_size(size_str)
                .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
            Ok(SizeCondition::Equals(size))
        }
        _ if value.contains('-') => {
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() != 2 {
                return Err(ParseError::InvalidSize(value.to_string()));
            }
            let min = config
                .parse_size(parts[0])
                .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
            let max = config
                .parse_size(parts[1])
                .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
            Ok(SizeCondition::Range(min, max))
        }
        _ => Err(ParseError::InvalidSize(value.to_string())),
    }
}

fn parse_path_pattern(value: &str) -> Result<glob::Pattern, ParseError> {
    glob::Pattern::new(value)
        .map_err(|_| ParseError::InvalidPattern(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vtags::types::{
        ExtTypeCategory, GitCondition, PermissionCondition, RangeCondition, 
        SizeCategory, SizeCondition, TimeCondition,
    };

    #[test]
    fn test_parse_modified_today() {
        let tag: VirtualTag = "modified:today".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Modified(TimeCondition::Today)));
    }

    #[test]
    fn test_parse_created_yesterday() {
        let tag: VirtualTag = "created:yesterday".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Created(TimeCondition::Yesterday)));
    }

    #[test]
    fn test_parse_accessed_last_7_days() {
        let tag: VirtualTag = "accessed:last-7-days".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Accessed(TimeCondition::LastNDays(7))));
    }

    #[test]
    fn test_parse_size_greater_than() {
        let tag: VirtualTag = "size:>1MB".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Size(SizeCondition::GreaterThan(_))));
    }

    #[test]
    fn test_parse_size_category() {
        let tag: VirtualTag = "size:large".try_into().unwrap();
        assert!(matches!(
            tag,
            VirtualTag::Size(SizeCondition::Category(SizeCategory::Large))
        ));
    }

    #[test]
    fn test_parse_extension() {
        let tag: VirtualTag = "ext:.rs".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Extension(s) if s == ".rs"));
    }

    #[test]
    fn test_parse_ext_type() {
        let tag: VirtualTag = "ext-type:source".try_into().unwrap();
        assert!(matches!(
            tag,
            VirtualTag::ExtensionType(ExtTypeCategory::Source)
        ));
    }

    #[test]
    fn test_parse_directory() {
        let tag: VirtualTag = "dir:/home/user".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Directory(p) if p == PathBuf::from("/home/user")));
    }

    #[test]
    fn test_parse_path_glob() {
        let tag: VirtualTag = "path:*.rs".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Path(_)));
    }

    #[test]
    fn test_parse_depth_range() {
        let tag: VirtualTag = "depth:>3".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Depth(RangeCondition::GreaterThan(3))));
    }

    #[test]
    fn test_parse_permission() {
        let tag: VirtualTag = "perm:executable".try_into().unwrap();
        assert!(matches!(
            tag,
            VirtualTag::Permission(PermissionCondition::Executable)
        ));
    }

    #[test]
    fn test_parse_lines() {
        let tag: VirtualTag = "lines:100-500".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Lines(RangeCondition::Range(100, 500))));
    }

    #[test]
    fn test_parse_git() {
        let tag: VirtualTag = "git:modified".try_into().unwrap();
        assert!(matches!(tag, VirtualTag::Git(GitCondition::Modified)));
    }

    // Error cases
    #[test]
    fn test_parse_missing_colon() {
        let result: Result<VirtualTag, _> = "modified".try_into();
        assert!(matches!(result, Err(ParseError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_empty_value() {
        let result: Result<VirtualTag, _> = "modified:".try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_prefix() {
        let result: Result<VirtualTag, _> = "unknown:value".try_into();
        assert!(matches!(result, Err(ParseError::UnknownPrefix(_))));
    }

    #[test]
    fn test_parse_invalid_size() {
        let result: Result<VirtualTag, _> = "size:potato".try_into();
        assert!(matches!(result, Err(ParseError::InvalidSize(_))));
    }

    #[test]
    fn test_parse_invalid_time() {
        let result: Result<VirtualTag, _> = "modified:invalid-time".try_into();
        assert!(matches!(result, Err(ParseError::InvalidDate(_))));
    }

    #[test]
    fn test_parse_invalid_range() {
        let result: Result<VirtualTag, _> = "depth:abc".try_into();
        assert!(matches!(result, Err(ParseError::InvalidRange(_))));
    }

    #[test]
    fn test_parse_invalid_ext_type() {
        let result: Result<VirtualTag, _> = "ext-type:unknown".try_into();
        assert!(matches!(result, Err(ParseError::InvalidValue(_))));
    }

    #[test]
    fn test_parse_invalid_permission() {
        let result: Result<VirtualTag, _> = "perm:invalid".try_into();
        assert!(matches!(result, Err(ParseError::InvalidValue(_))));
    }

    #[test]
    fn test_parse_invalid_git() {
        let result: Result<VirtualTag, _> = "git:unknown".try_into();
        assert!(matches!(result, Err(ParseError::InvalidValue(_))));
    }

    #[test]
    fn test_parse_invalid_glob_pattern() {
        let result: Result<VirtualTag, _> = "path:[invalid".try_into();
        assert!(matches!(result, Err(ParseError::InvalidPattern(_))));
    }
}
