
use crate::{TagName, store::Store, store::Connected};

#[derive(Debug)]
pub struct SearchCommand {
    tag: TagName,
}
impl SearchCommand {
    pub const fn new(tag: TagName) -> Self {
        Self {tag}
    }
}
pub fn run(cmd: SearchCommand, store: &Store<Connected>) {
    dbg!(cmd);

}
