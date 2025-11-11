use chrono::{DateTime, Utc};
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeCondition {
    Equals(u64),
    GreaterThan(u64),
    LessThan(u64),
    Range(u64, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCondition {
    Executable,
    Readable,
    Writable,
    ReadOnly,
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
