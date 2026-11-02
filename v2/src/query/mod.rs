use crate::{
    store::{Connected, Store, StoreError},
    types::TagrPath,
};
pub mod parse;

use super::types::QueryCriteria;

///# Errors
///# Panics
pub async fn execute(
    query: &QueryCriteria,
    store: &Store<Connected>,
) -> Result<Option<Vec<TagrPath>>, StoreError> {
    return Err(StoreError::NotImplementedError);
    /*if let Some(tags) = &query.tags {
        if tags.is_empty() {
            return Ok(None)
        }
        let files = store.search_file_by_tag(tags.first().unwrap()).await;
        //Ok(files)
    } else {
        //Ok(None)
    }
    */
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use sqlx::SqlitePool;

    use crate::{QueryCriteria, TagName, execute, store::Store, types::TagFilter};

    #[sqlx::test]
    async fn test_tag_execute(pool: SqlitePool) {
        let store = Store::from_pool("test", pool);
        let tag = TagName::from_str("test").unwrap();
        let path = PathBuf::from_str("/home").unwrap();
        store.tag_file(&[&tag], &path).await;
        let qc = QueryCriteria {
            tags: vec![TagName::from_str("test").unwrap()]
                .into_iter()
                .map(|name| TagFilter {
                    name,
                    negate: false,
                })
                .collect(),

            query: None,
            vtags: Vec::new(),
        };

        let a = execute(&qc, &store).await.unwrap().unwrap();
        assert!(a.contains(&path));
    }
}
