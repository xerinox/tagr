use crate::{TagName, store::{Connected, Store, StoreError}, types::TagrPath};

#[derive(Debug)]
pub struct SearchCommand {
    tag: TagName,
}
impl SearchCommand {
    #[must_use]
    pub const fn new(tag: TagName) -> Self {
        Self {tag}
    }
}
/// # Errors
pub async fn run(tag: &TagName,  store: &Store<Connected>) -> Result<Option<Vec<TagrPath>>, StoreError>{
    todo!();
    /*
    store.search_file_by_tag(tag).await
    */
}
