use std::{path::PathBuf, str::FromStr, time::{Duration, SystemTime}};

/// Newtype for tag names
#[derive(Debug, Clone)]
pub struct TagName(pub String);

pub type TagrPath = PathBuf;

impl FromStr for TagName {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Main search struct
pub struct QueryCriteria {
    pub query: Option<String>,
    pub tags: Vec<TagFilter>,
    pub vtags: Vec<VTagFilter>
}

pub struct TagFilter {
    pub name: TagName,
    pub negate: bool,
}

pub struct VTagFilter {
    pub kind: VTagKind,
    pub negate: bool,
}

pub enum VTagKind {
    Git(GitVTag),
    Fs(FsVTag),
    Mtime(MtimeVTag),
}

pub enum GitVTag {
    Untracked,
    Staged
}

pub enum FsVTag {
    Size(SizeConstraint),
    IsDir,
    IsSymLink
}

pub enum SizeConstraint {
    Greater(u64),
    Less(u64),
}

pub enum MtimeVTag {
    Created(MtimeConstraint),
    Modified(MtimeConstraint),
}

pub enum MtimeConstraint {
    Last(Duration),
    Before(SystemTime)
}
