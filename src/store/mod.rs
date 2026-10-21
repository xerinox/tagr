use std::str::FromStr;

pub struct NotConnected;
#[derive(Debug, Clone)]
pub struct Connected {
    pool: SqlitePool,
}

use sqlx::{
    Error as SqlxError, Row, Sqlite, SqlitePool,
    migrate::{MigrateDatabase, MigrateError},
};

use thiserror::Error;

use crate::TagName;

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

    ///
    /// # Errors
    /// # Panics
    pub async fn tag_file(&self, tags: &[&TagName], file: &str) -> Result<(), StoreError> {
        let mut tx = self.state.pool.begin().await?;
        let file_id: i64 =
            sqlx::query_scalar!("INSERT INTO files (path) VALUES (?) ON CONFLICT(path) DO UPDATE SET path = path RETURNING id", file)
                .fetch_one(&mut *tx)
                .await?;

        for tag in tags {
            let tag_id = sqlx::query_scalar!(
                "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
                tag.0
            )
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query!(
                "INSERT INTO tags_x_files (tag_id, file_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
                tag_id,
                file_id
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.map_err(StoreError::from)
    }

    ///# Errors
    pub async fn search_file_by_tag(
        &self,
        tag: &TagName,
    ) -> Result<Option<Vec<String>>, StoreError> {
        let recs = sqlx::query!(
            "SELECT files.path from files JOIN tags_x_files as txf ON files.id = txf.file_id JOIN tags ON txf.tag_id = tags.id WHERE tags.name = ?",
            tag.0
        ).fetch_all(&self.state.pool).await?;
        if recs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(recs.into_iter().map(|r| r.path).collect()))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        TagName,
        store::{Store, StoreError},
    };
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn test_create_database() {
        Store::create_database("test_create").await.unwrap();
        assert!(Store::db_exists("test_create").await.unwrap());
        Store::drop_database("test_create").await.unwrap();
        assert!(!Store::db_exists("test_create").await.unwrap());
    }

    #[sqlx::test]
    async fn test_read(pool: SqlitePool) -> Result<(), StoreError> {
        let store = Store::from_pool("test", pool);
        let test_tag = TagName("test".to_string());
        store.tag_file(&[&test_tag], "/tmp/testfile").await?;
        let results = store.search_file_by_tag(&test_tag).await?;
        assert!(results.is_some_and(|i| { i.contains(&"/tmp/testfile".to_string()) }));
        Ok(())
    }
}
