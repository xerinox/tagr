use crate::{store::{Connected, Store, StoreError}, types::TagrPath};

use super::types::QueryCriteria;

///# Errors
///# Panics
pub async fn execute(query: &QueryCriteria, store: &Store<Connected>) -> Result<Option<Vec<TagrPath>>, StoreError> {
    if let Some(tags) = &query.tags {
        if tags.is_empty() {
            return Ok(None)
        }
        let files = store.search_file_by_tag(tags.first().unwrap()).await?;
        Ok(files)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

use sqlx::SqlitePool;

use crate::{QueryCriteria, TagName, execute, store::Store};

    #[sqlx::test]
    async fn test_tag_execute(pool: SqlitePool) {
        let store = Store::from_pool("test", pool);
        let tag = TagName::from_str("test").unwrap();
        store.tag_file(&[&tag], "/home").await.unwrap();
        let qc = QueryCriteria{
            tags: Some(vec![TagName::from_str("test").unwrap()])
        };

        let a = execute(&qc, &store).await.unwrap().unwrap();
        assert!(a.contains(&tag));

    }
}
