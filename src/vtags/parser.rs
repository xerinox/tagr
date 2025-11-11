use crate::vtags::config::VirtualTagConfig;
use crate::vtags::types::{
    ExtTypeCategory, GitCondition, PermissionCondition, RangeCondition, SizeCategory,
    SizeCondition, TimeCondition, VirtualTag,
};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
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

pub struct VirtualTagParser {
    config: VirtualTagConfig,
}

impl VirtualTagParser {
    pub fn new(config: VirtualTagConfig) -> Self {
        Self { config }
    }

    pub fn parse(&self, input: &str) -> Result<VirtualTag, ParseError> {
        let (prefix, value) = input
            .split_once(':')
            .ok_or_else(|| ParseError::InvalidFormat(input.to_string()))?;

        match prefix {
            "modified" => Ok(VirtualTag::Modified(self.parse_time(value)?)),
            "created" => Ok(VirtualTag::Created(self.parse_time(value)?)),
            "accessed" => Ok(VirtualTag::Accessed(self.parse_time(value)?)),
            "size" => Ok(VirtualTag::Size(self.parse_size(value)?)),
            "ext" => Ok(VirtualTag::Extension(value.to_string())),
            "ext-type" => Ok(VirtualTag::ExtensionType(self.parse_ext_type(value)?)),
            "dir" => Ok(VirtualTag::Directory(PathBuf::from(value))),
            "path" => Ok(VirtualTag::Path(self.parse_path_pattern(value)?)),
            "depth" => Ok(VirtualTag::Depth(self.parse_range(value)?)),
            "perm" => Ok(VirtualTag::Permission(self.parse_permission(value)?)),
            "lines" => Ok(VirtualTag::Lines(self.parse_range(value)?)),
            "git" => Ok(VirtualTag::Git(self.parse_git(value)?)),
            _ => Err(ParseError::UnknownPrefix(prefix.to_string())),
        }
    }

