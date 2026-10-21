use std::{
    path::PathBuf,
    str::FromStr,
};

use sqlx::{
    Error as SqlxError, FromRow, Row, Sqlite, SqlitePool,
    migrate::{MigrateDatabase, MigrateError},
    sqlite::SqliteRow,
};

impl From<DbTag> for TagName {
    fn from(value: DbTag) -> Self {
        Self(value.0)
    }
}

const DB_URL: &str = "sqlite://sqlite.db";
use thiserror::Error;

use crate::TagName;
#[derive(Clone, Debug, sqlx::Type)]
#[sqlx(transparent)]
struct DbTag(String);
impl FromStr for DbTag {
    type Err = sqlx::error::BoxDynError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl FromRow<'_, SqliteRow> for DbTag {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self(row.try_get("name")?))
    }
}

#[derive(Clone, Debug, FromRow)]
struct File {
    id: i64,
    path: String,
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error(transparent)]
    SqlxError(#[from] SqlxError),
    #[error(transparent)]
    MigrateError(#[from] MigrateError),
    #[error("Could not store tag/file {0}")]
    TagError(String),
}
#[derive(Debug)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    ///
    ///# Errors
    ///# Panics
    pub async fn new() -> Result<Self, StoreError> {
        let pool = if Sqlite::database_exists(DB_URL).await.unwrap_or(false) {
            println!("Database already exists");
            SqlitePool::connect(DB_URL).await?
        } else {
            println!("setting up database{DB_URL}");
            match Sqlite::create_database(DB_URL).await {
                Ok(()) => SqlitePool::connect(DB_URL).await?,
                Err(error) => Err(error)?,
            }
        };
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let migrations = std::path::Path::new(&crate_dir).join("./migrations");

        sqlx::migrate::Migrator::new(migrations)
            .await?
            .run(&pool)
            .await?;

        sqlx::query(
            "SELECT name
            FROM sqlite_schema
            WHERE type ='table'
            AND name NOT LIKE 'sqlite_%';",
        )
        .fetch_all(&pool)
        .await?;

        Ok(Self { pool })
    }

    ///
    /// # Errors
    pub async fn list_tags(&self) -> Result<Vec<TagName>, StoreError> {
        Ok(sqlx::query("SELECT name FROM tags")
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|row| row.get::<DbTag, &str>("name").into())
            .collect())
    }

    ///
    /// # Errors
    /// # Panics
    pub async fn tag_file(tag: TagName, file: PathBuf) -> Result<(), StoreError> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use crate::store::Store;
    #[tokio::test]
    async fn test_db_connect() {
        let store = Store::new().await;
        assert!(store.is_ok());
    }
}
