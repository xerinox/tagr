use sqlx::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
#[sqlx(transparent)]
pub struct FileId(pub i64);

impl From<i64> for FileId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
#[sqlx(transparent)]
pub struct TagId(pub i64);

impl From<i64> for TagId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}
