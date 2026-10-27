use std::{fmt::Display, path::PathBuf, str::FromStr};

/// Newtype for tag names
#[derive(Debug, Clone)]
pub struct TagName(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagrPath(pub PathBuf);

impl From<PathBuf> for TagrPath {
    fn from(value: PathBuf) -> Self {
        Self(value)
    }
}

impl Display for TagrPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string_lossy())
    }
}

#[derive(Debug)]
pub struct TagrPathError {
}

impl FromStr for TagrPath {
    type Err = TagrPathError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let p = PathBuf::from(s);
        Ok(Self(p))
    }
}

impl FromStr for TagName {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Main search struct
pub struct QueryCriteria {
    pub tags: Option<Vec<TagName>>,
}

