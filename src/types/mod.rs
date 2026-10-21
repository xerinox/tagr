use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct TagName(pub String);

impl FromStr for TagName {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