    fn parse_time(&self, value: &str) -> Result<TimeCondition, ParseError> {
        match value {
            "today" => Ok(TimeCondition::Today),
            "yesterday" => Ok(TimeCondition::Yesterday),
            "this-week" => Ok(TimeCondition::ThisWeek),
            "this-month" => Ok(TimeCondition::ThisMonth),
            "this-year" => Ok(TimeCondition::ThisYear),
            _ if value.starts_with("last-") && value.ends_with("-days") => {
                let days_str = value
                    .strip_prefix("last-")
                    .and_then(|s| s.strip_suffix("-days"))
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let days = days_str
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidValue(value.to_string()))?;
                Ok(TimeCondition::LastNDays(days))
            }
            _ if value.starts_with("last-") && value.ends_with("-hours") => {
                let hours_str = value
                    .strip_prefix("last-")
                    .and_then(|s| s.strip_suffix("-hours"))
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let hours = hours_str
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidValue(value.to_string()))?;
                Ok(TimeCondition::LastNHours(hours))
            }
            _ if value.starts_with("after-") => {
                let date_str = value
                    .strip_prefix("after-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let date = self.parse_date(date_str)?;
                Ok(TimeCondition::After(date))
            }
            _ if value.starts_with("before-") => {
                let date_str = value
                    .strip_prefix("before-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let date = self.parse_date(date_str)?;
                Ok(TimeCondition::Before(date))
            }
            _ if value.starts_with("between-") => {
                let range_str = value
                    .strip_prefix("between-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let parts: Vec<&str> = range_str.split('-').collect();
                if parts.len() != 6 {
                    return Err(ParseError::InvalidDate(value.to_string()));
                }
                let start_date = self.parse_date(&format!("{}-{}-{}", parts[0], parts[1], parts[2]))?;
                let end_date = self.parse_date(&format!("{}-{}-{}", parts[3], parts[4], parts[5]))?;
                Ok(TimeCondition::Between(start_date, end_date))
            }
            _ => {
                let date = self.parse_date(value)?;
                let start = date;
                let end = start + Duration::days(1);
                Ok(TimeCondition::Between(start, end))
            }
        }
    }

    fn parse_date(&self, date_str: &str) -> Result<DateTime<Utc>, ParseError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|_| ParseError::InvalidDate(date_str.to_string()))?;
        Ok(date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| ParseError::InvalidDate(date_str.to_string()))?
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| ParseError::InvalidDate(date_str.to_string()))?
            .with_timezone(&Utc))
    }

    fn parse_size(&self, value: &str) -> Result<SizeCondition, ParseError> {
        match value {
            "empty" => Ok(SizeCondition::Empty),
            "tiny" => Ok(SizeCondition::Category(SizeCategory::Tiny)),
            "small" => Ok(SizeCondition::Category(SizeCategory::Small)),
            "medium" => Ok(SizeCondition::Category(SizeCategory::Medium)),
            "large" => Ok(SizeCondition::Category(SizeCategory::Large)),
            "huge" => Ok(SizeCondition::Category(SizeCategory::Huge)),
            _ if value.starts_with('>') => {
                let size_str = &value[1..];
                let size = self
                    .config
                    .parse_size(size_str)
                    .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
                Ok(SizeCondition::GreaterThan(size))
            }
            _ if value.starts_with('<') => {
                let size_str = &value[1..];
                let size = self
                    .config
                    .parse_size(size_str)
                    .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
                Ok(SizeCondition::LessThan(size))
            }
            _ if value.starts_with('=') => {
                let size_str = &value[1..];
                let size = self
                    .config
                    .parse_size(size_str)
                    .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
                Ok(SizeCondition::Equals(size))
            }
            _ if value.contains('-') => {
                let parts: Vec<&str> = value.split('-').collect();
                if parts.len() != 2 {
                    return Err(ParseError::InvalidSize(value.to_string()));
                }
                let min = self
                    .config
                    .parse_size(parts[0])
                    .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
                let max = self
                    .config
                    .parse_size(parts[1])
                    .ok_or_else(|| ParseError::InvalidSize(value.to_string()))?;
                Ok(SizeCondition::Range(min, max))
            }
            _ => Err(ParseError::InvalidSize(value.to_string())),
        }
    }

    fn parse_ext_type(&self, value: &str) -> Result<ExtTypeCategory, ParseError> {
        match value {
            "source" => Ok(ExtTypeCategory::Source),
            "document" => Ok(ExtTypeCategory::Document),
            "image" => Ok(ExtTypeCategory::Image),
            "archive" => Ok(ExtTypeCategory::Archive),
            "config" => Ok(ExtTypeCategory::Config),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }

    fn parse_path_pattern(&self, value: &str) -> Result<glob::Pattern, ParseError> {
        glob::Pattern::new(value)
            .map_err(|_| ParseError::InvalidPattern(value.to_string()))
    }

    fn parse_range(&self, value: &str) -> Result<RangeCondition, ParseError> {
        if value.starts_with('>') {
            let num_str = &value[1..];
            let num = num_str
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(RangeCondition::GreaterThan(num))
        } else if value.starts_with('<') {
            let num_str = &value[1..];
            let num = num_str
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(RangeCondition::LessThan(num))
        } else if value.contains('-') {
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() != 2 {
                return Err(ParseError::InvalidRange(value.to_string()));
            }
            let min = parts[0]
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            let max = parts[1]
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(RangeCondition::Range(min, max))
        } else {
            let num = value
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(RangeCondition::Equals(num))
        }
    }

    fn parse_permission(&self, value: &str) -> Result<PermissionCondition, ParseError> {
        match value {
            "executable" => Ok(PermissionCondition::Executable),
            "readable" => Ok(PermissionCondition::Readable),
            "writable" => Ok(PermissionCondition::Writable),
            "readonly" => Ok(PermissionCondition::ReadOnly),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }

    fn parse_git(&self, value: &str) -> Result<GitCondition, ParseError> {
        match value {
            "tracked" => Ok(GitCondition::Tracked),
            "untracked" => Ok(GitCondition::Untracked),
            "modified" => Ok(GitCondition::Modified),
            "staged" => Ok(GitCondition::Staged),
            "ignored" => Ok(GitCondition::Ignored),
            "committed-today" => Ok(GitCondition::CommittedToday),
            "never-committed" => Ok(GitCondition::NeverCommitted),
            "stale" => Ok(GitCondition::Stale),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }
}
