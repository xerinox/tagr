use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use std::path::PathBuf;

use super::parser::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualTag {
    Modified(TimeCondition),
    Created(TimeCondition),
    Accessed(TimeCondition),
    Size(SizeCondition),
    Extension(String),
    ExtensionType(ExtTypeCategory),
    Directory(PathBuf),
    Path(glob::Pattern),
    Depth(RangeCondition),
    Permission(PermissionCondition),
    Lines(RangeCondition),
    Git(GitCondition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeCondition {
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    ThisYear,
    LastNDays(u32),
    LastNHours(u32),
    After(DateTime<Utc>),
    Before(DateTime<Utc>),
    Between(DateTime<Utc>, DateTime<Utc>),
}

impl TryFrom<&str> for TimeCondition {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "today" => Ok(Self::Today),
            "yesterday" => Ok(Self::Yesterday),
            "this-week" => Ok(Self::ThisWeek),
            "this-month" => Ok(Self::ThisMonth),
            "this-year" => Ok(Self::ThisYear),
            _ if value.starts_with("last-") && value.ends_with("-days") => {
                let days_str = value
                    .strip_prefix("last-")
                    .and_then(|s| s.strip_suffix("-days"))
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let days = days_str
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidValue(value.to_string()))?;
                Ok(Self::LastNDays(days))
            }
            _ if value.starts_with("last-") && value.ends_with("-hours") => {
                let hours_str = value
                    .strip_prefix("last-")
                    .and_then(|s| s.strip_suffix("-hours"))
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let hours = hours_str
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidValue(value.to_string()))?;
                Ok(Self::LastNHours(hours))
            }
            _ if value.starts_with("after-") => {
                let date_str = value
                    .strip_prefix("after-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let date = parse_date(date_str)?;
                Ok(Self::After(date))
            }
            _ if value.starts_with("before-") => {
                let date_str = value
                    .strip_prefix("before-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let date = parse_date(date_str)?;
                Ok(Self::Before(date))
            }
            _ if value.starts_with("between-") => {
                let range_str = value
                    .strip_prefix("between-")
                    .ok_or_else(|| ParseError::InvalidValue(value.to_string()))?;
                let parts: Vec<&str> = range_str.split('-').collect();
                if parts.len() != 6 {
                    return Err(ParseError::InvalidDate(value.to_string()));
                }
                let start_date = parse_date(&format!("{}-{}-{}", parts[0], parts[1], parts[2]))?;
                let end_date = parse_date(&format!("{}-{}-{}", parts[3], parts[4], parts[5]))?;
                Ok(Self::Between(start_date, end_date))
            }
            _ => {
                let date = parse_date(value)?;
                let start = date;
                let end = start + Duration::days(1);
                Ok(Self::Between(start, end))
            }
        }
    }
}

fn parse_date(date_str: &str) -> Result<DateTime<Utc>, ParseError> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SizeCondition {
    Empty,
    Category(SizeCategory),
    GreaterThan(u64),
    LessThan(u64),
    Equals(u64),
    Range(u64, u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeCategory {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtTypeCategory {
    Source,
    Document,
    Image,
    Archive,
    Config,
}

impl TryFrom<&str> for ExtTypeCategory {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "source" => Ok(Self::Source),
            "document" => Ok(Self::Document),
            "image" => Ok(Self::Image),
            "archive" => Ok(Self::Archive),
            "config" => Ok(Self::Config),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeCondition {
    Equals(u64),
    GreaterThan(u64),
    LessThan(u64),
    Range(u64, u64),
}

impl TryFrom<&str> for RangeCondition {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if let Some(num_str) = value.strip_prefix('>') {
            let num = num_str
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(Self::GreaterThan(num))
        } else if let Some(num_str) = value.strip_prefix('<') {
            let num = num_str
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(Self::LessThan(num))
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
            Ok(Self::Range(min, max))
        } else {
            let num = value
                .parse::<u64>()
                .map_err(|_| ParseError::InvalidRange(value.to_string()))?;
            Ok(Self::Equals(num))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCondition {
    Executable,
    Readable,
    Writable,
    ReadOnly,
}

impl TryFrom<&str> for PermissionCondition {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "executable" => Ok(Self::Executable),
            "readable" => Ok(Self::Readable),
            "writable" => Ok(Self::Writable),
            "readonly" => Ok(Self::ReadOnly),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCondition {
    Tracked,
    Untracked,
    Modified,
    Staged,
    Ignored,
    CommittedToday,
    NeverCommitted,
    Stale,
}

impl TryFrom<&str> for GitCondition {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "tracked" => Ok(Self::Tracked),
            "untracked" => Ok(Self::Untracked),
            "modified" => Ok(Self::Modified),
            "staged" => Ok(Self::Staged),
            "ignored" => Ok(Self::Ignored),
            "committed-today" => Ok(Self::CommittedToday),
            "never-committed" => Ok(Self::NeverCommitted),
            "stale" => Ok(Self::Stale),
            _ => Err(ParseError::InvalidValue(value.to_string())),
        }
    }
}
