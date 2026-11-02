use sqlx::{Encode, sqlite::any};
use std::path::PathBuf;
use std::str::FromStr;
pub mod ids;
mod models;

pub struct NotConnected;
#[derive(Debug, Clone)]
pub struct Connected {
    pool: SqlitePool,
}

use sqlx::{
    Error as SqlxError, Row, Sqlite, SqlitePool, Type, TypeInfo,
    migrate::{MigrateDatabase, MigrateError},
    sqlite::SqliteTypeInfo,
};

use thiserror::Error;

use crate::{TagName, store::models::FileRecord, types::TagrPath};

#[derive(Error, Debug)]
pub enum StoreError {
    #[error(transparent)]
    SqlxError(#[from] SqlxError),
    #[error(transparent)]
    MigrateError(#[from] MigrateError),
    #[error("Could not store tag/file {0}")]
    TagError(String),
    #[error("Wrong command supplied to run")]
    MismatchedCommand,
    #[error("Delete this error when fixing next step")]
    NotImplementedError,
}

#[derive(Debug)]
pub struct Store<S> {
    db_name: String,
    state: S,
}

impl<S> Store<S> {
    fn name_to_url(name: &str) -> String {
        format!("sqlite://{name}.db")
    }
}

impl Store<NotConnected> {
    ///
    ///# Errors
    ///# Panics
    #[must_use]
    pub fn new(db_name: &str) -> Self {
        Self {
            db_name: db_name.to_string(),
            state: NotConnected,
        }
    }
    ///
    ///# Errors
    pub async fn create_database(name: &str) -> Result<(), StoreError> {
        Ok(Sqlite::create_database(&Self::name_to_url(name)).await?)
    }

    ///
    ///# Errors
    pub async fn db_exists(name: &str) -> Result<bool, StoreError> {
        let url = &Self::name_to_url(name);
        Sqlite::database_exists(url).await.map_err(StoreError::from)
    }

    ///
    ///# Errors
    pub async fn connect(self) -> Result<Store<Connected>, StoreError> {
        let url = &Self::name_to_url(&self.db_name);
        if !Sqlite::database_exists(url).await.unwrap_or(false) {
            Self::create_database(&self.db_name).await?;
        }
        let pool = SqlitePool::connect(url).await?;

        Ok(Store {
            db_name: self.db_name,
            state: Connected { pool },
        })
    }
}

impl Store<Connected> {
    /// # Errors
    pub async fn search_file_by_tag(&self, tag: &TagName) -> Result<Vec<FileRecord>, StoreError> {
        Ok(sqlx::query_as!(
            FileRecord,
            r#"SELECT files.id, 
            files.path, 
            files.basename, 
            files.extension, 
            files.size_bytes, 
            files.mtime_epoch_ms, 
            files.file_exists, 
            files.last_seen_at_epoch_ms
            FROM files 
            LEFT JOIN file_tags AS ft ON ft.file_id = files.id
            LEFT JOIN tags AS t ON t.id = ft.tag_id
            WHERE t.name = ?
            "#,
            tag.0
        )
        .fetch_all(&self.state.pool)
        .await?)
    }
    pub async fn search_file_by_path() {
        todo!();
    }
    pub async fn tag_file(&self, _tags: &[&TagName], _file: &PathBuf) {
        todo!();
    }
    #[must_use]
    pub fn from_pool(name: &str, pool: SqlitePool) -> Self {
        Self {
            db_name: name.to_string(),
            state: Connected { pool },
        }
    }
    ///
    /// # Errors
    pub async fn list_tags(&self) -> Result<Vec<TagName>, StoreError> {
        Ok(sqlx::query("SELECT name FROM tags")
            .fetch_all(&self.state.pool)
            .await?
            .iter()
            .filter_map(|row| TagName::from_str(row.get::<&str, &str>("name")).ok())
            .collect())
    }

    ///
    ///# Errors
    pub async fn drop_database(name: &str) -> Result<(), StoreError> {
        Ok(Sqlite::drop_database(&Self::name_to_url(name)).await?)
    }

    ///# Errors
    ///# Panics
    pub async fn migrate(&self) -> Result<String, StoreError> {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let migrations = std::path::Path::new(&crate_dir).join("./migrations");

        sqlx::migrate::Migrator::new(migrations)
            .await?
            .run(&self.state.pool)
            .await?;

        let _ = sqlx::query(
            "SELECT name
            FROM sqlite_schema
            WHERE type ='table'
            AND name NOT LIKE 'sqlite_%';",
        )
        .fetch_all(&self.state.pool)
        .await?;
        Ok(String::new())
    }
}

#[cfg(test)]
mod test {
    use super::models::*;
    use std::str::FromStr;

    use crate::{
        TagName,
        store::{Store, StoreError},
        types::TagrPath,
    };
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn test_create_database() {
        Store::create_database("test_create").await.unwrap();
        assert!(Store::db_exists("test_create").await.unwrap());
        Store::drop_database("test_create").await.unwrap();
        assert!(!Store::db_exists("test_create").await.unwrap());
    }
}
