
use crate::{TagName, store::{self, Connected, StoreError}};
#[derive(Debug)]
pub struct TagCommand {
    file: String,
    tag: TagName,
}

impl TagCommand {
    #[must_use]
    pub const fn new(file: String, tag: TagName) -> Self {
        Self { file, tag }
    }
}

///# Errors
pub async fn run(cmd: TagCommand, store: &store::Store<Connected>) -> Result<(), StoreError> {
    store.tag_file(&[&cmd.tag], &cmd.file).await
}
