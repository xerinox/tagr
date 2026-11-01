use std::{path::PathBuf, str::FromStr};

/// Newtype for tag names
#[derive(Debug, Clone)]
pub struct TagName(pub String);

pub struct Tag {

}

pub type TagrPath = PathBuf;

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

