
use crate::{TagName, store, store::Connected};
#[derive(Debug)]
pub struct TagCommand {
    file: Option<String>,
    tag: TagName,
}

impl TagCommand {
    #[must_use]
    pub const fn new(tag: TagName, file: Option<String>) -> Self {
        Self { tag, file }
    }
}
pub fn run(cmd: TagCommand, store: &store::Store<Connected>) {
    dbg!(cmd);
}
