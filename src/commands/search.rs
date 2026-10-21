
use crate::{TagName, store::{Connected, Store, StoreError}};

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
pub async fn run(cmd: SearchCommand, store: &Store<Connected>) -> Result<Option<Vec<String>>, StoreError>{
    store.search_file_by_tag(&cmd.tag).await

}
